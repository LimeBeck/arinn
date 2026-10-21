// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QString>
#include <QVariantList>
#include <functional>

// Calls org.arinn.SystemControl1 (ADR-0108) for the power menu and the volume
// HUD. Every call is asynchronous; the UI thread never waits for the daemon
// (spec §8.2). Nothing here reports a power action as done: an accepted
// request only means the platform took it (docs/architecture/system-control.md).
class SystemControl : public QObject {
  Q_OBJECT
  // "" before the first read, "loading" while a read runs, "ready" after.
  Q_PROPERTY(QString powerState READ powerState NOTIFY powerChanged)
  // available, inhibited, denied, unsupported or unknown.
  Q_PROPERTY(QString suspend READ suspend NOTIFY powerChanged)
  Q_PROPERTY(QString reboot READ reboot NOTIFY powerChanged)
  Q_PROPERTY(QString powerOff READ powerOff NOTIFY powerChanged)
  // Block inhibitors: {what: sleep|shutdown, who, why}.
  Q_PROPERTY(QVariantList inhibitors READ inhibitors NOTIFY powerChanged)
  // The action whose request is in flight: suspend, reboot or power-off.
  Q_PROPERTY(QString pendingAction READ pendingAction NOTIFY requestChanged)
  // The last action the platform accepted, until reset().
  Q_PROPERTY(QString acceptedAction READ acceptedAction NOTIFY requestChanged)
  // Why the last request was refused: inhibited, denied, unsupported,
  // unavailable, timeout or failed; empty when it was not.
  Q_PROPERTY(QString refusal READ refusal NOTIFY requestChanged)
  Q_PROPERTY(QString refusalAction READ refusalAction NOTIFY requestChanged)
  // The reply to the latest volume change or read.
  Q_PROPERTY(bool volumeKnown READ volumeKnown NOTIFY volumeChanged)
  Q_PROPERTY(bool volumeMuted READ volumeMuted NOTIFY volumeChanged)
  Q_PROPERTY(int volumeLevel READ volumeLevel NOTIFY volumeChanged)
public:
  explicit SystemControl(QObject *parent = nullptr);
  QString powerState() const { return m_powerState; }
  QString suspend() const { return m_suspend; }
  QString reboot() const { return m_reboot; }
  QString powerOff() const { return m_powerOff; }
  QVariantList inhibitors() const { return m_inhibitors; }
  QString pendingAction() const { return m_pending; }
  QString acceptedAction() const { return m_accepted; }
  QString refusal() const { return m_refusal; }
  QString refusalAction() const { return m_refusalAction; }
  bool volumeKnown() const { return m_volumeKnown; }
  bool volumeMuted() const { return m_volumeMuted; }
  int volumeLevel() const { return m_volumeLevel; }

  // Reads power availability; called when the power menu opens.
  Q_INVOKABLE void readPower();
  // Requests suspend, reboot or power-off. Ignored while one is in flight.
  Q_INVOKABLE void request(const QString &action);
  // Forgets the last refusal and accepted action.
  Q_INVOKABLE void reset();
  Q_INVOKABLE void readVolume();
  Q_INVOKABLE void stepVolume(int steps);
  // Volume Up: turns the sound back on, then raises the level by one step
  // (owner decision of 2026-10-21 on #477).
  Q_INVOKABLE void raiseVolume();
  Q_INVOKABLE void toggleMuted();
  Q_INVOKABLE void setMuted(bool muted);

  // The refusal for a D-Bus error name.
  static QString refusalFor(const QString &errorName);

signals:
  void powerChanged();
  void requestChanged();
  void volumeChanged();
  // A volume change succeeded: the HUD shows the new state.
  void volumeShown();
  // A volume change failed with this refusal; no level is shown.
  void volumeFailed(const QString &refusal);

private:
  void applyPower(const QVariant &value);
  // `then` runs after a successful reply; a failure is reported when `show`
  // or `then` is set.
  void volumeCall(const QString &method, const QVariantList &arguments,
                  bool show, std::function<void()> then = {});
  QString m_powerState;
  QString m_suspend = QStringLiteral("unknown");
  QString m_reboot = QStringLiteral("unknown");
  QString m_powerOff = QStringLiteral("unknown");
  QVariantList m_inhibitors;
  QString m_pending;
  QString m_accepted;
  QString m_refusal;
  QString m_refusalAction;
  // Only the reply to the latest power read applies.
  quint64 m_powerRead = 0;
  // Volume replies apply in request order; an older one never replaces a
  // newer state.
  quint64 m_volumeRequest = 0;
  quint64 m_volumeApplied = 0;
  bool m_volumeKnown = false;
  bool m_volumeMuted = false;
  int m_volumeLevel = 0;
};
