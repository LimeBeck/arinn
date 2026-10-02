// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "browser_catalog.h"
#include <QJsonArray>
#include <QJsonDocument>
#include <QUrl>
#include <QUuid>
#include <QtDBus>
#include <QtQml>
#include <algorithm>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
QDBusMessage daemonCall(const char *interface, const char *method) {
  return QDBusMessage::createMethodCall(service, path, interface, method);
}
QString presentationId(const QString &provider, const QString &generation,
                       const QString &kind, const QString &local) {
  return QUuid::createUuidV5(
             QUuid(), QJsonDocument(QJsonArray{provider, generation, kind, local})
                          .toJson(QJsonDocument::Compact))
      .toString();
}
QString errorCode(const QDBusMessage &reply) {
  auto name = reply.errorName();
  const auto dot = name.lastIndexOf('.');
  if (dot >= 0)
    name = name.mid(dot + 1);
  return name.isEmpty() ? QString("Unavailable") : name;
}
void registerBrowserCatalog() {
  qmlRegisterType<BrowserCatalog>("Arinn.Shell", 1, 0, "BrowserCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerBrowserCatalog)
struct Origin {
  QString scheme;
  QString host;
  int port = -1;
  bool ok = false;
};
Origin originOf(const QString &text) {
  const QUrl url(text);
  if (!url.isValid() || url.scheme().isEmpty() || url.host().isEmpty())
    return {};
  Origin origin;
  origin.scheme = url.scheme().toLower();
  auto host = url.host().toLower();
  if (host.startsWith(QLatin1String("www.")))
    host.remove(0, 4);
  origin.host = host;
  auto port = url.port();
  if (port < 0) {
    if (origin.scheme == QLatin1String("https"))
      port = 443;
    else if (origin.scheme == QLatin1String("http"))
      port = 80;
    else
      port = 0;
  }
  origin.port = port;
  origin.ok = !origin.host.isEmpty();
  return origin;
}
QString originToken(const Origin &origin) {
  if (!origin.ok || origin.scheme.isEmpty() || origin.host.isEmpty())
    return {};
  return origin.scheme + QLatin1Char('|') + origin.host + QLatin1Char('|') +
         QString::number(origin.port);
}
QString displayHost(const QString &text) {
  const QUrl url(text);
  if (!url.isValid() || url.host().isEmpty())
    return {};
  auto host = url.host().toLower();
  if (url.port() > 0)
    host += QLatin1Char(':') + QString::number(url.port());
  return host;
}
QString letterOf(const QString &text) {
  auto host = displayHost(text);
  const auto colon = host.indexOf(QLatin1Char(':'));
  if (colon >= 0)
    host.truncate(colon);
  for (const auto prefix : {QLatin1String("www."), QLatin1String("m."),
                            QLatin1String("hd.")}) {
    if (host.startsWith(prefix)) {
      host.remove(0, prefix.size());
      break;
    }
  }
  return host.isEmpty() ? QString() : host.left(1).toUpper();
}
} // namespace
struct BrowserCatalog::Catalog {
  quint64 epoch = 0, evidenceVersion = 0;
  QString owner;
  QSet<QString> seenProviders;
  QString snapshot;
  QVariantList providers;
  QVariantList windows, tabs;
  QHash<QString, TabRef> refs;
  QSet<QString> tokens, seenWindows, seenTabs, activeWindows;
  QString generation;
  quint64 revision = 0;
  int pages = 0, focused = 0;
  qsizetype windowStart = 0, tabStart = 0;
  bool hadError = false, noBrowser = false;
  QStringList failedProviders;
};
BrowserCatalog::BrowserCatalog(QObject *parent) : QObject(parent), m_association(this) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "Changed", this, SLOT(invalidateEvidence()));
  bus.connect(service, path, service, "ProvidersChanged", this, SLOT(invalidateEvidence()));
  bus.connect(service, path, service, "StateChanged", this, SLOT(invalidateEvidence()));
  bus.connect(service, path, "org.arinn.WebShortcuts1", "Changed", this,
              SLOT(refresh()));
  connect(&m_association, &ReturnAssociation::changed, this, [this] {
    presentTabs(m_tabs);
  });
  connect(&m_association, &ReturnAssociation::ownerChanged, this, [this] {
            m_completeEvidence = false;
            ++m_evidenceVersion;
            ++m_epoch;
            m_pending.clear();
            m_connected = false;
            m_loaded = false;
            m_noBrowser = false;
            m_snapshotError = false;
            m_generation.clear();
            m_home = false;
            m_tabs.clear();
            m_windows.clear();
            m_refs.clear();
            m_shortcuts.clear();
            m_origins.clear();
            m_pendingSaved.clear();
            m_saveError.clear();
            m_saveErrorTimer.stop();
            clearUseOrder();
            presentTabs({});
            m_opening = false;
            m_openingError.clear();
            m_openOperationPath.clear();
            emit windowsChanged();
            emit tabsChanged();
            emit shortcutsChanged();
            publish();
            refresh();
          });
  connect(&m_timer, &QTimer::timeout, this, &BrowserCatalog::refresh);
  m_timer.start(1000);
  m_holdTimer.setSingleShot(true);
  m_holdTimer.setInterval(1500);
  connect(&m_holdTimer, &QTimer::timeout, this, [this] { updatePresentation(true); });
  m_saveErrorTimer.setSingleShot(true);
  m_saveErrorTimer.setInterval(4000);
  connect(&m_saveErrorTimer, &QTimer::timeout, this,
          &BrowserCatalog::dismissSaveError);
  refresh();
}
QString BrowserCatalog::browserState() const {
  if (!m_connected || m_noBrowser)
    return "unavailable";
  if (!m_loaded)
    return "loading";
  if (m_snapshotError)
    return "error";
  return m_tabs.isEmpty() ? "empty" : "populated";
}
void BrowserCatalog::publish() { emit changed(); }
void BrowserCatalog::setOperation(const QString &state) {
  if (m_operation == state)
    return;
  m_operation = state;
  emit changed();
}
void BrowserCatalog::invalidateEvidence() {
  ++m_evidenceVersion;
  m_completeEvidence = false;
  presentTabs(m_tabs);
  refresh();
}
void BrowserCatalog::refresh() {
  if (m_association.owner().isEmpty()) return;
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  const auto epoch = m_epoch;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(QDBusMessage::createMethodCall(m_association.owner(), path, service, "Status"),
                                              3000),
      this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch) {
              m_refreshing = false;
              refresh();
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.arguments().isEmpty() ||
                reply.signature() != "(sbstbsssbbss)") {
              m_refreshing = false;
              m_completeEvidence = false;
              m_connected = false;
              m_loaded = false;
              m_noBrowser = false;
              m_snapshotError = false;
              if (!m_tabs.isEmpty() || !m_windows.isEmpty()) {
                m_tabs.clear();
                m_windows.clear();
                m_refs.clear();
                emit windowsChanged();
                emit tabsChanged();
              }
              if (!m_shortcuts.isEmpty()) {
                m_shortcuts.clear();
                emit shortcutsChanged();
              }
              presentTabs({});
              publish();
              return;
            }
            const QDBusArgument arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString daemonEpoch, active, klass, homeOutput, activeOutput,
                generation, returnWindow;
            qulonglong revision;
            bool full, platform, available, home;
            arg.beginStructure();
            arg >> daemonEpoch >> available >> generation >> revision >> home >>
                returnWindow >> active >> klass >> full >> platform >>
                homeOutput >> activeOutput;
            arg.endStructure();
            if (m_generation != generation) {
              m_loaded = false;
              m_snapshotError = false;
              m_noBrowser = false;
              clearUseOrder();
              if (!m_tabs.isEmpty() || !m_windows.isEmpty()) {
                m_tabs.clear();
                m_windows.clear();
                m_refs.clear();
                emit windowsChanged();
                emit tabsChanged();
              }
            }
            m_generation = generation;
            m_daemonEpoch = daemonEpoch;
            m_returnTarget = returnWindow;
            m_home = home;
            m_connected = true;
            presentTabs(m_tabs);
            publish();
            readShortcuts(epoch);
            auto catalog = std::make_shared<Catalog>();
            catalog->epoch = epoch;
            catalog->owner = m_association.owner();
            catalog->evidenceVersion = m_evidenceVersion;
            readProviders(catalog);
          },
          Qt::QueuedConnection);
}
void BrowserCatalog::readProviders(const std::shared_ptr<Catalog> &catalog,
                                   const QString &token) {
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    finishCatalog(catalog, false);
    return;
  }
  catalog->tokens.insert(token);
  auto msg = QDBusMessage::createMethodCall(catalog->owner, path, service, "Providers");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, catalog](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (catalog->epoch != m_epoch ||
                reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "(sa(sssssoasssststttt)s)") {
              finishCatalog(catalog, false);
              return;
            }
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString snapshot, next;
            arg.beginStructure();
            arg >> snapshot;
            if (!catalog->snapshot.isEmpty() && snapshot != catalog->snapshot) {
              finishCatalog(catalog, false);
              return;
            }
            catalog->snapshot = snapshot;
            arg.beginArray();
            while (!arg.atEnd()) {
              QString id, plugin, source, name, version, runtime, backend,
                  generation, error;
              QStringList capabilities;
              QDBusObjectPath endpoint;
              qulonglong revision, errorAt, recovered, restarts, cumulative;
              arg.beginStructure();
              arg >> id >> plugin >> source >> name >> version >> endpoint >>
                  capabilities >> runtime >> backend >> generation >> revision >>
                  error >> errorAt >> recovered >> restarts >> cumulative;
              arg.endStructure();
              if (!capabilities.contains(QLatin1String("browser")))
                continue;
              if (id.isEmpty() || QUuid(generation).isNull() || catalog->seenProviders.contains(id)) {
                finishCatalog(catalog, false); return;
              }
              catalog->seenProviders.insert(id);
              if (runtime != QLatin1String("ready") || backend != QLatin1String("available") ||
                  (!error.isEmpty() && recovered == 0)) catalog->hadError = true;
              if (runtime == QLatin1String("ready") &&
                  backend != QLatin1String("unavailable"))
                catalog->providers.append(
                    QVariantMap{{"id", id},
                                {"path", endpoint.path()},
                                {"generation", generation}});
              else if (runtime == QLatin1String("failed") && !id.isEmpty())
                catalog->failedProviders.append(id);
            }
            arg.endArray();
            arg >> next;
            arg.endStructure();
            if (!next.isEmpty())
              readProviders(catalog, next);
            else if (catalog->providers.isEmpty()) {
              catalog->noBrowser = true;
              finishCatalog(catalog, true);
            } else {
              catalog->tokens.clear();
              readTabs(catalog, 0);
            }
          },
          Qt::QueuedConnection);
}
void BrowserCatalog::readTabs(const std::shared_ptr<Catalog> &catalog,
                              int index, const QString &token) {
  if (index >= catalog->providers.size()) {
    finishCatalog(catalog, true);
    return;
  }
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    finishCatalog(catalog, false);
    return;
  }
  catalog->tokens.insert(token);
  if (token.isEmpty()) {
    catalog->windowStart = catalog->windows.size();
    catalog->tabStart = catalog->tabs.size();
    catalog->seenWindows.clear();
    catalog->seenTabs.clear();
    catalog->activeWindows.clear();
    catalog->focused = 0;
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      catalog->owner, provider.value("path").toString(), "org.arinn.Browser1",
      "GetTabs");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog, provider, index, token](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        auto reject = [this, catalog, index] {
          catalog->hadError = true;
          catalog->windows.resize(catalog->windowStart);
          catalog->tabs.resize(catalog->tabStart);
          const auto providerPath =
              catalog->providers.at(index).toMap().value("path").toString();
          for (auto it = catalog->refs.begin(); it != catalog->refs.end();) {
            if (it->path == providerPath)
              it = catalog->refs.erase(it);
            else
              ++it;
          }
          catalog->tokens.clear();
          readTabs(catalog, index + 1);
        };
        if (catalog->epoch != m_epoch ||
            reply.type() == QDBusMessage::ErrorMessage ||
            reply.arguments().isEmpty() ||
            reply.signature() != "(sta(ssb)a(ssssbb)s)") {
          reject();
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString generation, next;
        qulonglong revision = 0;
        QVariantList pageWindows, pageTabs;
        arg.beginStructure();
        arg >> generation >> revision;
        if (generation != provider.value("generation").toString() ||
            revision == 0 ||
            (!token.isEmpty() && (generation != catalog->generation ||
                                  revision != catalog->revision))) {
          reject();
          return;
        }
        catalog->generation = generation;
        catalog->revision = revision;
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, windowToken;
          bool focused = false;
          arg.beginStructure();
          arg >> id >> windowToken >> focused;
          arg.endStructure();
          if (windowToken.contains(QChar(0)) || windowToken.toUtf8().size() > 256) {
            reject(); return;
          }
          pageWindows.append(QVariantMap{{"localId", id}, {"focused", focused}, {"windowToken", windowToken}});
        }
        arg.endArray();
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, windowId, title, url;
          bool active = false, audible = false;
          arg.beginStructure();
          arg >> id >> windowId >> title >> url >> active >> audible;
          arg.endStructure();
          pageTabs.append(QVariantMap{{"localId", id},
                                      {"localWindow", windowId},
                                      {"title", title},
                                      {"url", url},
                                      {"active", active},
                                      {"audible", audible}});
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        for (const auto &entry : pageWindows) {
          const auto local = entry.toMap().value("localId").toString();
          if (local.isEmpty() || catalog->seenWindows.contains(local)) {
            reject();
            return;
          }
          catalog->seenWindows.insert(local);
          if (entry.toMap().value("focused").toBool() &&
              ++catalog->focused > 1) {
            reject();
            return;
          }
        }
        for (const auto &entry : pageTabs) {
          const auto map = entry.toMap();
          const auto local = map.value("localId").toString();
          const auto window = map.value("localWindow").toString();
          if (local.isEmpty() || catalog->seenTabs.contains(local) ||
              !catalog->seenWindows.contains(window) ||
              (map.value("active").toBool() &&
               catalog->activeWindows.contains(window))) {
            reject();
            return;
          }
          catalog->seenTabs.insert(local);
          if (map.value("active").toBool())
            catalog->activeWindows.insert(window);
        }
        const auto providerId = provider.value("id").toString();
        const auto providerPath = provider.value("path").toString();
        for (const auto &entry : pageWindows) {
          const auto map = entry.toMap();
          const auto key = presentationId(providerId, generation, "window",
                                          map.value("localId").toString());
          catalog->windows.append(QVariantMap{
              {"id", key},
              {"focused", map.value("focused").toBool()},
              {"provider", providerId},
              {"generation", generation},
              {"windowToken", map.value("windowToken")},
              {"localId", map.value("localId").toString()}});
        }
        for (const auto &entry : pageTabs) {
          const auto map = entry.toMap();
          const auto key = presentationId(providerId, generation, "tab",
                                          map.value("localId").toString());
          const auto windowKey =
              presentationId(providerId, generation, "window",
                             map.value("localWindow").toString());
          catalog->tabs.append(QVariantMap{{"id", key},
                                           {"windowId", windowKey},
                                           {"title", map.value("title")},
                                           {"url", map.value("url")},
                                           {"active", map.value("active")},
                                           {"audible", map.value("audible")}});
          catalog->refs.insert(key, TabRef{providerPath, generation,
                                           map.value("localId").toString()});
        }
        if (!next.isEmpty())
          readTabs(catalog, index, next);
        else {
          catalog->tokens.clear();
          readTabs(catalog, index + 1);
        }
      },
      Qt::QueuedConnection);
}
void BrowserCatalog::finishCatalog(const std::shared_ptr<Catalog> &catalog,
                                   bool success) {
  m_refreshing = false;
  if (catalog->epoch != m_epoch) {
    refresh();
    return;
  }
  m_completeEvidence = success && !catalog->hadError && catalog->evidenceVersion == m_evidenceVersion
      && catalog->owner == m_association.owner();
  m_snapshotOwner = catalog->owner;
  m_loaded = true;
  QVariantList windows, tabs;
  QHash<QString, TabRef> refs;
  QString failedProvider;
  if (!success) {
    m_snapshotError = true;
    m_noBrowser = false;
    m_retryAction = QStringLiteral("refresh");
    m_failedProviderId.clear();
  } else if (catalog->noBrowser) {
    m_snapshotError = false;
    m_noBrowser = true;
    failedProvider = catalog->failedProviders.isEmpty()
                         ? QString()
                         : catalog->failedProviders.first();
    m_failedProviderId = failedProvider;
    m_retryAction = failedProvider.isEmpty() ? QStringLiteral("refresh")
                                             : QStringLiteral("restart");
  } else {
    m_noBrowser = false;
    m_snapshotError = catalog->hadError && catalog->tabs.isEmpty();
    m_failedProviderId.clear();
    m_retryAction = QStringLiteral("refresh");
    bool focused = false;
    for (const auto &entry : catalog->windows) {
      const auto map = entry.toMap();
      const auto mark = map.value("focused").toBool() && !focused;
      if (mark)
        focused = true;
      auto row = map;
      row.insert("focused", mark);
      windows.append(row);
    }
    tabs = catalog->tabs;
    refs = catalog->refs;
  }
  if (m_windows != windows) {
    m_windows = windows;
    emit windowsChanged();
  }
  m_refs = refs;
  rememberFocus(windows);
  presentTabs(tabs);
  publish();
  if (m_dirty)
    QTimer::singleShot(0, this, &BrowserCatalog::refresh);
}
void BrowserCatalog::readShortcuts(quint64 epoch) {
  // A List issued before Add returns must not wipe a save that landed while
  // that call was in flight. Tokens inserted after this snapshot stay.
  const auto pendingAtCall = m_pendingSaved;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(
          daemonCall("org.arinn.WebShortcuts1", "List"), 3000),
      this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, pendingAtCall](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch || !m_connected)
              return;
            QVariantList shortcuts;
            QList<ShortcutOrigin> origins;
            QSet<QString> confirmed;
            const auto listed = reply.type() != QDBusMessage::ErrorMessage &&
                                !reply.arguments().isEmpty() &&
                                reply.signature() == "(ta(ssss))";
            if (listed) {
              const auto arg =
                  qvariant_cast<QDBusArgument>(reply.arguments().at(0));
              qulonglong revision = 0;
              arg.beginStructure();
              arg >> revision;
              arg.beginArray();
              QSet<QString> seen;
              while (!arg.atEnd()) {
                QString id, title, url, icon;
                arg.beginStructure();
                arg >> id >> title >> url >> icon;
                arg.endStructure();
                if (id.isEmpty() || seen.contains(id)) {
                  url.clear();
                  continue;
                }
                seen.insert(id);
                const auto origin = originOf(url);
                url.clear();
                const auto token = originToken(origin);
                if (!token.isEmpty())
                  confirmed.insert(token);
                origins.append(ShortcutOrigin{id, icon, origin.scheme,
                                              origin.host, origin.port,
                                              origin.ok});
                shortcuts.append(QVariantMap{{"id", id}, {"title", title}});
              }
              arg.endArray();
              arg.endStructure();
            }
            const auto previousTabs = m_tabs;
            const auto previousIds = m_openShortcutIds;
            const auto previousService = m_currentService;
            m_origins = origins;
            QSet<QString> kept;
            for (const auto &token : m_pendingSaved) {
              if (!pendingAtCall.contains(token))
                kept.insert(token);
            }
            m_pendingSaved = listed ? confirmed : QSet<QString>{};
            m_pendingSaved += kept;
            const auto listChanged = m_shortcuts != shortcuts;
            if (listChanged) {
              m_shortcuts = shortcuts;
              emit shortcutsChanged();
            }
            presentTabs(m_tabs);
            if (listChanged || m_tabs != previousTabs ||
                m_openShortcutIds != previousIds ||
                m_currentService != previousService)
              emit changed();
          },
          Qt::QueuedConnection);
}
void BrowserCatalog::markSaved(const QString &url) {
  const auto token = originToken(originOf(url));
  if (token.isEmpty())
    return;
  m_pendingSaved.insert(token);
  presentTabs(m_tabs);
}
void BrowserCatalog::setSaveError(const QString &code) {
  const auto different = m_saveError != code;
  m_saveError = code;
  if (code.isEmpty())
    m_saveErrorTimer.stop();
  else
    m_saveErrorTimer.start();
  if (different)
    emit changed();
}
void BrowserCatalog::dismissSaveError() { setSaveError(QString()); }
void BrowserCatalog::addShortcut(const QString &id) {
  if (!m_connected || m_noBrowser) {
    setSaveError(QStringLiteral("Unavailable"));
    return;
  }
  const auto pendingId = QStringLiteral("save:") + id;
  if (m_pending.contains(pendingId))
    return;
  QVariantMap tab;
  for (const auto &entry : m_tabs) {
    const auto map = entry.toMap();
    if (map.value(QStringLiteral("id")).toString() == id) {
      tab = map;
      break;
    }
  }
  if (tab.isEmpty()) {
    setSaveError(QStringLiteral("NotFound"));
    return;
  }
  if (tab.value(QStringLiteral("saved")).toBool())
    return;
  auto title = tab.value(QStringLiteral("title")).toString().trimmed();
  if (title.isEmpty())
    title = tab.value(QStringLiteral("domain")).toString();
  const auto url = tab.value(QStringLiteral("url")).toString();
  auto msg = daemonCall("org.arinn.WebShortcuts1", "Add");
  msg << title << url << QString()
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  const auto epoch = m_epoch;
  m_pending.insert(pendingId);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, pendingId, url, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            m_pending.remove(pendingId);
            if (epoch != m_epoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("s") ||
                reply.arguments().isEmpty() ||
                reply.arguments().at(0).toString().isEmpty()) {
              setSaveError(reply.type() == QDBusMessage::ErrorMessage
                               ? errorCode(reply)
                               : QStringLiteral("Unavailable"));
              return;
            }
            markSaved(url);
            setSaveError(QString());
          },
          Qt::QueuedConnection);
}
void BrowserCatalog::activateTab(const QString &id) {
  commandTab(id, "ActivateTab");
}
void BrowserCatalog::closeTab(const QString &id) { commandTab(id, "CloseTab"); }
void BrowserCatalog::commandTab(const QString &id, const char *method) {
  if (m_pending.contains(id) || !m_refs.contains(id)) {
    if (!m_refs.contains(id))
      setOperation("NotFound");
    return;
  }
  const auto ref = m_refs.value(id);
  auto msg = QDBusMessage::createMethodCall(service, ref.path,
                                            "org.arinn.Browser1", method);
  msg << ref.generation << ref.localId
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  const auto epoch = m_epoch;
  m_pending.insert(id);
  setOperation("running");
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, id, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "o" || reply.arguments().isEmpty()) {
              m_pending.remove(id);
              setOperation(reply.type() == QDBusMessage::ErrorMessage
                               ? errorCode(reply)
                               : QString("Unavailable"));
              return;
            }
            watchOperation(
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path(),
                id, epoch);
          },
          Qt::QueuedConnection);
}
void BrowserCatalog::openShortcut(const QString &id) {
  const auto pendingId = QString("shortcut:%1").arg(id);
  if (m_pending.contains(pendingId))
    return;
  QString title;
  bool known = false;
  for (const auto &entry : m_shortcuts) {
    const auto map = entry.toMap();
    if (map.value("id").toString() == id) {
      known = true;
      title = map.value("title").toString();
      break;
    }
  }
  if (!known) {
    setOperation("NotFound");
    m_opening = false;
    m_openingError = QStringLiteral("NotFound");
    emit changed();
    return;
  }
  m_openingId = id;
  m_openingTitle = title;
  m_openingError.clear();
  m_openCancelRequested = false;
  m_openOperationPath.clear();
  m_opening = true;
  emit changed();
  auto msg = daemonCall("org.arinn.WebShortcuts1", "Open");
  msg << id << QUuid::createUuid().toString(QUuid::WithoutBraces);
  const auto epoch = m_epoch;
  m_pending.insert(pendingId);
  setOperation("running");
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, pendingId, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "o" || reply.arguments().isEmpty()) {
              m_pending.remove(pendingId);
              const auto failed = reply.type() == QDBusMessage::ErrorMessage
                                      ? errorCode(reply)
                                      : QString("Unavailable");
              setOperation(failed);
              if (pendingId.startsWith(QLatin1String("shortcut:"))) {
                m_opening = false;
                m_openingError =
                    m_openCancelRequested ? QString() : failed;
                m_openCancelRequested = false;
                emit changed();
              }
              return;
            }
            const auto objectPath =
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path();
            if (pendingId.startsWith(QLatin1String("shortcut:"))) {
              m_openOperationPath = objectPath;
              if (m_openCancelRequested)
                cancelOpen();
            }
            watchOperation(objectPath, pendingId, epoch);
          },
          Qt::QueuedConnection);
}
void BrowserCatalog::watchOperation(const QString &objectPath,
                                    const QString &pendingId, quint64 epoch) {
  auto timer = new QTimer(this);
  timer->setInterval(100);
  connect(timer, &QTimer::timeout, this, [this, timer, objectPath, pendingId, epoch] {
    timer->stop();
    if (epoch != m_epoch) {
      m_pending.remove(pendingId);
      timer->deleteLater();
      return;
    }
    auto msg = QDBusMessage::createMethodCall(
        service, objectPath, "org.arinn.Operation1", "Get");
    auto pending = new QDBusPendingCallWatcher(
        QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this,
            [this, timer, pendingId, epoch](auto *call) {
              const auto reply = call->reply();
              call->deleteLater();
              if (epoch != m_epoch) {
                timer->deleteLater();
                return;
              }
              if (reply.type() == QDBusMessage::ErrorMessage ||
                  reply.signature() != "(ssbtta(sss)b)" ||
                  reply.arguments().isEmpty()) {
                m_pending.remove(pendingId);
                setOperation("Unavailable");
                if (pendingId.startsWith(QLatin1String("shortcut:"))) {
                  m_opening = false;
                  m_openingError = m_openCancelRequested
                                       ? QString()
                                       : QStringLiteral("Unavailable");
                  m_openCancelRequested = false;
                  m_openOperationPath.clear();
                  emit changed();
                }
                timer->deleteLater();
                return;
              }
              const QDBusArgument arg =
                  qvariant_cast<QDBusArgument>(reply.arguments().at(0));
              QString state, error;
              bool retryable = false, cancellable = false;
              qulonglong started = 0, ended = 0;
              arg.beginStructure();
              arg >> state >> error >> retryable >> started >> ended;
              arg.beginArray();
              while (!arg.atEnd()) {
                QString provider, capability, entity;
                arg.beginStructure();
                arg >> provider >> capability >> entity;
                arg.endStructure();
              }
              arg.endArray();
              arg >> cancellable;
              arg.endStructure();
              if (state == "running" || state == "queued") {
                timer->start();
                return;
              }
              m_pending.remove(pendingId);
              const auto failed =
                  state == "succeeded"
                      ? QString()
                      : (error.isEmpty() ? QString("failed") : error);
              setOperation(failed.isEmpty() ? QString("succeeded") : failed);
              if (pendingId.startsWith(QLatin1String("shortcut:"))) {
                m_openOperationPath.clear();
                m_opening = false;
                if (m_openCancelRequested || failed.isEmpty())
                  m_openingError.clear();
                else
                  m_openingError = failed;
                m_openCancelRequested = false;
                emit changed();
              }
              timer->deleteLater();
            },
            Qt::QueuedConnection);
  });
  timer->start();
}
void BrowserCatalog::clearUseOrder() {
  m_lastUse.clear();
  m_focusedWindow.clear();
  m_useTick = 0;
}
void BrowserCatalog::rememberFocus(const QVariantList &windows) {
  QString focused;
  for (const auto &entry : windows) {
    const auto map = entry.toMap();
    if (map.value(QStringLiteral("focused")).toBool()) {
      focused = map.value(QStringLiteral("id")).toString();
      break;
    }
  }
  if (focused.isEmpty()) {
    m_focusedWindow.clear();
    return;
  }
  if (focused != m_focusedWindow) {
    m_lastUse.insert(focused, ++m_useTick);
    m_focusedWindow = focused;
  }
}
void BrowserCatalog::presentTabs(QVariantList tabs) {
  QString onScreen;
  bool matched = false;
  if (m_home && m_association.valid() && m_completeEvidence &&
      m_snapshotOwner == m_association.owner() && m_daemonEpoch == m_association.epoch() &&
      m_returnTarget == m_association.target()) {
    int matches = 0;
    for (const auto &entry : m_windows) {
      const auto map = entry.toMap();
      if (map.value("windowToken").toString() == m_association.token()) {
        ++matches;
        onScreen = map.value("id").toString();
      }
    }
    if (matches != 1) onScreen.clear();
    int active = 0;
    for (const auto &entry : tabs) {
      const auto map = entry.toMap();
      if (!onScreen.isEmpty() && map.value("windowId").toString() == onScreen && map.value("active").toBool()) ++active;
    }
    matched = active == 1;
    if (!matched) onScreen.clear();
  } else if (!m_home && m_association.homeKnown() && !m_association.homeVisible()) {
    for (const auto &entry : m_windows) {
      const auto map = entry.toMap();
      if (map.value("focused").toBool()) { onScreen = map.value("id").toString(); break; }
    }
  }
  QStringList openIds;
  QVariantMap current;
  for (auto &entry : tabs) {
    auto map = entry.toMap();
    const auto url = map.value(QStringLiteral("url")).toString();
    const auto origin = originOf(url);
    QString icon;
    QString shortcutId;
    if (origin.ok) {
      for (const auto &shortcut : m_origins) {
        if (!shortcut.ok || shortcut.scheme != origin.scheme ||
            shortcut.host != origin.host || shortcut.port != origin.port)
          continue;
        shortcutId = shortcut.id;
        if (!shortcut.icon.isEmpty())
          icon = shortcut.icon;
        break;
      }
    }
    map.insert(QStringLiteral("letter"), letterOf(url));
    map.insert(QStringLiteral("domain"), displayHost(url));
    if (icon.isEmpty())
      map.remove(QStringLiteral("icon"));
    else
      map.insert(QStringLiteral("icon"), icon);
    map.insert(QStringLiteral("saved"),
               !shortcutId.isEmpty() ||
                   m_pendingSaved.contains(originToken(origin)));
    const auto active = map.value(QStringLiteral("active")).toBool();
    const auto windowId = map.value(QStringLiteral("windowId")).toString();
    if (active && !shortcutId.isEmpty() && !openIds.contains(shortcutId))
      openIds.append(shortcutId);
    if (current.isEmpty() && windowId == onScreen && active) {
      const auto domain = map.value(QStringLiteral("domain")).toString();
      const auto title = map.value(QStringLiteral("title")).toString();
      current = QVariantMap{
          {QStringLiteral("id"), map.value(QStringLiteral("id"))},
          {QStringLiteral("title"), title.isEmpty() ? domain : title},
          {QStringLiteral("subtitle"),
           domain.isEmpty() ? tr("Return")
                            : tr("%1 · Return").arg(domain)}};
    }
    entry = map;
  }
  std::stable_sort(tabs.begin(), tabs.end(),
                   [&](const QVariant &left, const QVariant &right) {
                     const auto a = left.toMap();
                     const auto b = right.toMap();
                     const auto aw = a.value(QStringLiteral("windowId")).toString();
                     const auto bw = b.value(QStringLiteral("windowId")).toString();
                     const auto aOn = !onScreen.isEmpty() && aw == onScreen;
                     const auto bOn = !onScreen.isEmpty() && bw == onScreen;
                     if (aOn != bOn)
                       return aOn;
                     const auto used =
                         m_lastUse.value(aw, 0) - m_lastUse.value(bw, 0);
                     if (used != 0)
                       return used > 0;
                     const auto aActive = a.value(QStringLiteral("active")).toBool();
                     const auto bActive = b.value(QStringLiteral("active")).toBool();
                     if (aw == bw && aActive != bActive)
                       return aActive;
                     return false;
                   });
  if (m_tabs != tabs) {
    m_tabs = tabs;
    emit tabsChanged();
  }
  if (m_openShortcutIds != openIds) {
    m_openShortcutIds = openIds;
    emit openShortcutIdsChanged();
  }
  if (current.value("title").toString().isEmpty()) matched = false;
  if (m_currentService != current || m_returnMatched != matched) {
    m_returnMatched = matched;
    m_currentService = current;
    emit currentServiceChanged();
  }
  updatePresentation();
}
// A daemon or association signal invalidates the evidence before it is read
// again, so returnMatched drops for the length of a read even when nothing
// changed. Home keeps showing the last match for at most the hold interval
// while the evidence is only pending. Anything that contradicts it (Home
// hidden, another Return target or daemon epoch, a completed negative read)
// clears it at once. The Return action always goes through daemon Back, so
// only the label is held (#227, ADR-0073).
void BrowserCatalog::updatePresentation(bool expired) {
  if (m_returnMatched && !m_currentService.isEmpty()) {
    m_holdTimer.stop();
    m_presentedTarget = m_association.target();
    m_presentedEpoch = m_association.epoch();
    setPresentation(true, m_currentService);
    return;
  }
  if (!m_presented)
    return;
  const bool contradicted =
      !m_home || (m_association.homeKnown() && !m_association.homeVisible()) ||
      m_association.target() != m_presentedTarget ||
      m_association.epoch() != m_presentedEpoch ||
      m_returnTarget != m_presentedTarget;
  const bool pending = !m_completeEvidence || m_association.pending();
  if (expired || contradicted || !pending) {
    m_holdTimer.stop();
    setPresentation(false, {});
    return;
  }
  if (!m_holdTimer.isActive())
    m_holdTimer.start();
}
void BrowserCatalog::setPresentation(bool presented, const QVariantMap &service) {
  if (m_presented == presented && m_presentedService == service)
    return;
  m_presented = presented;
  m_presentedService = service;
  emit presentationChanged();
}
void BrowserCatalog::retry() {
  if (m_retryAction == QLatin1String("restart") &&
      !m_failedProviderId.isEmpty()) {
    if (m_pending.contains(QStringLiteral("restart")))
      return;
    auto msg = daemonCall(service, "Restart");
    msg << m_failedProviderId
        << QUuid::createUuid().toString(QUuid::WithoutBraces);
    const auto epoch = m_epoch;
    m_pending.insert(QStringLiteral("restart"));
    setOperation(QStringLiteral("running"));
    auto pending = new QDBusPendingCallWatcher(
        QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this,
            [this, epoch](auto *call) {
              const auto reply = call->reply();
              call->deleteLater();
              m_pending.remove(QStringLiteral("restart"));
              if (epoch != m_epoch)
                return;
              if (reply.type() == QDBusMessage::ErrorMessage ||
                  reply.signature() != "o" || reply.arguments().isEmpty()) {
                setOperation(reply.type() == QDBusMessage::ErrorMessage
                                 ? errorCode(reply)
                                 : QStringLiteral("Unavailable"));
                return;
              }
              watchOperation(qvariant_cast<QDBusObjectPath>(reply.arguments().at(0))
                                 .path(),
                             QStringLiteral("restart"), epoch);
            },
            Qt::QueuedConnection);
    return;
  }
  refresh();
}
void BrowserCatalog::cancelOpen() {
  m_openCancelRequested = true;
  m_opening = false;
  m_openingError.clear();
  const auto path = m_openOperationPath;
  emit changed();
  if (path.isEmpty())
    return;
  auto msg = QDBusMessage::createMethodCall(service, path, "org.arinn.Operation1",
                                            "Cancel");
  QDBusConnection::sessionBus().asyncCall(msg, 3000);
}
void BrowserCatalog::dismissOpenFailure() {
  if (m_openingError.isEmpty())
    return;
  m_openingError.clear();
  emit changed();
}
void BrowserCatalog::retryOpen() {
  const auto id = m_openingId;
  dismissOpenFailure();
  if (!id.isEmpty())
    openShortcut(id);
}
