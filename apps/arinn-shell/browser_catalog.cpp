// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "browser_catalog.h"
#include <QJsonArray>
#include <QJsonDocument>
#include <QUuid>
#include <QtDBus>
#include <QtQml>
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
} // namespace
struct BrowserCatalog::Catalog {
  quint64 epoch = 0;
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
};
BrowserCatalog::BrowserCatalog(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "Changed", this, SLOT(refresh()));
  bus.connect(service, path, service, "ProvidersChanged", this, SLOT(refresh()));
  bus.connect(service, path, service, "StateChanged", this, SLOT(refresh()));
  bus.connect(service, path, "org.arinn.WebShortcuts1", "Changed", this,
              SLOT(refresh()));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &) {
            ++m_epoch;
            m_pending.clear();
            m_connected = false;
            m_loaded = false;
            m_noBrowser = false;
            m_snapshotError = false;
            m_generation.clear();
            m_tabs.clear();
            m_windows.clear();
            m_refs.clear();
            m_shortcuts.clear();
            emit windowsChanged();
            emit tabsChanged();
            emit shortcutsChanged();
            publish();
            refresh();
          });
  connect(&m_timer, &QTimer::timeout, this, &BrowserCatalog::refresh);
  m_timer.start(1000);
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
void BrowserCatalog::refresh() {
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  const auto epoch = m_epoch;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(daemonCall(service, "Status"),
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
              if (!m_tabs.isEmpty() || !m_windows.isEmpty()) {
                m_tabs.clear();
                m_windows.clear();
                m_refs.clear();
                emit windowsChanged();
                emit tabsChanged();
              }
            }
            m_generation = generation;
            m_connected = true;
            publish();
            readShortcuts(epoch);
            auto catalog = std::make_shared<Catalog>();
            catalog->epoch = epoch;
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
  auto msg = daemonCall(service, "Providers");
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
              if (runtime == "ready" && backend != "unavailable" &&
                  capabilities.contains("browser"))
                catalog->providers.append(
                    QVariantMap{{"id", id},
                                {"path", endpoint.path()},
                                {"generation", generation}});
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
      service, provider.value("path").toString(), "org.arinn.Browser1",
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
          (void)windowToken;
          pageWindows.append(QVariantMap{{"localId", id}, {"focused", focused}});
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
  m_loaded = true;
  QVariantList windows, tabs;
  QHash<QString, TabRef> refs;
  if (!success) {
    m_snapshotError = true;
    m_noBrowser = false;
  } else if (catalog->noBrowser) {
    m_snapshotError = false;
    m_noBrowser = true;
  } else {
    m_noBrowser = false;
    m_snapshotError = catalog->hadError && catalog->tabs.isEmpty();
    bool focused = false;
    for (const auto &entry : catalog->windows) {
      const auto map = entry.toMap();
      const auto mark = map.value("focused").toBool() && !focused;
      if (mark)
        focused = true;
      windows.append(QVariantMap{{"id", map.value("id")}, {"focused", mark}});
    }
    tabs = catalog->tabs;
    refs = catalog->refs;
  }
  if (m_windows != windows) {
    m_windows = windows;
    emit windowsChanged();
  }
  if (m_tabs != tabs) {
    m_tabs = tabs;
    emit tabsChanged();
  }
  m_refs = refs;
  publish();
  if (m_dirty)
    QTimer::singleShot(0, this, &BrowserCatalog::refresh);
}
void BrowserCatalog::readShortcuts(quint64 epoch) {
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(
          daemonCall("org.arinn.WebShortcuts1", "List"), 3000),
      this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch || !m_connected)
              return;
            QVariantList shortcuts;
            if (reply.type() != QDBusMessage::ErrorMessage &&
                !reply.arguments().isEmpty() &&
                reply.signature() == "(ta(ssss))") {
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
                (void)url;
                (void)icon;
                if (id.isEmpty() || seen.contains(id))
                  continue;
                seen.insert(id);
                shortcuts.append(QVariantMap{{"id", id}, {"title", title}});
              }
              arg.endArray();
              arg.endStructure();
            }
            if (m_shortcuts != shortcuts) {
              m_shortcuts = shortcuts;
              emit shortcutsChanged();
              emit changed();
            }
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
  bool known = false;
  for (const auto &entry : m_shortcuts) {
    if (entry.toMap().value("id").toString() == id) {
      known = true;
      break;
    }
  }
  if (!known) {
    setOperation("NotFound");
    return;
  }
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
              setOperation(reply.type() == QDBusMessage::ErrorMessage
                               ? errorCode(reply)
                               : QString("Unavailable"));
              return;
            }
            watchOperation(
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path(),
                pendingId, epoch);
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
              setOperation(state == "succeeded"
                               ? QString("succeeded")
                               : (error.isEmpty() ? QString("failed") : error));
              timer->deleteLater();
            },
            Qt::QueuedConnection);
  });
  timer->start();
}
