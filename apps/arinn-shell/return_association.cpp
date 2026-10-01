// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "return_association.h"
#include <QtDBus>
#include <QUuid>
#include <QXmlStreamReader>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
constexpr auto association = "org.arinn.ReturnAssociation1";
bool text(const QString &s, int limit) {
  return !s.contains(QChar(0)) && s.toUtf8().size() <= limit;
}
bool uuid(const QString &s) { return !QUuid(s).isNull(); }
QDBusMessage request(const QString &owner, const char *interface, const char *method) {
  return QDBusMessage::createMethodCall(owner, path, interface, method);
}
}
ReturnAssociation::ReturnAssociation(QObject *parent) : QObject(parent) {
  auto watcher = new QDBusServiceWatcher(service, QDBusConnection::sessionBus(),
      QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
      [this](const QString &, const QString &, const QString &owner) { setOwner(owner); });
  connect(&m_timer, &QTimer::timeout, this, &ReturnAssociation::refresh);
  m_timer.start(1000);
  refresh();
}
void ReturnAssociation::resolveOwner() {
  if (m_resolving) return;
  m_resolving = true;
  const auto lifetime = m_lifetime;
  auto msg = QDBusMessage::createMethodCall("org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "GetNameOwner");
  msg << QString(service);
  auto pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this, [this, lifetime](auto *call) {
    const auto reply = call->reply(); call->deleteLater();
    m_resolving = false;
    if (lifetime != m_lifetime) return;
    if (reply.type() != QDBusMessage::ErrorMessage && reply.signature() == "s")
      setOwner(reply.arguments().first().toString());
  });
}
void ReturnAssociation::setOwner(const QString &owner) {
  if (owner == m_owner) return;
  auto bus = QDBusConnection::sessionBus();
  if (!m_owner.isEmpty()) {
    bus.disconnect(m_owner, path, service, "Changed", this, SLOT(invalidate()));
    bus.disconnect(m_owner, path, association, "Changed", this, SLOT(associationChanged(QString,qulonglong)));
    bus.disconnect(m_owner, path, "org.arinn.PlatformWindows1", "WindowsChanged", this, SLOT(inventoryChanged(QString,qulonglong)));
  }
  ++m_lifetime; ++m_context;
  m_owner = owner;
  m_busy = m_dirty = m_known = m_supported = m_valid = m_homeKnown = false;
  m_epoch.clear(); m_target.clear(); m_token.clear(); m_platform.clear();
  m_signalEpoch.clear(); m_inventoryGeneration.clear();
  m_revision = m_floor = 0;
  if (!owner.isEmpty()) {
    // Subscribe before discovery/initial Get; all messages are owner-pinned.
    bus.connect(owner, path, service, "Changed", this, SLOT(invalidate()));
    bus.connect(owner, path, association, "Changed", this, SLOT(associationChanged(QString,qulonglong)));
    bus.connect(owner, path, "org.arinn.PlatformWindows1", "WindowsChanged", this, SLOT(inventoryChanged(QString,qulonglong)));
  }
  emit changed(); emit ownerChanged();
  if (!owner.isEmpty()) refresh();
}
void ReturnAssociation::invalidate() {
  ++m_context;
  m_valid = m_homeKnown = false;
  emit changed();
  QTimer::singleShot(0, this, &ReturnAssociation::refresh);
}
void ReturnAssociation::associationChanged(const QString &epoch, qulonglong revision) {
  if (m_signalEpoch != epoch) { m_signalEpoch = epoch; m_floor = 0; }
  m_floor = qMax(m_floor, revision);
  invalidate();
}
void ReturnAssociation::inventoryChanged(const QString &generation, qulonglong) {
  m_inventoryGeneration = generation;
  invalidate();
}
void ReturnAssociation::done(bool again) {
  m_busy = false;
  if (m_dirty || again) { m_dirty = false; QTimer::singleShot(0, this, &ReturnAssociation::refresh); }
}
void ReturnAssociation::refresh() {
  if (m_owner.isEmpty()) { resolveOwner(); return; }
  if (m_busy) { m_dirty = true; return; }
  m_busy = true; m_dirty = false;
  const auto lifetime = m_lifetime, context = m_context;
  if (!m_known) {
    auto call = request(m_owner, "org.freedesktop.DBus.Introspectable", "Introspect");
    auto pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(call, 3000), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this, [this, lifetime](auto *p) {
      auto reply = p->reply(); p->deleteLater();
      if (lifetime != m_lifetime) return;
      if (reply.type() != QDBusMessage::ErrorMessage && reply.signature() == "s") {
        auto xml = reply.arguments().first().toString();
        if (xml.size() <= 1024 * 1024) {
          QXmlStreamReader reader(xml); bool found = false;
          while (!reader.atEnd()) {
            reader.readNext();
            if (reader.isStartElement() && reader.name() == QLatin1String("interface") &&
                reader.attributes().value("name") == QLatin1String(association)) found = true;
          }
          m_supported = found && !reader.hasError();
          m_known = !reader.hasError();
        }
      }
      // Failed discovery retries on the timer, never a tight loop.
      done(m_known);
    });
    return;
  }
  auto pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(request(m_owner, service, "Status"), 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this, [this, lifetime, context](auto *p) {
    auto reply = p->reply(); p->deleteLater();
    if (lifetime != m_lifetime) return;
    if (context != m_context) { done(true); return; }
    if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != "(sbstbsssbbss)") {
      m_valid = m_homeKnown = false; emit changed(); done(); return;
    }
    const auto a = qvariant_cast<QDBusArgument>(reply.arguments().first());
    QString epoch, generation, target, active, klass, output, activeOutput;
    qulonglong revision; bool available, home, fullscreen, platform;
    a.beginStructure(); a >> epoch >> available >> generation >> revision >> home >> target
        >> active >> klass >> fullscreen >> platform >> output >> activeOutput; a.endStructure();
    if (!uuid(epoch) || generation != epoch || !text(target, 8192)) {
      m_valid = m_homeKnown = false; emit changed(); done(); return;
    }
    if (epoch != m_epoch || target != m_target || home != m_home) m_valid = false;
    if (epoch != m_epoch) m_revision = 0;
    m_epoch = epoch; m_target = target; m_home = home; m_homeKnown = true;
    if (!home || !m_supported || target.isEmpty() || !platform) {
      m_valid = false; emit changed(); done(); return;
    }
    emit changed();
    readAssociation(lifetime, context);
  });
}
void ReturnAssociation::readAssociation(quint64 lifetime, quint64 context) {
  auto pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(request(m_owner, association, "Get"), 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this, [this, lifetime, context](auto *p) {
    auto reply = p->reply(); p->deleteLater();
    if (lifetime != m_lifetime) return;
    if (context != m_context) { done(true); return; }
    auto reject = [this] { m_valid = false; emit changed(); done(); };
    if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != "(stsa(ss))") { reject(); return; }
    const auto a = qvariant_cast<QDBusArgument>(reply.arguments().first());
    QString epoch, target, generation, token; qulonglong revision; int count = 0;
    a.beginStructure(); a >> epoch >> revision >> target; a.beginArray();
    while (!a.atEnd()) { a.beginStructure(); a >> generation >> token; a.endStructure(); ++count; }
    a.endArray(); a.endStructure();
    if (!uuid(epoch) || epoch != m_epoch || target != m_target || !text(target, 8192) ||
        revision == 0 || revision < m_revision || count > 1 ||
        (!m_signalEpoch.isEmpty() && (epoch != m_signalEpoch || revision < m_floor)) ||
        (count == 1 && (target.isEmpty() || !uuid(generation) || token.isEmpty() || !text(token, 256) ||
          (!m_inventoryGeneration.isEmpty() && generation != m_inventoryGeneration))) ||
        (m_revision == revision && (target != m_installedTarget || token != m_installedToken || generation != m_installedPlatform))) {
      reject(); return;
    }
    m_revision = revision; m_installedTarget = target; m_installedToken = token; m_installedPlatform = generation;
    m_token = token; m_platform = generation; m_valid = count == 1;
    emit changed(); done();
  });
}
