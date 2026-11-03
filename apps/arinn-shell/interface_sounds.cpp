// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "interface_sounds.h"

#include <QCoreApplication>
#include <QEventLoop>
#include <QFile>
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

QUrl sourceUrlFor(const QString &name) {
#ifdef ARINN_SOUNDS_SOURCE_DIR
  const QString path =
      QString(ARINN_SOUNDS_SOURCE_DIR) + QLatin1Char('/') + name + QStringLiteral(".wav");
  if (QFile::exists(path))
    return QUrl::fromLocalFile(path);
#endif
  const char *url = resourceFor(name);
  return url ? QUrl(QString::fromLatin1(url)) : QUrl();
}
} // namespace

InterfaceSounds::InterfaceSounds(QObject *parent) : QObject(parent) {
  g_instance = this;
  m_clock.start();
}

InterfaceSounds::~InterfaceSounds() {
  if (g_instance == this)
    g_instance = nullptr;
}

InterfaceSounds *InterfaceSounds::instance() { return g_instance; }

void InterfaceSounds::setLevel(const QString &level) {
  const auto normalized = level == QLatin1String("off") ||
                                  level == QLatin1String("quiet") ||
                                  level == QLatin1String("normal")
                              ? level
                              : QStringLiteral("quiet");
  if (m_level == normalized)
    return;
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
  if (m_level == QLatin1String("off") || m_outputMuted)
    return 0.0;
  const double base =
      m_level == QLatin1String("normal") ? 0.75 : 0.35;
  return base * (repeat ? 0.5 : 1.0);
}

QSoundEffect *InterfaceSounds::effectFor(const QString &name) {
  const auto found = m_effects.find(name);
  if (found != m_effects.end())
    return found->second.get();
  const QUrl url = sourceUrlFor(name);
  if (url.isEmpty())
    return nullptr;
  auto effect = std::make_unique<QSoundEffect>(this);
  effect->setSource(url);
  effect->setLoopCount(1);
  auto *raw = effect.get();
  m_effects.emplace(name, std::move(effect));
  return raw;
}

bool InterfaceSounds::ensureLoaded(const QString &name) {
  auto *effect = effectFor(name);
  if (!effect)
    return false;
  if (effect->isLoaded())
    return true;
  for (int i = 0; i < 1000 && effect->status() != QSoundEffect::Ready; ++i) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
  }
  return effect->isLoaded();
}

void InterfaceSounds::play(const QString &name, bool repeat) {
  if (m_level == QLatin1String("off") || m_outputMuted)
    return;
  if (m_mediaActive && name != QLatin1String("error"))
    return;
  if (name == QLatin1String("tap")) {
    const qint64 now = m_clock.elapsed();
    if (now - m_lastTapMs < m_tapIntervalMs)
      return;
    m_lastTapMs = now;
  }
  auto *effect = effectFor(name);
  if (!effect || !effect->isLoaded())
    return;
  effect->setVolume(static_cast<float>(volume(repeat)));
  effect->play();
  emit played(name, repeat);
}

void registerInterfaceSounds() {
  qmlRegisterSingletonType<InterfaceSounds>(
      "Arinn.Shell", 1, 0, "InterfaceSounds",
      [](QQmlEngine *, QJSEngine *) -> QObject * {
        if (!InterfaceSounds::instance()) {
          // The shell constructs one instance before loading QML; tests may
          // register the type without constructing it yet.
          static InterfaceSounds fallback;
          return &fallback;
        }
        return InterfaceSounds::instance();
      });
}

Q_COREAPP_STARTUP_FUNCTION(registerInterfaceSounds)
