// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QList>
#include <QObject>
#include <QString>
#include <QTimer>

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
  // The default output's mute flag for the quick menu (ADR-0106):
  // "muted", "unmuted", or empty when unknown.
  Q_PROPERTY(QString audio READ audio NOTIFY changed)
  // Connectivity for the quick menu: "wifi", "wired", "other",
  // "disconnected", or empty when unknown.
  Q_PROPERTY(QString network READ network NOTIFY changed)
public:
  explicit SystemStatus(QObject *parent = nullptr);
  bool muted() const { return m_audio == QLatin1String("muted"); }
  bool wifi() const { return m_network == QLatin1String("wifi"); }
  int wifiLevel() const { return m_level; }
  QString audio() const { return m_audio; }
  QString network() const { return m_network; }
  // Waits before re-reading after a failed read (#409); tests shorten them.
  void setRetryDelays(const QList<int> &ms) { m_delays = ms; }
public slots:
  void refresh();
signals:
  void changed();
private slots:
  void statusChanged(const QString &epoch, qulonglong revision);

private:
  void apply(const QString &audio, const QString &network, int level);
  void retryLater(const QString &error);
  QString m_epoch;
  // Bumped on every daemon owner change: older replies are dropped.
  quint64 m_lifetime = 0;
  // Bumped on every read: only the latest read's reply applies, so a
  // superseded read that fails never clears a newer state.
  quint64 m_request = 0;
  quint64 m_revision = 0;
  QString m_audio;
  QString m_network;
  int m_level = 0;
  // A failed read is retried after each of these delays in turn, then not
  // until the next Changed or owner change.
  QList<int> m_delays{1000, 5000, 30000};
  int m_attempt = 0;
  QTimer m_retry;
};
