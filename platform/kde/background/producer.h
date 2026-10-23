// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "pixels.h"
#include <QElapsedTimer>
#include <QObject>
#include <QPointer>
#include <QQuickItem>
#include <QQuickItemGrabResult>
#include <QSharedPointer>
#include <QThreadPool>
#include <QTimer>
#include <QtQml/qqmlregistration.h>
#include <functional>
#include <memory>

// The trusted in-process KWin producer of Activity background frames
// (ADR-0087, specification §12.3). It runs inside KWin and uses KWin's own
// session bus connection, which owns org.kde.KWin, so the daemon accepts its
// publications as the selected platform owner's. No helper process, file,
// path or URL is involved.
//
// It follows org.arinn.Background1 pinned to the daemon owner. For a pending
// attempt with a resolved context it exposes `request` and `target`; the
// KWin script then shows the exact window in a WindowThumbnail and calls
// capture(). The grab is copied, scaled to the bounds and converted on a
// worker thread, never in the compositor callback, and published as one
// sealed memfd. A failure or the local deadline cancels the attempt; the
// daemon then keeps the normal background. Each request is captured once.
class BackgroundProducer : public QObject {
  Q_OBJECT
  QML_ELEMENT
  // The pending request to capture, empty when idle.
  Q_PROPERTY(QString request READ request NOTIFY requestChanged)
  // The exact Return identity of that request: KWin's window internalId.
  Q_PROPERTY(QString target READ target NOTIFY requestChanged)
  Q_PROPERTY(QString output READ output NOTIFY requestChanged)
public:
  explicit BackgroundProducer(QObject *parent = nullptr);
  ~BackgroundProducer() override;
  QString request() const { return m_request; }
  QString target() const { return m_target; }
  QString output() const { return m_output; }
  // Grab `item` for the current request. Ignored without a request or while
  // its grab is already running; cancelled as busy while an earlier image is
  // still being encoded.
  Q_INVOKABLE void capture(QQuickItem *item);
  // The window is missing or cannot be shown: cancel the current request.
  Q_INVOKABLE void fail();
  // The local guard; the daemon's own deadline is authoritative.
  void setDeadline(int ms) { m_deadline.setInterval(ms); }
  // Test seam: runs on the encoding worker before each encoding.
  void setEncodeHook(std::function<void()> hook) { m_encodeHook = std::move(hook); }
  // The logical stage size for a window of `width`×`height` logical pixels
  // on an output with `dpr`: its backing surface stays within the physical
  // frame bounds (ADR-0087), keeping the aspect ratio.
  Q_INVOKABLE QSize stageSize(qreal width, qreal height, qreal dpr) const;
  // The outstanding grab result, for tests: at most one, released as soon
  // as its image is taken or its request ends.
  QQuickItemGrabResult *pendingGrab() const { return m_grab.data(); }
signals:
  void requestChanged();
  // One request ended: published, cancelled, superseded or timed out.
  void finished(const QString &request, bool published);
public slots:
  void refresh();
private slots:
  void backgroundChanged(const QString &epoch, qulonglong revision);
private:
  struct Context {
    QString epoch, request, target, output, generation, token;
  };
  void setOwner(const QString &owner);
  void resolveOwner();
  void start(const Context &context);
  void end(bool published);
  void encoded(const QString &request, std::shared_ptr<arinn::background::Pixels> pixels,
               quint32 width, quint32 height);
  void publish(const QString &request, std::shared_ptr<arinn::background::Pixels> pixels,
               quint32 width, quint32 height);
  void cancel(const QString &request);
  void releaseGrab();
  QThreadPool m_pool;
  std::function<void()> m_encodeHook;
  QSharedPointer<QQuickItemGrabResult> m_grab;
  QTimer m_deadline;
  QElapsedTimer m_started;
  QString m_owner, m_request, m_target, m_output, m_done;
  Context m_context;
  quint64 m_lifetime = 0;
  bool m_busy = false, m_dirty = false, m_resolving = false;
  // An image is on the encoding worker; cleared when encoded() runs.
  bool m_encoding = false;
};
