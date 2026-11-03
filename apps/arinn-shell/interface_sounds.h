// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once
#include <QElapsedTimer>
#include <QHash>
#include <QObject>
#include <QString>
#include <memory>

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

  Q_INVOKABLE void play(const QString &name, bool repeat = false);

  // Tests shorten tap spacing without waiting in real time.
  void setTapIntervalMs(int ms) { m_tapIntervalMs = ms; }

signals:
  void levelChanged();
  void outputMutedChanged();
  void mediaActiveChanged();
  // Emitted once per accepted play() call (including error over media).
  void played(const QString &name, bool repeat);

private:
  double volume(bool repeat) const;
  QSoundEffect *effectFor(const QString &name);

  QString m_level = QStringLiteral("quiet");
  bool m_outputMuted = false;
  bool m_mediaActive = false;
  int m_tapIntervalMs = 40;
  qint64 m_lastTapMs = 0;
  QElapsedTimer m_clock;
  QHash<QString, std::unique_ptr<QSoundEffect>> m_effects;
};

void registerInterfaceSounds();
