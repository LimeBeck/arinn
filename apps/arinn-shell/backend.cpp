// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include <QUuid>
#include <QtDBus>
#include <QJsonDocument>
#include <QJsonArray>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
QDBusMessage request(const char *method) {
  return QDBusMessage::createMethodCall(service, path, service, method);
}
} // namespace
Backend::Backend(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "Changed", this, SLOT(refresh()));
  bus.connect(service, path, service, "ProvidersChanged", this, SLOT(refresh()));
  bus.connect(service, path, service, "StateChanged", this, SLOT(refresh()));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &) {
            ++m_connectionEpoch;
            m_connected = false;
            m_available = false;
            m_generation.clear();
            m_snapshotRevision = 0;
            m_items.clear();
            emit changed();
            refresh();
          });
  connect(&m_timer, &QTimer::timeout, this, &Backend::refresh);
  m_timer.start(1000);
  refresh();
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
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
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
          m_message = "Daemon недоступен · переподключение";
          emit changed();
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
          m_snapshotRevision = 0;
        }
        emit screenRequested(homeOutput);
        m_connected = true;
        if (!platform)
          m_message = "KWin adapter не подключён";
        else if (!m_available)
          m_message = "Provider недоступен";
        else if (m_message.startsWith("Daemon") ||
                 m_message.startsWith("Provider") ||
                 m_message.startsWith("KWin"))
          m_message.clear();
        emit changed();
        const auto generation = m_generation;
        auto snapshot = new QDBusPendingCallWatcher(
            QDBusConnection::sessionBus().asyncCall(request("HomeItems"), 3000),
            this);
        connect(snapshot, &QDBusPendingCallWatcher::finished, this,
                [this, epoch, generation](auto *call) {
                  auto reply = call->reply();
                  call->deleteLater();
                  m_refreshing = false;
                  if (m_dirty || epoch != m_connectionEpoch || generation != m_generation) {
                    refresh();
                    return;
                  }
                  if (epoch != m_connectionEpoch ||
                      generation != m_generation ||
                      reply.type() == QDBusMessage::ErrorMessage ||
                      reply.arguments().isEmpty() ||
                      reply.signature() != "a(sssss)")
                    return;
                  const QDBusArgument arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                  QVariantList items;
                  arg.beginArray();
                  while (!arg.atEnd()) {
                    QString provider, providerPath, gen, localId, title;
                    arg.beginStructure();
                    arg >> provider >> providerPath >> gen >> localId >> title;
                    arg.endStructure();
                    // UI identity is opaque; route with the separate fields below.
                    const auto key = QUuid::createUuidV5(QUuid(), QJsonDocument(QJsonArray{provider, gen, localId}).toJson(QJsonDocument::Compact)).toString();
                    items.append(QVariantMap{{"id", key}, {"provider", provider}, {"path", providerPath}, {"generation", gen}, {"localId", localId}, {"title", title}});
                  }
                  arg.endArray();
                  m_items = items;
                  m_available = !items.isEmpty();
                  emit changed();
                });
      });
}
void Backend::back() {
  auto call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(request("Back"), 3000), this);
  connect(call, &QDBusPendingCallWatcher::finished, this, [this](auto *call) {
    if (call->isError()) {
      m_message = call->error().message();
      emit changed();
    }
    call->deleteLater();
  });
}
void Backend::activate(const QString &id) {
  QVariantMap selected;
  for (const auto &item : m_items) {
    if (item.toMap().value("id").toString() == id) { selected = item.toMap(); break; }
  }
  if (selected.isEmpty()) { m_message = "NotFound"; emit changed(); return; }
  auto msg = QDBusMessage::createMethodCall(service, selected.value("path").toString(), "org.arinn.Application1", "Activate");
  msg << selected.value("generation").toString() << selected.value("localId").toString()
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  m_message = "Выполняется…";
  emit changed();
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (reply.type() == QDBusMessage::ErrorMessage) {
              m_message = reply.errorMessage();
              emit changed();
              return;
            }
            auto op =
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path();
            auto timer = new QTimer(this);
            timer->setInterval(100);
            connect(timer, &QTimer::timeout, this, [this, timer, op] {
              timer->stop();
              auto msg = QDBusMessage::createMethodCall(
                  service, op, "org.arinn.Operation1", "Get");
              auto p = new QDBusPendingCallWatcher(
                  QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
              connect(p, &QDBusPendingCallWatcher::finished, this,
                      [this, timer](auto *call) {
                        const auto reply = call->reply();
                        call->deleteLater();
                        if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != "(ssbtta(sss)b)" || reply.arguments().isEmpty()) {
                          m_message = "Операция недоступна";
                          timer->deleteLater();
                          emit changed();
                          return;
                        }
                        const QDBusArgument arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                        QString state, error;
                        bool retryable, cancellable;
                        qulonglong started, ended;
                        arg.beginStructure();
                        arg >> state >> error >> retryable >> started >> ended;
                        arg.beginArray();
                        while (!arg.atEnd()) { QString provider, capability, id; arg.beginStructure(); arg >> provider >> capability >> id; arg.endStructure(); }
                        arg.endArray(); arg >> cancellable; arg.endStructure();
                        if (state == "running" || state == "queued") {
                          timer->start();
                          return;
                        }
                        m_message = state == "succeeded"
                                        ? "Готово"
                                        : error;
                        timer->deleteLater();
                        emit changed();
                      });
            });
            timer->start();
          });
}
