// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QTimer>

// Optional read-only ADR-0073 evidence. Never routes Return or platform controls.
class ReturnAssociation : public QObject {
  Q_OBJECT
public:
  explicit ReturnAssociation(QObject *parent = nullptr);
  QString owner() const { return m_owner; }
  QString epoch() const { return m_epoch; }
  QString target() const { return m_target; }
  QString token() const { return m_valid ? m_token : QString(); }
  // Platform inventory generation of `token()`, empty while not valid.
  QString generation() const { return m_valid ? m_platform : QString(); }
  // The retained Home output and platform adapter liveness of the last
  // Status read; kept while a re-read is pending.
  QString output() const { return m_output; }
  bool platformConnected() const { return m_platformConnected; }
  bool valid() const { return m_valid && m_homeKnown && m_home; }
  bool homeKnown() const { return m_homeKnown; }
  bool homeVisible() const { return m_home; }
  // True from an invalidation until the next read completes or is rejected:
  // the evidence is being re-read, not known to be negative.
  bool pending() const { return m_pending; }
public slots:
  void refresh();
  void invalidate();
private slots:
  void associationChanged(const QString &epoch, qulonglong revision);
  void inventoryChanged(const QString &generation, qulonglong revision);
signals:
  void changed();
  void ownerChanged();
private:
  void resolveOwner();
  void setOwner(const QString &owner);
  void done(bool again = false);
  void readAssociation(quint64 lifetime, quint64 context);
  QString m_owner, m_epoch, m_target, m_token, m_platform, m_output;
  QString m_signalEpoch, m_inventoryGeneration, m_installedTarget,
      m_installedToken, m_installedPlatform;
  quint64 m_lifetime = 0, m_context = 0, m_floor = 0, m_revision = 0;
  bool m_platformConnected = false;
  bool m_valid = false, m_homeKnown = false, m_home = false,
       m_known = false, m_supported = false, m_busy = false,
       m_dirty = false, m_resolving = false, m_pending = false;
  QTimer m_timer;
};
