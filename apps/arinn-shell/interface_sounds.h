// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once
#include <QElapsedTimer>
#include <QObject>
#include <QString>
#include <memory>
#include <unordered_map>

class QSoundEffect;

// Pre-rendered Hearth light interface sounds (ADR-0136, spec §5.6). Playback
// never blocks callers; level follows the setting and system mute; media guard
// skips every sound except error while video or music plays.
class InterfaceSounds : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString level READ level WRITE setLevel NOTIFY levelChanged)
  Q_PROPERTY(bool outputMuted READ outputMuted WRITE setOutputMuted NOTIFY
                 outputMutedChanged)
  Q_PROPERTY(bool mediaActive READ mediaActive WRITE setMediaActive NOTIFY
                 mediaActiveChanged)
public:
  explicit InterfaceSounds(QObject *parent = nullptr);
  ~InterfaceSounds() override;
  static InterfaceSounds *instance();

  QString level() const { return m_level; }
  void setLevel(const QString &level);
  bool outputMuted() const { return m_outputMuted; }
  void setOutputMuted(bool muted);
  bool mediaActive() const { return m_mediaActive; }
  void setMediaActive(bool active);

  // Generic sounds (select, kindle, ignite, error, …).
  Q_INVOKABLE void play(const QString &name, bool repeat = false);

  // D-pad navigation: tap follows key autorepeat for half gain; suppressed when
  // open/back/close play in the same step.
  Q_INVOKABLE void playNavigationTap(bool autoRepeat);
  // Row edge bump: once per physical press (no autorepeat).
  Q_INVOKABLE void playEdgeBump(bool autoRepeat);
  Q_INVOKABLE void playOpen();
  Q_INVOKABLE void playClose();
  Q_INVOKABLE void playBack();

  // Tests wait for asynchronous QSoundEffect decoding.
  bool ensureLoaded(const QString &name);
  int effectStatus(const QString &name) const;

  void setTapIntervalMs(int ms) { m_tapIntervalMs = ms; }
  void resetTapThrottle() { m_lastTapMs = 0; }
  void resetEdgeThrottle() { m_lastEdgeMs = 0; }

#ifdef ARINN_INTERFACE_SOUNDS_TEST
  // Regression helper: navigation tap while open/back/close hold suppress.
  bool playNavigationTapWhileSuppressed(bool autoRepeat);
#endif

signals:
  void levelChanged();
  void outputMutedChanged();
  void mediaActiveChanged();
  void played(const QString &name, bool repeat);

private:
  bool playInternal(const QString &name, bool repeat, bool throttleTap,
                    bool throttleEdge);
  double volume(bool repeat) const;
  QSoundEffect *effectFor(const QString &name);
  void preloadAllEffects();

  QString m_level = QStringLiteral("quiet");
  bool m_outputMuted = false;
  bool m_mediaActive = false;
  bool m_suppressNavigationTap = false;
  int m_tapIntervalMs = 40;
  int m_edgeIntervalMs = 40;
  qint64 m_lastTapMs = 0;
  qint64 m_lastEdgeMs = 0;
  QElapsedTimer m_clock;
  std::unordered_map<QString, std::unique_ptr<QSoundEffect>> m_effects;
};

void registerInterfaceSounds();
