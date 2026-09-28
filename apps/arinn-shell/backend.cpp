// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include <QJsonArray>
#include <QJsonDocument>
#include <QSet>
#include <QUuid>
#include <QtDBus>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
QDBusMessage request(const char *method) {
  return QDBusMessage::createMethodCall(service, path, service, method);
}
} // namespace
struct Backend::Catalog {
  quint64 epoch;
  QString snapshot;
  QVariantList providers, applications, running;
  QSet<QString> tokens;
  QString generation;
  quint64 revision = 0;
  int pages = 0;
  qsizetype providerStart = 0, runningStart = 0;
  bool hadError = false;
};
Backend::Backend(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "Changed", this, SLOT(refresh()));
  bus.connect(service, path, service, "ProvidersChanged", this,
              SLOT(refresh()));
  bus.connect(service, path, service, "StateChanged", this, SLOT(refresh()));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &) {
            ++m_connectionEpoch;
            m_pendingIds.clear();
            m_connected = false;
            m_available = false;
            m_generation.clear();
            m_snapshotRevision = 0;
            m_items.clear();
            m_applications.clear();
            m_return.clear();
            m_loaded = false;
            m_snapshotError = false;
            publishState();
            refresh();
          });
  connect(&m_timer, &QTimer::timeout, this, &Backend::refresh);
  m_timer.start(1000);
  refresh();
}
void Backend::publishState() {
  if (m_publishedItems != m_items) {
    m_publishedItems = m_items;
    emit itemsChanged();
  }
  const auto entries = continueEntries();
  if (m_publishedContinue != entries) {
    m_publishedContinue = entries;
    emit continueEntriesChanged();
  }
  if (m_publishedApplications != m_applications) {
    m_publishedApplications = m_applications;
    emit applicationsChanged();
  }
  emit changed();
}
QVariantList Backend::continueEntries() const {
  if (!m_connected || m_return.isEmpty())
    return {};
  return {QVariantMap{{"id", m_return},
                      {"title", tr("Return to activity")},
                      {"subtitle", tr("Continue where you left off")},
                      {"running", true}}};
}
QString Backend::homeState() const {
  if (!m_connected)
    return "unavailable";
  if (!m_return.isEmpty() || !m_applications.isEmpty())
    return "ready";
  if (m_snapshotError)
    return "error";
  return m_loaded ? "empty" : "loading";
}
void Backend::refresh() {
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  const auto epoch = m_connectionEpoch;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(request("Status"), 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_connectionEpoch) {
              m_refreshing = false;
              refresh();
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.arguments().isEmpty() ||
                reply.signature() != "(sbstbsssbbss)") {
              m_refreshing = false;
              m_connected = false;
              m_available = false;
              m_items.clear();
              m_applications.clear();
              m_return.clear();
              m_loaded = false;
              m_message = tr("Daemon is unavailable · reconnecting");
              publishState();
              return;
            }
            const QDBusArgument arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            const auto previousGeneration = m_generation;
            QString daemonEpoch, active, klass, homeOutput, activeOutput;
            qulonglong revision;
            bool full, platform;
            arg.beginStructure();
            arg >> daemonEpoch >> m_available >> m_generation >> revision >>
                m_home >> m_return >> active >> klass >> full >> platform >>
                homeOutput >> activeOutput;
            arg.endStructure();
            if (previousGeneration != m_generation) {
              m_items.clear();
              m_applications.clear();
              m_snapshotRevision = 0;
              m_loaded = false;
            }
            emit screenRequested(homeOutput);
            m_connected = true;
            if (!platform)
              m_message = tr("KWin adapter is not connected");
            else if (!m_available)
              m_message = tr("Provider is unavailable");
            else if (m_message.startsWith("Daemon") ||
                     m_message.startsWith("Provider") ||
                     m_message.startsWith("KWin"))
              m_message.clear();
            publishState();
            auto catalog = std::make_shared<Catalog>();
            catalog->epoch = epoch;
            readProviders(catalog);
          });
}
void Backend::readProviders(const std::shared_ptr<Catalog> &catalog,
                            const QString &token) {
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    finishCatalog(catalog, false);
    return;
  }
  catalog->tokens.insert(token);
  auto msg = request("Providers");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (catalog->epoch != m_connectionEpoch ||
            reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != "(sa(sssssoasssststttt)s)") {
          finishCatalog(catalog, false);
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
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
              capabilities.contains("application"))
            catalog->providers.append(QVariantMap{{"id", id},
                                                  {"path", endpoint.path()},
                                                  {"generation", generation}});
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty())
          readProviders(catalog, next);
        else {
          catalog->tokens.clear();
          readApplications(catalog);
        }
      });
}
void Backend::readApplications(const std::shared_ptr<Catalog> &catalog,
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
    catalog->providerStart = catalog->applications.size();
    catalog->runningStart = catalog->running.size();
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value("path").toString(), "org.arinn.Application1",
      "GetApplications");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog, provider, index, token](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (catalog->epoch != m_connectionEpoch) {
          finishCatalog(catalog, false);
          return;
        }
        auto unavailable = [this, catalog, index] {
          catalog->hadError = true;
          catalog->applications.resize(catalog->providerStart);
          catalog->running.resize(catalog->runningStart);
          catalog->tokens.clear();
          readApplications(catalog, index + 1);
        };
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != "(sta(ssbs)a(ssssbb)s)") {
          unavailable();
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString generation, next;
        qulonglong revision;
        arg.beginStructure();
        arg >> generation >> revision;
        if (generation != provider.value("generation").toString() ||
            (!token.isEmpty() && (generation != catalog->generation ||
                                  revision != catalog->revision))) {
          unavailable();
          return;
        }
        catalog->generation = generation;
        catalog->revision = revision;
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, title, group;
          bool launchable;
          arg.beginStructure();
          arg >> id >> title >> launchable >> group;
          arg.endStructure();
          // Providers choose the group; unknown groups display as "other".
          static const QStringList groups{"online", "games", "media", "other"};
          if (!groups.contains(group))
            group = "other";
          const auto key =
              QUuid::createUuidV5(
                  QUuid(),
                  QJsonDocument(QJsonArray{provider.value("id").toString(),
                                           generation, "launch", id})
                      .toJson(QJsonDocument::Compact))
                  .toString();
          catalog->applications.append(
              QVariantMap{{"id", key},
                          {"title", title},
                          {"enabled", launchable},
                          {"group", group},
                          {"path", provider.value("path")},
                          {"generation", generation},
                          {"localId", id}});
        }
        arg.endArray();
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, application, title, window;
          bool activate, terminate;
          arg.beginStructure();
          arg >> id >> application >> title >> window >> activate >> terminate;
          arg.endStructure();
          const auto key =
              QUuid::createUuidV5(
                  QUuid(),
                  QJsonDocument(QJsonArray{provider.value("id").toString(),
                                           generation, "activate", id})
                      .toJson(QJsonDocument::Compact))
                  .toString();
          catalog->running.append(QVariantMap{{"id", key},
                                              {"title", title},
                                              {"enabled", activate},
                                              {"running", true},
                                              {"path", provider.value("path")},
                                              {"generation", generation},
                                              {"localId", id}});
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty())
          readApplications(catalog, index, next);
        else {
          catalog->tokens.clear();
          readApplications(catalog, index + 1);
        }
      });
}
void Backend::finishCatalog(const std::shared_ptr<Catalog> &catalog,
                            bool success) {
  m_refreshing = false;
  if (catalog->epoch != m_connectionEpoch) {
    refresh();
    return;
  }
  m_snapshotError = !success || catalog->hadError;
  m_loaded = success;
  m_applications = success ? catalog->applications : QVariantList{};
  m_items = success ? catalog->running : QVariantList{};
  publishState();
  if (m_dirty)
    QTimer::singleShot(0, this, &Backend::refresh);
}
void Backend::back() {
  auto call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(request("Back"), 3000), this);
  connect(call, &QDBusPendingCallWatcher::finished, this, [this](auto *call) {
    if (call->isError()) {
      m_message = call->error().message();
      publishState();
    }
    call->deleteLater();
  });
}
void Backend::launch(const QString &id) {
  runApplication(id, m_applications, "Launch");
}
void Backend::activate(const QString &id) {
  runApplication(id, m_items, "Activate");
}
void Backend::runApplication(const QString &id, const QVariantList &entries,
                             const char *method) {
  if (m_pendingIds.contains(id))
    return;
  QVariantMap selected;
  for (const auto &item : entries) {
    if (item.toMap().value("id").toString() == id) {
      selected = item.toMap();
      break;
    }
  }
  if (selected.isEmpty() || !selected.value("enabled", true).toBool()) {
    m_message = selected.isEmpty() ? "NotFound" : "Unavailable";
    publishState();
    return;
  }
  auto msg =
      QDBusMessage::createMethodCall(service, selected.value("path").toString(),
                                     "org.arinn.Application1", method);
  msg << selected.value("generation").toString()
      << selected.value("localId").toString()
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  const auto epoch = m_connectionEpoch;
  m_pendingIds.insert(id);
  m_message = tr("Working…");
  publishState();
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, id, epoch](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (epoch != m_connectionEpoch)
          return;
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != "o" || reply.arguments().isEmpty()) {
          m_pendingIds.remove(id);
          m_message = reply.type() == QDBusMessage::ErrorMessage
                          ? reply.errorMessage()
                          : tr("Operation is unavailable");
          publishState();
          return;
        }
        auto op =
            qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path();
        auto timer = new QTimer(this);
        timer->setInterval(100);
        connect(timer, &QTimer::timeout, this, [this, timer, op, id, epoch] {
          timer->stop();
          if (epoch != m_connectionEpoch) {
            timer->deleteLater();
            return;
          }
          auto msg = QDBusMessage::createMethodCall(
              service, op, "org.arinn.Operation1", "Get");
          auto p = new QDBusPendingCallWatcher(
              QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
          connect(p, &QDBusPendingCallWatcher::finished, this,
                  [this, timer, id, epoch](auto *call) {
                    const auto reply = call->reply();
                    call->deleteLater();
                    if (epoch != m_connectionEpoch) {
                      timer->deleteLater();
                      return;
                    }
                    if (reply.type() == QDBusMessage::ErrorMessage ||
                        reply.signature() != "(ssbtta(sss)b)" ||
                        reply.arguments().isEmpty()) {
                      m_pendingIds.remove(id);
                      m_message = tr("Operation is unavailable");
                      timer->deleteLater();
                      publishState();
                      return;
                    }
                    const QDBusArgument arg =
                        qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                    QString state, error;
                    bool retryable, cancellable;
                    qulonglong started, ended;
                    arg.beginStructure();
                    arg >> state >> error >> retryable >> started >> ended;
                    arg.beginArray();
                    while (!arg.atEnd()) {
                      QString provider, capability, id;
                      arg.beginStructure();
                      arg >> provider >> capability >> id;
                      arg.endStructure();
                    }
                    arg.endArray();
                    arg >> cancellable;
                    arg.endStructure();
                    if (state == "running" || state == "queued") {
                      timer->start();
                      return;
                    }
                    m_pendingIds.remove(id);
                    m_message = state == "succeeded" ? tr("Done") : error;
                    timer->deleteLater();
                    publishState();
                  });
        });
        timer->start();
      });
}
