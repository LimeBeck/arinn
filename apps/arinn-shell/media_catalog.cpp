// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "media_catalog.h"
#include <QUuid>
#include <QtDBus>
#include <QtQml>
#include <algorithm>
#include <cmath>
#include <limits>
#include <time.h>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
constexpr auto sessionPage =
    "(sta(sssssxxtasb(sss))s)";
QDBusMessage daemonCall(const char *interface, const char *method) {
  return QDBusMessage::createMethodCall(service, path, interface, method);
}
quint64 monotonicUs() {
  timespec ts{};
  if (clock_gettime(CLOCK_MONOTONIC, &ts) != 0)
    return 0;
  return static_cast<quint64>(ts.tv_sec) * 1000000ull +
         static_cast<quint64>(ts.tv_nsec) / 1000ull;
}
QString presentationId(const QString &provider, const QString &local) {
  return QUuid::createUuidV5(QUuid(), provider + QLatin1Char('\n') + local)
      .toString(QUuid::WithoutBraces);
}
QString qmlAction(const QString &token) {
  if (token == QLatin1String("play-pause"))
    return QStringLiteral("playPause");
  if (token == QLatin1String("play") || token == QLatin1String("pause") ||
      token == QLatin1String("seek") || token == QLatin1String("next") ||
      token == QLatin1String("previous") || token == QLatin1String("stop"))
    return token;
  return {};
}
QString wireAction(const QString &name) {
  if (name == QLatin1String("playPause"))
    return QStringLiteral("play-pause");
  return name;
}
const char *methodFor(const QString &action) {
  if (action == QLatin1String("play"))
    return "Play";
  if (action == QLatin1String("pause"))
    return "Pause";
  if (action == QLatin1String("playPause"))
    return "PlayPause";
  if (action == QLatin1String("seek"))
    return "Seek";
  if (action == QLatin1String("next"))
    return "Next";
  if (action == QLatin1String("previous"))
    return "Previous";
  if (action == QLatin1String("stop"))
    return "Stop";
  return nullptr;
}
QString errorCode(const QDBusMessage &reply) {
  auto name = reply.errorName();
  const auto dot = name.lastIndexOf(QLatin1Char('.'));
  if (dot >= 0)
    name = name.mid(dot + 1);
  return name.isEmpty() ? QStringLiteral("Unavailable") : name;
}
int stateRank(const QString &state) {
  if (state == QLatin1String("playing"))
    return 0;
  if (state == QLatin1String("paused"))
    return 1;
  return 2;
}
void registerMediaCatalog() {
  qmlRegisterType<MediaCatalog>("Arinn.Shell", 1, 0, "MediaCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerMediaCatalog)
} // namespace

struct MediaCatalog::Catalog {
  quint64 epoch = 0;
  QString snapshot;
  QVariantList providers;
  QList<SessionRow> rows;
  QSet<QString> tokens;
  qsizetype providerStart = 0;
  int pages = 0;
  int failed = 0;
  QString pageGeneration;
};

MediaCatalog::MediaCatalog(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "Changed", this, SLOT(refresh()));
  bus.connect(service, path, service, "ProvidersChanged", this, SLOT(refresh()));
  bus.connect(service, path, service, "StateChanged", this, SLOT(refresh()));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &next) {
            ++m_epoch;
            ++m_commandTicket;
            m_connected = false;
            m_loaded = false;
            m_snapshotError = false;
            m_sessions.clear();
            setOperation(QString());
            publish();
            refresh();
          });
  m_timer.setInterval(1000);
  connect(&m_timer, &QTimer::timeout, this, &MediaCatalog::refresh);
  m_timer.start();
  m_clock.setInterval(200);
  connect(&m_clock, &QTimer::timeout, this, [this] {
    if (!selected() || selected()->state != QLatin1String("playing")) {
      m_clock.stop();
      return;
    }
    emit positionChanged();
  });
  refresh();
}

QString MediaCatalog::sessionState() const {
  if (!m_connected || m_snapshotError)
    return QStringLiteral("unavailable");
  if (!m_loaded)
    return QStringLiteral("loading");
  if (!m_explicitId.isEmpty() && !findId(m_explicitId))
    return QStringLiteral("unavailable");
  if (!selected())
    return QStringLiteral("empty");
  return QStringLiteral("ready");
}
QString MediaCatalog::sessionTitle() const {
  const auto *row = selected();
  return row ? row->title : QString();
}
QString MediaCatalog::sessionSubtitle() const {
  const auto *row = selected();
  return row ? row->subtitle : QString();
}
QString MediaCatalog::playbackState() const {
  const auto *row = selected();
  return row ? row->state : QString();
}
double MediaCatalog::durationUs() const {
  const auto *row = selected();
  return row ? static_cast<double>(row->durationUs) : -1;
}
double MediaCatalog::positionUs() const {
  const auto *row = selected();
  return row ? static_cast<double>(displayedPosition(*row)) : 0;
}
QStringList MediaCatalog::sessionActions() const {
  const auto *row = selected();
  return row ? row->actions : QStringList();
}
QString MediaCatalog::selectedId() const {
  if (!m_explicitId.isEmpty())
    return findId(m_explicitId) ? m_explicitId : QString();
  const auto *row = selected();
  return row ? row->id : QString();
}
const MediaCatalog::SessionRow *MediaCatalog::findId(const QString &id) const {
  for (const auto &row : m_sessions)
    if (row.id == id)
      return &row;
  return nullptr;
}
const MediaCatalog::SessionRow *MediaCatalog::selected() const {
  if (!m_explicitId.isEmpty())
    return findId(m_explicitId);
  return m_sessions.isEmpty() ? nullptr : &m_sessions.first();
}
qint64 MediaCatalog::displayedPosition(const SessionRow &row) {
  auto position = row.positionUs;
  if (row.state == QLatin1String("playing")) {
    const auto now = monotonicUs();
    if (now > row.anchorUs) {
      const auto elapsed = now - row.anchorUs;
      const auto room = std::numeric_limits<qint64>::max() - position;
      if (elapsed > static_cast<quint64>(room))
        position = std::numeric_limits<qint64>::max();
      else
        position += static_cast<qint64>(elapsed);
    }
  }
  if (position < 0)
    position = 0;
  if (row.durationUs >= 0 && position > row.durationUs)
    position = row.durationUs;
  return position;
}
void MediaCatalog::select(const QString &id) {
  if (m_explicitId == id)
    return;
  m_explicitId = id;
  publish();
}
void MediaCatalog::play() { command(QStringLiteral("play")); }
void MediaCatalog::pause() { command(QStringLiteral("pause")); }
void MediaCatalog::playPause() { command(QStringLiteral("playPause")); }
void MediaCatalog::seek(double targetUs) {
  command(QStringLiteral("seek"), targetUs, true);
}
void MediaCatalog::next() { command(QStringLiteral("next")); }
void MediaCatalog::previous() { command(QStringLiteral("previous")); }
void MediaCatalog::stop() { command(QStringLiteral("stop")); }
void MediaCatalog::setOperation(const QString &state) {
  if (m_operation == state)
    return;
  m_operation = state;
  emit operationChanged();
}
void MediaCatalog::syncClock() {
  const auto *row = selected();
  if (row && row->state == QLatin1String("playing")) {
    if (!m_clock.isActive())
      m_clock.start();
  } else {
    m_clock.stop();
  }
}
void MediaCatalog::publish() {
  QVariantList entries;
  entries.reserve(m_sessions.size());
  for (const auto &row : m_sessions) {
    entries.append(QVariantMap{{QStringLiteral("id"), row.id},
                               {QStringLiteral("title"), row.title},
                               {QStringLiteral("subtitle"), row.subtitle},
                               {QStringLiteral("kind"), QStringLiteral("media")},
                               {QStringLiteral("running"),
                                row.state == QLatin1String("playing")}});
  }
  if (m_continue != entries) {
    m_continue = entries;
    emit continueEntriesChanged();
  }
  emit sessionChanged();
  emit positionChanged();
  syncClock();
}
void MediaCatalog::refresh() {
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  const auto epoch = m_epoch;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(daemonCall(service, "Status"),
                                              3000),
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
                reply.arguments().isEmpty() ||
                reply.signature() != "(sbstbsssbbss)") {
              m_refreshing = false;
              m_connected = false;
              m_loaded = false;
              m_snapshotError = false;
              m_sessions.clear();
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
void MediaCatalog::readProviders(const std::shared_ptr<Catalog> &catalog,
                                 const QString &token) {
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    finishCatalog(catalog, false);
    return;
  }
  catalog->tokens.insert(token);
  auto msg = daemonCall(service, "Providers");
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
          if (!capabilities.contains(QLatin1String("media")))
            continue;
          if (runtime == QLatin1String("ready") &&
              backend != QLatin1String("unavailable") && !id.isEmpty() &&
              !endpoint.path().isEmpty())
            catalog->providers.append(QVariantMap{
                {QStringLiteral("id"), id},
                {QStringLiteral("path"), endpoint.path()},
                {QStringLiteral("generation"), generation}});
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
          readSessions(catalog, 0);
        }
      });
}
void MediaCatalog::failProvider(const std::shared_ptr<Catalog> &catalog,
                                int index) {
  ++catalog->failed;
  catalog->rows.resize(catalog->providerStart);
  catalog->tokens.clear();
  catalog->pageGeneration.clear();
  readSessions(catalog, index + 1);
}
void MediaCatalog::readSessions(const std::shared_ptr<Catalog> &catalog,
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
    failProvider(catalog, index);
    return;
  }
  catalog->tokens.insert(token);
  if (token.isEmpty()) {
    catalog->providerStart = catalog->rows.size();
    catalog->pageGeneration.clear();
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value(QStringLiteral("path")).toString(),
      "org.arinn.Media1", "GetSessions");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog, provider, index](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (catalog->epoch != m_epoch) {
          finishCatalog(catalog, false);
          return;
        }
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.arguments().isEmpty() || reply.signature() != sessionPage) {
          failProvider(catalog, index);
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString generation, next;
        qulonglong revision = 0;
        QList<SessionRow> page;
        arg.beginStructure();
        arg >> generation >> revision;
        if (generation.isEmpty() ||
            (!catalog->pageGeneration.isEmpty() &&
             catalog->pageGeneration != generation)) {
          failProvider(catalog, index);
          return;
        }
        catalog->pageGeneration = generation;
        arg.beginArray();
        while (!arg.atEnd()) {
          SessionRow row;
          row.providerId = provider.value(QStringLiteral("id")).toString();
          row.path = provider.value(QStringLiteral("path")).toString();
          row.generation = generation;
          QString artwork;
          QStringList tokens;
          bool hasContent = false;
          qlonglong duration = -1, position = 0;
          qulonglong anchor = 0;
          arg.beginStructure();
          arg >> row.localId >> row.title >> row.subtitle >> artwork >>
              row.state >> duration >> position >> anchor >> tokens >>
              hasContent;
          arg.beginStructure();
          QString contentProvider, contentCapability, contentLocal;
          arg >> contentProvider >> contentCapability >> contentLocal;
          arg.endStructure();
          arg.endStructure();
          artwork.clear();
          contentProvider.clear();
          contentCapability.clear();
          contentLocal.clear();
          Q_UNUSED(hasContent);
          if (row.localId.isEmpty() || duration < -1 || position < 0 ||
              (duration >= 0 && position > duration)) {
            failProvider(catalog, index);
            return;
          }
          row.durationUs = duration;
          row.positionUs = position;
          row.anchorUs = anchor;
          row.id = presentationId(row.providerId, row.localId);
          for (const auto &token : tokens) {
            const auto name = qmlAction(token);
            if (name.isEmpty() || row.wireActions.contains(token))
              continue;
            row.wireActions.append(token);
            row.actions.append(name);
          }
          page.append(row);
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        for (const auto &row : page) {
          auto duplicate = false;
          for (const auto &kept : catalog->rows)
            if (kept.providerId == row.providerId && kept.localId == row.localId)
              duplicate = true;
          if (!duplicate)
            catalog->rows.append(row);
        }
        if (!next.isEmpty())
          readSessions(catalog, index, next);
        else {
          catalog->tokens.clear();
          catalog->pageGeneration.clear();
          readSessions(catalog, index + 1);
        }
      });
}
void MediaCatalog::finishCatalog(const std::shared_ptr<Catalog> &catalog,
                                 bool success) {
  m_refreshing = false;
  if (catalog->epoch != m_epoch) {
    refresh();
    return;
  }
  m_loaded = true;
  const auto everyProviderFailed =
      !catalog->providers.isEmpty() &&
      catalog->failed >= catalog->providers.size();
  if (!success || everyProviderFailed) {
    m_snapshotError = true;
    m_sessions.clear();
  } else {
    m_snapshotError = false;
    auto rows = catalog->rows;
    std::stable_sort(rows.begin(), rows.end(),
                     [](const SessionRow &a, const SessionRow &b) {
                       const auto rankA = stateRank(a.state);
                       const auto rankB = stateRank(b.state);
                       if (rankA != rankB)
                         return rankA < rankB;
                       return a.anchorUs > b.anchorUs;
                     });
    m_sessions = rows;
  }
  publish();
  if (m_dirty)
    QTimer::singleShot(0, this, &MediaCatalog::refresh);
}
void MediaCatalog::command(const QString &action, double seekTarget,
                           bool seeking) {
  ++m_commandTicket;
  const auto ticket = m_commandTicket;
  const auto epoch = m_epoch;
  const auto *row = selected();
  const auto wire = wireAction(action);
  if (!row || sessionState() != QLatin1String("ready")) {
    setOperation(QStringLiteral("Unavailable"));
    return;
  }
  if (!row->wireActions.contains(wire) || methodFor(action) == nullptr) {
    setOperation(QStringLiteral("Unsupported"));
    return;
  }
  qint64 position = 0;
  if (seeking) {
    if (!std::isfinite(seekTarget) || seekTarget < 0 ||
        seekTarget > static_cast<double>(std::numeric_limits<qint64>::max())) {
      setOperation(QStringLiteral("InvalidArgument"));
      return;
    }
    position = static_cast<qint64>(seekTarget);
  }
  setOperation(QStringLiteral("pending"));
  auto msg = QDBusMessage::createMethodCall(service, row->path,
                                            "org.arinn.Media1", methodFor(action));
  msg << row->generation << row->localId;
  if (seeking)
    msg << static_cast<qlonglong>(position);
  msg << QUuid::createUuid().toString(QUuid::WithoutBraces);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, ticket, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (ticket != m_commandTicket || epoch != m_epoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "o" || reply.arguments().isEmpty()) {
              setOperation(reply.type() == QDBusMessage::ErrorMessage
                               ? errorCode(reply)
                               : QStringLiteral("Unavailable"));
              return;
            }
            watchOperation(
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path(),
                ticket, epoch);
          });
}
void MediaCatalog::watchOperation(const QString &objectPath, quint64 ticket,
                                  quint64 epoch) {
  auto timer = new QTimer(this);
  timer->setInterval(100);
  connect(timer, &QTimer::timeout, this, [this, timer, objectPath, ticket, epoch] {
    timer->stop();
    if (ticket != m_commandTicket || epoch != m_epoch) {
      timer->deleteLater();
      return;
    }
    auto msg = QDBusMessage::createMethodCall(
        service, objectPath, "org.arinn.Operation1", "Get");
    auto pending = new QDBusPendingCallWatcher(
        QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this,
            [this, timer, ticket, epoch](auto *call) {
              const auto reply = call->reply();
              call->deleteLater();
              if (ticket != m_commandTicket || epoch != m_epoch) {
                timer->deleteLater();
                return;
              }
              if (reply.type() == QDBusMessage::ErrorMessage ||
                  reply.signature() != "(ssbtta(sss)b)" ||
                  reply.arguments().isEmpty()) {
                setOperation(QStringLiteral("Unavailable"));
                timer->deleteLater();
                return;
              }
              const auto arg =
                  qvariant_cast<QDBusArgument>(reply.arguments().at(0));
              QString state, error;
              bool retryable = false, cancellable = false;
              qulonglong started = 0, ended = 0;
              arg.beginStructure();
              arg >> state >> error >> retryable >> started >> ended;
              arg.beginArray();
              while (!arg.atEnd()) {
                QString provider, capability, entity;
                arg.beginStructure();
                arg >> provider >> capability >> entity;
                arg.endStructure();
              }
              arg.endArray();
              arg >> cancellable;
              arg.endStructure();
              if (state == QLatin1String("running") ||
                  state == QLatin1String("queued")) {
                timer->start();
                return;
              }
              if (state == QLatin1String("succeeded"))
                setOperation(QStringLiteral("succeeded"));
              else
                setOperation(error.isEmpty() ? state : error);
              timer->deleteLater();
            });
  });
  timer->start();
}
