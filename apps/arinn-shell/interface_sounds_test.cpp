// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "interface_sounds.h"

#include <QAudioDevice>
#include <QGuiApplication>
#include <QMediaDevices>
#include <QSoundEffect>
#include <QTest>

class InterfaceSoundsTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    connect(&m_sounds, &InterfaceSounds::played, this,
            [this](const QString &name, bool repeat) {
              m_events.append({name, repeat});
            });
  }

  void init() {
    InterfaceSounds::clearTestAudioOutputs();
    m_events.clear();
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setOutputMuted(false);
    m_sounds.setMediaActive(false);
    m_sounds.setTapIntervalMs(0);
    m_sounds.resetTapThrottle();
    m_sounds.resetEdgeThrottle();
  }

  void cleanup() { InterfaceSounds::clearTestAudioOutputs(); }

  static bool hasAudioOutput() {
    return !QMediaDevices::audioOutputs().isEmpty();
  }

  void noSoundEffectsUntilAudioOutputAndLoadOrPlay() {
    InterfaceSounds fresh;
    QCOMPARE(fresh.available(), hasAudioOutput());
    QCOMPARE(m_sounds.available(), hasAudioOutput());
    QCOMPARE(fresh.effectStatus(QStringLiteral("tap")),
             static_cast<int>(QSoundEffect::Null));
    QCOMPARE(fresh.effectStatus(QStringLiteral("open")),
             static_cast<int>(QSoundEffect::Null));
    if (!hasAudioOutput())
      return;
    QVERIFY(fresh.ensureLoaded(QStringLiteral("tap")));
    QCOMPARE(fresh.effectStatus(QStringLiteral("tap")),
             static_cast<int>(QSoundEffect::Ready));
    QCOMPARE(fresh.effectStatus(QStringLiteral("open")),
             static_cast<int>(QSoundEffect::Null));
  }

  void coldStartStaysUnavailableAfterOutputsAppear() {
    InterfaceSounds::setTestAudioOutputsPresent(false);
    InterfaceSounds cold;
    QVERIFY(!cold.available());
    cold.useTestAudioOutputs(true);
    QVERIFY(!cold.available());
    cold.setLevel(QStringLiteral("normal"));
    cold.play(QStringLiteral("tap"));
    QCOMPARE(cold.effectStatus(QStringLiteral("tap")),
             static_cast<int>(QSoundEffect::Null));
  }

  void outputsLostBeforeReadyStayUnavailable() {
    InterfaceSounds::setTestAudioOutputsPresent(true);
    InterfaceSounds warm;
    QVERIFY(warm.available());
    warm.useTestAudioOutputs(false);
    QVERIFY(!warm.available());
    warm.useTestAudioOutputs(true);
    QVERIFY(!warm.available());
    warm.setLevel(QStringLiteral("normal"));
    warm.play(QStringLiteral("open"));
    QCOMPARE(warm.effectStatus(QStringLiteral("open")),
             static_cast<int>(QSoundEffect::Null));
  }

  void embeddedResourcesExistAndDecode() {
    const char *names[] = {"tap",  "edge",   "select", "back", "open",
                           "close", "ignite", "kindle", "error"};
    for (const char *name : names) {
      const QString id = QString::fromLatin1(name);
      QVERIFY2(m_sounds.resourceEmbedded(id), name);
      QVERIFY2(m_sounds.ensureLoaded(id) || !hasAudioOutput(), name);
      if (hasAudioOutput())
        QCOMPARE(m_sounds.effectStatus(id),
                 static_cast<int>(QSoundEffect::Ready));
    }
  }

  void igniteSilentUntilLevelKnownAndWhenOff() {
    InterfaceSounds fresh;
    connect(&fresh, &InterfaceSounds::played, this,
            [this](const QString &name, bool repeat) {
              m_events.append({name, repeat});
            });
    m_events.clear();
    QVERIFY(!fresh.levelKnown());
    fresh.play(QStringLiteral("ignite"));
    QCOMPARE(m_events.size(), 0);
    fresh.setLevel(QStringLiteral("off"));
    QVERIFY(fresh.levelKnown());
    fresh.play(QStringLiteral("ignite"));
    QCOMPARE(m_events.size(), 0);
    fresh.setLevel(QStringLiteral("quiet"));
    fresh.play(QStringLiteral("ignite"));
    if (!hasAudioOutput())
      QSKIP("No audio output device");
    QCOMPARE(m_events.size(), 1);
    QCOMPARE(m_events.last().first, QStringLiteral("ignite"));
  }

  void openSuppressesEdgeInSameStep() {
    if (!hasAudioOutput())
      QSKIP("No audio output device");
    m_sounds.setLevel(QStringLiteral("normal"));
    QVERIFY(!m_sounds.playEdgeBumpWhileSuppressed(false));
    QCOMPARE(m_events.size(), 0);
    m_sounds.playOpen();
    QCOMPARE(m_events.size(), 1);
    QCOMPARE(m_events.last().first, QStringLiteral("open"));
  }

  void mediaGuardBlocksEverythingExceptError() {
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setMediaActive(true);
    m_sounds.playNavigationTap(false);
    m_sounds.play(QStringLiteral("select"));
    QCOMPARE(m_events.size(), 0);
    m_sounds.play(QStringLiteral("error"));
    if (!hasAudioOutput()) {
      QCOMPARE(m_events.size(), 0);
    } else {
      QCOMPARE(m_events.size(), 1);
      QCOMPARE(m_events.last().first, QStringLiteral("error"));
    }
    m_sounds.setMediaActive(false);
  }

  void levelOffIsSilent() {
    m_sounds.setLevel(QStringLiteral("off"));
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.size(), 0);
    m_sounds.setLevel(QStringLiteral("quiet"));
    m_sounds.playNavigationTap(false);
    if (!hasAudioOutput()) {
      QCOMPARE(m_events.size(), 0);
      return;
    }
    QCOMPARE(m_events.size(), 1);
  }

  void muteSilencesPlayback() {
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setOutputMuted(true);
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.size(), 0);
    m_sounds.setOutputMuted(false);
    m_sounds.playNavigationTap(false);
    if (!hasAudioOutput()) {
      QCOMPARE(m_events.size(), 0);
      return;
    }
    QCOMPARE(m_events.size(), 1);
  }

  void navigationTapThrottleAndAutoRepeatFlag() {
    if (!hasAudioOutput())
      QSKIP("No audio output device");
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setTapIntervalMs(40);
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.size(), 1);
    QCOMPARE(m_events.last().second, false);
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.size(), 1);
    QTest::qWait(45);
    m_sounds.playNavigationTap(true);
    QCOMPARE(m_events.size(), 2);
    QCOMPARE(m_events.last().second, true);
  }

  void edgeBumpOncePerPhysicalPress() {
    if (!hasAudioOutput())
      QSKIP("No audio output device");
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.playEdgeBump(false);
    QCOMPARE(m_events.size(), 1);
    QCOMPARE(m_events.last().first, QStringLiteral("edge"));
    m_sounds.playEdgeBump(true);
    QCOMPARE(m_events.size(), 1);
    m_sounds.resetEdgeThrottle();
    m_sounds.setTapIntervalMs(40);
    m_sounds.playEdgeBump(false);
    QCOMPARE(m_events.size(), 2);
    m_sounds.playEdgeBump(false);
    QCOMPARE(m_events.size(), 2);
  }

  void openBackCloseSuppressNavigationTap() {
    if (!hasAudioOutput())
      QSKIP("No audio output device");
    m_sounds.setLevel(QStringLiteral("normal"));
    QVERIFY(!m_sounds.playNavigationTapWhileSuppressed(false));
    m_sounds.playOpen();
    QCOMPARE(m_events.last().first, QStringLiteral("open"));
    m_sounds.playBack();
    QCOMPARE(m_events.last().first, QStringLiteral("back"));
    m_sounds.playClose();
    QCOMPARE(m_events.last().first, QStringLiteral("close"));
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.last().first, QStringLiteral("tap"));
  }

  void playRetriesAfterFailedDecodeInSameCall() {
    if (!hasAudioOutput())
      QSKIP("No audio output device");
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setTestFailNextDecodeOnce(QStringLiteral("tap"));
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.size(), 1);
    QCOMPARE(m_events.last().first, QStringLiteral("tap"));
  }

  void lateAudioRecoverOnSecondPlayWithoutOutputSignal() {
    if (!hasAudioOutput())
      QSKIP("No audio output device");
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setTestFailPlayAttempts(1);
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.size(), 0);
    m_sounds.playNavigationTap(false);
    QCOMPARE(m_events.size(), 1);
  }

private:
  InterfaceSounds m_sounds;
  QList<QPair<QString, bool>> m_events;
};

int main(int argc, char **argv) {
  Q_INIT_RESOURCE(ui_sounds);
  QGuiApplication app(argc, argv);
  InterfaceSoundsTest test;
  return QTest::qExec(&test, argc, argv);
}

#include "interface_sounds_test.moc"
