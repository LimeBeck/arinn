// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "media_catalog.h"
#include "asset_images.h"
#include <QDeadlineTimer>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSet>
#include <QUuid>
#include <QtDBus>
#include <QtQml>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <limits>
#include <time.h>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
constexpr auto sessionPage =
    "(sta(sssssxxtasb(sss))s)";
// StateChanged reloads the snapshot. This only reconciles a missed signal:
// one pass is Status, Providers and every GetSessions page.
constexpr int fallbackPollMs = 30000;
// Controls time out at 5 s (ADR-0058). Wait one second past that so a
// command that finishes on the deadline is still observed.
constexpr int operationWaitMs = 6000;
// Extension reads wait 5 s (ADR-0058). The thumbnail follows the timeline
// focus without a second seek; a short quiet period collapses a scrub.
constexpr int extensionReadMs = 5000;
constexpr int thumbnailDebounceMs = 400;
constexpr int maxTracks = 64;
constexpr int maxChapters = 256;
constexpr int maxRanges = 64;
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
// The tab presentation ID of BrowserCatalog, so a link names the same tab.
QString tabPresentationId(const QString &provider, const QString &generation,
                          const QString &local) {
  return QUuid::createUuidV5(
             QUuid(), QJsonDocument(QJsonArray{provider, generation,
                                               QStringLiteral("tab"), local})
                          .toJson(QJsonDocument::Compact))
      .toString();
}
constexpr int maxLinks = 64;
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
QStringList knownFeatures(const QStringList &reported) {
  QStringList kept;
  for (const auto &flag : {QStringLiteral("MediaOpen1"),
                           QStringLiteral("MediaTracks1"),
                           QStringLiteral("MediaTimeline1"),
                           QStringLiteral("MediaWindow1")}) {
    if (reported.contains(flag))
      kept.append(flag);
  }
  return kept;
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
  quint64 pageRevision = 0;
};

MediaCatalog::MediaCatalog(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, QString(), "org.arinn.MediaWindow1", "Changed", this,
              SLOT(windowChanged(QString)));
  bus.connect(service, path, service, "HomeRequested", this,
              SLOT(homeRequested(QString, QString, QString)));
  bus.connect(service, path, service, "Changed", this, SLOT(refresh()));
  bus.connect(service, path, service, "ProvidersChanged", this,
              SLOT(refresh()));
  bus.connect(service, path, service, "StateChanged", this, SLOT(refresh()));
  bus.connect(service, path, "org.arinn.MediaAssociation1", "Changed", this,
              SLOT(linksChanged(QString, qulonglong)));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &next) {
            ++m_epoch;
            ++m_commandTicket;
            m_connected = false;
            abandonPresentation();
            m_presentRow = {};
            m_releaseRow = {};
            m_daemonEpoch.clear();
            m_daemonOwner.clear();
            m_loaded = false;
            m_snapshotError = false;
            m_sessions.clear();
            m_links.clear();
            m_linksRevision = 0;
            m_linksWanted = 0;
            setOperation(QString());
            publish();
            refresh();
          });
  m_timer.setInterval(fallbackPollMs);
  connect(&m_timer, &QTimer::timeout, this, &MediaCatalog::refresh);
  m_timer.start();
  m_clock.setInterval(200);
  connect(&m_clock, &QTimer::timeout, this, [this] {
    if (!selected() || selected()->state != QLatin1String("playing")) {
      m_clock.stop();
      return;
    }
    emit positionChanged();
    requestDisplayedThumbnail();
  });
  m_thumbnailTimer.setSingleShot(true);
  m_thumbnailTimer.setInterval(thumbnailDebounceMs);
  connect(&m_thumbnailTimer, &QTimer::timeout, this, &MediaCatalog::sendThumbnail);
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
QString MediaCatalog::sessionArtwork() const {
  const auto *row = selected();
  if (!row)
    return {};
  return AssetImages::imageUrl(row->providerId, row->path, row->generation,
                               row->artworkId);
}
bool MediaCatalog::featureOn(const QString &flag) const {
  const auto *row = selected();
  return row && sessionState() == QLatin1String("ready") &&
         row->features.contains(flag);
}
bool MediaCatalog::playerSession() const {
  return featureOn(QStringLiteral("MediaOpen1")) ||
         featureOn(QStringLiteral("MediaTracks1")) ||
         featureOn(QStringLiteral("MediaTimeline1")) ||
         featureOn(QStringLiteral("MediaWindow1"));
}
bool MediaCatalog::tracksAvailable() const {
  return featureOn(QStringLiteral("MediaTracks1"));
}
bool MediaCatalog::timelineAvailable() const {
  return featureOn(QStringLiteral("MediaTimeline1"));
}
QVariantMap MediaCatalog::trackMap(const TrackRow &track) const {
  QStringList parts;
  if (!track.language.isEmpty())
    parts.append(track.language);
  if (!track.codec.isEmpty())
    parts.append(track.codec);
  if (track.channels > 0)
    parts.append(QString::number(track.channels));
  auto label = track.label;
  if (label.isEmpty())
    label = track.language.isEmpty() ? track.id : track.language;
  return QVariantMap{{QStringLiteral("id"), track.id},
                     {QStringLiteral("label"), label},
                     {QStringLiteral("detail"), parts.join(QStringLiteral(" · "))}};
}
QVariantList MediaCatalog::audioTracks() const {
  QVariantList list;
  if (!tracksAvailable())
    return list;
  for (const auto &track : m_tracks)
    if (track.kind == QLatin1String("audio"))
      list.append(trackMap(track));
  return list;
}
QVariantList MediaCatalog::subtitleTracks() const {
  QVariantList list;
  if (!tracksAvailable() || m_subtitleOffId.isEmpty())
    return list;
  list.append(QVariantMap{{QStringLiteral("id"), m_subtitleOffId},
                          {QStringLiteral("label"), tr("Off")}});
  for (const auto &track : m_tracks)
    if (track.kind == QLatin1String("subtitle"))
      list.append(trackMap(track));
  return list;
}
QString MediaCatalog::audioTrackId() const {
  if (!tracksAvailable())
    return {};
  for (const auto &track : m_tracks)
    if (track.kind == QLatin1String("audio") && track.selected)
      return track.id;
  return {};
}
QString MediaCatalog::subtitleTrackId() const {
  if (!tracksAvailable())
    return {};
  for (const auto &track : m_tracks)
    if (track.kind == QLatin1String("subtitle") && track.selected)
      return track.id;
  return m_subtitleOffId;
}
double MediaCatalog::bufferedSeconds() const {
  if (!timelineAvailable())
    return 0;
  const auto *row = selected();
  if (!row || m_ranges.isEmpty())
    return 0;
  const auto position = displayedPosition(*row);
  qint64 end = -1;
  for (const auto &range : m_ranges) {
    if (position >= range.startUs && position <= range.endUs && range.endUs > end)
      end = range.endUs;
  }
  if (end < 0)
    return 0;
  return static_cast<double>(end) / 1000000.0;
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
    // An anchor of 0 is not a sample: subtracting it adds the whole uptime
    // and the position jumps to the duration or to the int64 limit. A sample
    // ahead of this clock is not one either, so the published position stays.
    if (row.anchorUs != 0 && now > row.anchorUs) {
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
  if (!m_presentRow.id.isEmpty() && m_presentRow.id != id)
    release();
  m_explicitId = id;
  publish();
}
void MediaCatalog::play() { command(QStringLiteral("play")); }
void MediaCatalog::pause() { command(QStringLiteral("pause")); }
void MediaCatalog::playPause() { command(QStringLiteral("playPause")); }
void MediaCatalog::seek(double targetUs) {
  command(QStringLiteral("seek"), targetUs, true);
  // The thumbnail is a separate read. It must not send another Seek.
  if (m_operation == QLatin1String("pending") && timelineAvailable() &&
      std::isfinite(targetUs) && targetUs >= 0 &&
      targetUs <= static_cast<double>(std::numeric_limits<qint64>::max())) {
    m_thumbFromSeek = true;
    requestThumbnailAt(static_cast<qint64>(targetUs), true);
  }
}
void MediaCatalog::selectAudio(const QString &trackId) {
  if (!tracksAvailable()) {
    setOperation(QStringLiteral("Unsupported"));
    return;
  }
  for (const auto &track : m_tracks) {
    if (track.kind == QLatin1String("audio") && track.id == trackId) {
      extensionCommand("SelectTrack", {QStringLiteral("audio"), trackId});
      return;
    }
  }
  setOperation(QStringLiteral("InvalidArgument"));
}
void MediaCatalog::selectSubtitle(const QString &trackId) {
  if (!tracksAvailable()) {
    setOperation(QStringLiteral("Unsupported"));
    return;
  }
  if (!m_subtitleOffId.isEmpty() && trackId == m_subtitleOffId) {
    extensionCommand("DisableSubtitles", {});
    return;
  }
  for (const auto &track : m_tracks) {
    if (track.kind == QLatin1String("subtitle") && track.id == trackId) {
      extensionCommand("SelectTrack", {QStringLiteral("subtitle"), trackId});
      return;
    }
  }
  setOperation(QStringLiteral("InvalidArgument"));
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
  if (const auto *row = findId(m_releaseRow.id);
      row && row->generation != m_releaseRow.generation)
    m_releaseRow = {};
  if (inputLayerActive()) {
    const auto *row = findId(m_presentRow.id);
    if (!m_connected || !row || row->generation != m_presentRow.generation ||
        row->state == QLatin1String("stopped"))
      abandonPresentation();
  }
  QVariantList entries;
  entries.reserve(m_sessions.size());
  for (const auto &row : m_sessions) {
    QVariantMap entry{
        {QStringLiteral("id"), row.id},
        {QStringLiteral("title"), row.title},
        {QStringLiteral("subtitle"), row.subtitle},
        {QStringLiteral("kind"), QStringLiteral("media")},
        {QStringLiteral("running"), row.state == QLatin1String("playing")}};
    // The provider's reported position, not the advancing clock: the entry
    // changes only with the snapshot, so Home's row is not rebuilt per tick.
    if (row.durationUs > 0) {
      entry.insert(QStringLiteral("durationUs"),
                   static_cast<double>(row.durationUs));
      entry.insert(QStringLiteral("positionUs"),
                   static_cast<double>(row.positionUs));
    }
    const auto artwork = AssetImages::imageUrl(row.providerId, row.path,
                                               row.generation, row.artworkId);
    if (!artwork.isEmpty())
      entry.insert(QStringLiteral("artwork"), artwork);
    // The tab that plays this session; Home shows one card for both only
    // while that tab is in the current browser snapshot.
    for (const auto &link : m_links) {
      if (link.provider == row.providerId && link.generation == row.generation &&
          link.session == row.localId) {
        entry.insert(QStringLiteral("serviceId"), link.tab);
        break;
      }
    }
    entries.append(entry);
  }
  if (m_continue != entries) {
    m_continue = entries;
    emit continueEntriesChanged();
  }
  emit sessionChanged();
  emit positionChanged();
  syncClock();
  loadExtensions();
  requestDisplayedThumbnail();
}
void MediaCatalog::refresh() {
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  const auto epoch = m_epoch;
  auto ownerCall = QDBusMessage::createMethodCall(
      "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
      "GetNameOwner");
  ownerCall << QString::fromLatin1(service);
  auto ownerPending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(ownerCall, 3000), this);
  connect(ownerPending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_epoch) {
              m_refreshing = false;
              refresh();
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "s" || reply.arguments().isEmpty()) {
              m_refreshing = false;
              m_connected = false;
              m_loaded = false;
              m_sessions.clear();
              publish();
              if (m_dirty)
                refresh();
              return;
            }
            refreshStatus(reply.arguments().first().toString(), epoch);
          });
}
void MediaCatalog::refreshStatus(const QString &owner, quint64 epoch) {
  auto statusCall =
      QDBusMessage::createMethodCall(owner, path, service, "Status");
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(statusCall, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, owner, epoch](auto *call) {
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
            const auto status =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            status.beginStructure();
            status >> m_daemonEpoch;
            status.endStructure();
            m_daemonOwner = owner;
            m_connected = true;
            readLinks(owner, epoch);
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
          readFeatures(catalog, 0);
        }
      });
}
void MediaCatalog::readFeatures(const std::shared_ptr<Catalog> &catalog,
                                int index) {
  if (catalog->epoch != m_epoch) {
    finishCatalog(catalog, false);
    return;
  }
  if (index >= catalog->providers.size()) {
    readSessions(catalog, 0);
    return;
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value(QStringLiteral("path")).toString(),
      "org.arinn.Media1", "Features");
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, extensionReadMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, catalog, index](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (catalog->epoch != m_epoch) {
              finishCatalog(catalog, false);
              return;
            }
            QStringList features;
            if (reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == QLatin1String("as") &&
                !reply.arguments().isEmpty())
              features = knownFeatures(reply.arguments().at(0).toStringList());
            auto map = catalog->providers.at(index).toMap();
            map.insert(QStringLiteral("features"), features);
            catalog->providers[index] = map;
            readFeatures(catalog, index + 1);
          });
}
void MediaCatalog::failProvider(const std::shared_ptr<Catalog> &catalog,
                                int index) {
  ++catalog->failed;
  catalog->rows.resize(catalog->providerStart);
  catalog->tokens.clear();
  catalog->pageGeneration.clear();
  catalog->pageRevision = 0;
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
    catalog->pageRevision = 0;
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
      [this, catalog, provider, index, token](auto *call) {
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
        // One snapshot is (generation, revision). The generation is the
        // provider's, and every page of the snapshot carries that same pair.
        if (generation !=
                provider.value(QStringLiteral("generation")).toString() ||
            revision == 0 ||
            (!token.isEmpty() &&
             (generation != catalog->pageGeneration ||
              revision != catalog->pageRevision))) {
          failProvider(catalog, index);
          return;
        }
        catalog->pageGeneration = generation;
        catalog->pageRevision = revision;
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
          row.artworkId = artwork;
          arg.beginStructure();
          QString contentProvider, contentCapability, contentLocal;
          arg >> contentProvider >> contentCapability >> contentLocal;
          arg.endStructure();
          arg.endStructure();
          if (hasContent && contentCapability == QLatin1String("library") &&
              !contentProvider.isEmpty() && !contentLocal.isEmpty()) {
            row.contentProvider = contentProvider;
            row.contentLocal = contentLocal;
          }
          if (row.localId.isEmpty() || duration < -1 || position < 0 ||
              (duration >= 0 && position > duration)) {
            failProvider(catalog, index);
            return;
          }
          row.durationUs = duration;
          row.positionUs = position;
          row.anchorUs = anchor;
          row.revision = revision;
          row.features = provider.value(QStringLiteral("features")).toStringList();
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
          catalog->pageRevision = 0;
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
  dispatch(msg, ticket, epoch);
}
void MediaCatalog::extensionCommand(const char *method, const QStringList &middle) {
  ++m_commandTicket;
  const auto ticket = m_commandTicket;
  const auto epoch = m_epoch;
  const auto *row = selected();
  if (!row || sessionState() != QLatin1String("ready")) {
    setOperation(QStringLiteral("Unavailable"));
    return;
  }
  setOperation(QStringLiteral("pending"));
  auto msg = QDBusMessage::createMethodCall(service, row->path,
                                            "org.arinn.Media1", method);
  msg << row->generation << row->localId;
  for (const auto &part : middle)
    msg << part;
  msg << QUuid::createUuid().toString(QUuid::WithoutBraces);
  dispatch(msg, ticket, epoch);
}
void MediaCatalog::dispatch(const QDBusMessage &msg, quint64 ticket, quint64 epoch) {
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
  const auto deadline = QDeadlineTimer(operationWaitMs);
  connect(timer, &QTimer::timeout, this,
          [this, timer, objectPath, ticket, epoch, deadline] {
    timer->stop();
    if (ticket != m_commandTicket || epoch != m_epoch) {
      timer->deleteLater();
      return;
    }
    if (deadline.hasExpired()) {
      setOperation(QStringLiteral("Timeout"));
      timer->deleteLater();
      return;
    }
    auto msg = QDBusMessage::createMethodCall(
        service, objectPath, "org.arinn.Operation1", "Get");
    auto pending = new QDBusPendingCallWatcher(
        QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
    connect(pending, &QDBusPendingCallWatcher::finished, this,
            [this, timer, ticket, epoch, deadline](auto *call) {
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
                if (deadline.hasExpired()) {
                  setOperation(QStringLiteral("Timeout"));
                  timer->deleteLater();
                  return;
                }
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
void MediaCatalog::loadExtensions() {
  const auto *row = selected();
  if (!row || sessionState() != QLatin1String("ready")) {
    clearExtensions();
    return;
  }
  const auto wantTracks = row->features.contains(QLatin1String("MediaTracks1"));
  const auto wantTimeline = row->features.contains(QLatin1String("MediaTimeline1"));
  if (!wantTracks && !wantTimeline) {
    clearExtensions();
    return;
  }
  if (row->id != m_boundSessionId) {
    clearExtensions();
    m_boundSessionId = row->id;
  }
  if (m_extensionLoading)
    return;
  if (m_extensionsLoaded && row->revision == m_loadedRevision)
    return;
  m_extensionLoading = true;
  m_extensionsLoaded = false;
  m_loadingRevision = row->revision;
  m_tracksDone = !wantTracks;
  m_timelineDone = !wantTimeline;
  if (!wantTracks && !m_tracks.isEmpty()) {
    m_tracks.clear();
    m_subtitleOffId.clear();
    emit tracksChanged();
  }
  if (!wantTimeline && (!m_chapters.isEmpty() || !m_ranges.isEmpty())) {
    m_chapters.clear();
    m_ranges.clear();
    emit timelineChanged();
    emit positionChanged();
  }
  const auto epoch = ++m_extensionEpoch;
  if (wantTracks)
    readTracks(epoch, *row);
  if (wantTimeline)
    readTimeline(epoch, *row);
}
void MediaCatalog::finishExtensionLoad() {
  if (!m_tracksDone || !m_timelineDone)
    return;
  m_extensionLoading = false;
  m_extensionsLoaded = true;
  m_loadedRevision = m_loadingRevision;
  const auto *row = selected();
  if (!row || sessionState() != QLatin1String("ready")) {
    clearExtensions();
    return;
  }
  if (row->id != m_boundSessionId || row->revision != m_loadedRevision)
    loadExtensions();
}
void MediaCatalog::readTracks(quint64 epoch, const SessionRow &row) {
  auto msg = QDBusMessage::createMethodCall(service, row.path, "org.arinn.Media1",
                                            "ListTracks");
  msg << row.generation << row.localId;
  const auto sessionId = row.id;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, extensionReadMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, sessionId](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_extensionEpoch)
              return;
            QList<TrackRow> tracks;
            auto accepted = false;
            const auto *row = selected();
            if (row && row->id == sessionId &&
                reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == QLatin1String("a(sssssubb)") &&
                !reply.arguments().isEmpty()) {
              accepted = true;
              const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
              arg.beginArray();
              while (!arg.atEnd()) {
                TrackRow track;
                quint32 channels = 0;
                bool selected = false, external = false;
                arg.beginStructure();
                arg >> track.id >> track.kind >> track.language >> track.label >>
                    track.codec >> channels >> selected >> external;
                arg.endStructure();
                track.channels = channels;
                track.selected = selected;
                Q_UNUSED(external);
                tracks.append(track);
              }
              arg.endArray();
            }
            if (epoch != m_extensionEpoch)
              return;
            if (accepted)
              applyTracks(tracks);
            else {
              m_tracks.clear();
              m_subtitleOffId.clear();
              emit tracksChanged();
            }
            m_tracksDone = true;
            finishExtensionLoad();
          });
}
void MediaCatalog::applyTracks(const QList<TrackRow> &tracks) {
  QSet<QString> ids;
  QList<TrackRow> kept;
  bool audioSelected = false, subtitleSelected = false;
  for (auto track : tracks) {
    if (track.id.isEmpty() || ids.contains(track.id))
      continue;
    if (track.kind != QLatin1String("audio") &&
        track.kind != QLatin1String("subtitle"))
      continue;
    if (kept.size() >= maxTracks)
      break;
    if (track.selected && track.kind == QLatin1String("audio")) {
      if (audioSelected)
        track.selected = false;
      else
        audioSelected = true;
    }
    if (track.selected && track.kind == QLatin1String("subtitle")) {
      if (subtitleSelected)
        track.selected = false;
      else
        subtitleSelected = true;
    }
    ids.insert(track.id);
    kept.append(track);
  }
  auto off = QStringLiteral("arinn:subtitles-off");
  while (ids.contains(off))
    off += QLatin1Char('-');
  m_tracks = kept;
  m_subtitleOffId = off;
  emit tracksChanged();
}
void MediaCatalog::readTimeline(quint64 epoch, const SessionRow &row) {
  auto msg = QDBusMessage::createMethodCall(service, row.path, "org.arinn.Media1",
                                            "GetTimeline");
  msg << row.generation << row.localId;
  const auto sessionId = row.id;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, extensionReadMs), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, sessionId](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_extensionEpoch)
              return;
            QVariantList chapters;
            QList<RangeRow> ranges;
            const auto *row = selected();
            if (row && row->id == sessionId &&
                reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == QLatin1String("(a(sxs)a(xx))") &&
                !reply.arguments().isEmpty()) {
              const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
              arg.beginStructure();
              arg.beginArray();
              auto tooManyChapters = false;
              while (!arg.atEnd()) {
                QString id, title;
                qlonglong start = 0;
                arg.beginStructure();
                arg >> id >> start >> title;
                arg.endStructure();
                Q_UNUSED(id);
                Q_UNUSED(title);
                if (chapters.size() >= maxChapters)
                  tooManyChapters = true;
                else if (start >= 0)
                  chapters.append(static_cast<double>(start) / 1000000.0);
              }
              arg.endArray();
              if (tooManyChapters)
                chapters.clear();
              arg.beginArray();
              auto tooManyRanges = false;
              while (!arg.atEnd()) {
                qlonglong start = 0, end = 0;
                arg.beginStructure();
                arg >> start >> end;
                arg.endStructure();
                if (ranges.size() >= maxRanges)
                  tooManyRanges = true;
                else if (start >= 0 && end >= start)
                  ranges.append(RangeRow{start, end});
              }
              arg.endArray();
              if (tooManyRanges)
                ranges.clear();
              arg.endStructure();
            }
            if (epoch != m_extensionEpoch)
              return;
            m_chapters = chapters;
            m_ranges = ranges;
            emit timelineChanged();
            emit positionChanged();
            m_timelineDone = true;
            finishExtensionLoad();
          });
}
void MediaCatalog::clearExtensions() {
  m_thumbnailTimer.stop();
  ++m_extensionEpoch;
  ++m_thumbnailTicket;
  m_extensionLoading = false;
  m_extensionsLoaded = false;
  m_tracksDone = true;
  m_timelineDone = true;
  m_loadedRevision = 0;
  m_boundSessionId.clear();
  m_thumbFromSeek = false;
  m_thumbnailInFlight = false;
  m_pendingThumb = -1;
  m_thumbnailInFlightAt = -1;
  const auto tracks = !m_tracks.isEmpty() || !m_subtitleOffId.isEmpty();
  const auto timeline = !m_chapters.isEmpty() || !m_ranges.isEmpty();
  const auto thumb = !m_thumbnail.isEmpty() || m_thumbnailPosition >= 0;
  m_tracks.clear();
  m_subtitleOffId.clear();
  m_chapters.clear();
  m_ranges.clear();
  m_thumbnail.clear();
  m_thumbnailPosition = -1;
  if (tracks)
    emit tracksChanged();
  if (timeline) {
    emit timelineChanged();
    emit positionChanged();
  }
  if (thumb)
    emit thumbnailChanged();
}
void MediaCatalog::setThumbnailWanted(bool wanted) {
  if (m_thumbnailWanted == wanted)
    return;
  m_thumbnailWanted = wanted;
  emit thumbnailWantedChanged();
  if (wanted)
    requestDisplayedThumbnail();
  else {
    m_thumbnailTimer.stop();
    // A seek arms one thumbnail. Leaving the timeline before it is sent
    // must drop that arm, or the next focus never requests one.
    m_thumbFromSeek = false;
  }
}
void MediaCatalog::requestDisplayedThumbnail() {
  if (m_thumbFromSeek || !m_thumbnailWanted || !timelineAvailable())
    return;
  const auto *row = selected();
  if (!row)
    return;
  requestThumbnailAt(displayedPosition(*row), false);
}
void MediaCatalog::requestThumbnailAt(qint64 positionUs, bool restart) {
  if (!timelineAvailable() || positionUs < 0)
    return;
  // The position clock ticks faster than this delay. Restarting on every
  // tick would never send a thumbnail while playback is moving. A seek
  // still restarts so its target replaces the position already waiting.
  if (!restart && m_thumbnailTimer.isActive())
    return;
  m_pendingThumb = (positionUs / 1000000) * 1000000;
  m_thumbnailTimer.start();
}
void MediaCatalog::sendThumbnail() {
  m_thumbFromSeek = false;
  if (!timelineAvailable())
    return;
  const auto *row = selected();
  if (!row || sessionState() != QLatin1String("ready"))
    return;
  const auto position = m_pendingThumb;
  if (position < 0)
    return;
  if (position == m_thumbnailPosition && !m_thumbnail.isEmpty())
    return;
  if (m_thumbnailInFlight && position == m_thumbnailInFlightAt)
    return;
  ++m_thumbnailTicket;
  const auto ticket = m_thumbnailTicket;
  const auto epoch = m_epoch;
  m_thumbnailInFlight = true;
  m_thumbnailInFlightAt = position;
  auto msg = QDBusMessage::createMethodCall(service, row->path, "org.arinn.Media1",
                                            "RequestThumbnail");
  msg << row->generation << row->localId << static_cast<qlonglong>(position)
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, ticket, epoch, position](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (ticket != m_thumbnailTicket || epoch != m_epoch) {
              if (ticket == m_thumbnailTicket)
                m_thumbnailInFlight = false;
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != "o" ||
                reply.arguments().isEmpty()) {
              m_thumbnailInFlight = false;
              return;
            }
            watchThumbnail(
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path(),
                ticket, epoch, position);
          });
}
void MediaCatalog::watchThumbnail(const QString &objectPath, quint64 ticket,
                                  quint64 epoch, qint64 positionUs) {
  auto timer = new QTimer(this);
  timer->setInterval(100);
  const auto deadline = QDeadlineTimer(operationWaitMs);
  connect(timer, &QTimer::timeout, this,
          [this, timer, objectPath, ticket, epoch, positionUs, deadline] {
            timer->stop();
            if (ticket != m_thumbnailTicket || epoch != m_epoch) {
              timer->deleteLater();
              return;
            }
            if (deadline.hasExpired()) {
              m_thumbnailInFlight = false;
              timer->deleteLater();
              return;
            }
            auto msg = QDBusMessage::createMethodCall(
                service, objectPath, "org.arinn.Operation1", "Get");
            auto pending = new QDBusPendingCallWatcher(
                QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
            connect(pending, &QDBusPendingCallWatcher::finished, this,
                    [this, timer, ticket, epoch, positionUs,
                     deadline](auto *call) {
                      const auto reply = call->reply();
                      call->deleteLater();
                      if (ticket != m_thumbnailTicket || epoch != m_epoch) {
                        timer->deleteLater();
                        return;
                      }
                      if (reply.type() == QDBusMessage::ErrorMessage ||
                          reply.signature() != "(ssbtta(sss)b)" ||
                          reply.arguments().isEmpty()) {
                        m_thumbnailInFlight = false;
                        timer->deleteLater();
                        return;
                      }
                      const auto arg =
                          qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                      QString state, error, asset, assetProvider;
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
                        if (asset.isEmpty() && capability == QLatin1String("media") &&
                            !entity.isEmpty()) {
                          asset = entity;
                          assetProvider = provider;
                        }
                      }
                      arg.endArray();
                      arg >> cancellable;
                      arg.endStructure();
                      Q_UNUSED(error);
                      Q_UNUSED(retryable);
                      Q_UNUSED(cancellable);
                      if (state == QLatin1String("running") ||
                          state == QLatin1String("queued")) {
                        if (deadline.hasExpired()) {
                          m_thumbnailInFlight = false;
                          timer->deleteLater();
                          return;
                        }
                        timer->start();
                        return;
                      }
                      m_thumbnailInFlight = false;
                      timer->deleteLater();
                      if (state != QLatin1String("succeeded") || asset.isEmpty())
                        return;
                      const auto *row = selected();
                      if (!row || row->providerId != assetProvider)
                        return;
                      const auto url = AssetImages::imageUrl(
                          row->providerId, row->path, row->generation, asset);
                      if (url.isEmpty())
                        return;
                      m_thumbnail = url;
                      m_thumbnailPosition = positionUs;
                      emit thumbnailChanged();
                    });
          });
  timer->start();
}

bool MediaCatalog::windowAvailable() const {
  return featureOn(QStringLiteral("MediaWindow1"));
}
void MediaCatalog::setPresentation(const QString &state) {
  if (m_presentation == state)
    return;
  m_presentation = state;
  if (m_presentFlow.active()) {
    if (state == QLatin1String("active")) {
      m_presentFlow.end("platform.present");
      m_presentFlow.begin("window.check");
    } else if (state.isEmpty()) {
      m_presentFlow.finish("Cancelled");
    } else if (state != QLatin1String("pending")) {
      m_presentFlow.finish(FlowTrace::outcome(state));
    }
  }
  emit presentationChanged();
}
void MediaCatalog::abandonPresentation() {
  // An active presentation whose window check has not confirmed it failed.
  if (m_presentFlow.active() && m_presentation == QLatin1String("active"))
    m_presentFlow.finish("failed");
  const auto row = m_presentRow;
  const auto ticket = ++m_presentationTicket;
  m_presentRow = {};
  m_windowCheckInFlight = false;
  setPresentation({});
  if (m_connected && !row.path.isEmpty()) {
    m_releaseRow = row;
    m_releaseEpoch = m_epoch;
    windowCommand(row, false, ticket, m_epoch);
  }
}
void MediaCatalog::homeRequested(const QString &epoch, const QString &,
                                 const QString &) {
  if (m_connected && epoch == m_daemonEpoch)
    emit presentationHomeRequested();
}
QVariantMap MediaCatalog::resumeTarget(const QString &id) const {
  const auto *row = findId(id);
  if (!row || row->state != QLatin1String("stopped") || row->contentLocal.isEmpty())
    return {};
  return {{QStringLiteral("provider"), row->contentProvider},
          {QStringLiteral("local"), row->contentLocal}};
}
QVariantMap MediaCatalog::libraryContent(const QString &id) const {
  const auto *row = findId(id);
  if (!row || row->contentLocal.isEmpty())
    return {};
  return {{QStringLiteral("provider"), row->contentProvider},
          {QStringLiteral("local"), row->contentLocal}};
}
void MediaCatalog::linksChanged(const QString &epoch, qulonglong revision) {
  if (!m_connected || m_daemonOwner.isEmpty())
    return;
  // A newer revision invalidates the applied links now: until a reply at
  // that revision is applied, both cards stay, whatever the read returns.
  if (epoch == m_daemonEpoch && revision > m_linksRevision) {
    m_linksWanted = qMax(m_linksWanted, quint64(revision));
    if (!m_links.isEmpty()) {
      m_links.clear();
      publish();
    }
  }
  readLinks(m_daemonOwner, m_epoch);
}
// Optional (ADR-0096): a daemon without MediaAssociation1, an error or a
// snapshot of another daemon epoch leaves no link, so both cards stay.
void MediaCatalog::readLinks(const QString &owner, quint64 epoch) {
  auto call = QDBusMessage::createMethodCall(
      owner, path, QStringLiteral("org.arinn.MediaAssociation1"),
      QStringLiteral("Get"));
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *watcher) {
            const auto reply = watcher->reply();
            watcher->deleteLater();
            if (epoch != m_epoch)
              return;
            QList<LinkRow> links;
            quint64 applied = 0;
            if (reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == QLatin1String("(sta(ssssss))") &&
                !reply.arguments().isEmpty()) {
              const auto arg =
                  qvariant_cast<QDBusArgument>(reply.arguments().at(0));
              QString daemonEpoch;
              qulonglong revision = 0;
              arg.beginStructure();
              arg >> daemonEpoch >> revision;
              arg.beginArray();
              while (!arg.atEnd()) {
                QString mediaProvider, mediaGeneration, session,
                    browserProvider, browserGeneration, tab;
                arg.beginStructure();
                arg >> mediaProvider >> mediaGeneration >> session >>
                    browserProvider >> browserGeneration >> tab;
                arg.endStructure();
                if (links.size() < maxLinks)
                  links.append(LinkRow{mediaProvider, mediaGeneration, session,
                                       tabPresentationId(browserProvider,
                                                         browserGeneration,
                                                         tab)});
              }
              arg.endArray();
              arg.endStructure();
              if (daemonEpoch != m_daemonEpoch || revision == 0)
                links.clear();
              else if (revision < qMax(m_linksRevision, m_linksWanted))
                return; // Older than one applied or announced.
              else
                applied = revision;
            }
            // A failed read keeps the last applied revision, so an older
            // reply still in flight cannot apply after it.
            if (applied)
              m_linksRevision = applied;
            if (links != m_links) {
              m_links = links;
              publish();
            }
          });
}
void MediaCatalog::sessionAction(const QString &id, const QString &requested) {
  const auto *row = findId(id);
  if (!row || !m_connected)
    return;
  const bool playing = row->state == QLatin1String("playing");
  const auto supports = [row](const QString &action) {
    return row->wireActions.contains(wireAction(action));
  };
  QString action = requested;
  if (requested == QLatin1String("playPause")) {
    if (!supports(action))
      action = playing ? QStringLiteral("pause") : QStringLiteral("play");
  } else if (requested == QLatin1String("play") ||
             requested == QLatin1String("pause")) {
    // Without the dedicated action, the toggle stands in only when it moves
    // the session the asked way; Play on a playing session does nothing.
    if (!supports(action)) {
      if ((requested == QLatin1String("play")) == playing)
        return;
      action = QStringLiteral("playPause");
    }
  } else {
    return;
  }
  if (!supports(action) || !methodFor(action)) {
    setOperation(QStringLiteral("Unsupported"));
    return;
  }
  ++m_commandTicket;
  setOperation(QStringLiteral("pending"));
  auto msg = QDBusMessage::createMethodCall(service, row->path,
                                            "org.arinn.Media1", methodFor(action));
  msg << row->generation << row->localId
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  dispatch(msg, m_commandTicket, m_epoch);
}
void MediaCatalog::requestHome() {
  if (!m_connected || m_daemonOwner.isEmpty())
    return;
  auto msg =
      QDBusMessage::createMethodCall(m_daemonOwner, path, service, "Home");
  QDBusConnection::sessionBus().asyncCall(msg, 3000);
}
void MediaCatalog::present() {
  const auto *row = selected();
  if (!row || !windowAvailable())
    return;
  if (inputLayerActive() && m_presentRow.id == row->id &&
      m_presentRow.generation == row->generation)
    return;
  // A new explicit intent supersedes retry state for the previous scope.
  m_releaseRow = {};
  m_presentRow = *row;
  m_windowCheckInFlight = false;
  const auto ticket = ++m_presentationTicket;
  m_presentFlow.start("playback.present");
  m_presentFlow.begin("window.resolve");
  setPresentation(QStringLiteral("pending"));
  resolveWindow(*row, ticket, m_epoch, monotonicUs() + 5000000);
}
void MediaCatalog::resolveWindow(SessionRow row, quint64 ticket, quint64 epoch,
                                 quint64 deadline) {
  if (ticket != m_presentationTicket || epoch != m_epoch)
    return;
  if (monotonicUs() >= deadline) { setPresentation(QStringLiteral("Timeout")); return; }
  auto msg = QDBusMessage::createMethodCall(
      m_daemonOwner, row.path, "org.arinn.MediaWindow1", "GetWindow");
  msg << row.generation << row.localId;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, row, ticket, epoch, deadline](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (ticket != m_presentationTicket || epoch != m_epoch)
              return;
            if (monotonicUs() >= deadline) {
              setPresentation(QStringLiteral("Timeout"));
              return;
            }
            bool found = false;
            if (reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == "a(ss)" && !reply.arguments().isEmpty()) {
              const auto arg =
                  qvariant_cast<QDBusArgument>(reply.arguments().first());
              arg.beginArray();
              if (!arg.atEnd()) {
                QString generation, token;
                arg.beginStructure();
                arg >> generation >> token;
                arg.endStructure();
                found = !generation.isEmpty() && !token.isEmpty();
                if (found) { m_windowGeneration = generation; m_windowToken = token; }
              }
              arg.endArray();
            }
            if (found) {
              m_presentFlow.end("window.resolve");
              m_presentFlow.begin("platform.present");
              windowCommand(row, true, ticket, epoch);
              return;
            }
            if (monotonicUs() >= deadline) {
              setPresentation(QStringLiteral("Unavailable"));
              return;
            }
            QTimer::singleShot(150, this, [this, row, ticket, epoch, deadline] {
              resolveWindow(row, ticket, epoch, deadline);
            });
          });
}
void MediaCatalog::release() {
  const auto row = !m_presentRow.path.isEmpty() ? m_presentRow
      : (m_releaseEpoch == m_epoch ? m_releaseRow : SessionRow{});
  const auto ticket = ++m_presentationTicket;
  setPresentation({});
  m_presentRow = {};
  m_windowCheckInFlight = false;
  if (m_connected && !row.path.isEmpty()) {
    m_releaseRow = row;
    m_releaseEpoch = m_epoch;
    windowCommand(row, false, ticket, m_epoch);
  }
}
void MediaCatalog::failRelease() {
  if (!m_releaseRow.path.isEmpty() && m_releaseEpoch == m_epoch &&
      (m_presentation.isEmpty() || m_presentation == QLatin1String("ReleaseFailed")))
    setPresentation(QStringLiteral("ReleaseFailed"));
}
void MediaCatalog::windowCommand(const SessionRow &row, bool presenting,
                                 quint64 ticket, quint64 epoch, bool detachedRelease) {
  auto msg = QDBusMessage::createMethodCall(m_daemonOwner, row.path,
                                            "org.arinn.MediaWindow1",
                                            presenting ? "Present" : "Release");
  msg << row.generation << row.localId
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, row, presenting, ticket, epoch, detachedRelease](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (epoch != m_epoch)
          return;
        if (ticket != m_presentationTicket) {
          // Admission may have completed after Back. Fence that exact session;
          // never release a newer presentation of the same session.
          if (presenting && m_presentRow.id != row.id)
            windowCommand(row, false, m_presentationTicket, epoch, true);
          return;
        }
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != "o" || reply.arguments().isEmpty()) {
          if (presenting)
            failPresentation(reply.type() == QDBusMessage::ErrorMessage
                                ? errorCode(reply)
                                : QStringLiteral("Unavailable"));
          else if (!detachedRelease) failRelease();
          return;
        }
        // Cleanup for an obsolete admission is independent of the current
        // session's acknowledged Release and retry bookkeeping.
        if (detachedRelease)
          return;
        watchWindowOperation(
            qvariant_cast<QDBusObjectPath>(reply.arguments().first()).path(),
            presenting, ticket, epoch, monotonicUs() + 6000000);
      });
}
void MediaCatalog::watchWindowOperation(const QString &objectPath,
                                        bool presenting, quint64 ticket,
                                        quint64 epoch, quint64 deadline) {
  if (ticket != m_presentationTicket || epoch != m_epoch)
    return;
  if (monotonicUs() >= deadline) {
    if (presenting)
      failPresentation(QStringLiteral("Timeout"));
    else failRelease();
    return;
  }
  auto msg = QDBusMessage::createMethodCall(m_daemonOwner, objectPath,
                                            "org.arinn.Operation1", "Get");
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, objectPath, presenting, ticket, epoch, deadline](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (ticket != m_presentationTicket || epoch != m_epoch)
              return;
            if (monotonicUs() >= deadline) {
              if (presenting)
                failPresentation(QStringLiteral("Timeout"));
              else failRelease();
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "(ssbtta(sss)b)" ||
                reply.arguments().isEmpty()) {
              if (presenting)
                failPresentation(QStringLiteral("Unavailable"));
              else failRelease();
              return;
            }
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().first());
            QString state, error;
            bool retryable, cancellable;
            qulonglong started, ended;
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
            if (state == QLatin1String("queued") ||
                state == QLatin1String("running")) {
              QTimer::singleShot(
                  100, this,
                  [this, objectPath, presenting, ticket, epoch, deadline] {
                    watchWindowOperation(objectPath, presenting, ticket, epoch,
                                         deadline);
                  });
            } else if (presenting) {
              if (state == QLatin1String("succeeded")) setPresentation(QStringLiteral("active"));
              else failPresentation(error.isEmpty() ? state : error);
              if (m_presentation == QLatin1String("active"))
                checkPresentedWindow(ticket, epoch, monotonicUs() + 1000000);
            } else if (state == QLatin1String("succeeded")) {
              m_releaseRow = {};
            } else {
              failRelease();
            }
          });
}

void MediaCatalog::windowChanged(const QString &generation) {
  if (m_presentation == QLatin1String("active") &&
      generation == m_presentRow.generation)
    checkPresentedWindow(m_presentationTicket, m_epoch,
                         monotonicUs() + 1000000);
}
void MediaCatalog::checkPresentedWindow(quint64 ticket, quint64 epoch,
                                        quint64 deadline) {
  if (ticket != m_presentationTicket || epoch != m_epoch ||
      m_presentation != QLatin1String("active"))
    return;
  if (monotonicUs() >= deadline) { abandonPresentation(); return; }
  if (m_windowCheckInFlight)
    return;
  m_windowCheckInFlight = true;
  const auto row = m_presentRow;
  auto msg = QDBusMessage::createMethodCall(
      m_daemonOwner, row.path, "org.arinn.MediaWindow1", "GetWindow");
  msg << row.generation << row.localId;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 1000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, ticket, epoch, deadline](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (ticket != m_presentationTicket || epoch != m_epoch ||
                m_presentation != QLatin1String("active"))
              return;
            m_windowCheckInFlight = false;
            if (monotonicUs() >= deadline) { abandonPresentation(); return; }
            if (reply.type() == QDBusMessage::ErrorMessage) {
              // A field-only inventory commit briefly closes the report
              // barrier.
              if (monotonicUs() < deadline &&
                  errorCode(reply) == QLatin1String("Unavailable")) {
                QTimer::singleShot(100, this, [this, ticket, epoch, deadline] {
                  checkPresentedWindow(ticket, epoch, deadline);
                });
                return;
              }
              abandonPresentation();
              return;
            }
            if (reply.signature() != "a(ss)" || reply.arguments().isEmpty()) {
              abandonPresentation();
              return;
            }
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().first());
            arg.beginArray();
            bool matches = false;
            if (!arg.atEnd()) {
              QString generation, token;
              arg.beginStructure();
              arg >> generation >> token;
              arg.endStructure();
              matches =
                  generation == m_windowGeneration && token == m_windowToken;
            }
            arg.endArray();
            if (!matches) {
              abandonPresentation();
            } else if (m_presentFlow.active()) {
              m_presentFlow.end("window.check");
              m_presentFlow.finish("ok");
            }
          });
}

QByteArray MediaCatalog::surfaceRecord(const QString &surface, const QString &reason,
                                       const QString &presentation, const QString &session,
                                       bool inputLayer, bool windowAvailable) {
  // State names are short identifiers; anything else is not logged as given.
  static const QRegularExpression name(QStringLiteral("^[A-Za-z_-]{0,32}$"));
  const auto fixed = [](const QString &value) {
    return name.match(value).hasMatch() ? QJsonValue(value) : QJsonValue(QStringLiteral("other"));
  };
  const bool failed = surface == QLatin1String("home") && !presentation.isEmpty();
  const QJsonObject record{
    {QStringLiteral("level"), QStringLiteral("info")},
    {QStringLiteral("component"), QStringLiteral("arinn-shell")},
    {QStringLiteral("provider_id"), QJsonValue::Null},
    {QStringLiteral("request_id"), QJsonValue::Null},
    {QStringLiteral("operation"), QStringLiteral("media.surface")},
    {QStringLiteral("duration_ms"), QJsonValue::Null},
    {QStringLiteral("error"), failed ? fixed(presentation) : QJsonValue(QJsonValue::Null)},
    {QStringLiteral("surface"), fixed(surface)},
    {QStringLiteral("reason"), fixed(reason)},
    {QStringLiteral("presentationState"), fixed(presentation)},
    {QStringLiteral("sessionState"), fixed(session)},
    {QStringLiteral("inputLayer"), inputLayer},
    {QStringLiteral("windowAvailable"), windowAvailable},
  };
  return QJsonDocument(record).toJson(QJsonDocument::Compact);
}
void MediaCatalog::logSurface(const QString &surface, const QString &reason) const {
  const auto line = surfaceRecord(surface, reason, presentationState(), sessionState(),
                                  inputLayerActive(), windowAvailable()) + '\n';
  std::fwrite(line.constData(), 1, static_cast<size_t>(line.size()), stderr);
  std::fflush(stderr);
}
void MediaCatalog::failPresentation(const QString &state) {
  // The compositor may have acted before the operation read failed. Fence and
  // teardown the remembered scope even when its outcome is now unknown.
  m_presentFlow.finish(FlowTrace::outcome(state));
  release();
  setPresentation(state);
}
