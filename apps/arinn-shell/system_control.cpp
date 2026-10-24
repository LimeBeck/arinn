// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "system_control.h"
#include <QCoreApplication>
#include <QQmlEngine>
#include <QtDBus>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString interface = QStringLiteral("org.arinn.SystemControl1");
// Client bounds cover the whole daemon operation, so the shell never gives
// up on a change the daemon still makes. A power request: system bus
// connect 2 s, inhibitor list 2 s, logind 10 s, list again on refusal 2 s.
constexpr int powerTimeout = 20000;
// GetPower: connect, list and three Can* reads, 2 s each.
constexpr int readTimeout = 12000;
// GetSessionSwitch only reads the daemon's environment and one file.
constexpr int sessionSwitchTimeout = 5000;
// A volume change: up to four pactl runs of 2 s each.
constexpr int volumeTimeout = 10000;
// Reader bounds of the contract.
constexpr int maxInhibitors = 16;
constexpr int maxLevel = 150;

QString availability(const QString &value) {
  static const QStringList known = {
      QStringLiteral("available"), QStringLiteral("inhibited"),
      QStringLiteral("denied"), QStringLiteral("unsupported")};
  return known.contains(value) ? value : QStringLiteral("unknown");
}

QString method(const QString &action) {
  if (action == QLatin1String("suspend"))
    return QStringLiteral("Suspend");
  if (action == QLatin1String("reboot"))
    return QStringLiteral("Reboot");
  if (action == QLatin1String("power-off"))
    return QStringLiteral("PowerOff");
  // The dedicated session's Switch to desktop (#438, ADR-0107). The daemon
  // shares the power lock with the actions above.
  if (action == QLatin1String("switch-to-desktop"))
    return QStringLiteral("SwitchToDesktop");
  return {};
}
} // namespace

void registerSystemControl() {
  qmlRegisterType<SystemControl>("Arinn.Shell", 1, 0, "SystemControl");
}
Q_COREAPP_STARTUP_FUNCTION(registerSystemControl)

SystemControl::SystemControl(QObject *parent) : QObject(parent) {}

QString SystemControl::refusalFor(const QString &errorName) {
  if (errorName == QLatin1String("org.arinn.Error1.Conflict"))
    return QStringLiteral("inhibited");
  if (errorName == QLatin1String("org.arinn.Error1.PermissionDenied"))
    return QStringLiteral("denied");
  if (errorName == QLatin1String("org.arinn.Error1.Unsupported"))
    return QStringLiteral("unsupported");
  if (errorName == QLatin1String("org.arinn.Error1.Timeout") ||
      errorName == QLatin1String("org.freedesktop.DBus.Error.NoReply") ||
      errorName == QLatin1String("org.freedesktop.DBus.Error.Timeout"))
    return QStringLiteral("timeout");
  // No daemon, or a daemon without SystemControl1.
  if (errorName == QLatin1String("org.arinn.Error1.Unavailable") ||
      errorName == QLatin1String("org.freedesktop.DBus.Error.ServiceUnknown") ||
      errorName == QLatin1String("org.freedesktop.DBus.Error.NameHasNoOwner") ||
      errorName == QLatin1String("org.freedesktop.DBus.Error.UnknownMethod") ||
      errorName ==
          QLatin1String("org.freedesktop.DBus.Error.UnknownInterface") ||
      errorName == QLatin1String("org.freedesktop.DBus.Error.UnknownObject"))
    return QStringLiteral("unavailable");
  return QStringLiteral("failed");
}

void SystemControl::readPower() {
  const quint64 read = ++m_powerRead;
  if (m_powerState != QLatin1String("loading")) {
    m_powerState = QStringLiteral("loading");
    emit powerChanged();
  }
  const auto call = QDBusMessage::createMethodCall(service, path, interface,
                                                   QStringLiteral("GetPower"));
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, readTimeout), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, read](QDBusPendingCallWatcher *watcher) {
            const QDBusMessage reply = watcher->reply();
            watcher->deleteLater();
            if (read != m_powerRead)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("(sssa(sss))") ||
                reply.arguments().isEmpty())
              applyPower({});
            else
              applyPower(reply.arguments().at(0));
          });
  readSessionSwitch();
}

void SystemControl::readSessionSwitch() {
  const quint64 read = ++m_sessionSwitchRead;
  const auto call = QDBusMessage::createMethodCall(
      service, path, interface, QStringLiteral("GetSessionSwitch"));
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, sessionSwitchTimeout),
      this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, read](QDBusPendingCallWatcher *watcher) {
            const QDBusMessage reply = watcher->reply();
            watcher->deleteLater();
            if (read != m_sessionSwitchRead)
              return;
            // No daemon, a daemon without the method, or a wrong reply:
            // unknown, and the action is not offered.
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("s") ||
                reply.arguments().isEmpty())
              applySessionSwitch({});
            else
              applySessionSwitch(reply.arguments().at(0).toString());
          });
}

// Reader rule of the contract: any value but these two is unknown.
void SystemControl::applySessionSwitch(const QString &value) {
  const QString state = value == QLatin1String("available") ||
                                value == QLatin1String("unsupported")
                            ? value
                            : QStringLiteral("unknown");
  if (state == m_sessionSwitch)
    return;
  m_sessionSwitch = state;
  emit sessionSwitchChanged();
}

// A failed or unreadable reply is "unknown" for every action: the menu keeps
// the actions and lets the daemon answer each request.
void SystemControl::applyPower(const QVariant &value) {
  QString suspend = QStringLiteral("unknown");
  QString reboot = suspend;
  QString powerOff = suspend;
  QVariantList inhibitors;
  if (value.isValid()) {
    const auto arg = qvariant_cast<QDBusArgument>(value);
    QString s, r, p;
    arg.beginStructure();
    arg >> s >> r >> p;
    arg.beginArray();
    while (!arg.atEnd()) {
      QString what, who, why;
      arg.beginStructure();
      arg >> what >> who >> why;
      arg.endStructure();
      if ((what == QLatin1String("sleep") ||
           what == QLatin1String("shutdown")) &&
          inhibitors.size() < maxInhibitors)
        inhibitors.append(
            QVariantMap{{"what", what}, {"who", who}, {"why", why}});
    }
    arg.endArray();
    arg.endStructure();
    suspend = availability(s);
    reboot = availability(r);
    powerOff = availability(p);
  }
  m_powerState = QStringLiteral("ready");
  m_suspend = suspend;
  m_reboot = reboot;
  m_powerOff = powerOff;
  m_inhibitors = inhibitors;
  emit powerChanged();
}

void SystemControl::request(const QString &action) {
  const QString member = method(action);
  if (member.isEmpty() || !m_pending.isEmpty())
    return;
  m_pending = action;
  m_accepted.clear();
  m_refusal.clear();
  m_refusalAction.clear();
  emit requestChanged();
  const auto call =
      QDBusMessage::createMethodCall(service, path, interface, member);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, powerTimeout), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, action](QDBusPendingCallWatcher *watcher) {
            const QDBusMessage reply = watcher->reply();
            watcher->deleteLater();
            m_pending.clear();
            if (reply.type() == QDBusMessage::ErrorMessage) {
              m_refusal = refusalFor(reply.errorName());
              m_refusalAction = action;
            } else {
              m_accepted = action;
            }
            emit requestChanged();
            if (action == QLatin1String("switch-to-desktop")) {
              // No inhibitor holds the switch; Conflict means another power
              // request. Unsupported means the session cannot switch: read
              // it again so the menu stops offering it.
              if (m_refusal == QLatin1String("unsupported"))
                readSessionSwitch();
              return;
            }
            // An inhibitor refusal names its holder: read who holds it now.
            if (m_refusal == QLatin1String("inhibited") ||
                m_refusal == QLatin1String("denied"))
              readPower();
          });
}

void SystemControl::reset() {
  if (m_accepted.isEmpty() && m_refusal.isEmpty())
    return;
  m_accepted.clear();
  m_refusal.clear();
  m_refusalAction.clear();
  emit requestChanged();
}

// Pending volume key presses beyond this are dropped; a held key never
// builds a long backlog.
constexpr int maxQueuedVolumeOps = 8;

void SystemControl::readVolume() {
  queueVolume({{QStringLiteral("GetVolume"), {}, false}});
}
void SystemControl::stepVolume(int steps) {
  queueVolume({{QStringLiteral("StepVolume"), {steps}, true}});
}
void SystemControl::raiseVolume() {
  // Unmuting an unmuted output changes nothing, so the shell never has to
  // trust a mute state it read earlier. The step waits for the unmute.
  queueVolume({{QStringLiteral("SetMuted"), {false}, false},
               {QStringLiteral("StepVolume"), {1}, true}});
}
void SystemControl::toggleMuted() {
  queueVolume({{QStringLiteral("ToggleMuted"), {}, true}});
}
void SystemControl::setMuted(bool muted) {
  queueVolume({{QStringLiteral("SetMuted"), {muted}, true}});
}

void SystemControl::queueVolume(QList<VolumeOp> ops) {
  if (m_volumeQueue.size() + ops.size() > maxQueuedVolumeOps)
    return;
  const quint64 chain = ++m_volumeChain;
  for (auto &op : ops) {
    op.chain = chain;
    m_volumeQueue.append(op);
  }
  sendVolume();
}

void SystemControl::sendVolume() {
  if (m_volumeBusy || m_volumeQueue.isEmpty())
    return;
  m_volumeBusy = true;
  const VolumeOp op = m_volumeQueue.takeFirst();
  auto call =
      QDBusMessage::createMethodCall(service, path, interface, op.member);
  call.setArguments(op.arguments);
  // A step that waits for its unmute shows the HUD; the unmute itself does
  // not, but its failure is still reported.
  const bool chained = !m_volumeQueue.isEmpty() &&
                       m_volumeQueue.first().chain == op.chain;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, volumeTimeout), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, op, chained](QDBusPendingCallWatcher *watcher) {
            const QDBusMessage reply = watcher->reply();
            watcher->deleteLater();
            m_volumeBusy = false;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("(su)") ||
                reply.arguments().isEmpty()) {
              // The rest of this key's chain is dropped.
              m_volumeQueue.removeIf(
                  [&op](const VolumeOp &next) { return next.chain == op.chain; });
              if (op.show || chained)
                emit volumeFailed(
                    reply.type() == QDBusMessage::ErrorMessage
                        ? refusalFor(reply.errorName())
                        : QStringLiteral("failed"));
              sendVolume();
              return;
            }
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString audio;
            uint level = 0;
            arg.beginStructure();
            arg >> audio >> level;
            arg.endStructure();
            // Reader rules: an unknown state shows no level.
            const bool known = audio == QLatin1String("muted") ||
                               audio == QLatin1String("unmuted");
            m_volumeKnown = known;
            m_volumeMuted = known && audio == QLatin1String("muted");
            m_volumeLevel =
                known ? static_cast<int>(qMin<uint>(level, maxLevel)) : 0;
            emit volumeChanged();
            if (op.show && known)
              emit volumeShown();
            else if (op.show)
              emit volumeFailed(QStringLiteral("unavailable"));
            sendVolume();
          });
}
