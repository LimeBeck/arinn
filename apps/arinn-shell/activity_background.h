// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include "frame_gate.h"
#include "return_association.h"
#include <QImage>
#include <QMutex>
#include <QObject>
#include <QQmlParserStatus>
#include <QQuickImageProvider>
#include <QThreadPool>
#include <QUrl>
#include <memory>
#include <optional>

class QDBusPendingCallWatcher;

// The image a QML Image reads through "image://arinn-activity/<serial>".
// Shared between the source and the engine's provider; guarded by a mutex
// because asynchronous Images request from a loader thread.
struct ActivityFrameStore {
  QMutex mutex;
  QImage image;
  quint64 serial = 0;
};

// Consumer of org.arinn.Background1 (ADR-0087, specification §12.3). Reads
// the Activity's last frame for Home's receded background. Owner-pinned,
// asynchronous and optional: Home and local focus never wait for it, and an
// older daemon without the interface simply shows the normal background.
//
// The source never arms capture, never asks a provider for pixels and never
// writes a file. Evidence comes from ReturnAssociation (Home, target, output,
// Return window) and the shell's own session guard (`blocked`): a blocked
// guard clears the frame at once, without daemon IPC.
class ActivityBackgroundSource : public QObject, public QQmlParserStatus {
  Q_OBJECT
  Q_INTERFACES(QQmlParserStatus)
  // Empty while no frame is ready for the current Home context.
  Q_PROPERTY(QUrl source READ source NOTIFY sourceChanged)
  Q_PROPERTY(bool available READ available NOTIFY sourceChanged)
  // Bind to the session guard: true while locked, asleep or unknown.
  Q_PROPERTY(bool blocked READ blocked WRITE setBlocked NOTIFY blockedChanged)
public:
  explicit ActivityBackgroundSource(QObject *parent = nullptr);
  ~ActivityBackgroundSource() override;
  QUrl source() const { return m_source; }
  bool available() const { return !m_source.isEmpty(); }
  bool blocked() const { return m_blocked; }
  void setBlocked(bool blocked);
  // Pixel dimensions of the installed frame, for tests and diagnostics.
  QSize frameSize() const;
  void classBegin() override {}
  void componentComplete() override;
  // Adds the "arinn-activity" image provider to `engine` for this source.
  void install(class QQmlEngine *engine);
public slots:
  void refresh();
signals:
  void sourceChanged();
  void blockedChanged();
private slots:
  void backgroundChanged(const QString &epoch, qulonglong revision);
private:
  void setOwner(const QString &owner);
  void evidenceChanged();
  void read();
  void readDone(const arinn::background::FrameGate::Ticket &ticket,
                const arinn::background::Context &echoed, quint32 width,
                quint32 height,
                std::optional<std::vector<std::uint8_t>> rgba, QImage image);
  void publish();
  ReturnAssociation m_association;
  // One bounded copy at a time; the destructor waits for it.
  QThreadPool m_pool;
  arinn::background::FrameGate m_gate;
  std::shared_ptr<ActivityFrameStore> m_store;
  QString m_owner;
  // The last evidence given to the gate.
  std::optional<arinn::background::Evidence> m_evidence;
  // Last resolved Return window of the current target. A repeated Home
  // clears the association until the adapter reports it again; the daemon
  // invalidates the frame itself when the window really goes away.
  QString m_windowTarget, m_windowGeneration, m_windowToken;
  QUrl m_source;
  // The latest snapshot's ready request, and a read that failed for it: a
  // failed request is not read again until a new revision, so a persistent
  // failure never becomes a retry loop.
  std::pair<std::string, quint64> m_latest;
  std::optional<std::pair<std::string, quint64>> m_failed;
  quint64 m_lifetime = 0, m_request = 0;
  bool m_blocked = true, m_busy = false, m_dirty = false, m_reading = false,
       m_unsupported = false;
};

class ActivityFrameProvider : public QQuickImageProvider {
public:
  explicit ActivityFrameProvider(std::shared_ptr<ActivityFrameStore> store)
      : QQuickImageProvider(QQuickImageProvider::Image), m_store(std::move(store)) {}
  QImage requestImage(const QString &id, QSize *size,
                      const QSize &requestedSize) override;
private:
  std::shared_ptr<ActivityFrameStore> m_store;
};
