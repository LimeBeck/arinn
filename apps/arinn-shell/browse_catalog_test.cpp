// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "browse_catalog.h"
#include <QtDBus>
#include <QtTest>

struct Resolution {
  QString kind, url, engine;
};
Q_DECLARE_METATYPE(Resolution)
QDBusArgument &operator<<(QDBusArgument &a, const Resolution &v) {
  a.beginStructure();
  a << v.kind << v.url << v.engine;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, Resolution &v) {
  a.beginStructure();
  a >> v.kind >> v.url >> v.engine;
  a.endStructure();
  return a;
}
struct Effect {
  QString provider, capability, id;
};
Q_DECLARE_METATYPE(Effect)
QDBusArgument &operator<<(QDBusArgument &a, const Effect &v) {
  a.beginStructure();
  a << v.provider << v.capability << v.id;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, Effect &) { return a; }
struct Result {
  QString state, error;
};
Q_DECLARE_METATYPE(Result)
QDBusArgument &operator<<(QDBusArgument &a, const Result &v) {
  a.beginStructure();
  a << v.state << v.error << false << qulonglong(0) << qulonglong(0)
    << QList<Effect>{} << false;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, Result &) { return a; }
class FakeBrowse : public QDBusVirtualObject {
public:
  QStringList recent{"old.example"};
  QString opened;
  int opens = 0;
  QString state = "succeeded", failure;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &m,
                     const QDBusConnection &bus) override {
    if (m.member() == "GetRecent")
      bus.send(m.createReply(QVariant::fromValue(recent)));
    else if (m.member() == "ClearRecent") {
      recent.clear();
      bus.send(m.createReply());
    } else if (m.member() == "Resolve") {
      const auto text = m.arguments().at(0).toString();
      Resolution r{text == "invalid" ? "invalid" : "address", "https://" + text,
                   "Packaged engine"};
      QTimer::singleShot(text == "slow.example" ? 150 : 0, this, [bus, m, r] {
        bus.send(m.createReply(QVariant::fromValue(r)));
      });
    } else if (m.member() == "Open") {
      ++opens;
      opened = m.arguments().at(0).toString();
      recent.prepend(opened);
      bus.send(
          m.createReply(QVariant::fromValue(QDBusObjectPath("/operation"))));
    } else if (m.member() == "Get")
      bus.send(m.createReply(QVariant::fromValue(Result{state, failure})));
    else
      bus.send(m.createErrorReply("org.arinn.Error.Unavailable",
                                  "Fixture unavailable"));
    return true;
  }
};
class BrowseTest : public QObject {
  Q_OBJECT
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<Resolution>();
    qDBusRegisterMetaType<Effect>();
    qDBusRegisterMetaType<QList<Effect>>();
    qDBusRegisterMetaType<Result>();
    auto bus = QDBusConnection::connectToBus(
        qEnvironmentVariable("DBUS_SESSION_BUS_ADDRESS"), "browse-fixture");
    QVERIFY(bus.registerService("org.arinn.Daemon1"));
    QVERIFY(bus.registerVirtualObject("/org/arinn/Daemon1", &fake));
    QVERIFY(bus.registerVirtualObject("/operation", &fake));
  }
  void resolutionRaceAndOpen() {
    BrowseCatalog model;
    QTRY_COMPARE(model.recent().size(), 1);
    model.setText("slow.example");
    model.setText("latest.example");
    QTRY_COMPARE(model.url(), QString("https://latest.example"));
    QTest::qWait(180);
    QCOMPARE(model.url(), QString("https://latest.example"));
    model.setTabs({QVariantMap{{"title", "Already open"},
                               {"url", "https://www.latest.example/path"},
                               {"active", true}}});
    QCOMPARE(model.openTitle(), QString("Already open"));
    model.setTabs({QVariantMap{{"title", "Wrong scheme"},
                               {"url", "http://latest.example"},
                               {"active", true}}});
    QVERIFY(model.openTitle().isEmpty());
    model.open();
    model.open();
    QTRY_COMPARE(model.operationState(), QString("succeeded"));
    QCOMPARE(fake.opens, 1);
    QCOMPARE(fake.opened, QString("latest.example"));
    QTRY_COMPARE(model.recent().first(), QString("latest.example"));
    model.clearRecent();
    QTRY_VERIFY(model.recent().isEmpty());
  }
  void invalidAndResetRejectPendingReplies() {
    BrowseCatalog model;
    model.setText("invalid");
    QTRY_COMPARE(model.kind(), QString("invalid"));
    const int before = fake.opens;
    model.open();
    QTest::qWait(50);
    QCOMPARE(fake.opens, before);
    model.setText("slow.example");
    model.reset();
    QTest::qWait(180);
    QVERIFY(model.text().isEmpty());
    QVERIFY(model.url().isEmpty());
    QVERIFY(model.kind().isEmpty());
  }
  // ADR-0103: recent entries, then open services, up to four; never web
  // shortcuts.
  void suggestionsAreRecentEntriesThenOpenServices() {
    fake.recent = {"media query", "old.example", "news.example/today",
                   "third.example"};
    BrowseCatalog model;
    QTRY_COMPARE(model.recent().size(), 4);
    model.setTabs({QVariantMap{{"title", "Media Library"},
                               {"url", "https://media.example/path?x=1"},
                               {"active", true}},
                   QVariantMap{{"title", "Background tab"},
                               {"url", "https://hidden.example/"},
                               {"active", false}},
                   QVariantMap{{"title", ""},
                               {"url", "http://nas.local:8096/web"},
                               {"active", true}}});
    // An empty field: recent entries first, four in total.
    auto rows = model.suggestions();
    QCOMPARE(rows.size(), 4);
    QCOMPARE(rows.first().toMap().value("kind").toString(), QString("recent"));
    QCOMPARE(rows.first().toMap().value("value").toString(),
             QString("media query"));
    // Word prefixes of recent entries and of open services' titles and hosts.
    model.setText("med");
    rows = model.suggestions();
    QCOMPARE(rows.size(), 2);
    QCOMPARE(rows[0].toMap().value("title").toString(), QString("media query"));
    const auto open = rows[1].toMap();
    QCOMPARE(open.value("kind").toString(), QString("open"));
    QCOMPARE(open.value("title").toString(), QString("Media Library"));
    QCOMPARE(open.value("subtitle").toString(), QString("media.example"));
    QCOMPARE(open.value("value").toString(), QString("https://media.example"));
    QVERIFY(open.value("open").toBool());
    // A service without a title shows its host, with the port.
    model.setText("nas");
    rows = model.suggestions();
    QCOMPARE(rows.size(), 1);
    QCOMPARE(rows[0].toMap().value("title").toString(),
             QString("nas.local:8096"));
    QCOMPARE(rows[0].toMap().value("value").toString(),
             QString("http://nas.local:8096"));
    // Inactive tabs are not services on screen; an exact entry is not
    // suggested back.
    model.setText("hidden");
    QVERIFY(model.suggestions().isEmpty());
    model.setText("old.example");
    QVERIFY(model.suggestions().isEmpty());
    model.setText("today");
    QCOMPARE(model.suggestions().size(), 1);
    fake.recent = {"old.example"};
  }
  void operationFailure() {
    fake.state = "failed";
    fake.failure = "Unavailable";
    BrowseCatalog model;
    model.setText("page.example");
    QTRY_COMPARE(model.kind(), QString("address"));
    model.open();
    QTRY_COMPARE(model.operationState(), QString("Unavailable"));
    fake.state = "succeeded";
    fake.failure.clear();
  }

private:
  FakeBrowse fake;
};
QTEST_GUILESS_MAIN(BrowseTest)
#include "browse_catalog_test.moc"
