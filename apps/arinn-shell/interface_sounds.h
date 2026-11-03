// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once
#include <QElapsedTimer>
#include <QMediaDevices>
#include <QObject>
#include <QString>
#include <memory>
#include <unordered_map>

class QSoundEffect;

// Pre-rendered Hearth light interface sounds (ADR-0136, spec §5.6). Playback
// never blocks callers; level follows the setting and system mute; media guard
// skips every sound except error while video or music plays. QSoundEffect
// instances are created only after QMediaDevices reports a non-empty audio
// output list (first play or audioOutputsChanged). If no output is present
// at start, or none is present before the first Ready decode, playback stays
// unavailable for the life of the process.
class InterfaceSounds : public QObject {
  Q_OBJECT
  Q_PROPERTY(QString level READ level WRITE setLevel NOTIFY levelChanged)
  Q_PROPERTY(bool levelKnown READ levelKnown NOTIFY levelChanged)
  Q_PROPERTY(bool outputMuted READ outputMuted WRITE setOutputMuted NOTIFY
                 outputMutedChanged)
  Q_PROPERTY(bool mediaActive READ mediaActive WRITE setMediaActive NOTIFY
                 mediaActiveChanged)
  // Latched for this process (owner decision, #730). False when no audio
  // output was present at start, or before the first Ready decode. A later
  // output does not set it true.
  Q_PROPERTY(bool available READ available NOTIFY availableChanged)
public:
  explicit InterfaceSounds(QObject *parent = nullptr);
  ~InterfaceSounds() override;
  static InterfaceSounds *instance();

  QString level() const { return m_level; }
  bool levelKnown() const { return m_levelKnown; }
  void setLevel(const QString &level);
  bool outputMuted() const { return m_outputMuted; }
  void setOutputMuted(bool muted);
  bool mediaActive() const { return m_mediaActive; }
  void setMediaActive(bool active);
  bool available() const;

  // Generic sounds (select, kindle, ignite, error, …).
  Q_INVOKABLE void play(const QString &name, bool repeat = false);

  // D-pad navigation: tap follows key autorepeat for half gain; suppressed when
  // a higher-priority sound plays in the same step (open/back/close).
  Q_INVOKABLE void playNavigationTap(bool autoRepeat);
  // Row edge bump: once per physical press (no autorepeat).
  Q_INVOKABLE void playEdgeBump(bool autoRepeat);
  Q_INVOKABLE void playOpen();
  Q_INVOKABLE void playClose();
  Q_INVOKABLE void playBack();

  // Tests wait for asynchronous QSoundEffect decoding.
  bool ensureLoaded(const QString &name);
  bool resourceEmbedded(const QString &name) const;
  int effectStatus(const QString &name) const;

  void setTapIntervalMs(int ms) { m_tapIntervalMs = ms; }
  void resetTapThrottle() { m_lastTapMs = 0; }
  void resetEdgeThrottle() { m_lastEdgeMs = 0; }

#ifdef ARINN_INTERFACE_SOUNDS_TEST
  bool playNavigationTapWhileSuppressed(bool autoRepeat);
  bool playEdgeBumpWhileSuppressed(bool autoRepeat);
  void setTestFailNextDecodeOnce(const QString &name);
  void setTestFailPlayAttempts(int count);
  // Copied into objects constructed until clearTestAudioOutputs().
  static void setTestAudioOutputsPresent(bool present);
  static void clearTestAudioOutputs();
  // This object only. Does not change objects already constructed.
  void useTestAudioOutputs(bool present);
#endif

signals:
  void levelChanged();
  void outputMutedChanged();
  void mediaActiveChanged();
  void availableChanged();
  void played(const QString &name, bool repeat);

private slots:
  void onAudioOutputsChanged();

private:
  bool playInternal(const QString &name, bool repeat, bool throttleTap,
                    bool throttleEdge);
  bool playEffectReady(const QString &name, bool repeat);
  double volume(bool repeat) const;
  QSoundEffect *effectFor(const QString &name);
  void reloadAllEffects();
  void resetAllEffects();
  bool waitForEffectReady(const QString &name);
  bool outputsReported() const;

  QString m_level;
  bool m_levelKnown = false;
  bool m_outputMuted = false;
  bool m_mediaActive = false;
  bool m_suppressNavigationTap = false;
  bool m_suppressEdgeBump = false;
  int m_tapIntervalMs = 40;
  int m_edgeIntervalMs = 40;
  qint64 m_lastTapMs = 0;
  qint64 m_lastEdgeMs = 0;
  QElapsedTimer m_clock;
  QMediaDevices m_mediaDevices{this};
  // Set when an output list was empty before any cue reached Ready.
  bool m_unavailableLatched = false;
  bool m_seenReady = false;
  std::unordered_map<QString, std::unique_ptr<QSoundEffect>> m_effects;

#ifdef ARINN_INTERFACE_SOUNDS_TEST
  QString m_testFailNextDecodeName;
  int m_testFailPlayAttempts = 0;
  bool m_testOutputsOverride = false;
  bool m_testOutputsPresent = false;
#endif
};

void registerInterfaceSounds();
