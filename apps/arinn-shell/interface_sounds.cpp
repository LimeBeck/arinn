// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "interface_sounds.h"

#include <QCoreApplication>
#include <QEventLoop>
#include <QAudioDevice>
#include <QMediaDevices>
#include <QResource>
#include <QSoundEffect>
#include <QQmlEngine>

namespace {
InterfaceSounds *g_instance = nullptr;

const char *resourceFor(const QString &name) {
  if (name == QLatin1String("tap"))
    return "qrc:/sounds/tap.wav";
  if (name == QLatin1String("edge"))
    return "qrc:/sounds/edge.wav";
  if (name == QLatin1String("select"))
    return "qrc:/sounds/select.wav";
  if (name == QLatin1String("back"))
    return "qrc:/sounds/back.wav";
  if (name == QLatin1String("open"))
    return "qrc:/sounds/open.wav";
  if (name == QLatin1String("close"))
    return "qrc:/sounds/close.wav";
  if (name == QLatin1String("ignite"))
    return "qrc:/sounds/ignite.wav";
  if (name == QLatin1String("kindle"))
    return "qrc:/sounds/kindle.wav";
  if (name == QLatin1String("error"))
    return "qrc:/sounds/error.wav";
  return nullptr;
}

const char *kAllSounds[] = {"tap",  "edge",   "select", "back", "open",
                              "close", "ignite", "kindle", "error"};
} // namespace

InterfaceSounds::InterfaceSounds(QObject *parent) : QObject(parent) {
  g_instance = this;
  m_clock.start();
  preloadAllEffects();
  connect(&m_mediaDevices, &QMediaDevices::audioOutputsChanged, this,
          &InterfaceSounds::onAudioOutputsChanged);
}

InterfaceSounds::~InterfaceSounds() {
  if (g_instance == this)
    g_instance = nullptr;
}

InterfaceSounds *InterfaceSounds::instance() { return g_instance; }

void InterfaceSounds::onAudioOutputsChanged() {
  if (QMediaDevices::audioOutputs().isEmpty())
    return;
  resetAllEffects();
  preloadAllEffects();
}

void InterfaceSounds::resetAllEffects() { m_effects.clear(); }

void InterfaceSounds::preloadAllEffects() {
  for (const char *name : kAllSounds)
    waitForEffectDecode(QString::fromLatin1(name));
}

void InterfaceSounds::setLevel(const QString &level) {
  if (level.isEmpty()) {
    if (!m_levelKnown && m_level.isEmpty())
      return;
    m_levelKnown = false;
    m_level.clear();
    emit levelChanged();
    return;
  }
  const auto normalized = level == QLatin1String("off") ||
                                  level == QLatin1String("quiet") ||
                                  level == QLatin1String("normal")
                              ? level
                              : QStringLiteral("quiet");
  if (m_levelKnown && m_level == normalized)
    return;
  m_levelKnown = true;
  m_level = normalized;
  emit levelChanged();
}

void InterfaceSounds::setOutputMuted(bool muted) {
  if (m_outputMuted == muted)
    return;
  m_outputMuted = muted;
  emit outputMutedChanged();
}

void InterfaceSounds::setMediaActive(bool active) {
  if (m_mediaActive == active)
    return;
  m_mediaActive = active;
  emit mediaActiveChanged();
}

double InterfaceSounds::volume(bool repeat) const {
  if (!m_levelKnown || m_level == QLatin1String("off") || m_outputMuted)
    return 0.0;
  const double base = m_level == QLatin1String("normal") ? 0.75 : 0.35;
  return base * (repeat ? 0.5 : 1.0);
}

QSoundEffect *InterfaceSounds::effectFor(const QString &name) {
  const auto found = m_effects.find(name);
  if (found != m_effects.end())
    return found->second.get();
  const char *url = resourceFor(name);
  if (!url)
    return nullptr;
  auto effect = std::make_unique<QSoundEffect>(this);
  effect->setSource(QUrl(QString::fromLatin1(url)));
  effect->setLoopCount(1);
  auto *raw = effect.get();
  m_effects.emplace(name, std::move(effect));
  return raw;
}

bool InterfaceSounds::resourceEmbedded(const QString &name) const {
  return QResource(
             QStringLiteral(":/sounds/%1.wav").arg(name).toUtf8())
      .isValid();
}

bool InterfaceSounds::waitForEffectDecode(const QString &name) {
  auto *effect = effectFor(name);
  if (!effect)
    return false;
  for (int i = 0; i < 1000; ++i) {
    const auto status = effect->status();
    if (status != QSoundEffect::Loading && status != QSoundEffect::Null)
      return true;
    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
  }
  return effect->status() != QSoundEffect::Loading &&
         effect->status() != QSoundEffect::Null;
}

bool InterfaceSounds::ensureLoaded(const QString &name) {
  if (!resourceEmbedded(name) || !waitForEffectDecode(name))
    return false;
  auto *effect = effectFor(name);
  if (!effect)
    return false;
  if (effect->status() == QSoundEffect::Ready)
    return true;
  return QMediaDevices::audioOutputs().isEmpty() &&
         effect->status() != QSoundEffect::Loading;
}

int InterfaceSounds::effectStatus(const QString &name) const {
  const auto found = m_effects.find(name);
  if (found == m_effects.end())
    return static_cast<int>(QSoundEffect::Null);
  return static_cast<int>(found->second->status());
}

bool InterfaceSounds::playInternal(const QString &name, bool repeat,
                                   bool throttleTap, bool throttleEdge) {
  if (!m_levelKnown || m_level == QLatin1String("off") || m_outputMuted)
    return false;
  if (m_mediaActive && name != QLatin1String("error"))
    return false;
  if (name == QLatin1String("tap") && m_suppressNavigationTap)
    return false;
  if (name == QLatin1String("edge") && m_suppressEdgeBump)
    return false;
  if (throttleTap && name == QLatin1String("tap") && m_tapIntervalMs > 0) {
    const qint64 now = m_clock.elapsed();
    if (m_lastTapMs != 0 && now - m_lastTapMs < m_tapIntervalMs)
      return false;
  }
  if (throttleEdge && name == QLatin1String("edge") && m_edgeIntervalMs > 0) {
    const qint64 now = m_clock.elapsed();
    if (m_lastEdgeMs != 0 && now - m_lastEdgeMs < m_edgeIntervalMs)
      return false;
  }
  if (!waitForEffectDecode(name))
    return false;
  auto *effect = effectFor(name);
  if (!effect || effect->status() != QSoundEffect::Ready) {
    if (!QMediaDevices::audioOutputs().isEmpty())
      resetAllEffects();
    return false;
  }
  effect->setVolume(static_cast<float>(volume(repeat)));
  effect->play();
  emit played(name, repeat);
  if (name == QLatin1String("tap"))
    m_lastTapMs = m_clock.elapsed();
  if (name == QLatin1String("edge"))
    m_lastEdgeMs = m_clock.elapsed();
  return true;
}

void InterfaceSounds::play(const QString &name, bool repeat) {
  playInternal(name, repeat, name == QLatin1String("tap"),
               name == QLatin1String("edge"));
}

void InterfaceSounds::playNavigationTap(bool autoRepeat) {
  playInternal(QStringLiteral("tap"), autoRepeat, true, false);
}

void InterfaceSounds::playEdgeBump(bool autoRepeat) {
  if (autoRepeat)
    return;
  playInternal(QStringLiteral("edge"), false, false, true);
}

void InterfaceSounds::playOpen() {
  m_suppressNavigationTap = true;
  m_suppressEdgeBump = true;
  playInternal(QStringLiteral("open"), false, false, false);
  m_suppressNavigationTap = false;
  m_suppressEdgeBump = false;
}

void InterfaceSounds::playClose() {
  m_suppressNavigationTap = true;
  m_suppressEdgeBump = true;
  playInternal(QStringLiteral("close"), false, false, false);
  m_suppressNavigationTap = false;
  m_suppressEdgeBump = false;
}

void InterfaceSounds::playBack() {
  m_suppressNavigationTap = true;
  m_suppressEdgeBump = true;
  playInternal(QStringLiteral("back"), false, false, false);
  m_suppressNavigationTap = false;
  m_suppressEdgeBump = false;
}

#ifdef ARINN_INTERFACE_SOUNDS_TEST
bool InterfaceSounds::playNavigationTapWhileSuppressed(bool autoRepeat) {
  m_suppressNavigationTap = true;
  const bool played =
      playInternal(QStringLiteral("tap"), autoRepeat, true, false);
  m_suppressNavigationTap = false;
  return played;
}

bool InterfaceSounds::playEdgeBumpWhileSuppressed(bool autoRepeat) {
  m_suppressEdgeBump = true;
  const bool played = playInternal(QStringLiteral("edge"), false, false, true);
  m_suppressEdgeBump = false;
  Q_UNUSED(autoRepeat);
  return played;
}

void InterfaceSounds::recreateEffectsForTest() { onAudioOutputsChanged(); }
#endif

void registerInterfaceSounds() {
  qmlRegisterSingletonType<InterfaceSounds>(
      "Arinn.Shell", 1, 0, "InterfaceSounds",
      [](QQmlEngine *, QJSEngine *) -> QObject * {
        if (!InterfaceSounds::instance()) {
          static InterfaceSounds fallback;
          return &fallback;
        }
        return InterfaceSounds::instance();
      });
}

Q_COREAPP_STARTUP_FUNCTION(registerInterfaceSounds)
