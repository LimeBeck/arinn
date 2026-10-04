// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// #233: DownloadCatalog reads org.arinn.Downloads1 from a fake daemon on a
// private bus: paging and revisions, every state, actions as operations,
// import states, the toast-once rule and an unavailable backend.
#include "download_catalog.h"
#include <QCoreApplication>
#include <QSignalSpy>
#include <QTest>
#include <QUuid>
#include <QtDBus>
namespace {
const QString providerPath = QStringLiteral("/org/arinn/Daemon1/providers/p_dl");
QString presentationId(const QString &local) {
  return QUuid::createUuidV5(QUuid(), QStringLiteral("dl\n") + local)
      .toString(QUuid::WithoutBraces);
}
struct ProviderWire {
  QString id = QStringLiteral("dl");
  QDBusObjectPath path{providerPath};
  QStringList capabilities{QStringLiteral("download")};
  QString runtime = QStringLiteral("ready");
  QString backend = QStringLiteral("available");
  QString generation = QStringLiteral("g1");
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
  a << p.id << QStringLiteral("plugin") << QString() << QStringLiteral("name")
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
Q_DECLARE_METATYPE(ProviderWire)
Q_DECLARE_METATYPE(OutputWire)
Q_DECLARE_METATYPE(RefWire)
Q_DECLARE_METATYPE(DownloadWire)

namespace {
class Daemon : public QDBusVirtualObject {
public:
  QList<ProviderWire> providers{ProviderWire{}};
  QList<DownloadWire> page1, page2;
  QString pageGeneration = QStringLiteral("g1");
  qulonglong page1Revision = 3, page2Revision = 3;
  bool loopSecond = false;
  QString downloadsError; // error name for GetDownloads
  QString operationState = QStringLiteral("succeeded");
  QString operationError;
  bool holdOperation = false;
  QStringList commands;
  QString lastGeneration, lastLocal;
  int reads = 0;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &message,
                     const QDBusConnection &bus) override {
    QDBusArgument body;
    const auto member = message.member();
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
          next = QStringLiteral("p2");
      } else {
        rows = page2;
        if (loopSecond)
          next = QStringLiteral("p2");
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
    bus.send(message.createReply(QVariant::fromValue(body)));
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
} // namespace

class DownloadCatalogTest : public QObject {
  Q_OBJECT
  Daemon *daemon = nullptr;

  void signalStateChanged(const QString &capability) {
    auto signal = QDBusMessage::createSignal(QStringLiteral("/org/arinn/Daemon1"),
                                             QStringLiteral("org.arinn.Daemon1"),
                                             QStringLiteral("StateChanged"));
    signal << QStringLiteral("dl") << capability << QStringLiteral("g1")
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
    daemon = new Daemon;
    QVERIFY(QDBusConnection::sessionBus().registerVirtualObject(
        QStringLiteral("/org/arinn/Daemon1"), daemon, QDBusConnection::SubPath));
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
    // A size or progress without its flag is never a percentage.
    queued.progress = 0.5;
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

    const auto b = jobById(catalog, QStringLiteral("b"));
    QCOMPARE(b.value(QStringLiteral("progress")).toDouble(), -1.0);
    QCOMPARE(b.value(QStringLiteral("availableActions")).toStringList(),
             QStringList{QStringLiteral("start_now")});
    QCOMPARE(b.value(QStringLiteral("subtitle")).toString(), QString());

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

    daemon->pageGeneration = QStringLiteral("g1");
    daemon->page1.clear();
    daemon->page2.clear();
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("empty"));
  }

  void unavailableService() {
    daemon->page1 = {job(QStringLiteral("a"), QStringLiteral("queued"))};
    daemon->providers.first().backend = QStringLiteral("unavailable");
    DownloadCatalog catalog;
    QTRY_COMPARE(catalog.downloadsState(), QStringLiteral("unavailable"));
    QCOMPARE(daemon->reads, 0);

    daemon->providers.first().backend = QStringLiteral("available");
    daemon->downloadsError = QStringLiteral("Unavailable");
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("unavailable"));

    daemon->downloadsError = QStringLiteral("Internal");
    reload(catalog);
    QCOMPARE(catalog.downloadsState(), QStringLiteral("error"));

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
    QCOMPARE(daemon->lastGeneration, QStringLiteral("g1"));
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
};

QTEST_MAIN(DownloadCatalogTest)
#include "download_catalog_test.moc"
