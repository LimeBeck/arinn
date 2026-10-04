// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "download_catalog.h"
#include <QDeadlineTimer>
#include <QUuid>
#include <QtDBus>
#include <QtQml>
#include <cmath>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
constexpr auto interface = "org.arinn.Downloads1";
constexpr auto downloadPage = "(sta(ssstbttxbdasssa(ss)sa(sss))s)";
constexpr int operationWaitMs = 31000;
constexpr int fallbackPollMs = 30000;
QString presentationId(const QString &provider, const QString &local) {
  return QUuid::createUuidV5(QUuid(), provider + QLatin1Char('\n') + local)
      .toString(QUuid::WithoutBraces);
}
QString errorCode(const QDBusMessage &reply) {
  auto name = reply.errorName();
  const auto dot = name.lastIndexOf(QLatin1Char('.'));
  if (dot >= 0)
    name = name.mid(dot + 1);
  return name.isEmpty() ? QStringLiteral("Unavailable") : name;
}
bool knownState(const QString &state) {
  static const QStringList states{
      QStringLiteral("queued"),    QStringLiteral("downloading"),
      QStringLiteral("paused"),    QStringLiteral("verifying"),
      QStringLiteral("completed"), QStringLiteral("error")};
  return states.contains(state);
}
// Wire action tokens and the view's action names.
QString viewAction(const QString &wire) {
  return wire == QLatin1String("start-now") ? QStringLiteral("start_now") : wire;
}
QString wireAction(const QString &action) {
  return action == QLatin1String("start_now") ? QStringLiteral("start-now")
                                               : action;
}
const char *method(const QString &action) {
  if (action == QLatin1String("pause"))
    return "Pause";
  if (action == QLatin1String("resume"))
    return "Resume";
  if (action == QLatin1String("start_now"))
    return "StartNow";
  if (action == QLatin1String("retry"))
    return "Retry";
  return nullptr;
}
QString size(quint64 bytes) {
  static const char *units[] = {"B", "KB", "MB", "GB", "TB"};
  auto value = static_cast<double>(bytes);
  int unit = 0;
  while (value >= 1000 && unit < 4) {
    value /= 1000;
    ++unit;
  }
  const auto digits = unit == 0 || value >= 100 ? 0 : 1;
  return QString::number(value, 'f', digits) + QLatin1Char(' ') +
         QLatin1String(units[unit]);
}
QString remaining(qint64 seconds) {
  if (seconds < 60)
    return QObject::tr("Less than a minute left");
  // Round up without adding: any eta up to INT64_MAX is valid on the wire.
  const auto minutes = seconds / 60 + (seconds % 60 ? 1 : 0);
  if (minutes < 60)
    return QObject::tr("%1 min left").arg(minutes);
  return QObject::tr("%1 h %2 min left").arg(minutes / 60).arg(minutes % 60);
}
QString failureText(const QString &code) {
  if (code == QLatin1String("Unavailable"))
    return QObject::tr("Service unavailable");
  if (code == QLatin1String("Timeout"))
    return QObject::tr("The service did not answer");
  if (code == QLatin1String("Unsupported"))
    return QObject::tr("Not supported for this download");
  if (code == QLatin1String("Conflict") || code == QLatin1String("NotFound"))
    return QObject::tr("The queue changed. Try again");
  if (code == QLatin1String("PermissionDenied"))
    return QObject::tr("Not allowed");
  return QObject::tr("Something went wrong");
}
QString actionTitle(const QString &action) {
  if (action == QLatin1String("pause"))
    return QObject::tr("Could not pause");
  if (action == QLatin1String("resume"))
    return QObject::tr("Could not resume");
  if (action == QLatin1String("start_now"))
    return QObject::tr("Could not start");
  return QObject::tr("Could not retry");
}
// A wire token: 1–32 of a–z, 0–9 and '-'.
bool wireToken(const QString &value) {
  if (value.isEmpty() || value.size() > 32)
    return false;
  for (const auto c : value)
    if (!(c.isDigit() || (c >= QLatin1Char('a') && c <= QLatin1Char('z')) ||
          c == QLatin1Char('-')))
      return false;
  return true;
}
const QStringList errorCodes{
    QStringLiteral("NotFound"),         QStringLiteral("Unavailable"),
    QStringLiteral("Unsupported"),      QStringLiteral("PermissionDenied"),
    QStringLiteral("Timeout"),          QStringLiteral("InvalidArgument"),
    QStringLiteral("Conflict"),         QStringLiteral("Internal"),
    QStringLiteral("Cancelled")};
const QStringList importStates{
    QStringLiteral("none"), QStringLiteral("pending"), QStringLiteral("imported"),
    QStringLiteral("unavailable"), QStringLiteral("unsupported")};
constexpr int maxActions = 8;
constexpr int maxOutputs = 64;
void registerDownloadCatalog() {
  qmlRegisterType<DownloadCatalog>("Arinn.Shell", 1, 0, "DownloadCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerDownloadCatalog)
} // namespace

struct DownloadCatalog::Catalog {
  quint64 epoch = 0;
  QString snapshot;
  bool sawDownload = false;
  QVariantList providers;
  QList<Row> rows;
  QSet<QString> tokens;
  qsizetype providerStart = 0;
  int pages = 0;
  int failed = 0;
  int unavailable = 0;
  QString pageGeneration;
  quint64 pageRevision = 0;
};

DownloadCatalog::DownloadCatalog(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "ProvidersChanged", this, SLOT(refresh()));
  bus.connect(service, path, service, "StateChanged", this,
              SLOT(stateChanged(QString, QString, QString, qulonglong)));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &) {
            ++m_epoch;
            m_connected = false;
            m_loaded = false;
            m_error = false;
            m_unreachable = false;
            m_rows.clear();
            m_busy.clear();
            m_failures.clear();
            publish();
            refresh();
          });
  m_timer.setInterval(fallbackPollMs);
  connect(&m_timer, &QTimer::timeout, this, &DownloadCatalog::refresh);
  m_timer.start();
  refresh();
}

QString DownloadCatalog::downloadsState() const {
  if (!m_loaded)
    return QStringLiteral("loading");
  if (!m_connected || m_unreachable)
    return QStringLiteral("unavailable");
  if (m_error)
    return QStringLiteral("error");
  if (m_jobs.isEmpty())
    return QStringLiteral("empty");
  return QStringLiteral("populated");
}

void DownloadCatalog::stateChanged(const QString &, const QString &capability,
                                   const QString &, qulonglong) {
  if (capability == QLatin1String("download"))
    refresh();
}

// The Download invariants (crates/arinn-protocol/src/download.rs).
bool DownloadCatalog::validRow(const Row &row, const QStringList &outputs,
                               int refs, bool refsValid) {
  if (row.localId.isEmpty() || !wireToken(row.state) || row.eta < -1)
    return false;
  if (row.hasTotal ? row.completedBytes > row.totalBytes : row.totalBytes != 0)
    return false;
  if (row.hasProgress ? !(row.progress >= 0 && row.progress <= 1)
                      : row.progress != 0.0)
    return false;
  if (row.actions.size() > maxActions ||
      QSet<QString>(row.actions.cbegin(), row.actions.cend()).size() !=
          row.actions.size())
    return false;
  for (const auto &action : row.actions)
    if (!wireToken(action))
      return false;
  if (row.state == QLatin1String("error")
          ? !errorCodes.contains(row.errorCode)
          : !(row.errorCode.isEmpty() && row.errorMessage.isEmpty()))
    return false;
  if (outputs.size() > maxOutputs ||
      QSet<QString>(outputs.cbegin(), outputs.cend()).size() != outputs.size() ||
      outputs.contains(QString()))
    return false;
  if (!importStates.contains(row.importState) || refs > maxOutputs || !refsValid ||
      (row.importState != QLatin1String("imported") && refs > 0))
    return false;
  return true;
}

const DownloadCatalog::Row *DownloadCatalog::findId(const QString &id) const {
  for (const auto &row : m_rows)
    if (row.id == id)
      return &row;
  return nullptr;
}

void DownloadCatalog::publish() {
  QVariantList jobs;
  jobs.reserve(m_rows.size());
  for (const auto &row : m_rows) {
    const auto known = knownState(row.state);
    QStringList actions;
    if (!row.backendUnavailable)
      for (const auto &action : row.actions)
        actions.append(viewAction(action));
    QStringList details;
    if (row.hasTotal && row.state != QLatin1String("completed"))
      details.append(tr("%1 of %2").arg(size(row.completedBytes), size(row.totalBytes)));
    else if (row.hasTotal)
      details.append(size(row.totalBytes));
    else if (row.completedBytes > 0)
      details.append(size(row.completedBytes));
    if (row.state == QLatin1String("downloading")) {
      if (row.rate > 0)
        details.append(size(row.rate) + QStringLiteral("/s"));
      if (row.eta >= 0)
        details.append(remaining(row.eta));
    }
    QString statusText;
    if (!known)
      statusText = tr("Unsupported state");
    else if (row.state == QLatin1String("completed")) {
      if (row.importState == QLatin1String("pending"))
        statusText = tr("Done · Adding to the library…");
      else if (row.importState == QLatin1String("unavailable"))
        statusText = tr("Done · No library folder covers this download");
    }
    const auto failure = m_failures.value(row.id);
    if (!failure.message.isEmpty())
      statusText = failure.message;
    QString errorText;
    if (row.state == QLatin1String("error"))
      errorText = row.errorMessage.isEmpty() ? failureText(row.errorCode)
                                             : row.errorMessage;
    // The view reads progress below 0 as an unknown size.
    const auto progress = row.hasProgress && std::isfinite(row.progress)
                              ? row.progress
                              : -1.0;
    jobs.append(QVariantMap{
        {QStringLiteral("id"), row.id},
        {QStringLiteral("title"), row.title},
        {QStringLiteral("subtitle"), details.join(QStringLiteral(" · "))},
        {QStringLiteral("state"), known ? row.state : QStringLiteral("unsupported")},
        {QStringLiteral("progress"), progress},
        {QStringLiteral("statusText"), statusText},
        {QStringLiteral("errorText"), errorText},
        {QStringLiteral("availableActions"), actions},
        {QStringLiteral("importState"), row.importState},
        {QStringLiteral("busy"), m_busy.contains(row.id)},
        {QStringLiteral("enabled"), true}});
  }
  m_jobs = jobs;
  emit catalogChanged();
}

void DownloadCatalog::refresh() {
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  const auto epoch = m_epoch;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(
          QDBusMessage::createMethodCall(service, path, service, "Status"), 3000),
      this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch) {
              m_refreshing = false;
              refresh();
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.arguments().isEmpty()) {
              m_refreshing = false;
              m_connected = false;
              m_loaded = true;
              m_error = false;
              m_unreachable = false;
              m_rows.clear();
              publish();
              if (m_dirty)
                refresh();
              return;
            }
            m_connected = true;
            auto catalog = std::make_shared<Catalog>();
            catalog->epoch = epoch;
            readProviders(catalog);
          });
}

void DownloadCatalog::readProviders(const std::shared_ptr<Catalog> &catalog,
                                    const QString &token) {
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    finishCatalog(catalog, false);
    return;
  }
  catalog->tokens.insert(token);
  auto msg = QDBusMessage::createMethodCall(service, path, service, "Providers");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (catalog->epoch != m_epoch ||
            reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != "(sa(sssssoasssststttt)s)") {
          finishCatalog(catalog, false);
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString snapshot, next;
        arg.beginStructure();
        arg >> snapshot;
        if (!catalog->snapshot.isEmpty() && snapshot != catalog->snapshot) {
          finishCatalog(catalog, false);
          return;
        }
        catalog->snapshot = snapshot;
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, plugin, source, name, version, runtime, backend,
              generation, error;
          QStringList capabilities;
          QDBusObjectPath endpoint;
          qulonglong revision = 0, errorAt = 0, recovered = 0, restarts = 0,
                     cumulative = 0;
          arg.beginStructure();
          arg >> id >> plugin >> source >> name >> version >> endpoint >>
              capabilities >> runtime >> backend >> generation >> revision >>
              error >> errorAt >> recovered >> restarts >> cumulative;
          arg.endStructure();
          if (!capabilities.contains(QLatin1String("download")))
            continue;
          catalog->sawDownload = true;
          // A provider whose backend is unavailable still serves its cached
          // rows; only its commands are refused (ADR-0067).
          if (runtime == QLatin1String("ready") && !id.isEmpty() &&
              !endpoint.path().isEmpty() && !generation.isEmpty())
            catalog->providers.append(QVariantMap{
                {QStringLiteral("id"), id},
                {QStringLiteral("path"), endpoint.path()},
                {QStringLiteral("generation"), generation},
                {QStringLiteral("backendUnavailable"),
                 backend == QLatin1String("unavailable")}});
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty())
          readProviders(catalog, next);
        else if (catalog->providers.isEmpty())
          finishCatalog(catalog, true);
        else {
          catalog->tokens.clear();
          readDownloads(catalog, 0);
        }
      });
}

void DownloadCatalog::failProvider(const std::shared_ptr<Catalog> &catalog,
                                   int index, bool unavailable) {
  ++catalog->failed;
  if (unavailable)
    ++catalog->unavailable;
  catalog->rows.resize(catalog->providerStart);
  catalog->tokens.clear();
  readDownloads(catalog, index + 1);
}

void DownloadCatalog::readDownloads(const std::shared_ptr<Catalog> &catalog,
                                    int index, const QString &token) {
  if (catalog->epoch != m_epoch) {
    finishCatalog(catalog, false);
    return;
  }
  if (index >= catalog->providers.size()) {
    finishCatalog(catalog, true);
    return;
  }
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    failProvider(catalog, index, false);
    return;
  }
  catalog->tokens.insert(token);
  if (token.isEmpty()) {
    catalog->providerStart = catalog->rows.size();
    catalog->pageGeneration.clear();
    catalog->pageRevision = 0;
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value(QStringLiteral("path")).toString(), interface,
      "GetDownloads");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog, provider, index, token](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (catalog->epoch != m_epoch) {
          finishCatalog(catalog, false);
          return;
        }
        if (reply.type() == QDBusMessage::ErrorMessage) {
          failProvider(catalog, index,
                       errorCode(reply) == QLatin1String("Unavailable"));
          return;
        }
        if (reply.arguments().isEmpty() || reply.signature() != downloadPage) {
          failProvider(catalog, index, false);
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString generation, next;
        qulonglong revision = 0;
        arg.beginStructure();
        arg >> generation >> revision;
        if (generation != provider.value(QStringLiteral("generation")).toString() ||
            revision == 0 ||
            (!token.isEmpty() && (generation != catalog->pageGeneration ||
                                  revision != catalog->pageRevision))) {
          failProvider(catalog, index, false);
          return;
        }
        catalog->pageGeneration = generation;
        catalog->pageRevision = revision;
        const auto providerId = provider.value(QStringLiteral("id")).toString();
        arg.beginArray();
        while (!arg.atEnd()) {
          Row row;
          row.providerId = providerId;
          row.path = provider.value(QStringLiteral("path")).toString();
          row.generation = generation;
          row.backendUnavailable =
              provider.value(QStringLiteral("backendUnavailable")).toBool();
          qulonglong completed = 0, total = 0, rate = 0;
          qlonglong eta = -1;
          arg.beginStructure();
          arg >> row.localId >> row.title >> row.state >> completed >>
              row.hasTotal >> total >> rate >> eta >> row.hasProgress >>
              row.progress >> row.actions >> row.errorCode >> row.errorMessage;
          // Output titles are not shown; the shell never sees output files.
          QStringList outputs;
          arg.beginArray();
          while (!arg.atEnd()) {
            QString outputId, outputTitle;
            arg.beginStructure();
            arg >> outputId >> outputTitle;
            arg.endStructure();
            outputs.append(outputId);
          }
          arg.endArray();
          arg >> row.importState;
          int refs = 0;
          bool refsValid = true;
          arg.beginArray();
          while (!arg.atEnd()) {
            QString refProvider, capability, refLocal;
            arg.beginStructure();
            arg >> refProvider >> capability >> refLocal;
            arg.endStructure();
            ++refs;
            if (refProvider.isEmpty() || capability.isEmpty() || refLocal.isEmpty())
              refsValid = false;
            else if (row.refLocal.isEmpty()) {
              row.refProvider = refProvider;
              row.refLocal = refLocal;
            }
          }
          arg.endArray();
          arg.endStructure();
          // A malformed row fails its provider (the Download invariants of
          // ADR-0067); nothing is normalized into a misleading row.
          row.completedBytes = completed;
          row.totalBytes = total;
          row.eta = eta;
          if (!validRow(row, outputs, refs, refsValid)) {
            failProvider(catalog, index, false);
            return;
          }
          row.rate = rate;
          row.id = presentationId(providerId, row.localId);
          for (const auto &kept : std::as_const(catalog->rows)) {
            if (kept.id == row.id) {
              failProvider(catalog, index, false);
              return;
            }
          }
          catalog->rows.append(row);
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty())
          readDownloads(catalog, index, next);
        else {
          catalog->tokens.clear();
          readDownloads(catalog, index + 1);
        }
      });
}

void DownloadCatalog::finishCatalog(const std::shared_ptr<Catalog> &catalog,
                                    bool success) {
  m_refreshing = false;
  if (catalog->epoch != m_epoch) {
    refresh();
    return;
  }
  // A StateChanged arrived during this read: its snapshot is already stale.
  // Read again instead of publishing it, a few times at most so a busy
  // queue still updates the screen.
  if (m_dirty && m_superseded < 3) {
    ++m_superseded;
    QTimer::singleShot(0, this, &DownloadCatalog::refresh);
    return;
  }
  m_superseded = 0;
  m_loaded = true;
  m_connected = true;
  const auto everyProviderFailed =
      !catalog->providers.isEmpty() &&
      catalog->failed >= catalog->providers.size();
  // With every backend unavailable and nothing cached, the service is
  // unavailable rather than an empty queue.
  auto everyBackendDown = !catalog->providers.isEmpty();
  for (const auto &provider : std::as_const(catalog->providers))
    if (!provider.toMap().value(QStringLiteral("backendUnavailable")).toBool())
      everyBackendDown = false;
  if (!success) {
    m_error = true;
    m_unreachable = false;
    m_rows.clear();
  } else if (catalog->providers.isEmpty() ||
             (everyBackendDown && catalog->rows.isEmpty()) ||
             (everyProviderFailed &&
              catalog->unavailable >= catalog->providers.size())) {
    // No download service, or none that can answer: service unavailable.
    m_error = false;
    m_unreachable = true;
    m_rows.clear();
  } else if (everyProviderFailed) {
    m_error = true;
    m_unreachable = false;
    m_rows.clear();
  } else {
    m_error = false;
    m_unreachable = false;
    m_rows = catalog->rows;
    // A failure message lasts until its download changes state; a command
    // for another provider generation no longer describes this row.
    for (auto it = m_failures.begin(); it != m_failures.end();) {
      const auto *row = findId(it.key());
      if (!row || row->state != it.value().state ||
          row->generation != it.value().generation)
        it = m_failures.erase(it);
      else
        ++it;
    }
    for (auto it = m_busy.begin(); it != m_busy.end();) {
      const auto *row = findId(it.key());
      if (!row || row->generation != it.value().generation)
        it = m_busy.erase(it);
      else
        ++it;
    }
    announceImports();
  }
  publish();
  if (m_dirty)
    QTimer::singleShot(0, this, &DownloadCatalog::refresh);
}

void DownloadCatalog::announceImports() {
  for (const auto &row : std::as_const(m_rows)) {
    const auto imported = row.importState == QLatin1String("imported");
    const auto seen = m_imported.constFind(row.id);
    // A download that is already imported when first seen is not news.
    if (imported && seen != m_imported.cend() && !seen.value() &&
        !m_announced.contains(row.id)) {
      m_announced.insert(row.id);
      emit downloadImported(tr("%1 is ready to watch").arg(row.title));
    }
    m_imported.insert(row.id, imported);
  }
}

QVariantMap DownloadCatalog::watchTarget(const QString &id) const {
  const auto *row = findId(id);
  if (!row || row->importState != QLatin1String("imported") ||
      row->refLocal.isEmpty())
    return {};
  return {{QStringLiteral("provider"), row->refProvider},
          {QStringLiteral("local"), row->refLocal}};
}

void DownloadCatalog::request(const QString &id, const QString &action) {
  const auto *row = findId(id);
  const auto *name = method(action);
  if (!row || !name || m_busy.contains(id) || row->backendUnavailable ||
      !row->actions.contains(wireAction(action)))
    return;
  const auto epoch = m_epoch;
  const auto ticket = ++m_ticket;
  m_failures.remove(id);
  m_busy.insert(id, {ticket, row->generation});
  publish();
  auto msg = QDBusMessage::createMethodCall(service, row->path, interface, name);
  msg << row->generation << row->localId
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, id, ticket, action, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "o" || reply.arguments().isEmpty()) {
              finishCommand(id, ticket, action,
                            reply.type() == QDBusMessage::ErrorMessage
                                ? errorCode(reply)
                                : QStringLiteral("Unavailable"));
              return;
            }
            watchOperation(
                id, ticket, action,
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path(),
                epoch);
          });
}

void DownloadCatalog::watchOperation(const QString &id, quint64 ticket,
                                     const QString &action,
                                     const QString &objectPath, quint64 epoch) {
  auto timer = new QTimer(this);
  timer->setInterval(100);
  const auto deadline = QDeadlineTimer(operationWaitMs);
  connect(timer, &QTimer::timeout, this,
          [this, timer, id, ticket, action, objectPath, epoch, deadline] {
            timer->stop();
            if (epoch != m_epoch) {
              timer->deleteLater();
              return;
            }
            if (deadline.hasExpired()) {
              finishCommand(id, ticket, action, QStringLiteral("Timeout"));
              timer->deleteLater();
              return;
            }
            auto msg = QDBusMessage::createMethodCall(
                service, objectPath, "org.arinn.Operation1", "Get");
            auto pending = new QDBusPendingCallWatcher(
                QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
            connect(pending, &QDBusPendingCallWatcher::finished, this,
                    [this, timer, id, ticket, action, epoch](auto *call) {
                      const auto reply = call->reply();
                      call->deleteLater();
                      if (epoch != m_epoch) {
                        timer->deleteLater();
                        return;
                      }
                      if (reply.type() == QDBusMessage::ErrorMessage ||
                          reply.signature() != "(ssbtta(sss)b)" ||
                          reply.arguments().isEmpty()) {
                        finishCommand(id, ticket, action, QStringLiteral("Unavailable"));
                        timer->deleteLater();
                        return;
                      }
                      const auto arg =
                          qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                      QString state, error;
                      arg.beginStructure();
                      arg >> state >> error;
                      if (state == QLatin1String("running") ||
                          state == QLatin1String("queued")) {
                        timer->start();
                        return;
                      }
                      finishCommand(id, ticket, action,
                                    state == QLatin1String("succeeded")
                                        ? QString()
                                        : (error.isEmpty() ? state : error));
                      timer->deleteLater();
                    });
          });
  timer->start();
}

void DownloadCatalog::finishCommand(const QString &id, quint64 ticket,
                                    const QString &action,
                                    const QString &result) {
  // A command a provider restart already dropped reports nothing.
  const auto command = m_busy.constFind(id);
  if (command == m_busy.cend() || command->ticket != ticket)
    return;
  const auto generation = command->generation;
  m_busy.remove(id);
  const auto *row = findId(id);
  if (!result.isEmpty() && row && row->generation == generation)
    m_failures.insert(id, {row->state, generation,
                           actionTitle(action) + QStringLiteral(" · ") +
                               failureText(result)});
  publish();
  refresh();
}
