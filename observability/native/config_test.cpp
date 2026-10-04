// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "telemetry.h"
#include <QFile>
#include <QtTest>

class ConfigurationTest : public QObject {
  Q_OBJECT
private slots:
  void publicDefaultAndOverrides() {
    // No startup hook or SDK initialization is linked into this test process.
    QFile source(QStringLiteral(ARINN_DSN_SOURCE));
    QVERIFY(source.open(QIODevice::ReadOnly));
    QByteArray expected;
    for (auto line : source.readAll().split('\n'))
      if (line.startsWith("https://")) expected = line;
    QVERIFY(!expected.isEmpty());
    QCOMPARE(ArinnTelemetry::resolveDsn(std::nullopt, std::nullopt), expected);
    QCOMPARE(ArinnTelemetry::resolveDsn(std::nullopt, QByteArray("configured")), QByteArray("configured"));
    QCOMPARE(ArinnTelemetry::resolveDsn(QByteArray("environment"), QByteArray("configured")), QByteArray("environment"));
    QCOMPARE(ArinnTelemetry::resolveDsn(QByteArray(""), std::nullopt), QByteArray(""));
    QCOMPARE(ArinnTelemetry::resolveDsn(std::nullopt, QByteArray("")), QByteArray(""));
  }
};
QTEST_GUILESS_MAIN(ConfigurationTest)
#include "config_test.moc"
