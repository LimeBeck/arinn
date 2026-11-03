// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "interface_sounds.h"

#include <QCoreApplication>
#include <QSignalSpy>
#include <QTest>

class InterfaceSoundsTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setOutputMuted(false);
    m_sounds.setMediaActive(false);
    for (const char *name :
         {"tap", "edge", "select", "back", "open", "close", "ignite", "kindle",
          "error"}) {
      QSignalSpy ready(&m_sounds, &InterfaceSounds::played);
      for (int attempt = 0; attempt < 300 && ready.isEmpty(); ++attempt) {
        m_sounds.play(QString::fromLatin1(name));
        QCoreApplication::processEvents();
        QTest::qWait(5);
      }
      QVERIFY2(!ready.isEmpty(), name);
    }
  }

  void init() { m_events.clear(); }

  void mediaGuardBlocksEverythingExceptError() {
    m_sounds.setMediaActive(true);
    m_sounds.play(QStringLiteral("tap"));
    m_sounds.play(QStringLiteral("select"));
    QCOMPARE(m_events.size(), 0);
    m_sounds.play(QStringLiteral("error"));
    QCOMPARE(m_events.size(), 1);
    QCOMPARE(m_events.last().first, QStringLiteral("error"));
    m_sounds.setMediaActive(false);
  }

  void levelOffIsSilent() {
    m_sounds.setLevel(QStringLiteral("off"));
    m_sounds.play(QStringLiteral("tap"));
    QCOMPARE(m_events.size(), 0);
    m_sounds.setLevel(QStringLiteral("quiet"));
    m_sounds.play(QStringLiteral("tap"));
    QCOMPARE(m_events.size(), 1);
  }

  void muteSilencesPlayback() {
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setOutputMuted(true);
    m_sounds.play(QStringLiteral("tap"));
    QCOMPARE(m_events.size(), 0);
    m_sounds.setOutputMuted(false);
    m_sounds.play(QStringLiteral("tap"));
    QCOMPARE(m_events.size(), 1);
  }

  void tapThrottleAndRepeatGain() {
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setOutputMuted(false);
    m_sounds.setTapIntervalMs(40);
    m_sounds.play(QStringLiteral("tap"));
    QCOMPARE(m_events.size(), 1);
    QCOMPARE(m_events.last().second, false);
    m_sounds.play(QStringLiteral("tap"));
    QCOMPARE(m_events.size(), 1);
    QTest::qWait(45);
    m_sounds.play(QStringLiteral("tap"), true);
    QCOMPARE(m_events.size(), 2);
    QCOMPARE(m_events.last().second, true);
  }

private:
  InterfaceSounds m_sounds;
  QList<QPair<QString, bool>> m_events;

  InterfaceSoundsTest() {
    connect(&m_sounds, &InterfaceSounds::played, this,
            [this](const QString &name, bool repeat) {
              m_events.append({name, repeat});
            });
  }
};

QTEST_MAIN(InterfaceSoundsTest)
#include "interface_sounds_test.moc"
