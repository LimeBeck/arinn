// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "interface_sounds.h"

#include <QGuiApplication>
#include <QSignalSpy>
#include <QTest>

class InterfaceSoundsTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    connect(&m_sounds, &InterfaceSounds::played, this,
            [this](const QString &name, bool repeat) {
              m_events.append({name, repeat});
            });
    m_sounds.setLevel(QStringLiteral("normal"));
    m_sounds.setOutputMuted(false);
    m_sounds.setMediaActive(false);
  }

  void init() {
    m_events.clear();
    m_sounds.setTapIntervalMs(0);
    m_sounds.resetTapThrottle();
  }

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
};

int main(int argc, char **argv) {
  Q_INIT_RESOURCE(ui_sounds);
  QGuiApplication app(argc, argv);
  InterfaceSoundsTest test;
  return QTest::qExec(&test, argc, argv);
}

#include "interface_sounds_test.moc"
