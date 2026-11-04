// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// FolderBrowser against a fake org.arinn.Folders1 on a private bus (#764,
// handoff PK-1…7).
#include "folder_browser.h"
#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusMetaType>
#include <QDBusVirtualObject>
#include <QSignalSpy>
#include <QtTest>

struct PlaceWire {
  QString kind, path;
};
Q_DECLARE_METATYPE(PlaceWire)
QDBusArgument &operator<<(QDBusArgument &argument, const PlaceWire &place) {
  argument.beginStructure();
  argument << place.kind << place.path;
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, PlaceWire &place) {
  argument.beginStructure();
  argument >> place.kind >> place.path;
  argument.endStructure();
  return argument;
}

class FoldersDaemon : public QDBusVirtualObject {
public:
  QList<PlaceWire> places{{QStringLiteral("home"), QStringLiteral("/home/user")},
                          {QStringLiteral("mount"), QStringLiteral("/media/user/Disk")},
                          {QStringLiteral("root"), QStringLiteral("/")}};
  QHash<QString, QStringList> folders;
  QHash<QString, QString> errors;
  QStringList listed;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message, const QDBusConnection &bus) override {
    if (message.interface() != QLatin1String("org.arinn.Folders1"))
      return false;
    if (message.member() == QLatin1String("Places")) {
      QDBusArgument body;
      body.beginArray(QMetaType::fromType<PlaceWire>());
      for (const auto &place : places)
        body << place;
      body.endArray();
      bus.send(message.createReply(QVariant::fromValue(body)));
      return true;
    }
    if (message.member() == QLatin1String("List")) {
      const auto path = message.arguments().value(0).toString();
      const auto after = message.arguments().value(1).toString();
      const auto limit = message.arguments().value(2).toUInt();
      listed.append(path + QLatin1Char('|') + after);
      if (errors.contains(path)) {
        bus.send(message.createErrorReply(errors.value(path), QStringLiteral("x")));
        return true;
      }
      auto names = folders.value(path);
      std::sort(names.begin(), names.end());
      const auto start = after.isEmpty() ? 0 : names.indexOf(after) + 1;
      const auto page = names.mid(start, limit);
      const bool more = start + page.size() < names.size();
      auto reply = message.createReply();
      reply << page << more;
      bus.send(reply);
      return true;
    }
    return false;
  }
};

class FolderBrowserTest : public QObject {
  Q_OBJECT
  FoldersDaemon *daemon = nullptr;
private slots:
  void initTestCase() { qDBusRegisterMetaType<PlaceWire>(); }
  void init() {
    daemon = new FoldersDaemon;
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn/Daemon1"), daemon,
                                      QDBusConnection::SingleNode));
  }
  void cleanup() {
    auto bus = QDBusConnection::sessionBus();
    bus.unregisterObject(QStringLiteral("/org/arinn/Daemon1"));
    bus.unregisterService(QStringLiteral("org.arinn.Daemon1"));
    delete daemon;
    daemon = nullptr;
  }
  void placesAndSortedFolders() {
    daemon->folders.insert(QStringLiteral("/home/user"),
                           {QStringLiteral("films"), QStringLiteral("Archive"),
                            QStringLiteral("series 10"), QStringLiteral("series 9")});
    FolderBrowser browser;
    browser.refreshPlaces();
    QTRY_COMPARE(browser.places().size(), 3);
    const auto mount = browser.places().at(1).toMap();
    QCOMPARE(mount.value(QStringLiteral("kind")).toString(), QStringLiteral("mount"));
    QCOMPARE(mount.value(QStringLiteral("title")).toString(), QStringLiteral("Disk"));
    browser.open(QStringLiteral("/home/user"));
    QCOMPARE(browser.state(), QStringLiteral("loading"));
    QTRY_COMPARE(browser.state(), QStringLiteral("ready"));
    // Case-insensitive, with numbers in numeric order (PK-3).
    QCOMPARE(browser.folders(),
             QStringList({QStringLiteral("Archive"), QStringLiteral("films"),
                          QStringLiteral("series 9"), QStringLiteral("series 10")}));
    QCOMPARE(browser.path(), QStringLiteral("/home/user"));
    QCOMPARE(FolderBrowser::child(QStringLiteral("/"), QStringLiteral("mnt")), QStringLiteral("/mnt"));
    QCOMPARE(FolderBrowser::parentOf(QStringLiteral("/home/user")), QStringLiteral("/home"));
    QCOMPARE(FolderBrowser::parentOf(QStringLiteral("/home")), QStringLiteral("/"));
    QCOMPARE(FolderBrowser::parentOf(QStringLiteral("/")), QString());
  }
  void pagesAreReadToTheEnd() {
    QStringList many;
    for (int i = 0; i < 600; ++i)
      many.append(QStringLiteral("f%1").arg(i, 4, 10, QLatin1Char('0')));
    daemon->folders.insert(QStringLiteral("/data"), many);
    FolderBrowser browser;
    browser.open(QStringLiteral("/data"));
    QTRY_COMPARE(browser.state(), QStringLiteral("ready"));
    QCOMPARE(browser.folders().size(), 600);
    QVERIFY(!browser.truncated());
    QCOMPARE(daemon->listed.size(), 3);
  }
  void emptyDeniedAndGone() {
    daemon->folders.insert(QStringLiteral("/empty"), {});
    daemon->errors.insert(QStringLiteral("/secret"), QStringLiteral("org.arinn.Error1.PermissionDenied"));
    daemon->errors.insert(QStringLiteral("/media/user/Share"), QStringLiteral("org.arinn.Error1.NotFound"));
    daemon->errors.insert(QStringLiteral("/slow"), QStringLiteral("org.arinn.Error1.Timeout"));
    FolderBrowser browser;
    browser.open(QStringLiteral("/empty"));
    QTRY_COMPARE(browser.state(), QStringLiteral("empty"));
    browser.open(QStringLiteral("/secret"));
    QTRY_COMPARE(browser.state(), QStringLiteral("denied"));
    browser.open(QStringLiteral("/media/user/Share"));
    QTRY_COMPARE(browser.state(), QStringLiteral("gone"));
    browser.open(QStringLiteral("/slow"));
    QTRY_COMPARE(browser.state(), QStringLiteral("unavailable"));
    // A relative path is never sent.
    const auto calls = daemon->listed.size();
    browser.open(QStringLiteral("relative"));
    QTest::qWait(100);
    QCOMPARE(daemon->listed.size(), calls);
  }
  void aNewerOpenWins() {
    daemon->folders.insert(QStringLiteral("/a"), {QStringLiteral("one")});
    daemon->folders.insert(QStringLiteral("/b"), {QStringLiteral("two")});
    FolderBrowser browser;
    browser.open(QStringLiteral("/a"));
    browser.open(QStringLiteral("/b"));
    QTRY_COMPARE(browser.state(), QStringLiteral("ready"));
    QTest::qWait(100);
    QCOMPARE(browser.path(), QStringLiteral("/b"));
    QCOMPARE(browser.folders(), QStringList{QStringLiteral("two")});
    browser.up();
    QCOMPARE(browser.path(), QStringLiteral("/"));
  }
};

QTEST_MAIN(FolderBrowserTest)
#include "folder_browser_test.moc"
