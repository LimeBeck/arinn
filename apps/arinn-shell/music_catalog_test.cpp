// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// MusicCatalog against a scripted daemon on the session bus (ADR-0134).
// Synthetic providers and tracks only.
#include "music_catalog.h"
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QtDBus>
#include <QtTest>
namespace {
struct ProviderRow {
  QString id;
  QString name;
  QString generation;
};
QDBusArgument &operator<<(QDBusArgument &argument, const ProviderRow &row) {
  argument.beginStructure();
  argument << row.id << QStringLiteral("plugin") << QStringLiteral("system")
           << row.name << QStringLiteral("1")
           << QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_") + row.id)
           << QStringList{QStringLiteral("music")} << QStringLiteral("ready")
           << QStringLiteral("available") << row.generation << qulonglong(1) << QString()
           << qulonglong(0) << qulonglong(0) << qulonglong(0) << qulonglong(0);
  argument.endStructure();
  return argument;
}
const QDBusArgument &operator>>(const QDBusArgument &argument, ProviderRow &) {
  return argument;
}
class MusicDaemon : public QDBusVirtualObject {
public:
  QList<ProviderRow> providers;
  // GetState JSON per provider, or an error code name.
  QHash<QString, QJsonObject> states;
  QHash<QString, QString> stateErrors;
  QHash<QString, int> pageReads;
  QStringList commands;
  QString active = QStringLiteral("null");
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message, const QDBusConnection &bus) override {
    const auto member = message.member();
    if (member == QLatin1String("GetProviders")) {
      QDBusArgument body;
      body.beginStructure();
      body << QStringLiteral("snapshot");
      body.beginArray(QMetaType::fromType<ProviderRow>());
      for (const auto &row : providers)
        body << row;
      body.endArray();
      body << QString();
      body.endStructure();
      bus.send(message.createReply(QVariant::fromValue(body)));
      return true;
    }
    if (member == QLatin1String("GetActive")) {
      bus.send(message.createReply(active));
      return true;
    }
    const auto id = message.path().section(QStringLiteral("p_"), 1);
    if(member == QLatin1String("Command")) {
      const auto command=QJsonDocument::fromJson(message.arguments().at(1).toString().toUtf8()).object();
      commands.append(command.value("action").toString());
      bus.send(message.createReply(QString::fromUtf8(QJsonDocument(states.value(id)).toJson(QJsonDocument::Compact))));return true;
    }
    if (member == QLatin1String("GetState")) {
      if (stateErrors.contains(id)) {
        const auto code = stateErrors.value(id);
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.") + code, code));
        return true;
      }
      bus.send(message.createReply(
          QString::fromUtf8(QJsonDocument(states.value(id)).toJson(QJsonDocument::Compact))));
      return true;
    }
    if (member == QLatin1String("GetPage")) {
      ++pageReads[id];
      // As a provider without a catalogue answers.
      if (states.value(id).value("facets").toArray().isEmpty()) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unsupported"),
                                          QStringLiteral("Unsupported")));
        return true;
      }
      QJsonObject page{{"generation", message.arguments().value(0).toString()},
                       {"revision", 1},
                       {"entries", QJsonArray{QJsonObject{{"id", "t1"},
                                                          {"kind", "track"},
                                                          {"title", "Synthetic"},
                                                          {"available", true}}}},
                       {"next_token", ""}};
      bus.send(message.createReply(
          QString::fromUtf8(QJsonDocument(page).toJson(QJsonDocument::Compact))));
      return true;
    }
    return false;
  }
};
QJsonObject state(const QJsonArray &facets, const QString &account = QStringLiteral("local")) {
  return QJsonObject{{"revision", 1},     {"catalogue_revision", 1},
                     {"backend", "available"}, {"account", account},
                     {"facets", facets},  {"actions", QJsonArray{}}};
}
} // namespace

class MusicCatalogTest : public QObject {
  Q_OBJECT
  MusicDaemon daemon;
private slots:
  void initTestCase() {
    qDBusRegisterMetaType<ProviderRow>();
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
    QVERIFY(bus.registerVirtualObject(QStringLiteral("/org/arinn"), &daemon,
                                      QDBusConnection::SubPath));
  }
  void init() {
    daemon.providers.clear();
    daemon.states.clear();
    daemon.stateErrors.clear();
    daemon.pageReads.clear();
    daemon.active = QStringLiteral("null");
  }
  // Review of #710: a source without facets (an account that is not
  // connected) was asked for a page, so "This action is not available"
  // stayed under its Connect prompt.
  void aSourceWithoutFacetsIsNotPaged() {
    daemon.providers = {{"account", "Account music", "g1"}};
    daemon.states.insert("account", state({}, "disconnected"));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.state().value("account").toString(), QStringLiteral("disconnected"));
    QTest::qWait(1500);
    QCOMPARE(daemon.pageReads.value("account"), 0);
    QCOMPARE(catalog.errorText(), QString());
    QVERIFY(catalog.entries().isEmpty());
  }
  // Review of #710: a transient state error stayed on screen after the
  // state read again, because only a page load or command cleared it, and a
  // source without facets loads no page.
  void aTransientStateErrorClears() {
    daemon.providers = {{"account", "Account music", "g1"}};
    daemon.stateErrors.insert("account", "Timeout");
    MusicCatalog catalog;
    QTRY_VERIFY(!catalog.errorText().isEmpty());
    daemon.stateErrors.clear();
    daemon.states.insert("account", state({}, "disconnected"));
    QTRY_COMPARE_WITH_TIMEOUT(catalog.errorText(), QString(), 3000);
  }
  // Review of #710: a selected source that disappeared stayed selected with
  // no endpoint, and the view showed an empty account prompt.
  void aDisappearedSourceFallsBackToAnAvailableOne() {
    daemon.providers = {{"first", "First music", "g1"}, {"second", "Second music", "g1"}};
    daemon.states.insert("first", state({"tracks"}));
    daemon.states.insert("second", state({"tracks"}));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.sourceId(), QStringLiteral("first"));
    catalog.choose(QStringLiteral("second"));
    QCOMPARE(catalog.sourceId(), QStringLiteral("second"));
    QSignalSpy resets(&catalog, &MusicCatalog::browseReset);
    daemon.providers = {{"first", "First music", "g1"}};
    QTRY_COMPARE_WITH_TIMEOUT(catalog.sourceId(), QStringLiteral("first"), 3000);
    QVERIFY(resets.count() >= 1);
    QTRY_COMPARE(catalog.state().value("account").toString(), QStringLiteral("local"));
  }
  void freshStationCanStartBeforeAnyTrackIsPlaying() {
    daemon.providers={{"local","Fixture station source","g1"}};
    auto initial=state({"selections"});initial["account"]="connected";initial["actions"]=QJsonArray{"play","radio"};
    daemon.states.insert("local",initial);MusicCatalog catalog;
    QTRY_COMPARE(catalog.state().value("account").toString(),QStringLiteral("connected"));
    const auto before=daemon.commands.size();catalog.command(QStringLiteral("radio"),QStringLiteral("station:wave"));
    QTRY_COMPARE(daemon.commands.size(),before+1);QCOMPARE(daemon.commands.last(),QStringLiteral("radio"));
  }
  // The Home card names the source of the active session.
  void theHomeCardNamesTheActiveSource() {
    daemon.providers = {{"local", "Local music", "g1"}};
    daemon.states.insert("local", state({"tracks"}));
    daemon.active = QString::fromUtf8(QJsonDocument(QJsonObject{
        {"provider_id", "local"},
        {"generation", "g1"},
        {"state", QJsonObject{{"revision", 1},
                              {"session", "s"},
                              {"playback", "playing"},
                              {"actions", QJsonArray{"pause"}},
                              {"current", QJsonObject{{"occurrence", "o"},
                                                      {"track", QJsonObject{{"id", "t1"},
                                                                            {"kind", "track"},
                                                                            {"title", "Synthetic"}}}}}}}})
                                          .toJson(QJsonDocument::Compact));
    MusicCatalog catalog;
    QTRY_COMPARE(catalog.homeEntries().size(), 1);
    QTRY_COMPARE(catalog.homeEntries().first().toMap().value("statusText").toString(),
                 QStringLiteral("Local music · Playing"));
  }
};
QTEST_GUILESS_MAIN(MusicCatalogTest)
#include "music_catalog_test.moc"
