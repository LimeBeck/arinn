// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "language_change.h"
#include "provider_messages.h"
#include <QCoreApplication>
#include <QDateTime>
#include <QDBusConnection>
#include <QDBusArgument>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusServiceWatcher>
#include <QPointer>
#include <QScopeGuard>
#include <QTimer>
#include <functional>

// Shared presentation cache. Only the daemon is contacted, asynchronously.
// Snapshot plaintext remains in each model; a lookup never changes identity,
// sends a content command or formats the English source as a translation.
class ProviderTextClient final : public QObject {
  struct Request {
    QString scope, id, field, fallback;
    quint64 revision = 0;
    QByteArray wire() const {
      auto bytes = QJsonDocument(QJsonObject{{"scope", scope}, {"objectId", id},
          {"field", field}, {"fallbackText", fallback}}).toJson(QJsonDocument::Compact);
      bytes.chop(1);
      // QJsonValue cannot represent every unsigned snapshot revision.
      return bytes + ",\"revision\":" + QByteArray::number(revision) + '}';
    }
  };
  struct Cached {
    Request request;
    std::optional<ProviderMessages::Reference> reference;
    bool pending = false, complete = false;
    bool owned = false;
    qsizetype cost = 0;
  };
  struct Provider {
    QString id, generation;
    std::optional<ProviderMessages::Catalog> catalog;
    QMap<QByteArray, Cached> fields;
    QMap<QString, std::shared_ptr<ProviderMessages::Dictionary>> dictionaries;
    QStringList localPaths;
    QSet<QString> remotePending, remoteAbsent;
    QMap<QString, qsizetype> remoteBlocked;
    QString locale;
    bool catalogPending = false, unsupported = false;
    qint64 retryAt = 0;
  };
  using Key = QPair<QString, QString>;
  using Context = QPair<quintptr, QString>;
  using Fields = QMap<Key, QSet<QByteArray>>;
  struct Frame { Context context; Fields fields; Frame *previous = nullptr; };
  struct Lease { QPointer<QObject> owner; Fields fields; };
  QMap<Context, Lease> m_leases;
  Frame *m_recording = nullptr;
  struct Subscriber { QPointer<QObject> owner; std::function<void()> changed; };
  QMap<Key, std::shared_ptr<Provider>> m_providers;
  QList<Subscriber> m_subscribers;
  QString m_owner;
  quint64 m_epoch = 0;
  int m_pending = 0;
  QTimer m_retryTimer;
  bool m_flushScheduled = false, m_notifyScheduled = false;
  bool m_registryPending = false, m_registryDirty = false;
  bool m_registryKnown = false;
  qint64 m_registryRetryAt = 0;
  QMap<QString, QPair<QString, QString>> m_identities;
  QMap<QString, QString> m_names;
  static constexpr auto service = "org.arinn.Daemon1";
  static constexpr auto path = "/org/arinn/Daemon1";
  static constexpr auto interface = "org.arinn.Texts1";
  static constexpr qsizetype referenceBudget = 4 * 1024 * 1024;
  static constexpr qsizetype dictionaryBudget = 8 * 1024 * 1024;

  QString locale() const {
    const auto raw = qApp->property("arinn.translationLocale").toString();
    return LocalTranslations::validLocale(raw) ? raw : LocalTranslations::currentLocaleTag();
  }
  bool current(const std::shared_ptr<Provider> &p, quint64 epoch) const {
    return epoch == m_epoch && !m_owner.isEmpty() && m_providers.value({p->id, p->generation}) == p &&
           (!m_registryKnown || m_identities.value(p->id).first == p->generation);
  }
  void notify() {
    if (m_notifyScheduled) return;
    m_notifyScheduled = true;
    QTimer::singleShot(0, this, [this] {
      m_notifyScheduled = false;
      // Callbacks can subscribe, so iterate a copy and discard dead owners.
      const auto subscribers = m_subscribers;
      for (const auto &subscriber : subscribers)
        if (subscriber.owner) subscriber.changed();
      m_subscribers.removeIf([](const auto &s) { return !s.owner; });
    });
  }
  void schedule() {
    if (m_flushScheduled) return;
    m_flushScheduled = true;
    QTimer::singleShot(0, this, [this] { m_flushScheduled = false; flush(); });
  }
  void wakeForRetry(qint64 deadline) {
    const auto delay = int(qBound(qint64(1), deadline - QDateTime::currentMSecsSinceEpoch(), qint64(3000)));
    if (!m_retryTimer.isActive() || m_retryTimer.remainingTime() > delay)
      m_retryTimer.start(delay);
  }
  void retry(const std::shared_ptr<Provider> &p) {
    p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000;
    wakeForRetry(p->retryAt);
  }
  void ownerChanged(const QString &owner) {
    ++m_epoch;
    m_owner = owner;
    m_retryTimer.stop();
    m_providers.clear();
    m_leases.clear();
    m_registryPending = m_registryKnown = false;
    m_registryDirty = true;
    m_registryRetryAt = 0;
    m_identities.clear();
    m_names.clear();
    // Old calls still consume their slot until their bounded reply arrives.
    notify();
    schedule();
  }
  void call(const char *method, const QVariantList &arguments,
            std::function<void(const QDBusMessage &)> finished, const char *api = interface) {
    ++m_pending;
    auto message = QDBusMessage::createMethodCall(m_owner, path, api, method);
    message.setArguments(arguments);
    auto *pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message, 3000), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this,
        [this, pending, finished = std::move(finished)] {
      const QDBusMessage reply = pending->reply();
      pending->deleteLater();
      --m_pending;
      finished(reply);
      schedule();
    });
  }
  struct Registry {
    QString snapshot;
    QSet<QString> tokens;
    QMap<QString, QPair<QString, QString>> identities; // id -> generation, plugin
    QMap<QString, QString> names;
    qsizetype bytes = 0;
    int pages = 0;
  };
  void readRegistry(const std::shared_ptr<Registry> &registry, const QString &token = {}) {
    m_registryPending = true;
    const auto epoch = m_epoch;
    call("Providers", {token}, [this, registry, epoch](const QDBusMessage &reply) {
      if (epoch != m_epoch) return;
      m_registryPending = false;
      bool successful = false;
      const auto retry = qScopeGuard([&] {
        if (!successful) {
          m_registryDirty = true;
          m_registryRetryAt = QDateTime::currentMSecsSinceEpoch() + 3000;
          wakeForRetry(m_registryRetryAt);
        }
      });
      if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != QLatin1String("(sa(sssssoasssststttt)s)") || reply.arguments().size() != 1) return;
      const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().first());
      QString snapshot, next;
      arg.beginStructure();
      arg >> snapshot;
      if ((!registry->snapshot.isEmpty() && registry->snapshot != snapshot) || ++registry->pages > 16) return;
      registry->snapshot = snapshot;
      arg.beginArray();
      while (!arg.atEnd()) {
        QString id, plugin, source, name, version, runtime, backend, generation, error;
        QStringList capabilities;
        QDBusObjectPath endpoint;
        qulonglong revision = 0, errorAt = 0, recovered = 0, restarts = 0, cumulative = 0;
        arg.beginStructure();
        arg >> id >> plugin >> source >> name >> version >> endpoint >> capabilities >> runtime >> backend >> generation >> revision >> error >> errorAt >> recovered >> restarts >> cumulative;
        arg.endStructure();
        if (id.isEmpty() || id.toUtf8().size() > 256 || LocalTranslations::pluginDirectory(plugin).isEmpty() ||
            generation.toUtf8().size() > 128 || name.toUtf8().size() > 8192 || registry->identities.contains(id) || registry->identities.size() >= 4096) return;
        registry->bytes += id.toUtf8().size() + plugin.toUtf8().size() + generation.toUtf8().size() + name.toUtf8().size();
        if (registry->bytes > 4 * 1024 * 1024) return;
        registry->identities.insert(id, {generation, plugin});
        registry->names.insert(id, name);
      }
      arg.endArray();
      arg >> next;
      arg.endStructure();
      if (!next.isEmpty()) {
        if (next.toUtf8().size() > 4096 || registry->tokens.contains(next)) return;
        registry->tokens.insert(next);
        successful = true;
        readRegistry(registry, next);
        return;
      }
      bool changed = !m_registryKnown || m_identities != registry->identities || m_names != registry->names;
      m_identities = registry->identities;
      m_names = registry->names;
      m_registryKnown = true;
      m_registryRetryAt = 0;
      for (auto it = m_providers.begin(); it != m_providers.end();) {
        const auto identity = registry->identities.constFind(it.key().first);
        if (identity == registry->identities.end() || identity->first != it.key().second ||
            (it.value()->catalog && identity->second != it.value()->catalog->pluginId)) {
          it = m_providers.erase(it);
          changed = true;
        } else {
          auto &fields = it.value()->fields;
          for (auto field = fields.begin(); field != fields.end();)
            field = field->request.scope == QLatin1String("provider") && field->request.field == QLatin1String("name") &&
                field->request.fallback != m_names.value(it.key().first) ? fields.erase(field) : std::next(field);
          ++it;
        }
      }
      successful = true;
      if (changed) notify();
    }, service);
  }
  qsizetype dictionaryBytes() const {
    qsizetype total = 0;
    for (const auto &p : m_providers)
      for (const auto &dictionary : p->dictionaries) total += dictionary->byteSize();
    return total;
  }
  bool retainedElsewhere(const Key &provider, const QByteArray &key, const Context &except) const {
    for (auto it = m_leases.begin(); it != m_leases.end(); ++it)
      if (it.key() != except && it->owner && it->fields.value(provider).contains(key)) return true;
    for (auto *frame = m_recording; frame; frame = frame->previous)
      if (frame->fields.value(provider).contains(key)) return true;
    return false;
  }
  void collectUnretained() {
    for (auto p = m_providers.begin(); p != m_providers.end(); ++p) {
      for (auto field = p.value()->fields.begin(); field != p.value()->fields.end();)
        field = field->owned && !retainedElsewhere(p.key(), field.key(), {})
            ? p.value()->fields.erase(field) : std::next(field);
    }
  }
  void finishPresentation(QObject *owner, Frame *frame) {
    m_recording = frame->previous;
    m_leases.insert(frame->context, {owner, std::move(frame->fields)});
    for (auto it = m_leases.begin(); it != m_leases.end();)
      it = it->owner ? std::next(it) : m_leases.erase(it);
    collectUnretained();
  }
  bool reserveReference(qsizetype bytes, int additionalCount = 1) {
    for (;;) {
      qsizetype total = bytes;
      int count = additionalCount;
      for (const auto &p : m_providers) {
        for (const auto &field : p->fields) {
          total += field.cost;
          ++count;
        }
      }
      if (total <= referenceBudget && count <= 512) return true;
      bool retired = false;
      if (m_recording) {
        // An updated model may replace its previous presentation, but never
        // evict a field already seen in this pass or retained by another
        // view. This keeps oversized static presentations from churning.
        const auto previous = m_leases.value(m_recording->context).fields;
        for (auto p = m_providers.begin(); p != m_providers.end() && !retired; ++p) {
          for (auto field = p.value()->fields.begin(); field != p.value()->fields.end(); ++field) {
            if (field->owned && previous.value(p.key()).contains(field.key()) &&
                !retainedElsewhere(p.key(), field.key(), m_recording->context)) {
              p.value()->fields.erase(field);
              retired = true;
              break;
            }
          }
        }
      }
      if (!retired) return false;
    }
  }
  void loadLocal(const std::shared_ptr<Provider> &p) {
    if (!p->catalog) return;
    const auto requested = locale();
    const auto chain = LocalTranslations::locales(requested);
    for (auto it = p->dictionaries.begin(); it != p->dictionaries.end();) {
      if (it.key().startsWith(":remote:") && !chain.contains(it.key().mid(8))) it = p->dictionaries.erase(it);
      else ++it;
    }
    const auto roots = LocalTranslations::roots();
    const auto paths = LocalTranslations::providerCatalogPaths(roots, p->catalog->pluginId, requested, false);
    const auto oldDictionaries = p->dictionaries;
    for (const auto &old : p->localPaths) p->dictionaries.remove(old);
    for (const auto &fileName : paths) {
      auto dictionary = std::make_shared<ProviderMessages::Dictionary>(p->catalog->pluginId);
      QFile file(fileName);
      bool loaded = false;
      if (std::any_of(roots.cbegin(), roots.cend(), [&](const auto &root) {
            return LocalTranslations::safeFile(root, fileName, ProviderMessages::maxQmBytes);
          }) && file.open(QIODevice::ReadOnly)) {
        const auto bytes = file.read(ProviderMessages::maxQmBytes + 1);
        loaded = dictionaryBytes() + bytes.size() <= dictionaryBudget && dictionary->load(bytes);
      }
      if (loaded) p->dictionaries.insert(fileName, dictionary);
      else if (p->locale == requested && QFileInfo::exists(fileName) && oldDictionaries.contains(fileName) &&
               dictionaryBytes() + oldDictionaries.value(fileName)->byteSize() <= dictionaryBudget)
        p->dictionaries.insert(fileName, oldDictionaries.value(fileName));
    }
    p->localPaths = paths;
    p->locale = requested;
  }
  void fetchCatalog(const std::shared_ptr<Provider> &p) {
    p->catalogPending = true;
    const auto epoch = m_epoch;
    call("GetCatalog", {p->id}, [this, p, epoch](const QDBusMessage &reply) {
      if (!current(p, epoch)) return;
      p->catalogPending = false;
      if (reply.type() == QDBusMessage::ErrorMessage) {
        const auto name = reply.errorName();
        p->unsupported = name.endsWith(".Unsupported") || name.endsWith(".UnknownInterface") || name.endsWith(".UnknownMethod");
        if (!p->unsupported) retry(p);
        return;
      }
      if (reply.signature() != QLatin1String("ss") || reply.arguments().size() != 2 || reply.arguments().at(0).toString() != p->generation) {
        retry(p);
        return;
      }
      const auto bytes = reply.arguments().at(1).toString().toUtf8();
      if (bytes.size() > ProviderMessages::maxJsonBytes) { retry(p); return; }
      const auto object = QJsonDocument::fromJson(bytes).object();
      p->catalog = ProviderMessages::catalog(bytes, object.value("pluginId").toString());
      if (!p->catalog) { retry(p); return; }
      loadLocal(p);
      notify();
    });
  }
  void fetchDictionary(const std::shared_ptr<Provider> &p, const QString &tag) {
    p->remoteBlocked.remove(tag);
    p->remotePending.insert(tag);
    const auto epoch = m_epoch;
    call("GetTranslation", {p->id, p->generation, tag}, [this, p, epoch, tag](const QDBusMessage &reply) {
      if (!current(p, epoch)) return;
      p->remotePending.remove(tag);
      if (!LocalTranslations::locales(locale()).contains(tag)) return;
      if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != QLatin1String("ay") || reply.arguments().size() != 1) {
        if (reply.errorName().endsWith(".NotFound")) p->remoteAbsent.insert(tag);
        else retry(p);
        return;
      }
      const auto bytes = reply.arguments().first().toByteArray();
      auto dictionary = std::make_shared<ProviderMessages::Dictionary>(p->catalog->pluginId);
      if (!dictionary->load(bytes)) { p->remoteAbsent.insert(tag); return; }
      if (dictionaryBytes() + bytes.size() <= dictionaryBudget) {
        p->dictionaries.insert(QStringLiteral(":remote:") + tag, dictionary);
        notify();
      } else p->remoteBlocked.insert(tag, bytes.size());
    });
  }
  void resolve(const std::shared_ptr<Provider> &p) {
    QList<QByteArray> keys;
    QByteArray requests("[");
    for (auto it = p->fields.begin(); it != p->fields.end() && keys.size() < 128; ++it) {
      if (it->complete || it->pending) continue;
      const auto wire = it->request.wire();
      if (requests.size() + wire.size() + 2 > ProviderMessages::maxJsonBytes) break;
      if (!keys.isEmpty()) requests += ',';
      requests += wire;
      keys.append(it.key());
      it->pending = true;
    }
    if (keys.isEmpty()) return;
    requests += ']';
    const auto epoch = m_epoch;
    call("Resolve", {p->id, p->generation, QString::fromUtf8(requests)}, [this, p, epoch, keys](const QDBusMessage &reply) {
      if (!current(p, epoch)) return;
      bool retired = false;
      for (const auto &key : keys) {
        auto field = p->fields.find(key);
        if (field == p->fields.end()) retired = true;
        else field->pending = false;
      }
      if (retired) return;
      const auto bytes = reply.arguments().value(0).toString().toUtf8();
      const auto reject = [this, p] { retry(p); };
      if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != QLatin1String("s") || bytes.size() > ProviderMessages::maxJsonBytes) { reject(); return; }
      QJsonParseError error;
      const auto document = QJsonDocument::fromJson(bytes, &error);
      if (error.error != QJsonParseError::NoError || !document.isArray() || document.array().size() > keys.size()) { reject(); return; }
      QMap<int, ProviderMessages::Reference> resolved;
      QMap<int, qsizetype> costs;
      qsizetype cost = 0;
      for (const auto &value : document.array()) {
        const auto object = value.toObject();
        const auto index = ProviderMessages::integer(object.value("requestIndex"));
        if (!value.isObject() || !ProviderMessages::keys(object, {"requestIndex", "fallbackText", "message"}) || !index || *index < 0 || *index >= keys.size() || resolved.contains(int(*index)) ||
            !object.value("fallbackText").isString() || object.value("fallbackText").toString() != p->fields.value(keys.at(int(*index))).request.fallback || !object.value("message").isObject()) { reject(); return; }
        const auto reference = ProviderMessages::reference(object.value("message").toObject(), *p->catalog);
        if (!reference) { reject(); return; }
        resolved.insert(int(*index), *reference);
        const auto referenceCost = QJsonDocument(object).toJson(QJsonDocument::Compact).size();
        costs.insert(int(*index), referenceCost);
        cost += referenceCost;
      }
      // Keep this batch pinned while making room for its typed arguments.
      for (const auto &key : keys) p->fields[key].pending = true;
      const bool room = reserveReference(cost, 0);
      for (qsizetype i = 0; i < keys.size(); ++i) {
        auto &field = p->fields[keys.at(i)];
        field.pending = false;
        field.complete = true;
        if (room && resolved.contains(int(i))) {
          field.reference = resolved.value(int(i));
          field.cost += costs.value(int(i));
        }
      }
      notify();
    });
  }
  void flush() {
    if (m_owner.isEmpty()) return;
    if (m_registryDirty && !m_registryPending && m_pending < 4) {
      if (m_registryRetryAt > QDateTime::currentMSecsSinceEpoch()) wakeForRetry(m_registryRetryAt);
      else {
        m_registryDirty = false;
        readRegistry(std::make_shared<Registry>());
      }
    }
    if (!m_registryKnown) return;
    const auto providers = m_providers;
    for (const auto &p : providers) {
      if (m_pending >= 4) return;
      if (p->unsupported || p->fields.isEmpty()) continue;
      if (p->retryAt > QDateTime::currentMSecsSinceEpoch()) { wakeForRetry(p->retryAt); continue; }
      if (!p->catalog) { if (!p->catalogPending) fetchCatalog(p); continue; }
      for (const auto &tag : LocalTranslations::locales(locale())) {
        if (m_pending >= 4) break;
        if (p->catalog->locales.contains(tag) && !p->dictionaries.contains(QStringLiteral(":remote:") + tag) &&
            !p->remotePending.contains(tag) && !p->remoteAbsent.contains(tag)) {
          if (p->remoteBlocked.contains(tag) && dictionaryBytes() + p->remoteBlocked.value(tag) > dictionaryBudget) continue;
          fetchDictionary(p, tag);
        }
      }
      if (m_pending < 4) resolve(p);
    }
  }
  explicit ProviderTextClient(QObject *parent) : QObject(parent) {
    // One shared wakeup bounds timers across providers. Precise timing avoids
    // waking before the deadline; flush rearms it for any later retry.
    m_retryTimer.setSingleShot(true);
    m_retryTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_retryTimer, &QTimer::timeout, this, [this] { schedule(); });
    // The built-in timer slot keeps this helper independent of a new moc
    // target. A registry change retires identities, unlike a local repaint.
    auto *registry = new QTimer(this);
    registry->setSingleShot(true);
    registry->setInterval(0);
    QDBusConnection::sessionBus().connect(service, path, service, "ProvidersChanged", registry, SLOT(start()));
    connect(registry, &QTimer::timeout, this, [this] { m_registryDirty = true; schedule(); });
    auto *watcher = new QDBusServiceWatcher(service, QDBusConnection::sessionBus(), QDBusServiceWatcher::WatchForOwnerChange, this);
    connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
        [this](const QString &, const QString &, const QString &owner) { ownerChanged(owner); });
    auto message = QDBusMessage::createMethodCall("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner");
    message << QString::fromLatin1(service);
    const auto epoch = m_epoch;
    auto *pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message, 3000), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this, [this, pending, epoch] {
      const QDBusMessage reply = pending->reply();
      pending->deleteLater();
      if (epoch == m_epoch && reply.type() != QDBusMessage::ErrorMessage && reply.signature() == QLatin1String("s")) ownerChanged(reply.arguments().first().toString());
    });
    new LanguageChangeObserver(this, [this] {
      // Retire obsolete resources across every provider before admitting any
      // new local dictionary, so iteration order cannot defeat precedence.
      const auto requested = locale();
      const auto chain = LocalTranslations::locales(requested);
      for (const auto &p : m_providers) {
        for (auto it = p->dictionaries.begin(); it != p->dictionaries.end();) {
          const bool remote = it.key().startsWith(":remote:");
          if ((remote && !chain.contains(it.key().mid(8))) || (!remote && p->locale != requested)) it = p->dictionaries.erase(it);
          else ++it;
        }
      }
      for (const auto &p : m_providers) loadLocal(p);
      notify();
      schedule();
    });
  }
public:
  class Presentation {
    ProviderTextClient *m_client = nullptr;
    QObject *m_owner = nullptr;
    bool m_retained = false;
    Frame m_frame;
    friend class ProviderTextClient;
    Presentation(ProviderTextClient *client, const QObject *owner, const QString &channel) {
      const Context context{reinterpret_cast<quintptr>(owner), channel};
      for (auto it = client->m_leases.begin(); it != client->m_leases.end();)
        it = it->owner ? std::next(it) : client->m_leases.erase(it);
      client->collectUnretained();
      m_client = client;
      m_owner = const_cast<QObject *>(owner);
      m_frame.context = context;
      m_frame.previous = client->m_recording;
      m_retained = owner && (client->m_leases.size() < 512 || client->m_leases.contains(context));
      client->m_recording = m_retained ? &m_frame : nullptr;
    }
  public:
    Presentation(const Presentation &) = delete;
    Presentation &operator=(const Presentation &) = delete;
    ~Presentation() {
      if (m_retained) m_client->finishPresentation(m_owner, &m_frame);
      else if (m_client) m_client->m_recording = m_frame.previous;
    }
  };
  Presentation presentation(const QObject *owner, const QString &channel) {
    return Presentation(this, owner, channel);
  }
  static ProviderTextClient *instance() {
    static QPointer<ProviderTextClient> client;
    if (!client) client = new ProviderTextClient(qApp);
    return client;
  }
  void observe(QObject *owner, std::function<void()> changed) {
    for (const auto &s : m_subscribers) if (s.owner == owner) return;
    m_subscribers.append({owner, std::move(changed)});
  }
  QString translate(const QString &provider, const QString &generation, const QString &scope,
                    const QString &id, quint64 revision, const QString &field, const QString &fallback,
                    std::optional<QString> displayFallback = {}) {
    const auto plain = displayFallback.value_or(fallback);
    if (m_owner.isEmpty() || generation.isEmpty() || !ProviderMessages::text(fallback)) return plain;
    if (m_registryKnown && m_identities.value(provider).first != generation) return plain;
    if (m_registryKnown && scope == QLatin1String("provider") && field == QLatin1String("name") && m_names.value(provider) != fallback) return plain;
    const Key identity{provider, generation};
    auto p = m_providers.value(identity);
    if (!p) {
      if (m_providers.size() >= 64) return plain;
      p = std::make_shared<Provider>();
      p->id = provider;
      p->generation = generation;
      m_providers.insert(identity, p);
    }
    const Request request{scope, id, field, fallback, revision};
    for (auto it = p->fields.begin(); it != p->fields.end();)
      it = it->request.scope == scope && it->request.id == id && it->request.revision == revision &&
          it->request.field == field && it->request.fallback != fallback ? p->fields.erase(it) : std::next(it);
    const auto wire = request.wire();
    const auto key = QCryptographicHash::hash(wire, QCryptographicHash::Sha256);
    auto it = p->fields.find(key);
    if (it == p->fields.end()) {
      if (wire.size() > ProviderMessages::maxJsonBytes || !reserveReference(wire.size())) return plain;
      it = p->fields.insert(key, {request, {}, false, false, false, wire.size()});
    }
    if (m_recording) {
      m_recording->fields[identity].insert(key);
      it->owned = true;
    }
    schedule();
    if (!p->catalog || !it->reference) return plain;
    QList<const ProviderMessages::Dictionary *> dictionaries;
    for (const auto &file : p->localPaths)
      if (p->dictionaries.contains(file)) dictionaries.append(p->dictionaries.value(file).get());
    for (const auto &tag : LocalTranslations::locales(locale())) {
      const auto dictionary = p->dictionaries.value(QStringLiteral(":remote:") + tag);
      if (dictionary) dictionaries.append(dictionary.get());
    }
    return ProviderMessages::render(*p->catalog, *it->reference, dictionaries, QLocale(), plain);
  }
};
