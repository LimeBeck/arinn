// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "system_status.h"
#include <QCoreApplication>
#include <QQmlEngine>
#include <QUuid>
#include <QtDBus>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString interface = QStringLiteral("org.arinn.SystemStatus1");
constexpr int maxLevel = 4;
} // namespace

void registerSystemStatus() {
  qmlRegisterType<SystemStatus>("Arinn.Shell", 1, 0, "SystemStatus");
}
Q_COREAPP_STARTUP_FUNCTION(registerSystemStatus)

SystemStatus::SystemStatus(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  // The Changed match is installed before the first Get, so no change
  // between them is missed.
  bus.connect(service, path, interface, QStringLiteral("Changed"), this,
              SLOT(statusChanged(QString, qulonglong)));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &next) {
            ++m_lifetime;
            m_epoch.clear();
            m_revision = 0;
            apply(false, false, 0);
            if (!next.isEmpty())
              refresh();
          });
  refresh();
}

void SystemStatus::statusChanged(const QString &epoch, qulonglong revision) {
  if (!m_epoch.isEmpty() && epoch == m_epoch && revision <= m_revision)
    return;
  refresh();
}

void SystemStatus::refresh() {
  const auto call = QDBusMessage::createMethodCall(service, path, interface,
                                                   QStringLiteral("Get"));
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, 3000), this);
  const quint64 lifetime = m_lifetime;
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, lifetime](QDBusPendingCallWatcher *watcher) {
            const QDBusMessage reply = watcher->reply();
            watcher->deleteLater();
            if (lifetime != m_lifetime)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("(stsssy)") ||
                reply.arguments().isEmpty()) {
              apply(false, false, 0);
              return;
            }
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString epoch, audio, network, link;
            qulonglong revision = 0;
            uchar level = 0;
            arg.beginStructure();
            arg >> epoch >> revision >> audio >> network >> link >> level;
            arg.endStructure();
            const QUuid uuid(epoch);
            if (uuid.isNull() || revision == 0) {
              apply(false, false, 0);
              return;
            }
            if (epoch != m_epoch) {
              m_epoch = epoch;
              m_revision = 0;
            } else if (revision < m_revision) {
              return; // Older than one already applied.
            }
            m_revision = revision;
            // Reader rules: anything unknown or inconsistent shows nothing,
            // field by field.
            const bool wifi = network == QLatin1String("connected") &&
                              link == QLatin1String("wifi");
            apply(audio == QLatin1String("muted"), wifi,
                  wifi && level <= maxLevel ? level : 0);
          });
}

void SystemStatus::apply(bool muted, bool wifi, int level) {
  if (muted == m_muted && wifi == m_wifi && level == m_level)
    return;
  m_muted = muted;
  m_wifi = wifi;
  m_level = level;
  emit changed();
}
