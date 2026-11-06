// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "build_identity.h"
#include "system_about.h"
#include <QClipboard>
#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QScreen>
#include <QLocale>
#include <QTemporaryDir>
#include <QtTest>

namespace {
QVariantMap sample(bool daemon, bool mismatch) {
  const auto shell = QStringLiteral("0.2.0+g8fc320b8e67f");
  const auto daemonId = mismatch ? QStringLiteral("0.2.0+g4676886d1e20") : shell;
  return QVariantMap{
      {QStringLiteral("shellBuildId"), shell},
      {QStringLiteral("shellShort"), QStringLiteral("0.2.0 · 8fc320b8e67f")},
      {QStringLiteral("daemonAvailable"), daemon},
      {QStringLiteral("daemonBuildId"), daemon ? daemonId : QString()},
      {QStringLiteral("daemonShort"),
       daemon ? (mismatch ? QStringLiteral("0.2.0 · 4676886d1e20")
                          : QStringLiteral("0.2.0 · 8fc320b8e67f"))
              : QString()},
      {QStringLiteral("daemonMismatch"), mismatch},
      {QStringLiteral("channelDate"), QStringLiteral("master · Nov 3, 2026")},
      {QStringLiteral("providerDetail"),
       daemon ? QStringLiteral("Library 0.2.0 · Downloads 0.2.0 · Browser 0.2.0")
              : QString()},
      {QStringLiteral("providerCount"), daemon ? 3 : -1},
      {QStringLiteral("osName"), QStringLiteral("Arch Linux")},
      {QStringLiteral("kernel"), QStringLiteral("6.12.113")},
      {QStringLiteral("plasma"), QStringLiteral("6.4.5")},
      {QStringLiteral("qtVersion"), QStringLiteral("6.8.3")},
      {QStringLiteral("session"), QStringLiteral("KDE · Wayland")},
      {QStringLiteral("display"), QStringLiteral("1920 × 1080 · 60 Hz")},
      {QStringLiteral("audio"), QStringLiteral("HDMI 1")},
      {QStringLiteral("connection"), QStringLiteral("Ethernet · enp3s0")},
      {QStringLiteral("ipAddress"), QStringLiteral("203.0.113.24")},
      {QStringLiteral("storage"), QStringLiteral("412 GB of 931 GB")},
      {QStringLiteral("deviceName"), QStringLiteral("Living room")},
  };
}
} // namespace

class SystemAboutTest : public QObject {
  Q_OBJECT
private slots:
  void shortBuildSplitsTheId();
  void channelDateFollowsTheLocale();
  void displayUsesDevicePixels_data();
  void displayUsesDevicePixels();
  void liveDisplayUsesTheScreenScale();
  void reportOmitsPrivateFactsUntilOptIn();
  void reportShowsTheMismatchAndRawIds();
  void reportMarksAMissingDaemon();
  void unknownIsNeverBlankOrADash();
  void copyWritesTheSameText();
  void liveShellStaysAvailableWithoutDaemon();
};

void SystemAboutTest::shortBuildSplitsTheId() {
  QCOMPARE(SystemAbout::shortBuild(QStringLiteral("0.2.0+g8fc320b8e67f")),
           QStringLiteral("0.2.0 · 8fc320b8e67f"));
  QCOMPARE(SystemAbout::shortBuild(QStringLiteral("0.2.0+g8fc320b8e67f-dirty")),
           QStringLiteral("0.2.0 · 8fc320b8e67f"));
  QCOMPARE(SystemAbout::shortBuild(QStringLiteral("0.2.0+g4676886d1e20"),
                                   QStringLiteral("0.2.0")),
           QStringLiteral("0.2.0 · 4676886d1e20"));
  QCOMPARE(SystemAbout::shortBuild(QStringLiteral("0.2.0")), QStringLiteral("0.2.0"));
}

void SystemAboutTest::channelDateFollowsTheLocale() {
  const auto previous = QLocale();
  QLocale::setDefault(QLocale(QLocale::English, QLocale::UnitedStates));
  QCOMPARE(SystemAbout::formatChannelDate(QStringLiteral("master"), QDate(2026, 11, 3)),
           QStringLiteral("master · Nov 3, 2026"));
  QLocale::setDefault(QLocale(QLocale::Russian, QLocale::Russia));
  QCOMPARE(SystemAbout::formatChannelDate(QStringLiteral("master"), QDate(2026, 11, 3)),
           QStringLiteral("master · 3 ноября 2026"));
  QCOMPARE(SystemAbout::formatChannelDate(QString(), QDate(2026, 11, 3)),
           QStringLiteral("3 ноября 2026"));
  QCOMPARE(SystemAbout::formatChannelDate(QStringLiteral("HEAD"), QDate()), QString());
  QLocale::setDefault(previous);
}

void SystemAboutTest::displayUsesDevicePixels_data() {
  QTest::addColumn<QSize>("logical");
  QTest::addColumn<qreal>("scale");
  QTest::addColumn<qreal>("hz");
  QTest::addColumn<QString>("expected");
  QTest::newRow("4k-200-percent") << QSize(1920, 1080) << qreal(2) << qreal(60)
      << QStringLiteral("3840 × 2160 · 60 Hz");
  QTest::newRow("4k-unscaled") << QSize(3840, 2160) << qreal(1) << qreal(59.94)
      << QStringLiteral("3840 × 2160 · 60 Hz");
  QTest::newRow("full-hd-unscaled") << QSize(1920, 1080) << qreal(1) << qreal(60)
      << QStringLiteral("1920 × 1080 · 60 Hz");
  QTest::newRow("4k-150-percent") << QSize(2560, 1440) << qreal(1.5) << qreal(60)
      << QStringLiteral("3840 × 2160 · 60 Hz");
  QTest::newRow("4k-125-percent") << QSize(3072, 1728) << qreal(1.25) << qreal(0)
      << QStringLiteral("3840 × 2160");
  QTest::newRow("fractional-rounding") << QSize(1097, 617) << qreal(1.75) << qreal(60)
      << QStringLiteral("1920 × 1080 · 60 Hz");
  QTest::newRow("missing-screen") << QSize() << qreal(1) << qreal(60) << QString();
  QTest::newRow("invalid-scale") << QSize(1920, 1080) << qreal(0) << qreal(60) << QString();
}

void SystemAboutTest::displayUsesDevicePixels() {
  QFETCH(QSize, logical);
  QFETCH(qreal, scale);
  QFETCH(qreal, hz);
  QFETCH(QString, expected);
  QCOMPARE(SystemAbout::formatDisplay(logical, scale, hz), expected);
}

void SystemAboutTest::liveDisplayUsesTheScreenScale() {
  const auto *screen = QGuiApplication::primaryScreen();
  QVERIFY(screen);
  SystemAbout about;
  QCOMPARE(about.displayText(), SystemAbout::formatDisplay(
      screen->size(), screen->devicePixelRatio(), screen->refreshRate()));
}

void SystemAboutTest::reportOmitsPrivateFactsUntilOptIn() {
  SystemAbout about;
  about.useFixture(sample(true, false));
  const auto quiet = about.report(false);
  QVERIFY(quiet.contains(QStringLiteral("0.2.0+g8fc320b8e67f")));
  QVERIFY(quiet.contains(QStringLiteral("Arch Linux")));
  QVERIFY(quiet.contains(QStringLiteral("412 GB of 931 GB")));
  QVERIFY(!quiet.contains(QStringLiteral("203.0.113.24")));
  QVERIFY(!quiet.contains(QStringLiteral("enp3s0")));
  QVERIFY(!quiet.contains(QStringLiteral("Living room")));
  QVERIFY(!quiet.contains(QStringLiteral("IP address:")));
  QVERIFY(!quiet.contains(QStringLiteral("Device name:")));
  const auto opted = about.report(true);
  QVERIFY(opted.contains(QStringLiteral("203.0.113.24")));
  QVERIFY(opted.contains(QStringLiteral("Ethernet · enp3s0")));
  QVERIFY(opted.contains(QStringLiteral("Living room")));
  QCOMPARE(quiet.contains(QLatin1Char('\n')), true);
}

void SystemAboutTest::reportShowsTheMismatchAndRawIds() {
  SystemAbout about;
  about.useFixture(sample(true, true));
  const auto text = about.report(false);
  QVERIFY(text.contains(QStringLiteral("0.2.0+g8fc320b8e67f")));
  QVERIFY(text.contains(QStringLiteral("0.2.0+g4676886d1e20")));
  QVERIFY(text.contains(QStringLiteral("Different build from Arinn. Restart Arinn")));
  QVERIFY(!text.contains(QStringLiteral("203.0.113.24")));
}

void SystemAboutTest::reportMarksAMissingDaemon() {
  SystemAbout about;
  about.useFixture(sample(false, false));
  const auto text = about.report(false);
  QVERIFY(text.contains(QStringLiteral("0.2.0+g8fc320b8e67f")));
  QVERIFY(text.contains(QStringLiteral("arinnd service: Unavailable")));
  QVERIFY(text.contains(QStringLiteral("The arinnd service is not responding")));
  QVERIFY(text.contains(QStringLiteral("Providers: Unavailable")));
  QVERIFY(text.contains(QStringLiteral("Arch Linux")));
  QVERIFY(!text.contains(QStringLiteral("Different build from Arinn")));
  QVERIFY(!text.contains(QStringLiteral("203.0.113.24")));
}

void SystemAboutTest::unknownIsNeverBlankOrADash() {
  SystemAbout about;
  about.useFixture(QVariantMap{{QStringLiteral("daemonAvailable"), true},
                               {QStringLiteral("providerCount"), -1}});
  const auto text = about.report(true);
  QVERIFY(text.contains(QStringLiteral("Arinn version: Unknown")));
  QVERIFY(text.contains(QStringLiteral("OS: Unknown")));
  QVERIFY(text.contains(QStringLiteral("IP address: Unknown")));
  QVERIFY(text.contains(QStringLiteral("Device name: Unknown")));
  QVERIFY(!text.contains(QStringLiteral(": \n")));
  QVERIFY(!text.contains(QStringLiteral("—")));
  QVERIFY(!text.contains(QStringLiteral("--")));
}

void SystemAboutTest::copyWritesTheSameText() {
  QTemporaryDir dir;
  QVERIFY(dir.isValid());
  SystemAbout about;
  about.useFixture(sample(true, false));
  about.setDownloadsDirectory(dir.path());
  const auto code = about.copyReport(false);
  QVERIFY2(code == 0 || code == 2, qPrintable(QString::number(code)));
  const auto path = dir.filePath(about.fileName());
  QFile file(path);
  QVERIFY2(file.open(QIODevice::ReadOnly), qPrintable(path));
  const auto body = QString::fromUtf8(file.readAll());
  QCOMPARE(body, about.report(false));
  QVERIFY(about.fileName().startsWith(QStringLiteral("arinn-about-")));
  QVERIFY(about.fileName().endsWith(QStringLiteral(".txt")));
  QVERIFY(!body.contains(QStringLiteral("203.0.113.24")));
  if (code == 0) {
    const auto *clip = QGuiApplication::clipboard();
    QVERIFY(clip);
    QCOMPARE(clip->text(), body);
  }
}

void SystemAboutTest::liveShellStaysAvailableWithoutDaemon() {
  SystemAbout about;
  QTRY_VERIFY(!about.shellBuildId().isEmpty());
  QCOMPARE(about.shellBuildId(), BuildIdentity::shell());
  QTRY_VERIFY(!about.daemonAvailable());
  QTemporaryDir dir;
  QVERIFY(dir.isValid());
  about.setDownloadsDirectory(dir.path());
  const auto code = about.copyReport(false);
  QVERIFY(code == 0 || code == 2);
  QFile file(dir.filePath(about.fileName()));
  QVERIFY(file.open(QIODevice::ReadOnly));
  const auto body = QString::fromUtf8(file.readAll());
  QVERIFY(body.contains(BuildIdentity::shell()));
  QVERIFY(body.contains(QStringLiteral("Unavailable")));
  QVERIFY(!body.contains(QStringLiteral("IP address:")));
}

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  SystemAboutTest test;
  return QTest::qExec(&test, argc, argv);
}
#include "system_about_test.moc"
