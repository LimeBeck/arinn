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
    qsizetype cost = 0;
  };
  struct Provider {
    QString id, generation;
    std::optional<ProviderMessages::Catalog> catalog;
    QMap<QByteArray, Cached> fields;
    QMap<QString, std::shared_ptr<ProviderMessages::Dictionary>> dictionaries;
    QStringList localPaths;
    QSet<QString> remotePending, remoteAbsent;
    QMap<QString, quint64> revisions;
    QString locale;
    bool catalogPending = false, unsupported = false;
    qint64 retryAt = 0;
  };
  using Key = QPair<QString, QString>;
  struct Subscriber { QPointer<QObject> owner; std::function<void()> changed; };
  QMap<Key, std::shared_ptr<Provider>> m_providers;
  QList<Subscriber> m_subscribers;
  QString m_owner;
  quint64 m_epoch = 0;
  int m_pending = 0;
  bool m_flushScheduled = false, m_notifyScheduled = false;
  bool m_registryPending = false, m_registryDirty = false;
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
    return epoch == m_epoch && !m_owner.isEmpty() && m_providers.value({p->id, p->generation}) == p;
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
  void ownerChanged(const QString &owner) {
    ++m_epoch;
    m_owner = owner;
    m_providers.clear();
    m_registryPending = m_registryDirty = false;
    // Old calls still consume their slot until their bounded reply arrives.
    notify();
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
    int pages = 0;
  };
  void readRegistry(const std::shared_ptr<Registry> &registry, const QString &token = {}) {
    m_registryPending = true;
    const auto epoch = m_epoch;
    call("Providers", {token}, [this, registry, epoch](const QDBusMessage &reply) {
      if (epoch != m_epoch) return;
      m_registryPending = false;
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
            generation.toUtf8().size() > 128 || registry->identities.contains(id) || registry->identities.size() >= 4096) return;
        registry->identities.insert(id, {generation, plugin});
      }
      arg.endArray();
      arg >> next;
      arg.endStructure();
      if (!next.isEmpty()) {
        if (next.toUtf8().size() > 4096 || registry->tokens.contains(next)) return;
        registry->tokens.insert(next);
        readRegistry(registry, next);
        return;
      }
      bool changed = false;
      for (auto it = m_providers.begin(); it != m_providers.end();) {
        const auto identity = registry->identities.constFind(it.key().first);
        if (identity == registry->identities.end() || identity->first != it.key().second ||
            (it.value()->catalog && identity->second != it.value()->catalog->pluginId)) {
          it = m_providers.erase(it);
          changed = true;
        } else ++it;
      }
      if (changed) notify();
    }, service);
  }
  qsizetype dictionaryBytes() const {
    qsizetype total = 0;
    for (const auto &p : m_providers)
      for (const auto &dictionary : p->dictionaries) total += dictionary->byteSize();
    return total;
  }
  bool reserveReference(qsizetype bytes, int additionalCount = 1) {
    qsizetype total = bytes;
    int count = additionalCount;
    for (const auto &p : m_providers) {
      for (const auto &field : p->fields) {
        total += field.cost;
        ++count;
      }
    }
    // Stable admission: own repaint notifications must not evict and
    // re-request a presentation larger than the cache. A newer owning
    // revision retires its old fields; excess fields keep plaintext.
    return total <= referenceBudget && count <= 512;
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
        p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000;
        return;
      }
      if (reply.signature() != QLatin1String("ss") || reply.arguments().size() != 2 || reply.arguments().at(0).toString() != p->generation) {
        p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000;
        return;
      }
      const auto bytes = reply.arguments().at(1).toString().toUtf8();
      if (bytes.size() > ProviderMessages::maxJsonBytes) { p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000; return; }
      const auto object = QJsonDocument::fromJson(bytes).object();
      p->catalog = ProviderMessages::catalog(bytes, object.value("pluginId").toString());
      if (!p->catalog) { p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000; return; }
      loadLocal(p);
      notify();
    });
  }
  void fetchDictionary(const std::shared_ptr<Provider> &p, const QString &tag) {
    p->remotePending.insert(tag);
    const auto epoch = m_epoch;
    call("GetTranslation", {p->id, p->generation, tag}, [this, p, epoch, tag](const QDBusMessage &reply) {
      if (!current(p, epoch)) return;
      p->remotePending.remove(tag);
      if (!LocalTranslations::locales(locale()).contains(tag)) return;
      if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != QLatin1String("ay") || reply.arguments().size() != 1) {
        if (reply.errorName().endsWith(".NotFound")) p->remoteAbsent.insert(tag);
        p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000;
        return;
      }
      const auto bytes = reply.arguments().first().toByteArray();
      auto dictionary = std::make_shared<ProviderMessages::Dictionary>(p->catalog->pluginId);
      if (!dictionary->load(bytes)) { p->remoteAbsent.insert(tag); return; }
      if (dictionaryBytes() + bytes.size() <= dictionaryBudget) {
        p->dictionaries.insert(QStringLiteral(":remote:") + tag, dictionary);
        notify();
      } else p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000;
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
      const auto reject = [p] { p->retryAt = QDateTime::currentMSecsSinceEpoch() + 3000; };
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
      m_registryDirty = false;
      readRegistry(std::make_shared<Registry>());
    }
    const auto providers = m_providers;
    for (const auto &p : providers) {
      if (m_pending >= 4) return;
      if (p->unsupported || p->retryAt > QDateTime::currentMSecsSinceEpoch()) continue;
      if (!p->catalog) { if (!p->catalogPending) fetchCatalog(p); continue; }
      for (const auto &tag : LocalTranslations::locales(locale())) {
        if (m_pending >= 4) break;
        if (p->catalog->locales.contains(tag) && !p->dictionaries.contains(QStringLiteral(":remote:") + tag) &&
            !p->remotePending.contains(tag) && !p->remoteAbsent.contains(tag)) fetchDictionary(p, tag);
      }
      if (m_pending < 4) resolve(p);
    }
  }
  explicit ProviderTextClient(QObject *parent) : QObject(parent) {
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
    const Key identity{provider, generation};
    auto p = m_providers.value(identity);
    if (!p) {
      for (auto it = m_providers.begin(); it != m_providers.end();)
        it = it.key().first == provider ? m_providers.erase(it) : std::next(it);
      if (m_providers.size() >= 64) return plain;
      p = std::make_shared<Provider>();
      p->id = provider;
      p->generation = generation;
      m_providers.insert(identity, p);
    }
    if (revision) {
      const auto latest = p->revisions.value(scope);
      if (revision < latest) return plain;
      if (revision > latest) {
        p->revisions.insert(scope, revision);
        for (auto it = p->fields.begin(); it != p->fields.end();)
          it = it->request.scope == scope && it->request.revision != revision ? p->fields.erase(it) : std::next(it);
      }
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
      it = p->fields.insert(key, {request, {}, false, false, wire.size()});
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
