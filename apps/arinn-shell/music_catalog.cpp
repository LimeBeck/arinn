// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "music_catalog.h"
#include "asset_images.h"
#include <QJsonArray>
#include <algorithm>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQmlEngine>
#include <QUuid>
#include <QtDBus>
#include <algorithm>
namespace {
constexpr auto service = "org.arinn.Daemon1";
QDBusMessage call(const QString &path, const char *interface,
                  const char *method) {
  return QDBusMessage::createMethodCall(service, path, interface, method);
}
QJsonObject object(const QDBusMessage &reply) {
  if (reply.type() == QDBusMessage::ErrorMessage ||
      reply.signature() != QStringLiteral("s") || reply.arguments().size() != 1)
    return {};
  const auto bytes = reply.arguments().first().toString().toUtf8();
  if (bytes.size() > 256 * 1024)
    return {};
  const auto doc = QJsonDocument::fromJson(bytes);
  return doc.isObject() ? doc.object() : QJsonObject();
}
QString code(const QDBusMessage &reply) {
  auto value = reply.errorName().section('.', -1);
  return value.isEmpty() ? QStringLiteral("Unavailable") : value;
}
QVariantMap imageEntity(QVariantMap entity, const QString &id,
                        const QString &path, const QString &generation) {
  entity.insert(QStringLiteral("image"),
                AssetImages::imageUrl(
                    id, path, generation,
                    entity.value(QStringLiteral("artwork")).toString()));
  return entity;
}
QVariantMap imageState(QVariantMap state, const QString &id,
                       const QString &path, const QString &generation) {
  auto entry = [&](QVariantMap value) {
    value.insert(QStringLiteral("track"),
                 imageEntity(value.value(QStringLiteral("track")).toMap(), id,
                             path, generation));
    return value;
  };
  if (!state.value(QStringLiteral("current")).toMap().isEmpty())
    state.insert(QStringLiteral("current"),
                 entry(state.value(QStringLiteral("current")).toMap()));
  for (const auto *key : {"history", "upcoming"}) {
    QVariantList values;
    for (const auto &value : state.value(key).toList())
      values.append(entry(value.toMap()));
    state.insert(key, values);
  }
  return state;
}
// The music sources this repository ships, named in the shell's language
// wherever a source is named (spec §5.6, #838). Matched by plugin id and name
// together, as Settings does, so a third-party source keeps its own name.
QString sourceName(const QString &plugin, const QString &name) {
  static const struct {
    const char *plugin, *name;
  } known[] = {
      {"music-local", QT_TRANSLATE_NOOP("MusicCatalog", "Local music")},
      {"yandex-music", QT_TRANSLATE_NOOP("MusicCatalog", "Yandex Music")},
  };
  for (const auto &entry : known)
    if (plugin == QLatin1String(entry.plugin) && name == QLatin1String(entry.name))
      return QCoreApplication::translate("MusicCatalog", entry.name);
  return name;
}
void registerMusic() {
  qmlRegisterType<MusicCatalog>("Arinn.Shell", 1, 0, "MusicCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerMusic)
} // namespace
MusicCatalog::MusicCatalog(QObject *parent) : QObject(parent) {
  // The derived lines read the status, the browsed state and the session;
  // the one-second poll re-emits those, so notify only on a real change.
  for (auto changed :
       {&MusicCatalog::statusChanged, &MusicCatalog::stateChanged,
        &MusicCatalog::activeChanged})
    connect(this, changed, this, &MusicCatalog::updateProblems);
  auto *watch =
      new QDBusServiceWatcher(service, QDBusConnection::sessionBus(),
                              QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watch, &QDBusServiceWatcher::serviceOwnerChanged, this, [this] {
    ++m_commandTicket;
    ++m_epoch;
    ++m_pageTicket;
    ++m_stateTicket;
    ++m_registryTicket;
    m_registryBusy = m_pageBusy = m_stateBusy = m_commandBusy = m_activeBusy =
        m_commandWaitingRefresh = false;
    m_uncertainStart = false;
    m_path.clear();
    m_generation.clear();
    m_sources.clear();
    m_entries.clear();
    m_state.clear();
    m_active.clear();
    m_activeId.clear();
    m_activeGeneration.clear();
    m_next.clear();
    emit activeChanged();
    emit sourcesChanged();
    emit entriesChanged();
    emit stateChanged();
    emit statusChanged();
    refresh();
  });
  m_timer.setInterval(1000);
  connect(&m_timer, &QTimer::timeout, this, &MusicCatalog::refresh);
  // A pin mirror waits while another command is in flight (#804).
  connect(this, &MusicCatalog::statusChanged, this, &MusicCatalog::mirrorPin);
  connect(this, &MusicCatalog::activeChanged, this, &MusicCatalog::mirrorPin);
  connect(this, &MusicCatalog::sourcesChanged, this, &MusicCatalog::mirrorPin);
  m_timer.start();
  QTimer::singleShot(0, this, &MusicCatalog::refresh);
}
QString MusicCatalog::sectionTitle() const { return tr("Music"); }
// "Source · Station · Playing": the station tells a continuous station from an
// album or a playlist on the Home card (music brief, #756 M24).
QString MusicCatalog::statusText(const QString &status) const {
  const auto station = m_active.value(QStringLiteral("station"))
                           .toMap()
                           .value(QStringLiteral("title"))
                           .toString();
  auto source = activeSourceName();
  if (!station.isEmpty())
    source = source.isEmpty() ? station : tr("%1 · %2").arg(source, station);
  return source.isEmpty() ? tr("Music · %1").arg(status)
                          : tr("%1 · %2").arg(source, status);
}
QVariantList MusicCatalog::homeEntries() const {
  const auto current = m_active.value("current").toMap();
  if (current.isEmpty())
    return {};
  const auto track = current.value("track").toMap();
  const auto playback = m_active.value("playback").toString();
  const bool playing = playback == QStringLiteral("playing"),
             paused = playback == QStringLiteral("paused");
  const auto status = playing                                 ? tr("Playing")
                      : paused                                ? tr("Paused")
                      : playback == QStringLiteral("loading") ? tr("Loading…")
                                                              : tr("Stopped");
  // The Continue card's variant 1 (brief *Presence*, MH-2, #803): the
  // caption names the artist and the station or album; a missing cover takes
  // the station's; the Ember line shows the position.
  const auto station = m_active.value("station").toMap();
  const auto context = !station.value("title").toString().isEmpty()
                      ? station.value("title").toString()
                      : track.value("album").toString();
  QStringList subtitle;
  for (const auto &part : {track.value("artist").toString(), context})
    if (!part.isEmpty())
      subtitle.append(part);
  auto artwork = track.value("image").toString();
  if (track.value("artwork").toString().isEmpty() &&
      !station.value("artwork").toString().isEmpty()) {
    for (const auto &row : m_sources) {
      const auto source = row.toMap();
      // Only the session's own generation: a stale one would name a cover
      // that cannot load (review of #827).
      if (source.value("id").toString() == m_activeId &&
          source.value("generation").toString() == m_activeGeneration)
        artwork = AssetImages::imageUrl(
            m_activeId, source.value("path").toString(), m_activeGeneration,
            station.value("artwork").toString());
    }
  }
  const auto duration = track.value("duration_us").toDouble();
  const auto position = m_active.value("position_us").toDouble();
  QVariantMap entry{{"id", sessionEntryId()},
                    {"kind", QStringLiteral("music")},
                    {"title", track.value("title")},
                    {"subtitle", subtitle.join(QStringLiteral(" · "))},
                    {"artist", track.value("artist")},
                    {"station", station.value("title")},
                    {"musicState", playing  ? QStringLiteral("playing")
                                   : paused ? QStringLiteral("paused")
                                            : QString()},
                    {"artwork", artwork},
                    // The source's mark when there is no cover at all, as the
                    // music screens draw it (MusicSourceMark): this computer
                    // or a service (Codex review of #827).
                    {"sourceIcon",
                     m_active.value("account").toString() == QStringLiteral("local")
                         ? QStringLiteral("screen")
                         : QStringLiteral("public")},
                    {"statusText", statusText(status)},
                    {"secondaryIcon", playing ? QStringLiteral("pause")
                                              : QStringLiteral("play")},
                    {"secondaryText", playing ? tr("Pause") : tr("Play")},
                    {"secondaryEnabled", playing || paused},
                    {"live", playing || paused},
                    // Play/Pause has something to do: the session offers the
                    // action its state needs (Codex review of #827).
                    {"playPause",
                     m_active.value("actions").toStringList().contains(
                         playing ? QStringLiteral("pause") : QStringLiteral("resume"))},
                    {"busy", m_commandBusy}};
  if ((playing || paused) && duration > 0)
    entry.insert("progress", std::clamp(position / duration, 0.0, 1.0));
  return {entry};
}
void MusicCatalog::fail(const QString &value) {
  if (value == QStringLiteral("Conflict"))
    m_error = tr("The music source changed. Try again.");
  else if (value == QStringLiteral("Timeout"))
    m_error = tr("The music source did not respond.");
  else if (value == QStringLiteral("Unsupported"))
    m_error = tr("This action is not available.");
  else
    m_error = tr("The music source is unavailable.");
  emit statusChanged();
}
void MusicCatalog::refresh() {
  readState();
  readActive();
  if (m_registryBusy)
    return;
  m_registryBusy = true;
  providers(QString(), ++m_registryTicket, QString(), {}, {});
}
void MusicCatalog::providers(const QString &token, quint64 ticket,
                             const QString &snapshot, QVariantList rows,
                             QSet<QString> tokens) {
  if (tokens.size() >= 32 || rows.size() > 1024) {
    m_registryBusy = false;
    fail(QStringLiteral("Unavailable"));
    return;
  }
  if (!token.isEmpty() && tokens.contains(token)) {
    m_registryBusy = false;
    fail(QStringLiteral("Conflict"));
    return;
  }
  tokens.insert(token);
  auto message = call(QStringLiteral("/org/arinn/Daemon1"), "org.arinn.Daemon1",
                      "Providers");
  message << token;
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, ticket, snapshot, rows, tokens](auto *watch) mutable {
        const auto reply = watch->reply();
        watch->deleteLater();
        if (ticket != m_registryTicket)
          return;
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != QStringLiteral("(sa(sssssoasssststttt)s)")) {
          m_registryBusy = false;
          fail(code(reply));
          return;
        }
        const auto arg =
            qvariant_cast<QDBusArgument>(reply.arguments().first());
        QString seen, next;
        arg.beginStructure();
        arg >> seen;
        if (!snapshot.isEmpty() && snapshot != seen) {
          m_registryBusy = false;
          fail(QStringLiteral("Conflict"));
          return;
        }
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, plugin, source, name, version, runtime, backend,
              generation, error;
          QStringList capabilities;
          QDBusObjectPath endpoint;
          qulonglong revision, errorAt, recovered, restarts, cumulative;
          arg.beginStructure();
          arg >> id >> plugin >> source >> name >> version >> endpoint >>
              capabilities >> runtime >> backend >> generation >> revision >>
              error >> errorAt >> recovered >> restarts >> cumulative;
          arg.endStructure();
          if (capabilities.contains(QStringLiteral("music")))
            rows.append(
                QVariantMap{{"id", id},
                            {"name", sourceName(plugin, name)},
                            {"path", endpoint.path()},
                            {"generation", generation},
                            {"ready", runtime == QStringLiteral("ready")}});
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty()) {
          providers(next, ticket, seen, rows, tokens);
          return;
        }
        m_registryBusy = false;
        if (m_sources != rows) {
          m_sources = rows;
          forgetStaleUnresolved();
          emit sourcesChanged();
          emit activeChanged();
        }
        QString selected = m_sourceId;
        bool present = false;
        for (const auto &row : rows)
          if (row.toMap().value("id").toString() == selected)
            present = true;
        if (!present)
          selected = rows.isEmpty()
                         ? QString()
                         : rows.first().toMap().value("id").toString();
        choose(selected);
      });
}
// A provider that restarted or left may know its stations again: its
// unresolved stations are asked again on the next start (#804).
QVariantList MusicCatalog::unresolvedStations() const {
  QVariantList rows;
  for (auto it = m_unresolved.cbegin(); it != m_unresolved.cend(); ++it)
    rows.append(QVariant(QStringList{it.key().first, it.key().second}));
  return rows;
}
void MusicCatalog::forgetStaleUnresolved() {
  QHash<QString, QString> generations;
  for (const auto &row : std::as_const(m_sources)) {
    const auto source = row.toMap();
    generations.insert(source.value("id").toString(),
                       source.value("generation").toString());
  }
  bool changed = false;
  for (auto it = m_unresolved.begin(); it != m_unresolved.end();) {
    if (generations.value(it.key().first) != it.value()) {
      it = m_unresolved.erase(it);
      changed = true;
    } else {
      ++it;
    }
  }
  if (changed)
    emit unresolvedChanged();
}
void MusicCatalog::choose(const QString &id) {
  QVariantMap selected;
  for (const auto &row : m_sources)
    if (row.toMap().value("id") == id) {
      selected = row.toMap();
      break;
    }
  const auto path = selected.value("path").toString(),
             generation = selected.value("generation").toString();
  if (m_sourceId == id && m_path == path && m_generation == generation)
    return;
  m_uncertainStart = false;
  ++m_epoch;
  ++m_pageTicket;
  ++m_stateTicket;
  m_pageBusy = m_stateBusy = m_activeBusy = false;
  m_sourceId = id;
  m_path = path;
  m_generation = generation;
  m_facet.clear();
  m_catalogueRevision = 0;
  m_entries.clear();
  m_state.clear();
  m_next.clear();
  m_error.clear();
  m_stateError = false;
  m_parent.clear();
  emit sourcesChanged();
  emit entriesChanged();
  emit stateChanged();
  emit statusChanged();
  emit browseReset();
  readState();
}
void MusicCatalog::readState() {
  if (m_path.isEmpty() || m_generation.isEmpty() || m_stateBusy ||
      (m_commandBusy && !m_commandWaitingRefresh))
    return;
  m_stateBusy = true;
  const auto epoch = m_epoch, ticket = ++m_stateTicket;
  auto message = call(m_path, "org.arinn.Music1", "GetState");
  message << m_generation;
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 3500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, ticket](auto *watch) {
            const auto reply = watch->reply();
            watch->deleteLater();
            if (epoch != m_epoch || ticket != m_stateTicket)
              return;
            m_stateBusy = false;
            const auto json = object(reply);
            if (json.isEmpty() || !json.contains("revision") ||
                !json.value("actions").isArray()) {
              fail(code(reply));
              m_stateError = true;
              return;
            }
            // A transient state error clears once the state reads again.
            if (m_stateError) {
              m_stateError = false;
              m_error.clear();
            }
            const auto revision =
                json.value("catalogue_revision").toVariant().toULongLong();
            m_state = imageState(json.toVariantMap(), m_sourceId, m_path,
                                 m_generation);
            emit stateChanged();
            emit statusChanged();
            const auto facets = json.value("facets").toArray();
            // A source without facets (an account that is not connected, or one
            // without a catalogue) has nothing to page: asking only yields
            // Unsupported under the Connect prompt (review of #710).
            if (facets.isEmpty()) {
              m_catalogueRevision = revision;
              if (!m_entries.isEmpty() || !m_next.isEmpty()) {
                m_entries.clear();
                m_next.clear();
                emit entriesChanged();
                emit statusChanged();
              }
              return;
            }
            if (m_catalogueRevision != revision) {
              m_catalogueRevision = revision;
              bool found = false;
              for (const auto &facet : facets)
                if (facet.toString() == m_facet)
                  found = true;
              if (!found)
                m_facet = facets.first().toString();
              readPage(false);
            }
          });
}
void MusicCatalog::browse(const QString &facet, const QString &parent,
                          const QString &search) {
  if (!QStringList{"tracks", "albums", "artists", "playlists", "selections"}
           .contains(facet) ||
      search.toUtf8().size() > 1024)
    return;
  if (m_facet != facet || m_parent != parent || m_search != search) {
    m_entries.clear();
    emit entriesChanged();
  }
  m_facet = facet;
  m_parent = parent;
  m_search = search;
  readPage(false);
}
void MusicCatalog::loadMore() {
  if (!m_pageBusy && !m_next.isEmpty())
    readPage(true);
}
void MusicCatalog::readPage(bool append) {
  if (m_path.isEmpty() || m_generation.isEmpty())
    return;
  const auto epoch = m_epoch, ticket = ++m_pageTicket;
  QString token = append ? m_next : QString();
  if (!append) {
    m_pageTokens.clear();
    m_pageRevision = 0;
  }
  if (!token.isEmpty() && m_pageTokens.contains(token)) {
    fail(QStringLiteral("Conflict"));
    emit pageFailed();
    return;
  }
  m_pageBusy = true;
  emit statusChanged();
  auto message = call(m_path, "org.arinn.Music1", "GetPage");
  const QJsonObject query{{"facet", m_facet},
                          {"parent", m_parent},
                          {"search", m_search},
                          {"token", token}};
  message << m_generation
          << QString::fromUtf8(
                 QJsonDocument(query).toJson(QJsonDocument::Compact));
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 5500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, ticket, append, token](auto *watch) {
            const auto reply = watch->reply();
            watch->deleteLater();
            if (epoch != m_epoch || ticket != m_pageTicket)
              return;
            m_pageBusy = false;
            const auto json = object(reply);
            const auto revision =
                json.value("revision").toVariant().toULongLong();
            if (json.isEmpty() ||
                json.value("generation").toString() != m_generation ||
                !json.value("entries").isArray() || revision == 0 ||
                (append && revision != m_pageRevision)) {
              fail(code(reply));
              emit pageFailed();
              return;
            }
            auto entries = append ? m_entries : QVariantList();
            QSet<QString> ids;
            for (const auto &row : entries)
              ids.insert(row.toMap().value("id").toString());
            const auto page = json.value("entries").toArray();
            if (page.size() > 100 || entries.size() + page.size() > 100000) {
              fail(QStringLiteral("Unavailable"));
              emit pageFailed();
              return;
            }
            for (const auto &item : page) {
              const auto row = imageEntity(item.toObject().toVariantMap(),
                                           m_sourceId, m_path, m_generation);
              const auto id = row.value("id").toString();
              if (id.isEmpty() || ids.contains(id)) {
                fail(QStringLiteral("Conflict"));
                emit pageFailed();
                return;
              }
              ids.insert(id);
              entries.append(row);
            }
            m_entries = entries;
            m_pageRevision = revision;
            m_next = json.value("next_token").toString();
            if (!token.isEmpty())
              m_pageTokens.insert(token);
            m_error.clear();
            emit entriesChanged();
            emit statusChanged();
          });
}
QString MusicCatalog::activeSourceName() const {
  for (const auto &row : m_sources) {
    const auto source = row.toMap();
    if (source.value("id").toString() == m_activeId)
      return source.value("name").toString();
  }
  return {};
}
void MusicCatalog::readActive() {
  if (m_activeBusy)
    return;
  m_activeBusy = true;
  const auto epoch = m_epoch;
  // A read asked after a command ended describes the session after it.
  // A read asked while an uncertain command may still be settling does not
  // count as the session after it.
  const auto commandsDone =
      m_activeFenceAfter.hasExpired() ? m_commandsDone : m_activeSeen;
  auto message = call(QStringLiteral("/org/arinn/Daemon1"),
                      "org.arinn.MusicController1", "GetActive");
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, epoch, commandsDone](auto *watch) {
        const auto reply = watch->reply();
        watch->deleteLater();
        if (epoch != m_epoch)
          return;
        m_activeBusy = false;
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != QStringLiteral("s")) {
          if (m_commandWaitingRefresh) {
            m_commandBusy = m_commandWaitingRefresh = false;
            emit statusChanged();
          }
          return;
        }
        if (m_commandWaitingRefresh) {
          m_commandBusy = m_commandWaitingRefresh = false;
          emit statusChanged();
        }
        // Only a valid answer, a session or none, counts as the session after
        // the command; a malformed one leaves the mirror waiting (Codex
        // review of #836).
        if (reply.arguments().first().toString() == QStringLiteral("null")) {
          m_active.clear();
          m_activeId.clear();
          m_activeGeneration.clear();
          m_activeSeen = std::max(m_activeSeen, commandsDone);
          emit activeChanged();
          return;
        }
        const auto json = object(reply);
        if (json.isEmpty())
          return;
        m_active = json.value("state").toObject().toVariantMap();
        m_activeId = json.value("provider_id").toString();
        m_activeGeneration = json.value("generation").toString();
        m_activeSeen = std::max(m_activeSeen, commandsDone);
        QString path;
        for (const auto &row : m_sources) {
          const auto source = row.toMap();
          if (source.value("id").toString() == m_activeId &&
              source.value("generation").toString() == m_activeGeneration) {
            path = source.value("path").toString();
            break;
          }
        }
        m_active = imageState(m_active, m_activeId, path, m_activeGeneration);
        if (m_uncertainStart && m_activeId == m_startProvider &&
            m_activeGeneration == m_startGeneration &&
            !m_active.value("session").toString().isEmpty() &&
            m_active.value("session").toString() != m_startSession &&
            m_active.value("error").isNull() &&
            QStringList{"loading", "playing", "paused"}.contains(
                m_active.value("playback").toString())) {
          m_uncertainStart = false;
          m_error.clear();
          emit statusChanged();
        }
        emit activeChanged();
      });
}
void MusicCatalog::command(const QString &action, const QString &entity,
                           const QString &occurrence, qint64 positionUs,
                           const QString &text) {
  send(action, entity, occurrence, positionUs, false, text);
}
void MusicCatalog::transport(const QString &action, const QString &entity,
                             const QString &occurrence, qint64 positionUs,
                             const QString &text) {
  send(action, entity, occurrence, positionUs, true, text);
}
void MusicCatalog::send(const QString &action, const QString &entity,
                        const QString &occurrence, qint64 positionUs,
                        bool active, const QString &text) {
  if (m_commandBusy || positionUs < 0)
    return;
  auto endpoint = m_path, generation = m_generation;
  auto state = m_state;
  if (active) {
    bool found = false;
    for (const auto &row : m_sources) {
      const auto source = row.toMap();
      if (source.value("id").toString() == m_activeId) {
        if (source.value("generation").toString() != m_activeGeneration)
          return;
        endpoint = source.value("path").toString();
        generation = m_activeGeneration;
        state = m_active;
        found = true;
        break;
      }
    }
    if (!found)
      return;
  }
  if (endpoint.isEmpty() || generation.isEmpty() ||
      !state.value("actions").toStringList().contains(action))
    return;
  m_uncertainStart = false;
  m_commandBusy = true;
  m_commandWaitingRefresh = false;
  m_error.clear();
  ++m_stateTicket;
  m_stateBusy = false;
  emit statusChanged();
  const auto epoch = m_epoch, commandTicket = ++m_commandTicket;
  const QJsonObject payload{
      {"request_id", QUuid::createUuid().toString(QUuid::WithoutBraces)},
      {"action", action},
      {"session", state.value("session").toString()},
      {"entity", entity},
      {"occurrence", occurrence},
      {"position_us", positionUs},
      {"replaces_provider", m_activeId},
      {"replaces_session", m_active.value("session").toString()},
      {"text", text}};
  auto message = call(endpoint, "org.arinn.Music1", "Command");
  message << generation
          << QString::fromUtf8(
                 QJsonDocument(payload).toJson(QJsonDocument::Compact));
  const auto provider = active ? m_activeId : m_sourceId,
             oldSession = state.value("session").toString();
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 5500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, active, commandTicket, action, provider, generation,
           oldSession, entity](auto *watch) {
            const auto reply = watch->reply();
            watch->deleteLater();
            if (commandTicket != m_commandTicket)
              return;
            if (epoch != m_epoch) {
              m_commandBusy = false;
              ++m_commandsDone;
              emit statusChanged();
              readActive();
              return;
            }
            const auto json = object(reply);
            const QPair<QString, QString> station{provider, entity};
            if (json.isEmpty()) {
              const auto failure = code(reply);
              // The provider no longer knows a stored station, or cannot
              // tell (music.md: NotFound where it can, otherwise
              // Unavailable): a pin of it shows as unavailable instead of
              // being retried. A new generation of the provider asks again
              // (#804).
              if (action == "radio" &&
                  (failure == "NotFound" || failure == "Unavailable") &&
                  m_unresolved.value(station) != generation) {
                m_unresolved.insert(station, generation);
                emit unresolvedChanged();
              }
              if ((action == "play" || action == "radio") &&
                  (failure == "Timeout" || failure == "NoReply")) {
                m_uncertainStart = true;
                m_startProvider = provider;
                m_startGeneration = generation;
                m_startSession = oldSession;
              }
              ++m_commandsDone;
              // A command that timed out may have been applied while the
              // daemon still reports the session before it: only a read
              // asked after the provider's next polls counts (Codex review
              // of #836).
              if (failure == "Timeout" || failure == "NoReply")
                m_activeFenceAfter.setRemainingTime(2500);
              fail(failure);
              m_commandWaitingRefresh = true;
              m_stateBusy = false;
              ++m_stateTicket;
              readState();
              readActive();
              return;
            }
            m_commandBusy = false;
            ++m_commandsDone;
            if (action == "radio" && m_unresolved.remove(station))
              emit unresolvedChanged();
            if (!active) {
              m_state = imageState(json.toVariantMap(), m_sourceId, m_path,
                                   m_generation);
              emit stateChanged();
            }
            emit statusChanged();
            readActive();
          });
}

bool MusicCatalog::stationResolved(const QString &providerId,
                                   const QString &stationId) const {
  return !m_unresolved.contains({providerId, stationId});
}

QString MusicCatalog::artworkUrl(const QString &providerId,
                                 const QString &artworkId) const {
  for (const auto &row : m_sources) {
    const auto source = row.toMap();
    if (source.value("id").toString() == providerId)
      return AssetImages::imageUrl(providerId, source.value("path").toString(),
                                   source.value("generation").toString(),
                                   artworkId);
  }
  return {};
}
void MusicCatalog::syncProviderPin(const QString &providerId,
                                   const QString &stationId, bool pinned) {
  // Only the playing station's change is kept: one favorites read may
  // report several stations, and another station's must not replace a
  // mirror still waiting (Codex review of #836).
  if (m_activeId != providerId ||
      m_active.value(QStringLiteral("station")).toMap().value(QStringLiteral("id")).toString() !=
          stationId)
    return;
  m_pinMirror = PinMirror{providerId, stationId, pinned};
  mirrorPin();
}
// The provider's own pin covers only the station that plays (`pinned` in its
// state); Arinn mirrors its pin there where the provider offers it. While
// another command is in flight the mirror waits for it (Codex review of
// #836); a station that stopped playing drops it.
void MusicCatalog::mirrorPin() {
  // While a command is in flight, and until the session is read again after
  // it, the active state may describe the session before the command: Stop
  // or a new station (Codex review of #836).
  if (!m_pinMirror || m_commandBusy || m_activeSeen < m_commandsDone)
    return;
  const auto station = m_active.value(QStringLiteral("station")).toMap();
  if (m_activeId != m_pinMirror->provider ||
      station.value(QStringLiteral("id")).toString() != m_pinMirror->station ||
      m_active.value(QStringLiteral("pinned")).toBool() == m_pinMirror->pinned ||
      !m_active.value(QStringLiteral("actions")).toStringList().contains(
          QStringLiteral("pin"))) {
    m_pinMirror.reset();
    return;
  }
  // Kept until transport sends it: a provider that restarted before the
  // sources caught up refuses the command, and the next sources or state
  // change tries again (Codex review of #836).
  const auto mirror = *m_pinMirror;
  m_pinMirror.reset();
  transport(QStringLiteral("pin"), mirror.station);
  if (!m_commandBusy)
    m_pinMirror = mirror;
}

QString MusicCatalog::sessionEntryId() {
  return QStringLiteral("arinn-music-session");
}

void MusicCatalog::updateProblems() {
  auto nowPlaying = this->nowPlaying();
  auto problem = playbackProblem();
  auto notice = this->notice();
  if (nowPlaying == m_nowPlaying && problem == m_playbackProblem &&
      notice == m_notice)
    return;
  m_nowPlaying = std::move(nowPlaying);
  m_playbackProblem = std::move(problem);
  m_notice = std::move(notice);
  emit problemsChanged();
}

bool MusicCatalog::audible() const {
  const auto playback = m_active.value(QStringLiteral("playback")).toString();
  return playback == QStringLiteral("playing") ||
         playback == QStringLiteral("loading");
}

QVariantMap MusicCatalog::nowPlaying() const {
  const auto current = m_active.value(QStringLiteral("current")).toMap();
  const auto playback = m_active.value(QStringLiteral("playback")).toString();
  const bool playing = playback == QStringLiteral("playing");
  if (current.isEmpty() || (!playing && playback != QStringLiteral("paused")))
    return {};
  const auto track = current.value(QStringLiteral("track")).toMap();
  const auto actions = m_active.value(QStringLiteral("actions")).toStringList();
  return {{QStringLiteral("id"), sessionEntryId()},
          {QStringLiteral("kind"), QStringLiteral("music")},
          {QStringLiteral("title"), track.value(QStringLiteral("title"))},
          {QStringLiteral("subtitle"), track.value(QStringLiteral("artist"))},
          {QStringLiteral("state"), playback},
          // Not tied to a command in flight: the row would lose focus while
          // it waits, and the catalogue drops a second press anyway.
          {QStringLiteral("playPause"),
           actions.contains(playing ? QStringLiteral("pause")
                                    : QStringLiteral("resume"))}};
}

QString MusicCatalog::statusLine() const {
  const auto session = nowPlaying();
  if (session.value(QStringLiteral("state")).toString() != QStringLiteral("playing"))
    return {};
  QStringList parts;
  for (const auto &key : {"title", "subtitle"}) {
    const auto part = session.value(QLatin1String(key)).toString();
    if (!part.isEmpty())
      parts.append(part);
  }
  return parts.join(QStringLiteral(" · "));
}

void MusicCatalog::togglePlayback() {
  const auto playback = m_active.value(QStringLiteral("playback")).toString();
  if (playback == QStringLiteral("playing"))
    transport(QStringLiteral("pause"));
  else if (playback == QStringLiteral("paused"))
    transport(QStringLiteral("resume"));
}

QString MusicCatalog::playbackProblem() const {
  const auto error = m_active.value(QStringLiteral("error")).toString();
  const auto playback = m_active.value(QStringLiteral("playback")).toString();
  // A provider keeps its last error after it skipped to the next track: only
  // a stopped session failed. One that plays, is paused, loads or ended
  // naturally has recovered (reviews of #797; provider side #810).
  if (error.isEmpty() || playback != QStringLiteral("stopped"))
    return {};
  if (error == QStringLiteral("NotFound"))
    return tr("The track was not found in this source.");
  if (error == QStringLiteral("PermissionDenied"))
    return tr("This source no longer allows playing the track. Sign in again "
              "in Settings.");
  if (error == QStringLiteral("Timeout"))
    return tr("The music source did not respond in time.");
  if (error == QStringLiteral("Unsupported"))
    return tr("This track cannot be played here.");
  return tr("The track could not be played.");
}

QString MusicCatalog::notice() const {
  if (!m_error.isEmpty())
    return m_error;
  if (m_state.value(QStringLiteral("account")).toString() ==
      QStringLiteral("restricted"))
    return tr("Music access was denied.");
  const auto problem = playbackProblem();
  if (!problem.isEmpty())
    return tr("Playback stopped. %1").arg(problem);
  // A degraded source still browses; some of its content is missing.
  if (m_state.value(QStringLiteral("backend")).toString() ==
          QStringLiteral("degraded") &&
      !m_state.value(QStringLiteral("error")).toString().isEmpty() &&
      !m_state.value(QStringLiteral("catalogue_loading")).toBool())
    return tr("Part of this source is unavailable. Refresh to try again.");
  return {};
}
