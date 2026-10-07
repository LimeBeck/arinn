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
  void qmlMessagesBecomeTemplates() {
    QMessageLogContext context("qrc:/qt/qml/Arinn/ui/HomeView.qml", 0, nullptr, "qml");
    auto shape = ArinnTelemetry::qtTemplate(context,
      QStringLiteral("qrc:/qt/qml/Arinn/ui/HomeView.qml:42:7: QML Image: Cannot open: file:///home/private-fixture/Movies/Private%20Title.mkv"));
    QCOMPARE(shape.location, QByteArray("HomeView.qml:42"));
    QCOMPARE(shape.category, QByteArray("qml"));
    QCOMPARE(shape.text, QByteArray("QML Image: Cannot open: <url>"));
    QVERIFY(!shape.benign);
    // C++ scopes are not URLs.
    QCOMPARE(ArinnTelemetry::qtTemplate(context, QStringLiteral("QQuickItem::stackAfter: Cannot stack")).text,
             QByteArray("QQuickItem::stackAfter: Cannot stack"));
  }
  void privateFragmentsAreRemoved() {
    QMessageLogContext none;
    const QString fixtures[] = {
      QStringLiteral("Playing \"Private Title\" from /home/private-fixture/Videos at 1:23:45"),
      QStringLiteral("Tab 'private-token-fixture' https://private.invalid/watch?v=42 closed"),
      QStringLiteral("Opened ~/Videos/private-fixture.mkv and ./Private.mp4"),
      QStringLiteral("Фильм «Приватное название» недоступен"),
      QStringLiteral("magnet:?xt=urn:btih:0123 data:image/png;base64,AAAA user@private.invalid"),
      QStringLiteral("Unterminated \"Private Title and more"),
      QStringLiteral("Missing </home/private-fixture/Private> and {/home/private-fixture/Videos}"),
      QStringLiteral("Bad -/home/private-fixture/Videos or private-fixture/Videos/Title"),
      QStringLiteral("Windows C:\\Users\\private-fixture\\Videos path"),
      QStringLiteral("Query key=Private&title=Private#Title"),
      QStringLiteral("Opaque urn:arinn:PrivateTitle and mailto:private-fixture"),
    };
    for (const auto &fixture : fixtures) {
      const auto shape = ArinnTelemetry::qtTemplate(none, fixture);
      for (auto marker : {"rivate", "Videos", "Title", "mkv", "mp4", "http", "magnet", "base64", "@", "/", "1", "4"})
        QVERIFY2(!shape.text.contains(marker), shape.text.constData());
      QVERIFY2(ArinnTelemetry::safeTemplateText(shape.text.constData()), shape.text.constData());
      QVERIFY(shape.location.isEmpty());
    }
  }
  void userPathsAreNotLocations() {
    QMessageLogContext none;
    for (auto message : {"file:///home/private-fixture/Main.qml:3: x", "/home/private-fixture/a.qml:3: x",
                         "file:///home/private-fixture/PrivateFile.cpp:42: x", "/home/private-fixture/Private.h:3: x"})
      QVERIFY(ArinnTelemetry::qtTemplate(none, QString::fromLatin1(message)).location.isEmpty());
    QMessageLogContext url("file:///home/private-fixture/PrivateFile.cpp", 7, nullptr, "default");
    QVERIFY(ArinnTelemetry::qtTemplate(url, QStringLiteral("x")).location.isEmpty());
    QMessageLogContext code("/build/private-fixture/apps/arinn-shell/backend.cpp", 12, nullptr, "default");
    const auto shape = ArinnTelemetry::qtTemplate(code, QStringLiteral("Reply failed"));
    QCOMPARE(shape.location, QByteArray("backend.cpp:12"));
    QVERIFY(shape.category.isEmpty());
  }
  void knownNoiseIsBenign() {
    QMessageLogContext none;
    for (auto message : {"qrc:/qt/qml/Arinn/ui/Card.qml:9:5: QML Image: Icon is unavailable",
                         "qrc:/qt/qml/Arinn/ui/Card.qml:9:5: QML Image: Site icon is unavailable",
                         "QWaylandDataOffer: timeout reading from pipe"})
      QVERIFY2(ArinnTelemetry::qtTemplate(none, QString::fromLatin1(message)).benign, message);
    QVERIFY(!ArinnTelemetry::qtTemplate(none, QStringLiteral("Binding loop detected")).benign);
  }
  void identicalTemplatesAreRateLimited() {
    QMessageLogContext none;
    const auto first = ArinnTelemetry::qtTemplate(none, QStringLiteral("Lost item 12 at /tmp/a"));
    const auto second = ArinnTelemetry::qtTemplate(none, QStringLiteral("Lost item 99 at /tmp/b"));
    QCOMPARE(first.text, second.text);
    const QByteArray key = "QtWarning\n" + first.location + '\n' + first.text;
    QVERIFY(ArinnTelemetry::admitTemplate(key, 1000));
    QVERIFY(!ArinnTelemetry::admitTemplate(key, 2000));
    QVERIFY(ArinnTelemetry::admitTemplate(key, 1000 + 10 * 60 * 1000));
    QVERIFY(ArinnTelemetry::admitTemplate("QtWarning\n\nother", 2000));
  }
  void crashSafeChecks() {
    QVERIFY(ArinnTelemetry::safeTemplateText("QML Image: Cannot open: <url>"));
    for (auto text : {"", "a/b", "quote\"", "it's", "n=1", "digit 7", "\xd0\xa4"})
      QVERIFY2(!ArinnTelemetry::safeTemplateText(text), text);
    QVERIFY(ArinnTelemetry::safeLocation("HomeView.qml:42"));
    QVERIFY(!ArinnTelemetry::safeLocation("/home/x.qml"));
    QVERIFY(!ArinnTelemetry::safeLocation("Main"));
    QVERIFY(ArinnTelemetry::safeCategory("qt.qpa.wayland"));
    QVERIFY(!ArinnTelemetry::safeCategory("Private Title"));
  }
};
QTEST_GUILESS_MAIN(ConfigurationTest)
#include "config_test.moc"
