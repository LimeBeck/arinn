// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "producer.h"
#include "encode.h"
#include <QQuickItemGrabResult>
#include <QQuickWindow>
#include <QUuid>
#include <QtDBus>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <optional>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString background = QStringLiteral("org.arinn.Background1");
const QString captureInterface = QStringLiteral("org.arinn.BackgroundCapture1");
constexpr int callTimeout = 3000;

struct FrameWire {
  QString epoch, request, target, output, generation, token;
  quint32 width = 0, height = 0;
  QDBusUnixFileDescriptor pixels;
};
} // namespace
Q_DECLARE_METATYPE(FrameWire)
QDBusArgument &operator<<(QDBusArgument &a, const FrameWire &f) {
  a.beginStructure();
  a.beginStructure();
  a << f.epoch << f.request << f.target << f.output;
  a.beginStructure();
  a << f.generation << f.token;
  a.endStructure();
  a.endStructure();
  a << f.width << f.height << f.pixels;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, FrameWire &) { return a; }

namespace {
bool uuid(const QString &s) { return s.size() == 36 && !QUuid(s).isNull(); }
bool text(const QString &s, int limit) {
  return !s.isEmpty() && !s.contains(QChar(0)) && s.toUtf8().size() <= limit;
}
// Normalized diagnostics only: no window, target, token or request ID.
void log(const char *operation, const char *outcome, qint64 ms) {
  std::fprintf(stderr,
               "{\"level\":\"info\",\"component\":\"arinn-kwin-background\","
               "\"provider_id\":null,\"request_id\":null,\"operation\":\"%s\","
               "\"outcome\":\"%s\",\"error\":null,\"duration_ms\":%lld}\n",
               operation, outcome, static_cast<long long>(ms));
}
} // namespace

BackgroundProducer::BackgroundProducer(QObject *parent) : QObject(parent) {
  qDBusRegisterMetaType<FrameWire>();
  m_pool.setMaxThreadCount(1);
  m_deadline.setSingleShot(true);
  m_deadline.setInterval(1500);
  connect(&m_deadline, &QTimer::timeout, this, [this] {
    if (m_request.isEmpty()) return;
    log("background-capture", "deadline", m_started.elapsed());
    cancel(m_request);
    end(false);
  });
  auto watcher = new QDBusServiceWatcher(service, QDBusConnection::sessionBus(),
                                         QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &owner) { setOwner(owner); });
  resolveOwner();
}

BackgroundProducer::~BackgroundProducer() { m_pool.waitForDone(); }

void BackgroundProducer::resolveOwner() {
  if (m_resolving) return;
  m_resolving = true;
  auto msg = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.DBus"),
                                            QStringLiteral("/org/freedesktop/DBus"),
                                            QStringLiteral("org.freedesktop.DBus"),
                                            QStringLiteral("GetNameOwner"));
  msg << service;
  const auto lifetime = m_lifetime;
  auto watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, callTimeout), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, lifetime](QDBusPendingCallWatcher *p) {
            const auto reply = p->reply();
            p->deleteLater();
            m_resolving = false;
            if (lifetime == m_lifetime && reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == QLatin1String("s"))
              setOwner(reply.arguments().first().toString());
          });
}

void BackgroundProducer::setOwner(const QString &owner) {
  if (owner == m_owner) return;
  auto bus = QDBusConnection::sessionBus();
  if (!m_owner.isEmpty())
    bus.disconnect(m_owner, path, background, QStringLiteral("Changed"), this,
                   SLOT(backgroundChanged(QString, qulonglong)));
  ++m_lifetime;
  m_owner = owner;
  m_busy = m_dirty = false;
  m_done.clear();
  m_floorEpoch.clear();
  m_floor = 0;
  // A new daemon owner never inherits the old one's attempt.
  if (!m_request.isEmpty()) end(false);
  if (owner.isEmpty()) return;
  // Subscribe before the first Get; every message is pinned to this owner.
  bus.connect(owner, path, background, QStringLiteral("Changed"), this,
              SLOT(backgroundChanged(QString, qulonglong)));
  refresh();
}

void BackgroundProducer::backgroundChanged(const QString &epoch, qulonglong revision) {
  // Raise the floor before any IPC: a Get already in flight may answer with
  // an older snapshot. Invalid metadata never lowers or replaces it.
  if (uuid(epoch) && revision > 0) {
    if (epoch != m_floorEpoch) {
      m_floorEpoch = epoch;
      m_floor = revision;
    } else if (revision > m_floor) {
      m_floor = revision;
    }
  }
  refresh();
}

void BackgroundProducer::refresh() {
  if (m_owner.isEmpty()) return;
  if (m_busy) {
    m_dirty = true;
    return;
  }
  m_busy = true;
  m_dirty = false;
  const auto lifetime = m_lifetime;
  auto call = QDBusMessage::createMethodCall(m_owner, path, background, QStringLiteral("Get"));
  auto watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, callTimeout), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, lifetime](QDBusPendingCallWatcher *p) {
            const auto reply = p->reply();
            p->deleteLater();
            if (lifetime != m_lifetime) return;
            m_busy = false;
            std::optional<Context> pending;
            if (reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == QLatin1String("(sta(ssss(ss))s)")) {
              const auto a = qvariant_cast<QDBusArgument>(reply.arguments().first());
              QString epoch, state;
              qulonglong revision = 0;
              QList<Context> contexts;
              a.beginStructure();
              a >> epoch >> revision;
              a.beginArray();
              while (!a.atEnd()) {
                Context c;
                a.beginStructure();
                a >> c.epoch >> c.request >> c.target >> c.output;
                a.beginStructure();
                a >> c.generation >> c.token;
                a.endStructure();
                a.endStructure();
                contexts.append(c);
              }
              a.endArray();
              a >> state;
              a.endStructure();
              // Older than a Changed already received (another epoch, or a
              // lower revision): skip it entirely, before any capture starts
              // or ends. That Changed marked the producer dirty, so a fresh
              // Get follows below.
              if (!m_floorEpoch.isEmpty() && (epoch != m_floorEpoch || revision < m_floor)) {
                if (m_dirty) refresh();
                return;
              }
              if (state == QLatin1String("pending") && contexts.size() == 1) {
                const auto &c = contexts.first();
                if (uuid(epoch) && c.epoch == epoch && uuid(c.request) &&
                    uuid(c.generation) && text(c.target, 8192) && text(c.output, 8192) &&
                    text(c.token, 256))
                  pending = c;
              }
            }
            // Only a still-pending current request keeps the capture going.
            if (!m_request.isEmpty() && (!pending || pending->request != m_request)) end(false);
            if (pending && m_request.isEmpty() && pending->request != m_done) start(*pending);
            if (m_dirty) refresh();
          });
}

void BackgroundProducer::start(const Context &context) {
  // A grab of a superseded request is released; its result is never used.
  releaseGrab();
  m_context = context;
  m_request = context.request;
  m_target = context.target;
  m_output = context.output;
  m_started.start();
  m_deadline.start();
  emit requestChanged();
}

void BackgroundProducer::end(bool published) {
  if (m_request.isEmpty()) return;
  const auto request = m_request;
  m_done = m_request;
  m_request.clear();
  m_target.clear();
  m_output.clear();
  m_deadline.stop();
  releaseGrab();
  emit requestChanged();
  emit finished(request, published);
}

QSize BackgroundProducer::grabSize(qreal width, qreal height, qreal dpr) {
  if (!std::isfinite(width) || !std::isfinite(height) || !(width > 0) || !(height > 0))
    return {1, 1};
  if (!std::isfinite(dpr) || !(dpr >= 1)) dpr = 1;
  // Only a size larger than the physical bounds (logical size times DPR) is
  // scaled down, keeping the aspect ratio; a smaller one is kept, never
  // enlarged. Floor, so the physical size never exceeds the bounds.
  const qreal scale = std::min({qreal(1),
                                qreal(arinn::background::maxWidth) / (width * dpr),
                                qreal(arinn::background::maxHeight) / (height * dpr)});
  return {std::max(1, int(std::floor(width * scale))),
          std::max(1, int(std::floor(height * scale)))};
}

QSize BackgroundProducer::stageSize(qreal width, qreal height, qreal dpr) const {
  // The stage's backing surface obeys the same bound, rounded the same way.
  return grabSize(width, height, dpr);
}

void BackgroundProducer::fail() {
  if (m_request.isEmpty()) return;
  log("background-capture", "window", m_started.elapsed());
  cancel(m_request);
  end(false);
}

void BackgroundProducer::capture(QQuickItem *item) {
  if (m_request.isEmpty() || m_grab) return;
  // At most one grab/worker (ADR-0087): while an earlier image is still being
  // encoded, this request gets no frame instead of a queued second image.
  if (m_encoding) {
    log("background-capture", "busy", m_started.elapsed());
    cancel(m_request);
    end(false);
    return;
  }
  if (!item || !item->window() ||
      item->width() <= 0 || item->height() <= 0)
    return;
  // Ask for physical pixels within the bounds; DPR can still enlarge the
  // result, which the worker scales down before encoding.
  const QSize size = grabSize(item->width(), item->height(),
                              item->window()->effectiveDevicePixelRatio());
  auto grab = item->grabToImage(size);
  if (!grab) {
    fail();
    return;
  }
  // The producer owns the only strong reference. The connection captures a
  // raw pointer, so it never keeps the result (and its image) alive.
  m_grab = grab;
  QQuickItemGrabResult *result = grab.data();
  const auto request = m_request;
  connect(result, &QQuickItemGrabResult::ready, this, [this, result, request] {
    if (m_grab.data() != result) return;
    // The compositor callback only takes a reference; all work is off-thread.
    const QImage image = result->image();
    releaseGrab();
    m_encoding = true;
    m_pool.start([this, image, request, hook = m_encodeHook] {
      if (hook) hook();
      auto bounded = arinn::background::boundGrab(image);
      auto pixels = std::make_shared<arinn::background::Pixels>(
          arinn::background::encodeGrab(bounded));
      const quint32 width = bounded.width(), height = bounded.height();
      QMetaObject::invokeMethod(
          this, [this, request, pixels, width, height] { encoded(request, pixels, width, height); },
          Qt::QueuedConnection);
    });
  });
}

void BackgroundProducer::releaseGrab() {
  if (!m_grab) return;
  // Never delete the result inside its own ready emission: hand the last
  // reference to a queued no-op, which drops it on the next event loop pass.
  auto retired = std::move(m_grab);
  QMetaObject::invokeMethod(this, [retired] {}, Qt::QueuedConnection);
}

void BackgroundProducer::encoded(const QString &request,
                                 std::shared_ptr<arinn::background::Pixels> pixels,
                                 quint32 width, quint32 height) {
  m_encoding = false;
  // A superseded or timed-out request closes its descriptor here.
  if (request != m_request) return;
  if (!*pixels) {
    log("background-capture", "encode", m_started.elapsed());
    cancel(request);
    end(false);
    return;
  }
  publish(request, std::move(pixels), width, height);
}

void BackgroundProducer::publish(const QString &request,
                                 std::shared_ptr<arinn::background::Pixels> pixels,
                                 quint32 width, quint32 height) {
  FrameWire frame{m_context.epoch, m_context.request, m_context.target, m_context.output,
                  m_context.generation, m_context.token, width, height,
                  QDBusUnixFileDescriptor(pixels->descriptor())};
  pixels->reset();
  auto call = QDBusMessage::createMethodCall(m_owner, path, captureInterface, QStringLiteral("Publish"));
  call << QVariant::fromValue(frame);
  const auto lifetime = m_lifetime;
  auto watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, callTimeout), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, lifetime, request](QDBusPendingCallWatcher *p) {
            const auto reply = p->reply();
            p->deleteLater();
            if (lifetime != m_lifetime || request != m_request) return;
            const bool ok = reply.type() != QDBusMessage::ErrorMessage;
            log("background-capture", ok ? "published" : "rejected", m_started.elapsed());
            end(ok);
          });
}

void BackgroundProducer::cancel(const QString &request) {
  if (m_owner.isEmpty()) return;
  auto call = QDBusMessage::createMethodCall(m_owner, path, captureInterface, QStringLiteral("Cancel"));
  call << request;
  // Fire and forget: a stale cancellation is a harmless Conflict.
  QDBusConnection::sessionBus().asyncCall(call, callTimeout);
}
