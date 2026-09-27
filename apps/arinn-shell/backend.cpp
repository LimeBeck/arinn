#include "backend.h"
#include <QUuid>
#include <QtDBus>
namespace {
constexpr auto service = "org.arinn.Spike1";
constexpr auto path = "/org/arinn/Spike1";
QDBusMessage request(const char *method) {
  return QDBusMessage::createMethodCall(service, path, service, method);
}
} // namespace
Backend::Backend(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "Changed", this, SLOT(refresh()));
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
        m_refreshing = false;
        if (epoch != m_connectionEpoch) {
          refresh();
          return;
        }
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.arguments().isEmpty() ||
            reply.signature() != "(sbstbsssbbss)") {
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
            QDBusConnection::sessionBus().asyncCall(request("Snapshot"), 3000),
            this);
        connect(snapshot, &QDBusPendingCallWatcher::finished, this,
                [this, epoch, generation](auto *call) {
                  auto reply = call->reply();
                  call->deleteLater();
                  if (epoch != m_connectionEpoch ||
                      generation != m_generation ||
                      reply.type() == QDBusMessage::ErrorMessage ||
                      reply.arguments().isEmpty() ||
                      reply.signature() != "(stba(ss))")
                    return;
                  const QDBusArgument arg =
                      qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                  QString gen;
                  qulonglong rev;
                  bool available;
                  QVariantList items;
                  arg.beginStructure();
                  arg >> gen >> rev >> available;
                  arg.beginArray();
                  while (!arg.atEnd()) {
                    QString id, title;
                    arg.beginStructure();
                    arg >> id >> title;
                    arg.endStructure();
                    items.append(QVariantMap{{"id", id}, {"title", title}});
                  }
                  arg.endArray();
                  arg.endStructure();
                  if (gen != m_generation || rev < m_snapshotRevision)
                    return;
                  m_snapshotRevision = rev;
                  m_items = items;
                  m_available = available;
                  emit changed();
                });
        if (m_dirty)
          refresh();
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
  auto msg = request("Activate");
  msg << m_generation << id
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
                  service, op, "org.freedesktop.DBus.Properties", "GetAll");
              msg << "org.arinn.SpikeOperation1";
              auto p = new QDBusPendingCallWatcher(
                  QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
              connect(p, &QDBusPendingCallWatcher::finished, this,
                      [this, timer](auto *call) {
                        QDBusPendingReply<QVariantMap> reply = *call;
                        call->deleteLater();
                        if (reply.isError()) {
                          m_message = "Операция недоступна";
                          timer->deleteLater();
                          emit changed();
                          return;
                        }
                        const auto values = reply.value();
                        const auto state = values.value("State").toString();
                        if (state == "running" || state == "queued") {
                          timer->start();
                          return;
                        }
                        m_message = state == "succeeded"
                                        ? "Готово"
                                        : values.value("Error").toString();
                        timer->deleteLater();
                        emit changed();
                      });
            });
            timer->start();
          });
}
