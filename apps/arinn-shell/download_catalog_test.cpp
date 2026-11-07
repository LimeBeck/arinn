// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// #233: DownloadCatalog reads org.arinn.Downloads1 from a fake daemon on a
// private bus: paging and revisions, every state, actions as operations,
// import states, the toast-once rule and an unavailable backend.
#include "download_catalog.h"
#include "flow_trace_recorder.h"
#include "provider_text_client.h"
#include <QFile>
#include <QLibraryInfo>
#include <QProcess>
#include <QScopeGuard>
#include <QTemporaryDir>
#include <QCoreApplication>
#include <limits>
#include <QSignalSpy>
#include <QTest>
#include <QTimer>
#include <functional>
#include <utility>
#include <QUuid>
#include <QtDBus>
namespace {
const char *const generation1 = "5b0e7e3a-7f43-4c1e-9a52-2f6c0d6a1b01";
const char *const generation2 = "5b0e7e3a-7f43-4c1e-9a52-2f6c0d6a1b02";
const QString providerPath = QStringLiteral("/org/arinn/Daemon1/providers/p_dl");
QString presentationId(const QString &local) {
  return QUuid::createUuidV5(QUuid(), QStringLiteral("2:dl") + local)
      .toString(QUuid::WithoutBraces);
}
struct ProviderWire {
  QString id = QStringLiteral("dl");
  QString name = QStringLiteral("name");
  QDBusObjectPath path{providerPath};
  QStringList capabilities{QStringLiteral("download")};
  QString runtime = QStringLiteral("ready");
  QString backend = QStringLiteral("available");
  QString generation = QString::fromLatin1(generation1);
};
struct OutputWire {
  QString id, title;
};
struct RefWire {
  QString provider, capability, local;
};
struct DownloadWire {
  QString id;
  QString title = QStringLiteral("Synthetic");
  QString state = QStringLiteral("downloading");
  qulonglong completed = 0;
  bool hasTotal = false;
  qulonglong total = 0;
  qulonglong rate = 0;
  qlonglong eta = -1;
  bool hasProgress = false;
  double progress = 0;
  QStringList actions;
  QString errorCode, errorMessage;
  QList<OutputWire> outputs;
  QString importState = QStringLiteral("none");
  QList<RefWire> refs;
};
QDBusArgument &operator<<(QDBusArgument &a, const ProviderWire &p) {
  a.beginStructure();
  a << p.id << QStringLiteral("plugin") << QString() << p.name
    << QStringLiteral("1") << p.path << p.capabilities << p.runtime << p.backend
    << p.generation << qulonglong(1) << QString() << qulonglong(0)
    << qulonglong(0) << qulonglong(0) << qulonglong(0);
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, ProviderWire &) { return a; }
QDBusArgument &operator<<(QDBusArgument &a, const OutputWire &o) {
  a.beginStructure();
  a << o.id << o.title;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, OutputWire &) { return a; }
QDBusArgument &operator<<(QDBusArgument &a, const RefWire &r) {
  a.beginStructure();
  a << r.provider << r.capability << r.local;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, RefWire &) { return a; }
QDBusArgument &operator<<(QDBusArgument &a, const DownloadWire &d) {
  a.beginStructure();
  a << d.id << d.title << d.state << d.completed << d.hasTotal << d.total
    << d.rate << d.eta << d.hasProgress << d.progress << d.actions
    << d.errorCode << d.errorMessage;
  a.beginArray(QMetaType::fromType<OutputWire>());
  for (const auto &o : d.outputs)
    a << o;
  a.endArray();
  a << d.importState;
  a.beginArray(QMetaType::fromType<RefWire>());
  for (const auto &r : d.refs)
    a << r;
  a.endArray();
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, DownloadWire &) { return a; }
} // namespace

namespace {
class Daemon : public QDBusVirtualObject {
public:
  QList<ProviderWire> providers{ProviderWire{}};
  QList<DownloadWire> page1, page2;
  QString pageGeneration = QString::fromLatin1(generation1);
  qulonglong page1Revision = 3, page2Revision = 3;
  bool loopSecond = false;
  QString downloadsError; // error name for GetDownloads
  QString operationState = QStringLiteral("succeeded");
  QString operationError;
  bool holdOperation = false;
  QStringList commands;
  QString lastGeneration, lastLocal;
  int reads = 0;
  bool texts = false;
  int catalogReads = 0, translationReads = 0, resolves = 0;
  QString failingTextMethod;
  int textFailures = 0;
  bool malformedTextReply = false;
  int resolveDelayMs = 0;
  qulonglong fixedRevision = 3;
  QString fixedError = QStringLiteral("Could not connect to Music");
  QHash<QString, QByteArray> translations;
  std::function<void()> onResolve;
  // Delay for GetDownloads replies; the page is built when the call arrives.
  int downloadsDelayMs = 0;
  QString page2Token = QStringLiteral("p2");
  // Single-page queues of further providers, by endpoint path.
  QHash<QString, QList<DownloadWire>> otherPages;
  // Runs when a GetDownloads call arrives, after its page is built.
  std::function<void()> onRead;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    QDBusArgument body;
    const auto member = message.member();
    if (message.interface() == QLatin1String("org.arinn.Texts1")) {
      if (!texts) {
        bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.Unsupported"), QStringLiteral("fixture")));
        return true;
      }
      if (member == QLatin1String("GetCatalog")) ++catalogReads;
      if (member == QLatin1String("GetTranslation")) ++translationReads;
      if (member == QLatin1String("Resolve")) ++resolves;
      if (member == failingTextMethod && textFailures > 0) {
        --textFailures;
        bus.send(malformedTextReply ? message.createReply(QVariantList{})
            : message.createErrorReply(QStringLiteral("org.arinn.Error1.Unavailable"), QStringLiteral("fixture")));
        return true;
      }
      if (member == QLatin1String("GetCatalog")) {
        auto source = QJsonDocument::fromJson(R"JSON({"formatVersion":1,"pluginId":"plugin","sourceLocale":"en","locales":[],"messages":{"connect":{"revision":1,"source":"Could not connect to %1","arguments":{"service":{"position":1,"type":"text"}}}}})JSON").object();
        QJsonArray locales;
        for (auto it = translations.begin(); it != translations.end(); ++it) locales.append(it.key());
        source.insert("locales", locales);
        bus.send(message.createReply(QVariantList{pageGeneration, QString::fromUtf8(QJsonDocument(source).toJson(QJsonDocument::Compact))}));
        return true;
      }
      if (member == QLatin1String("GetTranslation") && translations.contains(message.arguments().value(2).toString()) &&
          message.arguments().value(0).toString() != QLatin1String("aaa")) {
        bus.send(message.createReply(translations.value(message.arguments().value(2).toString())));
        return true;
      }
      if (member == QLatin1String("Resolve")) {
        const auto requests = QJsonDocument::fromJson(message.arguments().value(2).toString().toUtf8()).array();
        QJsonArray resolved;
        for (qsizetype i = 0; i < requests.size(); ++i) {
          const auto request = requests.at(i).toObject();
          if (request.value("scope") == QJsonValue(QStringLiteral("download.job")) &&
              request.value("objectId") == QJsonValue(QStringLiteral("fixed")) &&
              request.value("revision").toInteger() == qint64(fixedRevision) &&
              request.value("fallbackText").toString() == fixedError)
            resolved.append(QJsonObject{{"requestIndex", i}, {"fallbackText", fixedError},
                {"message", QJsonObject{{"messageKey", "connect"}, {"messageRevision", 1}, {"arguments", QJsonObject{{"service", "Music"}}}}}});
        }
        const auto reply = message.createReply(QString::fromUtf8(QJsonDocument(resolved).toJson(QJsonDocument::Compact)));
        if (onResolve) std::exchange(onResolve, nullptr)();
        if (resolveDelayMs) QTimer::singleShot(resolveDelayMs, [bus, reply] { bus.send(reply); });
        else bus.send(reply);
        return true;
      }
      bus.send(message.createErrorReply(QStringLiteral("org.arinn.Error1.NotFound"), QStringLiteral("fixture")));
      return true;
    }
    if (member == QLatin1String("Status")) {
      body.beginStructure();
      body << QStringLiteral("epoch") << true << QStringLiteral("generation")
           << qulonglong(1) << true << QString() << QString() << QString()
           << false << true << QString() << QString();
      body.endStructure();
    } else if (member == QLatin1String("Providers")) {
      body.beginStructure();
      body << QStringLiteral("snapshot");
      body.beginArray(QMetaType::fromType<ProviderWire>());
      for (const auto &p : providers)
        body << p;
      body.endArray();
      body << QString();
      body.endStructure();
    } else if (member == QLatin1String("GetDownloads")) {
      ++reads;
      if (otherPages.contains(message.path())) {
        body.beginStructure();
        body << pageGeneration << page1Revision;
        body.beginArray(QMetaType::fromType<DownloadWire>());
        for (const auto &row : otherPages.value(message.path()))
          body << row;
        body.endArray();
        body << QString();
        body.endStructure();
        bus.send(message.createReply(QVariant::fromValue(body)));
        return true;
      }
      if (message.path() != providerPath || !downloadsError.isEmpty()) {
        bus.send(message.createErrorReply(
            QStringLiteral("org.arinn.Error1.") +
                (downloadsError.isEmpty() ? QStringLiteral("NotFound")
                                          : downloadsError),
            QStringLiteral("fixture")));
        return true;
      }
      const auto token = message.arguments().value(0).toString();
      QList<DownloadWire> rows;
      QString next;
      if (token.isEmpty()) {
        rows = page1;
        if (!page2.isEmpty() || loopSecond)
          next = page2Token;
      } else {
        rows = page2;
        if (loopSecond)
          next = page2Token;
      }
      body.beginStructure();
      body << pageGeneration << (token.isEmpty() ? page1Revision : page2Revision);
      body.beginArray(QMetaType::fromType<DownloadWire>());
      for (const auto &row : rows)
        body << row;
      body.endArray();
      body << next;
      body.endStructure();
    } else if (message.interface() == QLatin1String("org.arinn.Downloads1")) {
      commands.append(member);
      lastGeneration = message.arguments().value(0).toString();
      lastLocal = message.arguments().value(1).toString();
      bus.send(message.createReply(QVariant::fromValue(
          QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/ops/dl")))));
      return true;
    } else if (member == QLatin1String("Get") &&
               message.interface() == QLatin1String("org.arinn.Operation1")) {
      body.beginStructure();
      body << (holdOperation ? QStringLiteral("running") : operationState)
           << operationError << false << qulonglong(1) << qulonglong(2);
      body.beginArray(QMetaType::fromType<RefWire>());
      body.endArray();
      body << false;
      body.endStructure();
    } else {
      bus.send(message.createErrorReply(QDBusError::NotSupported,
                                        QStringLiteral("fixture")));
      return true;
    }
    const auto reply = message.createReply(QVariant::fromValue(body));
    if (member == QLatin1String("GetDownloads") && onRead)
      std::exchange(onRead, nullptr)();
    if (member == QLatin1String("GetDownloads") && downloadsDelayMs > 0) {
      QTimer::singleShot(downloadsDelayMs, [bus, reply] { bus.send(reply); });
      return true;
    }
    bus.send(reply);
    return true;
  }
};
DownloadWire job(const QString &id, const QString &state) {
  DownloadWire row;
  row.id = id;
  row.title = QStringLiteral("Job ") + id;
  row.state = state;
  return row;
}
QVariantMap jobById(const DownloadCatalog &catalog, const QString &local) {
  for (const auto &value : catalog.jobs()) {
    const auto map = value.toMap();
    if (map.value(QStringLiteral("id")).toString() == presentationId(local))
      return map;
  }
  return {};
}
bool messageQm(const QString &root, const QString &locale, const QString &translation) {
  const auto directory = QDir(root).filePath(QStringLiteral("arinn/translations/overrides/%1/providers/plugin").arg(locale));
  if (!QDir().mkpath(directory)) return false;
  QFile source(QDir(directory).filePath(QStringLiteral("messages.ts")));
  if (!source.open(QIODevice::WriteOnly)) return false;
  source.write(QStringLiteral("<TS version=\"2.1\" language=\"%1\"><context><name></name><message id=\"connect@1\"><source>Could not connect to %2</source><translation>%3</translation></message></context></TS>")
      .arg(locale, QStringLiteral("%1"), translation.toHtmlEscaped()).toUtf8());
  source.close();
  QProcess compiler;
  compiler.start(QLibraryInfo::path(QLibraryInfo::BinariesPath) + QStringLiteral("/lrelease"),
      {QStringLiteral("-idbased"), QStringLiteral("-nounfinished"), source.fileName(), QStringLiteral("-qm"), QDir(directory).filePath(QStringLiteral("messages.qm"))});
  return compiler.waitForFinished(10000) && compiler.exitCode() == 0;
}
void translationLocale(const QString &locale) {
  qApp->setProperty("arinn.translationLocale", locale);
  QLocale::setDefault(QLocale(locale));
  QEvent event(QEvent::LanguageChange);
  QCoreApplication::sendEvent(qApp, &event);
}
} // namespace

class DownloadCatalogTest : public QObject {
  Q_OBJECT
  Daemon *daemon = nullptr;

  void signalStateChanged(const QString &capability) {
    auto signal = QDBusMessage::createSignal(QStringLiteral("/org/arinn/Daemon1"),
                                             QStringLiteral("org.arinn.Daemon1"),
                                             QStringLiteral("StateChanged"));
    signal << QStringLiteral("dl") << capability << QString::fromLatin1(generation1)
           << qulonglong(4);
    QDBusConnection::sessionBus().send(signal);
  }
  void reload(DownloadCatalog &catalog) {
    const auto before = daemon->reads;
    catalog.refresh();
    QTRY_VERIFY(daemon->reads > before);
    QTRY_VERIFY(catalog.downloadsState() != QLatin1String("loading"));
    QTest::qWait(50);
  }

private slots:
  void initTestCase() {
    qDBusRegisterMetaType<ProviderWire>();
    qDBusRegisterMetaType<OutputWire>();
    qDBusRegisterMetaType<RefWire>();
    qDBusRegisterMetaType<DownloadWire>();
    qDBusRegisterMetaType<QList<ProviderWire>>();
    qDBusRegisterMetaType<QList<OutputWire>>();
    qDBusRegisterMetaType<QList<RefWire>>();
    qDBusRegisterMetaType<QList<DownloadWire>>();
    auto bus = QDBusConnection::sessionBus();
    QVERIFY(bus.isConnected());
    QVERIFY(bus.registerService(QStringLiteral("org.arinn.Daemon1")));
  }
  // A fresh daemon fixture for every case.
  void init() {
    // Each fake daemon reuses its generation. Reset the shared presentation
    // cache at this synthetic boundary, unlike a real immutable registration.
    delete ProviderTextClient::instance();
    daemon = new Daemon;
    QVERIFY(QDBusConnection::sessionBus().registerVirtualObject(
        QStringLiteral("/org/arinn/Daemon1"), daemon, QDBusConnection::SubPath));
  }

  void localFixedErrorsFollowTheUiLanguageWithoutReadingContentAgain() {
    QTemporaryDir root;
    QVERIFY(root.isValid());
    const auto previousHome = qgetenv("XDG_DATA_HOME");
    const auto previousLocale = QLocale();
    const auto previousTag = qApp->property("arinn.translationLocale");
    const auto restore = qScopeGuard([&] {
      if (previousHome.isNull()) qunsetenv("XDG_DATA_HOME"); else qputenv("XDG_DATA_HOME", previousHome);
      QLocale::setDefault(previousLocale);
      qApp->setProperty("arinn.translationLocale", previousTag);
    });
    qputenv("XDG_DATA_HOME", root.path().toUtf8());
    QVERIFY(messageQm(root.path(), "de", "Keine Verbindung zu %1"));
    QVERIFY(messageQm(root.path(), "ru", "Нет соединения с %1"));
    translationLocale("de");
    daemon->texts = true;
    auto fixed = job("fixed", "error");
    fixed.title = "Could not connect to Music"; // identical content stays content
    fixed.errorCode = "Unavailable";
    fixed.errorMessage = daemon->fixedError;
    auto content = fixed;
    content.id = "content";
    daemon->page1 = {fixed, content};
    DownloadCatalog catalog;
    QTRY_COMPARE(jobById(catalog, "fixed").value("errorText").toString(), QStringLiteral("Keine Verbindung zu Music"));
    QCOMPARE(jobById(catalog, "fixed").value("title").toString(), fixed.title);
    QCOMPARE(jobById(catalog, "content").value("errorText").toString(), fixed.errorMessage);
    const auto reads = daemon->reads, resolves = daemon->resolves, catalogs = daemon->catalogReads;
    translationLocale("ru");
    QTRY_COMPARE(jobById(catalog, "fixed").value("errorText").toString(), QStringLiteral("Нет соединения с Music"));
    QCOMPARE(daemon->reads, reads);
    QCOMPARE(daemon->resolves, resolves);
    QCOMPARE(daemon->catalogReads, catalogs);
    QVERIFY(daemon->commands.isEmpty());
    const auto qm = root.filePath("arinn/translations/overrides/ru/providers/plugin/messages.qm");
    QFile corrupt(qm);
    QVERIFY(corrupt.open(QIODevice::WriteOnly | QIODevice::Truncate));
    corrupt.write("broken replacement");
    corrupt.close();
    translationLocale("ru");
    QTest::qWait(20);
    QCOMPARE(jobById(catalog, "fixed").value("errorText").toString(), QStringLiteral("Нет соединения с Music"));
    QVERIFY(QFile::remove(qm));
    translationLocale("ru");
    QTRY_COMPARE(jobById(catalog, "fixed").value("errorText").toString(), fixed.errorMessage);
    QCOMPARE(daemon->resolves, resolves);
  }

  void transientTextFailuresRecoverWithoutAnotherPresentationEvent_data() {
    QTest::addColumn<QString>("method");
    QTest::addColumn<bool>("malformed");
    for (const auto &method : {"GetCatalog", "GetTranslation", "Resolve"}) {
      QTest::newRow(qPrintable(QString::fromLatin1(method) + "-error")) << QString::fromLatin1(method) << false;
      QTest::newRow(qPrintable(QString::fromLatin1(method) + "-malformed")) << QString::fromLatin1(method) << true;
    }
  }

  void transientTextFailuresRecoverWithoutAnotherPresentationEvent() {
    QFETCH(QString, method);
    QFETCH(bool, malformed);
    QTemporaryDir sources;
    QVERIFY(sources.isValid());
    const auto previousLocale = QLocale();
    const auto previousTag = qApp->property("arinn.translationLocale");
    const auto restore = qScopeGuard([&] {
      QLocale::setDefault(previousLocale);
      qApp->setProperty("arinn.translationLocale", previousTag);
    });
    QVERIFY(messageQm(sources.path(), "de", "Deutsch %1"));
    QFile input(sources.filePath("arinn/translations/overrides/de/providers/plugin/messages.qm"));
    QVERIFY(input.open(QIODevice::ReadOnly));
    daemon->texts = true;
    daemon->translations.insert("de", input.readAll());
    daemon->failingTextMethod = method;
    daemon->textFailures = 1;
    daemon->malformedTextReply = malformed;
    translationLocale("de");
    QObject owner;
    auto *client = ProviderTextClient::instance();
    QString rendered;
    const auto publish = [&] {
      const auto presentation = client->presentation(&owner, "jobs");
      rendered = client->translate("dl", generation1, "download.job", "fixed", 3, "error_message", daemon->fixedError);
    };
    client->observe(&owner, publish);
    publish();
    // Poll only the rendered value: no repaint, language change or content
    // refresh may provide an accidental wakeup after the failed request.
    QTRY_COMPARE_WITH_TIMEOUT(rendered, QStringLiteral("Deutsch Music"), 10000);
    QCOMPARE(daemon->textFailures, 0);
    QCOMPARE(daemon->catalogReads, method == QLatin1String("GetCatalog") ? 2 : 1);
    QCOMPARE(daemon->translationReads, method == QLatin1String("GetTranslation") ? 2 : 1);
    QCOMPARE(daemon->resolves, method == QLatin1String("Resolve") ? 2 : 1);
    QCOMPARE(daemon->reads, 0);
    QVERIFY(daemon->commands.isEmpty());
  }

  void delayedFixedReferenceCannotTranslateAChangedSnapshot() {
    QTemporaryDir root;
    QVERIFY(root.isValid());
    const auto previousHome = qgetenv("XDG_DATA_HOME");
    const auto previousLocale = QLocale();
    const auto previousTag = qApp->property("arinn.translationLocale");
    const auto restore = qScopeGuard([&] {
      if (previousHome.isNull()) qunsetenv("XDG_DATA_HOME"); else qputenv("XDG_DATA_HOME", previousHome);
      QLocale::setDefault(previousLocale);
      qApp->setProperty("arinn.translationLocale", previousTag);
    });
    qputenv("XDG_DATA_HOME", root.path().toUtf8());
    QVERIFY(messageQm(root.path(), "de", "Keine Verbindung zu %1"));
    translationLocale("de");
    daemon->texts = true;
    auto fixed = job("fixed", "error");
    fixed.errorCode = "Unavailable";
    fixed.errorMessage = daemon->fixedError;
    daemon->page1 = {fixed};
    daemon->resolveDelayMs = 150;
    DownloadCatalog catalog;
    daemon->onResolve = [&] {
      ++daemon->page1Revision;
      // The same plaintext has become content. Only revision 3 is fixed.
      catalog.refresh();
    };
    QTRY_VERIFY(daemon->resolves >= 2);
    QTest::qWait(200);
    QCOMPARE(jobById(catalog, "fixed").value("errorText").toString(), fixed.errorMessage);
    QVERIFY(daemon->commands.isEmpty());
  }

  void localizationAdmissionSettlesBeyondTheFieldBudget() {
    daemon->texts = true;
    // A model with many content errors republishes the same working set
    // after each metadata response. This must eventually become idle.
    QObject presentation;
    auto *client = ProviderTextClient::instance();
    const auto publish = [&] {
      const auto fields = client->presentation(&presentation, "jobs");
      for (int i = 0; i < 520; ++i)
        client->translate("dl", generation1, "download.job", QString::number(i), 3, "error_message", "Content failure");
    };
    client->observe(&presentation, publish);
    QTRY_VERIFY(([&] { publish(); return daemon->resolves >= 4; })());
    QTest::qWait(30);
    const auto resolves = daemon->resolves;
    QTest::qWait(150);
    QCOMPARE(daemon->resolves, resolves);
    QVERIFY(resolves <= 4);
  }

  void aDestroyedPresentationFreesItsBudgetBeforeANewOwnersFirstPass() {
    daemon->texts = true;
    auto *client = ProviderTextClient::instance();
    auto *owner = new QObject;
    const auto old = [&] {
      const auto presentation = client->presentation(owner, "jobs");
      for (int i = 0; i < 512; ++i)
        client->translate("dl", generation1, "download.job", QStringLiteral("old-%1").arg(i), 3, "error_message", "Content failure");
    };
    client->observe(owner, old);
    QTRY_VERIFY(([&] { old(); return daemon->resolves >= 4; })());
    QTest::qWait(100);
    QCOMPARE(daemon->resolves, 4);
    delete owner;
    QObject fresh;
    const auto publish = [&] {
      const auto presentation = client->presentation(&fresh, "jobs");
      client->translate("dl", generation1, "download.job", "fresh", 4, "error_message", "New content failure");
    };
    client->observe(&fresh, publish);
    publish(); // no second caller pass is needed after freeing the old lease
    QTRY_COMPARE(daemon->resolves, 5);
  }

  void aFullPresentationCanReplaceItsSnapshotWithoutNotificationChurn() {
    daemon->texts = true;
    auto *client = ProviderTextClient::instance();
    QObject owner;
    bool replacement = false;
    const auto publish = [&] {
      const auto presentation = client->presentation(&owner, "jobs");
      for (int i = 0; i < 512; ++i)
        client->translate("dl", generation1, "download.job", QStringLiteral("%1-%2").arg(replacement ? "new" : "old").arg(i),
            replacement ? 4 : 3, "error_message", "Content failure");
    };
    client->observe(&owner, publish);
    QTRY_VERIFY(([&] { publish(); return daemon->resolves >= 4; })());
    QTest::qWait(50);
    QCOMPARE(daemon->resolves, 4);
    replacement = true;
    publish();
    QTRY_COMPARE(daemon->resolves, 8);
    QTest::qWait(100);
    QCOMPARE(daemon->resolves, 8);
    QCOMPARE(daemon->catalogReads, 1);
  }

  void localizationAdmissionSettlesBeyondTheProviderBudget() {
    daemon->texts = true;
    daemon->resolveDelayMs = 20;
    QObject presentation;
    auto *client = ProviderTextClient::instance();
    daemon->providers.clear();
    for (int i = 0; i < 65; ++i) {
      ProviderWire provider;
      provider.id = QStringLiteral("provider-%1").arg(i);
      daemon->providers.append(provider);
    }
    const auto publish = [&] {
      const auto fields = client->presentation(&presentation, "providers");
      for (int i = 0; i < 65; ++i)
        client->translate(QStringLiteral("provider-%1").arg(i), generation1, "download.job", "content", 3, "error_message", "Content failure");
    };
    client->observe(&presentation, publish);
    QTRY_VERIFY_WITH_TIMEOUT(([&] { publish(); return daemon->resolves >= 64; })(), 10000);
    QTest::qWait(50);
    const auto catalogs = daemon->catalogReads, resolves = daemon->resolves;
    QTest::qWait(150);
    QCOMPARE(daemon->catalogReads, catalogs);
    QCOMPARE(daemon->resolves, resolves);
    QCOMPARE(catalogs, 64);
    QCOMPARE(resolves, 64);
  }

  void unrelatedRegistryNotificationPreservesPendingAndSettledTranslations() {
    daemon->texts = true;
    daemon->resolveDelayMs = 80;
    QObject presentation;
    auto *client = ProviderTextClient::instance();
    const auto publish = [&] {
      client->translate("dl", generation1, "download.job", "content", 3, "error_message", "Content failure");
    };
    client->observe(&presentation, publish);
    QTRY_VERIFY(([&] { publish(); return daemon->resolves == 1; })());
    auto changed = QDBusMessage::createSignal("/org/arinn/Daemon1", "org.arinn.Daemon1", "ProvidersChanged");
    QVERIFY(QDBusConnection::sessionBus().send(changed));
    QTest::qWait(130);
    QCOMPARE(daemon->catalogReads, 1);
    QCOMPARE(daemon->resolves, 1);
    QVERIFY(QDBusConnection::sessionBus().send(changed));
    QTest::qWait(50);
    publish();
    QTest::qWait(50);
    QCOMPARE(daemon->catalogReads, 1);
    QCOMPARE(daemon->resolves, 1);
  }

  void languageSwitchRetiresAllObsoleteDictionariesBeforeLoadingPersonalOnes() {
    QTemporaryDir root, sources;
    QVERIFY(root.isValid() && sources.isValid());
    const auto previousHome = qgetenv("XDG_DATA_HOME");
    const auto previousLocale = QLocale();
    const auto previousTag = qApp->property("arinn.translationLocale");
    const auto restore = qScopeGuard([&] {
      if (previousHome.isNull()) qunsetenv("XDG_DATA_HOME"); else qputenv("XDG_DATA_HOME", previousHome);
      QLocale::setDefault(previousLocale);
      qApp->setProperty("arinn.translationLocale", previousTag);
    });
    qputenv("XDG_DATA_HOME", root.path().toUtf8());
    const auto bytes = [&](const QString &base, const QString &tag, const QString &label) {
      if (!messageQm(base, tag, label)) return QByteArray{};
      QFile input(QDir(base).filePath(QStringLiteral("arinn/translations/overrides/%1/providers/plugin/messages.qm").arg(tag)));
      if (!input.open(QIODevice::ReadOnly)) return QByteArray{};
      auto data = input.readAll();
      const auto padding = quint32(ProviderMessages::maxQmBytes - data.size() - 5);
      data.append(char(0x99)); // Qt skips an unknown self-contained block
      char size[4];
      qToBigEndian(padding, size);
      data.append(size, 4);
      data.append(QByteArray(padding, '\0'));
      return data;
    };
    const auto german = bytes(sources.path(), "de", "Deutsch %1");
    const auto personal = bytes(root.path(), "ru", "Личный %1");
    QVERIFY(!german.isEmpty() && !personal.isEmpty());
    ProviderMessages::Dictionary probe("plugin");
    QVERIFY(probe.load(german));
    QFile output(root.filePath("arinn/translations/overrides/ru/providers/plugin/messages.qm"));
    QVERIFY(output.open(QIODevice::WriteOnly | QIODevice::Truncate));
    QCOMPARE(output.write(personal), qint64(personal.size()));
    output.close();
    daemon->texts = true;
    daemon->translations.insert("de", german);
    daemon->providers.clear();
    for (int i = 0; i < 35; ++i) {
      ProviderWire provider;
      provider.id = i == 0 ? QStringLiteral("aaa") : QStringLiteral("provider-%1").arg(i, 2, 10, QLatin1Char('0'));
      daemon->providers.append(provider);
    }
    translationLocale("de");
    QObject presentation;
    auto *client = ProviderTextClient::instance();
    QMap<QString, QString> labels;
    const auto publish = [&] {
      for (int i = 0; i < 35; ++i) {
        const auto id = i == 0 ? QStringLiteral("aaa") : QStringLiteral("provider-%1").arg(i, 2, 10, QLatin1Char('0'));
        labels[id] = client->translate(id, generation1, "download.job", "fixed", 3, "error_message", daemon->fixedError);
      }
    };
    client->observe(&presentation, publish);
    QTRY_COMPARE_WITH_TIMEOUT(([&] { publish(); return labels.value("provider-34"); })(), QStringLiteral("Deutsch Music"), 10000);
    QTRY_COMPARE_WITH_TIMEOUT(daemon->resolves, 35, 10000);
    QCOMPARE(labels.value("aaa"), daemon->fixedError); // this instance supplies no de QM
    const auto resolves = daemon->resolves;
    translationLocale("ru");
    QTRY_COMPARE(labels.value("aaa"), QStringLiteral("Личный Music"));
    QCOMPARE(daemon->resolves, resolves);
    QCOMPARE(labels.value("provider-01"), QStringLiteral("Личный Music"));
  }

  void staleConsumerCannotEvictTheCurrentGenerationWhileMetadataIsPending() {
    daemon->texts = true;
    daemon->resolveDelayMs = 80;
    daemon->pageGeneration = generation2;
    daemon->providers[0].generation = generation2;
    QObject presentation;
    auto *client = ProviderTextClient::instance();
    const auto publish = [&] {
      client->translate("dl", generation2, "download.job", "content", 3, "error_message", "Content failure");
      client->translate("dl", generation1, "download.job", "content", 3, "error_message", "Content failure");
    };
    client->observe(&presentation, publish);
    QTRY_VERIFY(([&] { publish(); return daemon->resolves == 1; })());
    for (int i = 0; i < 8; ++i) { publish(); QTest::qWait(20); }
    QCOMPARE(daemon->catalogReads, 1);
    QCOMPARE(daemon->resolves, 1);
  }

  void changedMetadataNameCannotBeReplacedByAnOlderConsumer() {
    daemon->texts = true;
    QObject presentation;
    auto *client = ProviderTextClient::instance();
    bool renamed = false;
    const auto publish = [&] {
      if (renamed) client->translate("dl", generation1, "provider", "self", 0, "name", "New name");
      client->translate("dl", generation1, "provider", "self", 0, "name", "name");
    };
    client->observe(&presentation, publish);
    QTRY_VERIFY(([&] { publish(); return daemon->resolves == 1; })());
    daemon->providers[0].name = "New name";
    renamed = true;
    auto changed = QDBusMessage::createSignal("/org/arinn/Daemon1", "org.arinn.Daemon1", "ProvidersChanged");
    QVERIFY(QDBusConnection::sessionBus().send(changed));
    QTRY_COMPARE(daemon->resolves, 2);
    for (int i = 0; i < 8; ++i) { publish(); QTest::qWait(20); }
    QCOMPARE(daemon->catalogReads, 1);
    QCOMPARE(daemon->resolves, 2);
  }
  void cleanup() {
    QDBusConnection::sessionBus().unregisterObject(
        QStringLiteral("/org/arinn/Daemon1"), QDBusConnection::UnregisterTree);
    delete daemon;
    daemon = nullptr;
  }

  void statesAndPresentation() {
    auto downloading = job(QStringLiteral("a"), QStringLiteral("downloading"));
    downloading.completed = 1500000000;
    downloading.hasTotal = true;
    downloading.total = 4000000000;
    downloading.rate = 3100000;
    downloading.eta = 720;
    downloading.hasProgress = true;
    downloading.progress = 0.375;
    downloading.actions = {QStringLiteral("pause")};
    auto queued = job(QStringLiteral("b"), QStringLiteral("queued"));
    queued.actions = {QStringLiteral("start-now")};
    auto verifying = job(QStringLiteral("c"), QStringLiteral("verifying"));
    verifying.eta = 30;
    auto failed = job(QStringLiteral("d"), QStringLiteral("error"));
    failed.errorCode = QStringLiteral("Unavailable");
    failed.actions = {QStringLiteral("retry")};
    auto pending = job(QStringLiteral("e"), QStringLiteral("completed"));
    pending.importState = QStringLiteral("pending");
    pending.outputs = {{QStringLiteral("o"), QStringLiteral("file")}};
    auto unmapped = job(QStringLiteral("f"), QStringLiteral("completed"));
    unmapped.importState = QStringLiteral("unavailable");
    auto other = job(QStringLiteral("g"), QStringLiteral("completed"));
    other.importState = QStringLiteral("unsupported");
    auto future = job(QStringLiteral("h"), QStringLiteral("seeding-later"));
    daemon->page1 = {downloading, queued, verifying, failed, pending, unmapped,
                     other, future};

    DownloadCatalog catalog;
    QCOMPARE(catalog.downloadsState(), QStringLiteral("loading"));
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QCOMPARE(catalog.jobs().size(), 8);

    const auto a = jobById(catalog, QStringLiteral("a"));
    QCOMPARE(a.value(QStringLiteral("title")).toString(), QStringLiteral("Job a"));
    QCOMPARE(a.value(QStringLiteral("state")).toString(), QStringLiteral("downloading"));
    QCOMPARE(a.value(QStringLiteral("progress")).toDouble(), 0.375);
    QCOMPARE(a.value(QStringLiteral("subtitle")).toString(),
             QStringLiteral("1.5 GB of 4.0 GB · 3.1 MB/s · 12 min left"));
    QCOMPARE(a.value(QStringLiteral("availableActions")).toStringList(),
             QStringList{QStringLiteral("pause")});
    QCOMPARE(a.value(QStringLiteral("busy")).toBool(), false);
    // #756 B12: the summary line's rate sums the active downloads only.
    QCOMPARE(catalog.rateText(), QStringLiteral("3.1 MB/s"));

    const auto b = jobById(catalog, QStringLiteral("b"));
    QCOMPARE(b.value(QStringLiteral("progress")).toDouble(), -1.0);
    QCOMPARE(b.value(QStringLiteral("availableActions")).toStringList(),
             QStringList{QStringLiteral("start_now")});
    QCOMPARE(b.value(QStringLiteral("subtitle")).toString(), QString());

    // Review of #310: the largest valid eta does not overflow into a
    // negative time.
    daemon->page1.first().eta = std::numeric_limits<qlonglong>::max();
    reload(catalog);
    const auto far = jobById(catalog, QStringLiteral("a"))
                         .value(QStringLiteral("subtitle"))
                         .toString();
    QVERIFY2(far.endsWith(QStringLiteral("min left")) && !far.contains(QLatin1Char('-')),
             qPrintable(far));
    QVERIFY2(far.contains(QStringLiteral("2562047788015215 h 31 min left")), qPrintable(far));

    // Checking shows no invented time remaining.
    const auto c = jobById(catalog, QStringLiteral("c"));
    QCOMPARE(c.value(QStringLiteral("state")).toString(), QStringLiteral("verifying"));
    QVERIFY(!c.value(QStringLiteral("subtitle")).toString().contains(QStringLiteral("left")));

    const auto d = jobById(catalog, QStringLiteral("d"));
    QCOMPARE(d.value(QStringLiteral("errorText")).toString(),
             QStringLiteral("Service unavailable"));

    QCOMPARE(jobById(catalog, QStringLiteral("e")).value(QStringLiteral("statusText")).toString(),
             QStringLiteral("Done · Adding to the library…"));
    QCOMPARE(jobById(catalog, QStringLiteral("f")).value(QStringLiteral("statusText")).toString(),
             QStringLiteral("Done · No library folder covers this download"));
    const auto g = jobById(catalog, QStringLiteral("g"));
    QCOMPARE(g.value(QStringLiteral("importState")).toString(), QStringLiteral("unsupported"));
    QCOMPARE(g.value(QStringLiteral("statusText")).toString(), QString());
    QVERIFY(catalog.watchTarget(presentationId(QStringLiteral("g"))).isEmpty());

    const auto h = jobById(catalog, QStringLiteral("h"));
    QCOMPARE(h.value(QStringLiteral("state")).toString(), QStringLiteral("unsupported"));
    QCOMPARE(h.value(QStringLiteral("statusText")).toString(), QStringLiteral("Unsupported state"));

    // Nothing the shell receives names a file.
    for (const auto &value : catalog.jobs())
      QVERIFY(!value.toMap().contains(QStringLiteral("outputs")));
  }

  // §15: a provider's error message is shown as delivered; without one a
  // failed download shows the shell's text for its code, and sizes follow
  // the locale's number format.
  void errorsAndSizesSpeakTheShellsLanguage() {
    auto stopped = job(QStringLiteral("a"), QStringLiteral("error"));
    stopped.errorCode = QStringLiteral("Internal");
    stopped.actions = {QStringLiteral("retry")};
    auto odd = job(QStringLiteral("b"), QStringLiteral("error"));
    odd.errorCode = QStringLiteral("Unsupported");
    odd.errorMessage = QStringLiteral("provider text");
    auto running = job(QStringLiteral("c"), QStringLiteral("downloading"));
    running.completed = 1500000000;
    running.hasTotal = true;
    running.total = 4000000000;
    running.rate = 3100000;
    daemon->page1 = {stopped, odd, running};
    const auto previous = QLocale();
    QLocale::setDefault(QLocale(QStringLiteral("ru_RU")));
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QCOMPARE(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("errorText")).toString(),
             QStringLiteral("The download stopped"));
    QCOMPARE(jobById(catalog, QStringLiteral("b")).value(QStringLiteral("errorText")).toString(),
             QStringLiteral("provider text"));
    QCOMPARE(jobById(catalog, QStringLiteral("c")).value(QStringLiteral("subtitle")).toString(),
             QStringLiteral("1,5 GB of 4,0 GB · 3,1 MB/s"));
    QLocale::setDefault(previous);
  }

  // #367: Home's status chip sums the active downloads only.
  void aggregateProgressOfActiveDownloads() {
    auto big = job(QStringLiteral("a"), QStringLiteral("downloading"));
    big.completed = 300;
    big.hasTotal = true;
    big.total = 1000;
    big.hasProgress = true;
    big.progress = 0.3;
    auto small = job(QStringLiteral("b"), QStringLiteral("downloading"));
    small.completed = 100;
    small.hasTotal = true;
    small.total = 100;
    small.hasProgress = true;
    small.progress = 1;
    auto paused = job(QStringLiteral("c"), QStringLiteral("paused"));
    paused.hasTotal = true;
    paused.total = 100000;
    auto done = job(QStringLiteral("d"), QStringLiteral("completed"));
    daemon->page1 = {big, small, paused, done};

    DownloadCatalog catalog;
    QVERIFY(!catalog.downloading());
    QCOMPARE(catalog.downloadProgress(), -1.0);
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QVERIFY(catalog.downloading());
    // Bytes, so the large download weighs more: 400 of 1100.
    QCOMPARE(catalog.downloadProgress(), 400.0 / 1100.0);

    // An unsized download falls back to the mean of the known fractions.
    auto unsized = job(QStringLiteral("e"), QStringLiteral("downloading"));
    unsized.hasProgress = true;
    unsized.progress = 0.5;
    daemon->page1 = {big, unsized, done};
    reload(catalog);
    QCOMPARE(catalog.downloadProgress(), 0.4);
    // A sized download without reported progress still counts by its bytes.
    auto sizedOnly = big;
    sizedOnly.id = QStringLiteral("g");
    sizedOnly.hasProgress = false;
    sizedOnly.progress = 0;
    daemon->page1 = {sizedOnly, unsized, done};
    reload(catalog);
    QCOMPARE(catalog.downloadProgress(), 0.4);

    // Nothing knows its progress.
    daemon->page1 = {job(QStringLiteral("f"), QStringLiteral("downloading"))};
    reload(catalog);
    QVERIFY(catalog.downloading());
    QCOMPARE(catalog.downloadProgress(), -1.0);

    daemon->page1 = {paused, done};
    reload(catalog);
    QVERIFY(!catalog.downloading());
    QCOMPARE(catalog.downloadProgress(), -1.0);
  }

  // Review of #310: a row that breaks a Download invariant fails its
  // provider instead of being shown.
  void malformedRowsFailTheProvider() {
    QList<std::pair<const char *, std::function<void(DownloadWire &)>>> cases{
        {"total without flag", [](DownloadWire &d) { d.total = 5; }},
        {"progress without flag", [](DownloadWire &d) { d.progress = 0.5; }},
        {"NaN progress", [](DownloadWire &d) {
           d.hasProgress = true;
           d.progress = std::numeric_limits<double>::quiet_NaN();
         }},
        {"completed past total", [](DownloadWire &d) {
           d.hasTotal = true;
           d.total = 1;
           d.completed = 2;
         }},
        {"bad state token", [](DownloadWire &d) { d.state = QStringLiteral("Bad State"); }},
        {"duplicate action", [](DownloadWire &d) {
           d.actions = {QStringLiteral("pause"), QStringLiteral("pause")};
         }},
        {"error code outside error", [](DownloadWire &d) {
           d.errorCode = QStringLiteral("Internal");
         }},
        {"error without known code", [](DownloadWire &d) {
           d.state = QStringLiteral("error");
           d.errorCode = QStringLiteral("Oops");
         }},
        {"unknown import state", [](DownloadWire &d) { d.importState = QStringLiteral("done"); }},
        {"refs before import", [](DownloadWire &d) {
           d.importState = QStringLiteral("pending");
           d.refs = {{QStringLiteral("lib"), QStringLiteral("library"), QStringLiteral("x")}};
         }},
        {"non-library ref", [](DownloadWire &d) {
           d.state = QStringLiteral("completed");
           d.importState = QStringLiteral("imported");
           d.refs = {{QStringLiteral("web"), QStringLiteral("browser"), QStringLiteral("x")}};
         }},
        // D-Bus strings cannot carry NUL, so only the size limits are
        // reachable on the wire; validRow() still checks NUL like the protocol.
        {"title over 8 KiB", [](DownloadWire &d) { d.title = QString(8 * 1024 + 1, u'x'); }},
        {"id over 256 bytes", [](DownloadWire &d) { d.id = QString(257, u'x'); }},
        {"error message over 8 KiB", [](DownloadWire &d) {
           d.state = QStringLiteral("error");
           d.errorCode = QStringLiteral("Internal");
           d.errorMessage = QString(8 * 1024 + 1, u'x');
         }},
        {"ref id over 256 bytes", [](DownloadWire &d) {
           d.state = QStringLiteral("completed");
           d.importState = QStringLiteral("imported");
           d.refs = {{QStringLiteral("lib"), QStringLiteral("library"), QString(257, u'x')}};
         }},
        {"duplicate output", [](DownloadWire &d) {
           d.outputs = {{QStringLiteral("o"), QString()}, {QStringLiteral("o"), QString()}};
         }},
    };
    for (const auto &[name, breakRow] : cases) {
      // A fresh daemon fixture per case (the framework's cleanup runs last).
      cleanup();
      init();
      auto row = job(QStringLiteral("a"), QStringLiteral("queued"));
      breakRow(row);
      daemon->page1 = {row};
      DownloadCatalog catalog;
      QTRY_VERIFY2(catalog.downloadsState() == QLatin1String("error"), name);
      QVERIFY2(catalog.jobs().isEmpty(), name);
    }
  }

  // Review of #310: a page over the protocol's 200-entry limit fails the
  // provider; exactly 200 is accepted.
  void pageEntryLimit() {
    for (int i = 0; i < 200; ++i)
      daemon->page1.append(job(QStringLiteral("j%1").arg(i), QStringLiteral("queued")));
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QCOMPARE(catalog.jobs().size(), 200);
    daemon->page1.append(job(QStringLiteral("j200"), QStringLiteral("queued")));
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("error"));
  }

  // Review of #310: generations must be UUIDs, and a registry page over 200
  // providers fails the catalog.
  void generationsAndProviderPagesAreChecked() {
    daemon->page1 = {job(QStringLiteral("a"), QStringLiteral("queued"))};
    daemon->providers.first().generation = QStringLiteral("g1");
    daemon->pageGeneration = QStringLiteral("g1");
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("error"));
    daemon->providers.first().generation = QString::fromLatin1(generation1);
    daemon->pageGeneration = QString::fromLatin1(generation1);
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    ProviderWire other;
    other.capabilities = {QStringLiteral("library")};
    for (int i = 0; i < 200; ++i) {
      other.id = QStringLiteral("p%1").arg(i);
      daemon->providers.append(other);
    }
    catalog.refresh();
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("error"));
  }

  // Review of #310: ids may contain newlines; (a, "b\nc") and ("a\nb", c)
  // are two downloads, not a duplicate that fails a provider.
  void presentationIdsDoNotCollide() {
    daemon->providers.first().id = QStringLiteral("a");
    daemon->page1 = {job(QStringLiteral("b\nc"), QStringLiteral("queued"))};
    ProviderWire second;
    second.id = QStringLiteral("a\nb");
    second.path = QDBusObjectPath(QStringLiteral("/org/arinn/Daemon1/providers/p_dl2"));
    daemon->providers.append(second);
    daemon->otherPages.insert(second.path.path(), {job(QStringLiteral("c"), QStringLiteral("queued"))});
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QCOMPARE(catalog.jobs().size(), 2);
  }

  // Review of #310: a continuation token must be an id, and a page whose
  // encoding exceeds 256 KiB fails the provider.
  void pageTokenAndSizeLimits() {
    daemon->page1 = {job(QStringLiteral("a"), QStringLiteral("queued"))};
    daemon->page2 = {job(QStringLiteral("b"), QStringLiteral("queued"))};
    daemon->page2Token = QString(257, u't');
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("error"));

    // Near the limit D-Bus padding matters: 64 outputs with 4,070-byte titles
    // encode to about 261,800 bytes, with 4,080-byte titles to about 262,350,
    // over 256 KiB (sizes checked against DownloadPage::encoded_len()).
    daemon->page2.clear();
    const auto page = [](int titleBytes) {
      auto big = job(QStringLiteral("a"), QStringLiteral("completed"));
      for (int i = 0; i < 64; ++i)
        big.outputs.append({QStringLiteral("o%1").arg(i), QString(titleBytes, u'x')});
      return QList<DownloadWire>{big};
    };
    daemon->page1 = page(4070);
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    daemon->page1 = page(4080);
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("error"));
  }

  void pagingAndRevisions() {
    daemon->page1 = {job(QStringLiteral("a"), QStringLiteral("queued"))};
    daemon->page2 = {job(QStringLiteral("b"), QStringLiteral("queued"))};
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QCOMPARE(catalog.jobs().size(), 2);

    // A page from another revision is a torn read.
    daemon->page2Revision = 4;
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("error"));
    QVERIFY(catalog.jobs().isEmpty());

    // A token that repeats is a loop.
    daemon->page2Revision = 3;
    daemon->loopSecond = true;
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("error"));

    // A generation other than the provider's is stale.
    daemon->loopSecond = false;
    daemon->pageGeneration = QStringLiteral("old");
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("error"));

    daemon->pageGeneration = QString::fromLatin1(generation1);
    daemon->page1.clear();
    daemon->page2.clear();
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("empty"));
  }

  void unavailableService() {
    // Review of #310: with the backend unavailable the cached queue stays
    // readable and offers no command (ADR-0067).
    auto queued = job(QStringLiteral("a"), QStringLiteral("queued"));
    queued.actions = {QStringLiteral("start-now")};
    daemon->page1 = {queued};
    daemon->providers.first().backend = QStringLiteral("unavailable");
    recordFlows();
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    // The first read is reported; a repeat that changes nothing is not
    // (ADR-0093).
    QTRY_COMPARE(flowsNamed("downloads.read").size(), 1);
    QCOMPARE(flowsNamed("downloads.read").first().outcome, QString("ok"));
    reload(catalog);
    QCOMPARE(flowsNamed("downloads.read").size(), 1);
    const auto cached = jobById(catalog, QStringLiteral("a"));
    QCOMPARE(cached.value(QStringLiteral("state")).toString(), QStringLiteral("queued"));
    QVERIFY(cached.value(QStringLiteral("availableActions")).toStringList().isEmpty());
    catalog.request(presentationId(QStringLiteral("a")), QStringLiteral("start_now"));
    QTest::qWait(100);
    QVERIFY(daemon->commands.isEmpty());
    // Nothing cached: the service is unavailable, not an empty queue.
    daemon->page1.clear();
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("unavailable"));

    daemon->providers.first().backend = QStringLiteral("available");
    daemon->downloadsError = QStringLiteral("Unavailable");
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("unavailable"));

    daemon->downloadsError = QStringLiteral("Internal");
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("error"));
    // Every failed read is reported, even when the state repeats.
    QStringList outcomes;
    for (const auto &flow : flowsNamed("downloads.read"))
      outcomes << flow.outcome;
    QCOMPARE(outcomes, QStringList({"ok", "Unavailable", "Unavailable", "failed"}));

    // No download service at all.
    daemon->downloadsError.clear();
    daemon->providers.first().capabilities = {QStringLiteral("library")};
    catalog.refresh();
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("unavailable"));
  }

  void commandsAreOperations() {
    auto downloading = job(QStringLiteral("a"), QStringLiteral("downloading"));
    downloading.actions = {QStringLiteral("pause")};
    auto queued = job(QStringLiteral("b"), QStringLiteral("queued"));
    queued.actions = {QStringLiteral("start-now")};
    daemon->page1 = {downloading, queued};
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    const auto a = presentationId(QStringLiteral("a"));
    const auto b = presentationId(QStringLiteral("b"));

    // An action the download does not list is never sent.
    catalog.request(a, QStringLiteral("resume"));
    catalog.request(a, QStringLiteral("delete"));
    QTest::qWait(100);
    QVERIFY(daemon->commands.isEmpty());

    daemon->holdOperation = true;
    catalog.request(a, QStringLiteral("pause"));
    QTRY_COMPARE(daemon->commands, QStringList{QStringLiteral("Pause")});
    QCOMPARE(daemon->lastGeneration, QString::fromLatin1(generation1));
    QCOMPARE(daemon->lastLocal, QStringLiteral("a"));
    QVERIFY(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("busy")).toBool());
    // A busy download takes no second command.
    catalog.request(a, QStringLiteral("pause"));
    QTest::qWait(150);
    QCOMPARE(daemon->commands.size(), 1);
    daemon->holdOperation = false;
    QTRY_VERIFY(!jobById(catalog, QStringLiteral("a")).value(QStringLiteral("busy")).toBool());
    QCOMPARE(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("statusText")).toString(),
             QString());

    catalog.request(b, QStringLiteral("start_now"));
    QTRY_COMPARE(daemon->commands.size(), 2);
    QCOMPARE(daemon->commands.last(), QStringLiteral("StartNow"));
    QTRY_VERIFY(!jobById(catalog, QStringLiteral("b")).value(QStringLiteral("busy")).toBool());

    // A failed operation is reported on the row until the download changes.
    daemon->operationState = QStringLiteral("failed");
    daemon->operationError = QStringLiteral("Unavailable");
    catalog.request(a, QStringLiteral("pause"));
    QTRY_COMPARE(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("statusText")).toString(),
                 QStringLiteral("Could not pause · Service unavailable"));
    reload(catalog);
    QCOMPARE(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("statusText")).toString(),
             QStringLiteral("Could not pause · Service unavailable"));
    daemon->page1.first().state = QStringLiteral("paused");
    reload(catalog);
    QCOMPARE(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("statusText")).toString(),
             QString());
  }

  void completionToastOncePerSession() {
    auto early = job(QStringLiteral("a"), QStringLiteral("completed"));
    early.importState = QStringLiteral("imported");
    early.refs = {{QStringLiteral("lib"), QStringLiteral("library"), QStringLiteral("film")}};
    auto later = job(QStringLiteral("b"), QStringLiteral("completed"));
    later.importState = QStringLiteral("pending");
    daemon->page1 = {early, later};
    DownloadCatalog catalog;
    QSignalSpy toasts(&catalog, &DownloadCatalog::downloadImported);
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    // Already imported when first seen: no toast.
    QCOMPARE(toasts.size(), 0);
    QCOMPARE(catalog.watchTarget(presentationId(QStringLiteral("a"))),
             (QVariantMap{{QStringLiteral("provider"), QStringLiteral("lib")},
                          {QStringLiteral("local"), QStringLiteral("film")}}));
    QVERIFY(catalog.watchTarget(presentationId(QStringLiteral("b"))).isEmpty());

    daemon->page1.last().importState = QStringLiteral("imported");
    reload(catalog);
    QCOMPARE(toasts.size(), 1);
    QCOMPARE(toasts.first().first().toString(), QStringLiteral("Job b is ready to watch"));

    // Reloads, and an import that drops back and completes again, do not repeat.
    reload(catalog);
    daemon->page1.last().importState = QStringLiteral("pending");
    reload(catalog);
    daemon->page1.last().importState = QStringLiteral("imported");
    reload(catalog);
    QCOMPARE(toasts.size(), 1);
  }

  // Review of #310: a read that a newer change notification superseded is
  // not published.
  void supersededSnapshotIsNotPublished() {
    daemon->page1 = {job(QStringLiteral("a"), QStringLiteral("queued"))};
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QTest::qWait(50);
    QStringList seen;
    connect(&catalog, &DownloadCatalog::catalogChanged, this, [&] {
      seen.append(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("state")).toString());
    });
    daemon->downloadsDelayMs = 300;
    daemon->page1.first().state = QStringLiteral("downloading");
    // While that read is in flight the job changes and a change notification
    // asks for a reread (StateChanged for "download" calls refresh()).
    daemon->onRead = [this, &catalog] {
      daemon->page1.first().state = QStringLiteral("paused");
      QMetaObject::invokeMethod(&catalog, "refresh", Qt::QueuedConnection);
    };
    catalog.refresh();
    QTRY_COMPARE(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("state")).toString(),
                 QStringLiteral("paused"));
    QVERIFY2(!seen.contains(QStringLiteral("downloading")), qPrintable(seen.join(',')));
  }

  // Review of #310: a command sent to an old provider generation neither
  // keeps the new row busy nor reports its failure there.
  void providerRestartDropsTheCommand() {
    auto downloading = job(QStringLiteral("a"), QStringLiteral("downloading"));
    downloading.actions = {QStringLiteral("pause")};
    daemon->page1 = {downloading};
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    daemon->holdOperation = true;
    daemon->operationState = QStringLiteral("failed");
    daemon->operationError = QStringLiteral("Unavailable");
    catalog.request(presentationId(QStringLiteral("a")), QStringLiteral("pause"));
    QTRY_VERIFY(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("busy")).toBool());
    daemon->providers.first().generation = QString::fromLatin1(generation2);
    daemon->pageGeneration = QString::fromLatin1(generation2);
    reload(catalog);
    QTRY_VERIFY(!jobById(catalog, QStringLiteral("a")).value(QStringLiteral("busy")).toBool());
    daemon->holdOperation = false;
    QTest::qWait(400);
    const auto row = jobById(catalog, QStringLiteral("a"));
    QVERIFY(!row.value(QStringLiteral("busy")).toBool());
    QCOMPARE(row.value(QStringLiteral("statusText")).toString(), QString());
  }

  void reloadsOnDownloadStateChanged() {
    daemon->page1 = {job(QStringLiteral("a"), QStringLiteral("queued"))};
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QTest::qWait(50);
    const auto before = daemon->reads;
    signalStateChanged(QStringLiteral("library"));
    QTest::qWait(200);
    QCOMPARE(daemon->reads, before);
    daemon->page1.first().state = QStringLiteral("downloading");
    signalStateChanged(QStringLiteral("download"));
    QTRY_COMPARE(jobById(catalog, QStringLiteral("a")).value(QStringLiteral("state")).toString(),
                 QStringLiteral("downloading"));
  }
  void unchangedReloadDoesNotNotify() {
    // Every ProvidersChanged reloads the catalog; Home binds its download
    // state. An unchanged reload must not announce catalogChanged (#441).
    auto active = job(QStringLiteral("a"), QStringLiteral("downloading"));
    active.hasTotal = true;
    active.total = 4000;
    active.completed = 1000;
    daemon->page1 = {active};
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("populated"));
    QTest::qWait(50);
    QSignalSpy changes(&catalog, &DownloadCatalog::catalogChanged);
    for (int round = 0; round < 3; ++round)
      reload(catalog);
    QCOMPARE(changes.count(), 0);
    daemon->page1.first().completed = 2000;
    reload(catalog);
    QTRY_COMPARE(changes.count(), 1);
    QCOMPARE(catalog.downloadProgress(), 0.5);
  }
};

QTEST_MAIN(DownloadCatalogTest)
#include "download_catalog_test.moc"
