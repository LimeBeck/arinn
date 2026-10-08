// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QString>

// Reads org.arinn.SystemStatus1 (ADR-0100) for Home's status area. A daemon
// without the interface, a failed read and any value this build does not
// know show nothing (docs/architecture/system-status.md).
class SystemStatus : public QObject {
  Q_OBJECT
  // The default audio output is known to be muted.
  Q_PROPERTY(bool muted READ muted NOTIFY changed)
  // Known to be connected over Wi-Fi.
  Q_PROPERTY(bool wifi READ wifi NOTIFY changed)
  // Coarse Wi-Fi level 1–4 while `wifi`, else 0 (unknown).
  Q_PROPERTY(int wifiLevel READ wifiLevel NOTIFY changed)
public:
  explicit SystemStatus(QObject *parent = nullptr);
  bool muted() const { return m_muted; }
  bool wifi() const { return m_wifi; }
  int wifiLevel() const { return m_level; }
public slots:
  void refresh();
signals:
  void changed();
private slots:
  void statusChanged(const QString &epoch, qulonglong revision);

private:
  void apply(bool muted, bool wifi, int level);
  QString m_epoch;
  // Bumped on every daemon owner change: older replies are dropped.
  quint64 m_lifetime = 0;
  quint64 m_revision = 0;
  bool m_muted = false;
  bool m_wifi = false;
  int m_level = 0;
};
