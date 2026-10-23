// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "activity_background.h"
#include "pixels.h"
#include <QCoreApplication>
#include <QQmlEngine>
#include <QThreadPool>
#include <QtDBus>
#include <cstring>
#include <utility>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString interface = QStringLiteral("org.arinn.Background1");
const QString providerId = QStringLiteral("arinn-activity");
constexpr int callTimeout = 3000;

void registerActivityBackground() {
  qmlRegisterType<ActivityBackgroundSource>("Arinn.Shell", 1, 0,
                                            "ActivityBackgroundSource");
}

arinn::background::Context readContext(const QDBusArgument &a) {
  QString epoch, request, target, output, generation, token;
  a.beginStructure();
  a >> epoch >> request >> target >> output;
  a.beginStructure();
  a >> generation >> token;
  a.endStructure();
  a.endStructure();
  return {epoch.toStdString(),  request.toStdString(),
          target.toStdString(), output.toStdString(),
          generation.toStdString(), token.toStdString()};
}
} // namespace
Q_COREAPP_STARTUP_FUNCTION(registerActivityBackground)

QImage ActivityFrameProvider::requestImage(const QString &id, QSize *size,
                                           const QSize &) {
  QMutexLocker lock(&m_store->mutex);
  // A stale serial (a released or replaced frame) yields nothing.
  if (m_store->image.isNull() || id != QString::number(m_store->serial)) {
    if (size) *size = {};
    return {};
  }
  if (size) *size = m_store->image.size();
  return m_store->image;
}

ActivityBackgroundSource::ActivityBackgroundSource(QObject *parent)
    : QObject(parent), m_store(std::make_shared<ActivityFrameStore>()) {
  m_pool.setMaxThreadCount(1);
  connect(&m_association, &ReturnAssociation::changed, this,
          &ActivityBackgroundSource::evidenceChanged);
  connect(&m_association, &ReturnAssociation::ownerChanged, this,
          [this] { setOwner(m_association.owner()); });
  setOwner(m_association.owner());
}

ActivityBackgroundSource::~ActivityBackgroundSource() {
  // A copy in flight references this object's pool; never outlive it.
  m_pool.waitForDone();
}

void ActivityBackgroundSource::componentComplete() { install(qmlEngine(this)); }

void ActivityBackgroundSource::install(QQmlEngine *engine) {
  if (!engine) return;
  if (engine->imageProvider(providerId)) engine->removeImageProvider(providerId);
  engine->addImageProvider(providerId, new ActivityFrameProvider(m_store));
}

QSize ActivityBackgroundSource::frameSize() const {
  QMutexLocker lock(&m_store->mutex);
  return m_store->image.size();
}

void ActivityBackgroundSource::setBlocked(bool blocked) {
  if (blocked == m_blocked) return;
  m_blocked = blocked;
  emit blockedChanged();
  // Protect the lock screen locally, before any daemon IPC.
  evidenceChanged();
}

void ActivityBackgroundSource::setOwner(const QString &owner) {
  if (owner == m_owner && m_lifetime) return;
  auto bus = QDBusConnection::sessionBus();
  if (!m_owner.isEmpty())
    bus.disconnect(m_owner, path, interface, QStringLiteral("Changed"), this,
                   SLOT(backgroundChanged(QString, qulonglong)));
  ++m_lifetime;
  m_owner = owner;
  m_busy = m_dirty = m_unsupported = false;
  // Subscribe before the first Get; every message is pinned to this owner.
  if (!owner.isEmpty())
    bus.connect(owner, path, interface, QStringLiteral("Changed"), this,
                SLOT(backgroundChanged(QString, qulonglong)));
  evidenceChanged();
}

void ActivityBackgroundSource::evidenceChanged() {
  const auto &a = m_association;
  // An invalidated association is being re-read, not known to be negative.
  const bool home = a.homeVisible() && (a.homeKnown() || a.pending());
  const auto target = a.target();
  if (!home || target != m_windowTarget || a.owner() != m_owner) {
    m_windowTarget = home ? target : QString();
    m_windowGeneration.clear();
    m_windowToken.clear();
  }
  if (a.valid() && !a.token().isEmpty()) {
    m_windowGeneration = a.generation();
    m_windowToken = a.token();
  }
  arinn::background::Evidence evidence;
  evidence.owner = m_owner.toStdString();
  evidence.epoch = a.epoch().toStdString();
  evidence.target = target.toStdString();
  evidence.output = a.output().toStdString();
  evidence.generation = m_windowGeneration.toStdString();
  evidence.token = m_windowToken.toStdString();
  evidence.home = home;
  evidence.sessionSafe = !m_blocked;
  // The platform adapter is both the Activity's provider of window evidence
  // and the frame's producer.
  evidence.providerLive = evidence.producerLive = a.platformConnected();
  evidence.shellLive = true;
  // ReturnAssociation reports on every poll; an unchanged reading neither
  // touches the gate nor asks the daemon again.
  if (m_evidence && *m_evidence == evidence) return;
  m_evidence = evidence;
  ++m_evidenceSerial;
  m_gate.observe(std::move(evidence));
  publish();
  refresh();
}

void ActivityBackgroundSource::backgroundChanged(const QString &epoch,
                                                 qulonglong revision) {
  m_gate.notified(epoch.toStdString(), revision);
  publish();
  refresh();
}

void ActivityBackgroundSource::refresh() {
  if (m_owner.isEmpty() || m_unsupported || m_blocked) return;
  if (m_busy) {
    m_dirty = true;
    return;
  }
  m_busy = true;
  m_dirty = false;
  const auto lifetime = m_lifetime, serial = m_evidenceSerial;
  auto call = QDBusMessage::createMethodCall(m_owner, path, interface,
                                             QStringLiteral("Get"));
  auto watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, callTimeout), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, lifetime, serial](QDBusPendingCallWatcher *p) {
            const auto reply = p->reply();
            p->deleteLater();
            if (lifetime != m_lifetime) return;
            m_busy = false;
            // A reply that spans an evidence change (a lock, a hidden Home or
            // a lost provider, even if already restored) belongs to another
            // lifetime: it is never admitted, only asked again.
            if (serial != m_evidenceSerial) {
              refresh();
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage) {
              const auto name = reply.errorName();
              // An older daemon: no Activity background until it changes.
              if (name == QLatin1String("org.freedesktop.DBus.Error.UnknownMethod") ||
                  name == QLatin1String("org.freedesktop.DBus.Error.UnknownInterface") ||
                  name == QLatin1String("org.freedesktop.DBus.Error.UnknownObject"))
                m_unsupported = true;
              m_gate.failed();
            } else if (reply.signature() != QLatin1String("(sta(ssss(ss))s)")) {
              m_gate.failed();
            } else {
              const auto a = qvariant_cast<QDBusArgument>(reply.arguments().first());
              arinn::background::Snapshot snapshot;
              QString epoch, state;
              qulonglong revision = 0;
              int count = 0;
              a.beginStructure();
              a >> epoch >> revision;
              a.beginArray();
              while (!a.atEnd()) {
                const auto context = readContext(a);
                if (++count == 1) snapshot.context = context;
              }
              a.endArray();
              a >> state;
              a.endStructure();
              snapshot.epoch = epoch.toStdString();
              snapshot.revision = revision;
              snapshot.state = state.toStdString();
              m_latest = {snapshot.context ? snapshot.context->request : std::string(),
                          snapshot.revision};
              if (count > 1) m_gate.failed();
              else m_gate.accept(snapshot);
            }
            publish();
            // Read an admitted snapshot first; a dirty reconciliation
            // follows and cannot starve it.
            read();
            if (m_dirty) refresh();
          });
}

void ActivityBackgroundSource::read() {
  if (m_reading || m_blocked || m_latest == m_failed) return;
  const auto ticket = m_gate.beginRead();
  if (!ticket) return;
  m_reading = true;
  auto call = QDBusMessage::createMethodCall(m_owner, path, interface,
                                             QStringLiteral("ReadFrame"));
  call << QString::fromStdString(ticket->context.request);
  auto watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, callTimeout), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, ticket = *ticket](QDBusPendingCallWatcher *p) {
            const auto reply = p->reply();
            p->deleteLater();
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != QLatin1String("((ssss(ss))uuh)")) {
              readDone(ticket, {}, 0, 0, std::nullopt, {});
              return;
            }
            const auto a = qvariant_cast<QDBusArgument>(reply.arguments().first());
            quint32 width = 0, height = 0;
            QDBusUnixFileDescriptor fd;
            a.beginStructure();
            const auto echoed = readContext(a);
            a >> width >> height >> fd;
            a.endStructure();
            // Duplicate and validate seals and exact size before any copy.
            auto pixels = std::make_shared<arinn::background::Pixels>(
                arinn::background::Pixels::duplicate(fd.fileDescriptor(), width, height));
            if (!*pixels) {
              readDone(ticket, echoed, width, height, std::nullopt, {});
              return;
            }
            // The bounded copy runs off the GUI thread; focus never waits.
            m_pool.start([this, pixels, ticket, echoed, width, height] {
              auto rgba = pixels->read(width, height);
              pixels->reset();
              // The image copy is made here too: the GUI thread only
              // installs the result.
              QImage image;
              if (rgba) {
                image = QImage(int(width), int(height), QImage::Format_RGBA8888);
                const auto row = std::size_t(width) * 4;
                if (!image.isNull())
                  for (quint32 y = 0; y < height; ++y)
                    std::memcpy(image.scanLine(int(y)), rgba->data() + y * row, row);
              }
              QMetaObject::invokeMethod(
                  this,
                  [this, ticket, echoed, width, height, rgba = std::move(rgba),
                   image = std::move(image)]() mutable {
                    readDone(ticket, echoed, width, height, std::move(rgba), std::move(image));
                  },
                  Qt::QueuedConnection);
            });
          });
}

void ActivityBackgroundSource::readDone(
    const arinn::background::FrameGate::Ticket &ticket,
    const arinn::background::Context &echoed, quint32 width, quint32 height,
    std::optional<std::vector<std::uint8_t>> rgba, QImage image) {
  m_reading = false;
  // Every ticket completes exactly once, releasing the single worker slot.
  if (!m_gate.completeRead(ticket, echoed, width, height, std::move(rgba))) {
    m_failed = {ticket.context.request, ticket.revision};
    publish();
    refresh();
    return;
  }
  if (image.isNull() || image.size() != QSize(int(width), int(height))) {
    publish();
    return;
  }
  quint64 serial;
  {
    QMutexLocker lock(&m_store->mutex);
    m_store->image = std::move(image);
    serial = ++m_store->serial;
  }
  m_source = QUrl(QStringLiteral("image://%1/%2").arg(providerId).arg(serial));
  emit sourceChanged();
}

void ActivityBackgroundSource::publish() {
  if (m_gate.ready() || m_source.isEmpty()) return;
  {
    QMutexLocker lock(&m_store->mutex);
    m_store->image = QImage();
    ++m_store->serial;
  }
  m_source.clear();
  emit sourceChanged();
}
