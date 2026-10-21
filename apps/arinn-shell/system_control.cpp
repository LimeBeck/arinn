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
// The daemon bounds a power request at 10 s and lists inhibitors around it.
constexpr int powerTimeout = 15000;
constexpr int readTimeout = 5000;
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

void SystemControl::readVolume() {
  volumeCall(QStringLiteral("GetVolume"), {}, false);
}
void SystemControl::stepVolume(int steps) {
  volumeCall(QStringLiteral("StepVolume"), {steps}, true);
}
void SystemControl::toggleMuted() {
  volumeCall(QStringLiteral("ToggleMuted"), {}, true);
}
void SystemControl::setMuted(bool muted) {
  volumeCall(QStringLiteral("SetMuted"), {muted}, true);
}

void SystemControl::volumeCall(const QString &member,
                               const QVariantList &arguments, bool show) {
  auto call = QDBusMessage::createMethodCall(service, path, interface, member);
  call.setArguments(arguments);
  const quint64 request = ++m_volumeRequest;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, readTimeout), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, request, show](QDBusPendingCallWatcher *watcher) {
            const QDBusMessage reply = watcher->reply();
            watcher->deleteLater();
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("(su)") ||
                reply.arguments().isEmpty()) {
              if (show)
                emit volumeFailed(
                    reply.type() == QDBusMessage::ErrorMessage
                        ? refusalFor(reply.errorName())
                        : QStringLiteral("failed"));
              return;
            }
            if (request < m_volumeApplied)
              return;
            m_volumeApplied = request;
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
            if (show && known)
              emit volumeShown();
            else if (show)
              emit volumeFailed(QStringLiteral("unavailable"));
          });
}
