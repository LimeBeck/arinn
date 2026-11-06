// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "music_room.h"
#include "asset_images.h"
#include <QCollator>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQmlEngine>
#include <QUuid>
#include <QtDBus>
#include <algorithm>
#include <chrono>

namespace {
constexpr auto service = "org.arinn.Daemon1";
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
// The shelves of the root, in the brief's order (mh-01). Personal entries of
// the shelf's kind come from the `selections` facet; ordinary ones from the
// shelf's own facet, if any.
struct Shelf {
  const char *id, *kind, *facet;
  int limit;
};
constexpr Shelf shelfDefs[] = {{"stations", "station", nullptr, 24},
                               {"forYou", "playlist", "playlists", 24},
                               {"albums", "album", "albums", 24},
                               {"artists", "artist", "artists", 24},
                               {"recent", "track", "tracks", 12}};
const Shelf *shelfDef(const QString &id) {
  for (const auto &shelf : shelfDefs)
    if (id == QLatin1String(shelf.id))
      return &shelf;
  return nullptr;
}
QString shelfTitle(const QString &id) {
  if (id == QLatin1String("stations"))
    return MusicRoom::tr("Stations");
  if (id == QLatin1String("forYou"))
    return MusicRoom::tr("For you");
  if (id == QLatin1String("albums"))
    return MusicRoom::tr("Albums");
  if (id == QLatin1String("artists"))
    return MusicRoom::tr("Artists");
  return MusicRoom::tr("Recent tracks");
}
// A source browses only once it can page: an account that is not connected
// has no facets.
bool browsable(const QVariantMap &state) {
  return !state.value(QStringLiteral("facets")).toList().isEmpty();
}
qint64 now() {
  return std::chrono::duration_cast<std::chrono::milliseconds>(
             std::chrono::steady_clock::now().time_since_epoch())
      .count();
}
// The daemon keeps a continuation for 30 s; past this age a page is read
// again from the start instead.
constexpr qint64 leaseMs = 25000;
// A first page that failed is read again after this long, doubling with
// each failure up to a minute.
constexpr qint64 retryMs = 3000;
qint64 retryDelay(int retries) {
  return std::min<qint64>(retryMs << std::min(retries, 5), 60000);
}
// The facets a shelf pages: its personal entries in `selections`, and its
// own facet when it has one.
QStringList pagedFacets(const char *facet) {
  QStringList facets{QStringLiteral("selections")};
  if (facet)
    facets.append(QLatin1String(facet));
  return facets;
}
void registerRoom() {
  qmlRegisterType<MusicRoom>("Arinn.Shell", 1, 0, "MusicRoom");
}
Q_COREAPP_STARTUP_FUNCTION(registerRoom)
} // namespace

MusicRoom::MusicRoom(QObject *parent) : QObject(parent) {
  m_timer.setInterval(2000);
  connect(&m_timer, &QTimer::timeout, this, &MusicRoom::refresh);
  m_timer.start();
}

void MusicRoom::setCatalog(QObject *catalog) {
  if (m_catalog == catalog)
    return;
  if (m_catalog)
    disconnect(m_catalog, nullptr, this, nullptr);
  m_catalog = catalog;
  if (m_catalog && m_catalog->metaObject()->indexOfSignal("sourcesChanged()") >= 0)
    connect(m_catalog, SIGNAL(sourcesChanged()), this, SLOT(follow()));
  emit catalogChanged();
  follow();
}

void MusicRoom::follow() {
  const auto rows =
      m_catalog ? m_catalog->property("sources").toList() : QVariantList();
  QStringList order;
  for (const auto &value : rows) {
    const auto row = value.toMap();
    const auto id = row.value(QStringLiteral("id")).toString();
    if (id.isEmpty() || order.contains(id))
      continue;
    order.append(id);
    auto &source = m_sources[id];
    const auto path = row.value(QStringLiteral("path")).toString(),
               generation = row.value(QStringLiteral("generation")).toString();
    source.id = id;
    source.name = row.value(QStringLiteral("name")).toString();
    source.ready = row.value(QStringLiteral("ready")).toBool();
    if (source.path != path || source.generation != generation) {
      // A new generation is a new catalogue: nothing read before applies.
      ++source.ticket;
      source.path = path;
      source.generation = generation;
      source.state.clear();
      source.facets.clear();
      source.pending.clear();
      source.catalogueRevision = 0;
      source.stateBusy = source.commandBusy = false;
      source.answered = false;
    }
  }
  // A source that left takes its entries along (brief: a source drops out).
  for (auto it = m_sources.begin(); it != m_sources.end();)
    it = order.contains(it->first) ? std::next(it) : m_sources.erase(it);
  m_order = order;
  publishSources();
  rebuild();
  refresh();
}

void MusicRoom::setActive(bool active) {
  if (active == m_timer.isActive())
    return;
  if (active) {
    m_timer.start();
    refresh();
  } else {
    m_timer.stop();
  }
  emit activeChanged();
}

bool MusicRoom::settled() const {
  for (const auto &[id, source] : m_sources)
    if (!source.answered && source.ready)
      return false;
  return true;
}

// The Sources sheet and the first use read these rows; the state polled
// during playback changes every time (position_us), the rows do not.
void MusicRoom::publishSources() {
  const auto rows = sources();
  if (rows == m_sourceRows)
    return;
  m_sourceRows = rows;
  emit sourcesChanged();
}

void MusicRoom::refresh() {
  for (const auto &id : m_order)
    readState(id);
}

void MusicRoom::readState(const QString &id) {
  auto found = m_sources.find(id);
  if (found == m_sources.end())
    return;
  auto &source = found->second;
  if (source.stateBusy || source.commandBusy || source.path.isEmpty() ||
      source.generation.isEmpty())
    return;
  source.stateBusy = true;
  const auto ticket = source.ticket;
  auto message = QDBusMessage::createMethodCall(service, source.path,
                                                "org.arinn.Music1", "GetState");
  message << source.generation;
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 3500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, id, ticket](auto *watch) {
            const auto reply = watch->reply();
            watch->deleteLater();
            auto found = m_sources.find(id);
            if (found == m_sources.end() || found->second.ticket != ticket)
              return;
            auto &source = found->second;
            source.stateBusy = false;
            const auto json = object(reply);
            if (json.isEmpty() || !json.value("actions").isArray()) {
              // The source stops answering: its entries leave the shelves and
              // the Sources sheet says why.
              const bool had = !source.facets.empty();
              source.answered = true;
              source.stateFailed = true;
              source.stateError = reply.errorName().section('.', -1);
              // Replies still in flight belong to the catalogue that left.
              ++source.ticket;
              source.facets.clear();
              source.pending.clear();
              source.catalogueRevision = 0;
              publishSources();
              if (had)
                rebuild();
              return;
            }
            const auto state = json.toVariantMap();
            source.answered = true;
            source.stateFailed = false;
            source.stateError.clear();
            source.state = state;
            const auto revision =
                json.value("catalogue_revision").toVariant().toULongLong();
            if (!browsable(state)) {
              if (!source.facets.empty()) {
                ++source.ticket;
                source.facets.clear();
                source.pending.clear();
                rebuild();
              }
              source.catalogueRevision = revision;
            } else if (revision != source.catalogueRevision ||
                       source.facets.empty()) {
              source.catalogueRevision = revision;
              ++source.ticket;
              source.pending.clear();
              const auto facets = state.value("facets").toStringList();
              // A facet the source no longer offers leaves; the others keep
              // their entries until their new first page arrives.
              for (auto it = source.facets.begin(); it != source.facets.end();)
                it = facets.contains(it->first) ? std::next(it)
                                                : source.facets.erase(it);
              for (const auto *facet :
                   {"selections", "playlists", "albums", "artists", "tracks"})
                if (facets.contains(QLatin1String(facet))) {
                  auto &entry = source.facets[QLatin1String(facet)];
                  entry.busy = false;
                  entry.stale = true;
                  entry.buffer.clear();
                  entry.resumeTo = 0;
                  entry.next.clear();
                  entry.tokens.clear();
                  source.pending.append(QLatin1String(facet));
                }
              readNext(id);
              rebuild();
            } else if (source.pending.isEmpty()) {
              // The same catalogue: a first page that failed (a timeout, a
              // busy provider) is read again, not left out until the next
              // revision.
              bool busy = false;
              for (const auto &[name, facet] : source.facets)
                busy = busy || facet.busy;
              if (!busy)
                for (auto &[name, facet] : source.facets)
                  // Also a refresh that failed after a new revision, whose
                  // previous entries stay shown meanwhile (Codex review).
                  if (facet.failed &&
                      now() - facet.failedAt > retryDelay(facet.retries - 1))
                    source.pending.append(name);
              // A retried facet stays `loaded`: the room is not loading
              // again, so an empty room keeps its state.
              readNext(id);
            }
            publishSources();
          });
}

// One first page at a time per source: the daemon materialises at most two
// snapshots at once, and a source's facets share one.
void MusicRoom::readNext(const QString &id) {
  auto found = m_sources.find(id);
  if (found == m_sources.end() || found->second.pending.isEmpty())
    return;
  readPage(id, found->second.pending.takeFirst(), false);
}

void MusicRoom::readPage(const QString &id, const QString &facetId,
                         bool append) {
  auto found = m_sources.find(id);
  if (found == m_sources.end())
    return;
  auto &source = found->second;
  auto &facet = source.facets[facetId];
  if (facet.busy || (append && facet.next.isEmpty()))
    return;
  const auto token = append ? facet.next : QString();
  if (!token.isEmpty() && facet.tokens.contains(token))
    return;
  facet.busy = true;
  const auto ticket = source.ticket;
  auto message = QDBusMessage::createMethodCall(service, source.path,
                                                "org.arinn.Music1", "GetPage");
  const QJsonObject query{{"facet", facetId},
                          {"parent", QString()},
                          {"search", QString()},
                          {"token", token}};
  message << source.generation
          << QString::fromUtf8(
                 QJsonDocument(query).toJson(QJsonDocument::Compact));
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 5500), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, id, facetId, ticket, append, token](auto *watch) {
        const auto reply = watch->reply();
        watch->deleteLater();
        auto found = m_sources.find(id);
        if (found == m_sources.end() || found->second.ticket != ticket)
          return;
        auto &source = found->second;
        const auto entry = source.facets.find(facetId);
        if (entry == source.facets.end())
          return;
        auto &facet = entry->second;
        facet.busy = false;
        const bool resuming = facet.resumeTo > 0;
        const auto json = object(reply);
        const auto revision = json.value("revision").toVariant().toULongLong();
        const auto page = json.value("entries").toArray();
        if (json.isEmpty() ||
            json.value("generation").toString() != source.generation ||
            !json.value("entries").isArray() || revision == 0 ||
            page.size() > 100 || (append && revision != facet.revision)) {
          // An expired or invalid continuation reads again from the start
          // and pages forward to where the list was (review of #812).
          if (append && !resuming) {
            facet.resumeTo = facet.entries.size() + 1;
            facet.buffer.clear();
            facet.tokens.clear();
            readPage(id, facetId, false);
            return;
          }
          facet.resumeTo = 0;
          facet.buffer.clear();
          // A failed facet is left out; the others still fill their shelves,
          // and a later state poll reads its first page again.
          facet.failed = true;
          facet.failedAt = now();
          if (!append && !resuming)
            ++facet.retries;
          facet.loaded = true;
          facet.next.clear();
          // A failed new first page leaves the previous entries in place.
          facet.stale = false;
          if (!append && !resuming)
            readNext(id);
          rebuild();
          return;
        }
        auto entries = !append ? QVariantList()
                       : resuming ? facet.buffer
                                  : facet.entries;
        QSet<QString> ids;
        for (const auto &row : entries)
          ids.insert(row.toMap().value("id").toString());
        const bool local =
            source.state.value("account").toString() == QLatin1String("local");
        for (const auto &item : page) {
          auto row = item.toObject().toVariantMap();
          const auto entity = row.value("id").toString();
          if (entity.isEmpty() || ids.contains(entity) ||
              entries.size() >= 100000)
            continue;
          ids.insert(entity);
          row.insert("provider", id);
          row.insert("providerName", source.name);
          row.insert("local", local);
          // Two sources may hold the same id: the card is the source's own.
          row.insert("key", id + QLatin1Char('\n') + entity);
          row.insert("image",
                     AssetImages::imageUrl(id, source.path, source.generation,
                                           row.value("artwork").toString()));
          entries.append(row);
        }
        facet.revision = revision;
        facet.next = json.value("next_token").toString();
        facet.readAt = now();
        if (!token.isEmpty())
          facet.tokens.insert(token);
        if (resuming) {
          facet.buffer = entries;
          if (entries.size() < facet.resumeTo && !facet.next.isEmpty()) {
            readPage(id, facetId, true);
            return;
          }
          facet.buffer.clear();
          facet.resumeTo = 0;
        }
        facet.entries = entries;
        facet.failed = false;
        facet.retries = 0;
        facet.loaded = true;
        facet.stale = false;
        // Only the first pages chain: a further page is the All list's own.
        if (!append && !resuming)
          readNext(id);
        rebuild();
      });
}

QVariantList MusicRoom::order(const QList<QVariantList> &personal,
                              const QList<QVariantList> &ordinary) {
  QVariantList result;
  for (const auto &entries : personal)
    result.append(entries);
  struct Dated {
    qulonglong at;
    int provider, position;
    QVariant entry;
  };
  QList<Dated> dated;
  QList<QVariantList> undated;
  for (int provider = 0; provider < ordinary.size(); ++provider) {
    QVariantList rest;
    const auto &entries = ordinary.at(provider);
    for (int position = 0; position < entries.size(); ++position) {
      const auto at =
          entries.at(position).toMap().value("added_at").toULongLong();
      if (at > 0)
        dated.append({at, provider, position, entries.at(position)});
      else
        rest.append(entries.at(position));
    }
    undated.append(rest);
  }
  std::stable_sort(dated.begin(), dated.end(), [](const auto &a, const auto &b) {
    if (a.at != b.at)
      return a.at > b.at;
    if (a.provider != b.provider)
      return a.provider < b.provider;
    return a.position < b.position;
  });
  for (const auto &entry : dated)
    result.append(entry.entry);
  for (int index = 0;; ++index) {
    bool any = false;
    for (const auto &entries : undated)
      if (index < entries.size()) {
        result.append(entries.at(index));
        any = true;
      }
    if (!any)
      break;
  }
  return result;
}

QVariantList MusicRoom::stable(const QVariantList &shown,
                               const QVariantList &desired) {
  QHash<QString, QVariant> wanted;
  for (const auto &entry : desired)
    wanted.insert(entry.toMap().value("key").toString(), entry);
  QVariantList result;
  QSet<QString> kept;
  for (const auto &entry : shown) {
    const auto key = entry.toMap().value("key").toString();
    if (wanted.contains(key) && !kept.contains(key)) {
      result.append(wanted.value(key));
      kept.insert(key);
    }
  }
  for (const auto &entry : desired) {
    const auto key = entry.toMap().value("key").toString();
    if (!kept.contains(key)) {
      result.append(entry);
      kept.insert(key);
    }
  }
  return result;
}

QVariantList MusicRoom::gather(const QString &shelfId, const QString &only,
                               int limit) const {
  const auto *shelf = shelfDef(shelfId);
  if (!shelf)
    return {};
  QList<QVariantList> personal, ordinary;
  for (const auto &id : m_order) {
    if (!only.isEmpty() && id != only)
      continue;
    const auto found = m_sources.find(id);
    if (found == m_sources.end())
      continue;
    const auto &facets = found->second.facets;
    QVariantList own, rest;
    QSet<QString> ids;
    if (const auto selections = facets.find(QStringLiteral("selections"));
        selections != facets.end())
      for (const auto &value : selections->second.entries) {
        const auto row = value.toMap();
        if (row.value("kind").toString() == QLatin1String(shelf->kind)) {
          own.append(value);
          ids.insert(row.value("id").toString());
        }
      }
    if (shelf->facet)
      if (const auto facet = facets.find(QLatin1String(shelf->facet));
          facet != facets.end())
        for (const auto &value : facet->second.entries) {
          const auto row = value.toMap();
          // A personal entry the source also lists as ordinary shows once.
          if (row.value("kind").toString() == QLatin1String(shelf->kind) &&
              !ids.contains(row.value("id").toString()))
            rest.append(value);
        }
    personal.append(own);
    ordinary.append(rest);
  }
  auto result = order(personal, ordinary);
  if (limit > 0 && result.size() > limit)
    result = result.mid(0, limit);
  return result;
}

void MusicRoom::rebuild() {
  QVariantList shelves;
  bool dated = false;
  for (const auto &shelf : shelfDefs) {
    const auto entries = gather(QLatin1String(shelf.id), QString(), shelf.limit);
    for (const auto &entry : entries)
      dated = dated || entry.toMap().value("added_at").toULongLong() > 0;
    if (entries.isEmpty())
      continue;
    shelves.append(QVariantMap{{"id", QLatin1String(shelf.id)},
                               {"title", shelfTitle(QLatin1String(shelf.id))},
                               {"kind", QLatin1String(shelf.kind)},
                               {"entries", entries}});
  }
  const bool changed = shelves != m_shelves || dated != m_dated;
  m_shelves = shelves;
  m_dated = dated;
  if (changed)
    emit shelvesChanged();
  // The empty states follow `loading`; it changes with pages that leave
  // the shelves as they were (review of #812).
  const bool loading = this->loading();
  if (loading != m_loading) {
    m_loading = loading;
    emit loadingChanged();
  }
  emit listChanged();
}

QVariantList MusicRoom::list(const QString &shelf, const QString &source,
                             bool alphabetical) const {
  auto entries = gather(shelf, source, 0);
  if (alphabetical) {
    QCollator collator;
    collator.setNumericMode(true);
    collator.setCaseSensitivity(Qt::CaseInsensitive);
    std::stable_sort(entries.begin(), entries.end(),
                     [&collator](const QVariant &a, const QVariant &b) {
                       return collator.compare(a.toMap().value("title").toString(),
                                               b.toMap().value("title").toString()) < 0;
                     });
  }
  return entries;
}

bool MusicRoom::more(const QString &shelfId, const QString &only) const {
  const auto *shelf = shelfDef(shelfId);
  if (!shelf)
    return false;
  for (const auto &id : m_order) {
    if (!only.isEmpty() && id != only)
      continue;
    const auto found = m_sources.find(id);
    if (found == m_sources.end())
      continue;
    for (const auto &facetId : pagedFacets(shelf->facet)) {
      const auto facet = found->second.facets.find(facetId);
      if (facet != found->second.facets.end() && !facet->second.next.isEmpty())
        return true;
    }
  }
  return false;
}

void MusicRoom::loadMore(const QString &shelfId, const QString &only) {
  const auto *shelf = shelfDef(shelfId);
  if (!shelf)
    return;
  for (const auto &id : m_order) {
    if (!only.isEmpty() && id != only)
      continue;
    const auto found = m_sources.find(id);
    if (found == m_sources.end())
      continue;
    for (const auto &facetId : pagedFacets(shelf->facet)) {
      const auto entry = found->second.facets.find(facetId);
      if (entry == found->second.facets.end())
        continue;
      auto &facet = entry->second;
      if (facet.next.isEmpty() || facet.busy || facet.resumeTo > 0)
        continue;
      if (now() - facet.readAt > leaseMs) {
        facet.resumeTo = facet.entries.size() + 1;
        facet.buffer.clear();
        facet.tokens.clear();
        readPage(id, facetId, false);
      } else {
        readPage(id, facetId, true);
      }
    }
  }
}

bool MusicRoom::connected() const {
  for (const auto &[id, source] : m_sources)
    if (!source.stateFailed && browsable(source.state))
      return true;
  return false;
}

// First pages not read yet. A facet that is read again (a new revision, a
// retry) stays loaded, so the room does not go back to its placeholders.
bool MusicRoom::loading() const {
  for (const auto &[id, source] : m_sources)
    for (const auto &[name, facet] : source.facets)
      if (!facet.loaded)
        return true;
  return false;
}

QVariantList MusicRoom::sources() const {
  QVariantList rows;
  static const QStringList housekeeping{"connect", "authorize", "cancel", "scan",
                                        "disconnect"};
  for (const auto &id : m_order) {
    const auto found = m_sources.find(id);
    if (found == m_sources.end())
      continue;
    const auto &source = found->second;
    const auto account = source.state.value("account").toString();
    const bool local = account == QLatin1String("local");
    QString status, reason;
    if (source.stateFailed || !source.ready) {
      status = QStringLiteral("unavailable");
      reason = source.stateError == QLatin1String("Timeout")
                   ? tr("The source did not respond.")
                   : tr("The source is not running.");
    } else if (source.state.isEmpty()) {
      status = QStringLiteral("connecting");
    } else if (account == QLatin1String("restricted")) {
      status = QStringLiteral("unavailable");
      reason = tr("The service denied access to this account.");
    } else if (account == QLatin1String("unavailable")) {
      status = QStringLiteral("unavailable");
      reason = tr("The service is not responding.");
    } else if (account == QLatin1String("connecting")) {
      status = QStringLiteral("connecting");
    } else if (account == QLatin1String("disconnected") ||
               account == QLatin1String("expired") ||
               account == QLatin1String("unconfigured")) {
      status = QStringLiteral("signin");
    } else if (source.state.value("catalogue_loading").toBool()) {
      status = QStringLiteral("scanning");
    } else {
      status = QStringLiteral("connected");
    }
    QStringList actions;
    for (const auto &action : source.state.value("actions").toStringList())
      if (housekeeping.contains(action))
        actions.append(action);
    rows.append(QVariantMap{{"id", id},
                            {"name", source.name},
                            {"local", local},
                            {"account", account},
                            {"status", status},
                            {"reason", reason},
                            {"actions", actions},
                            {"prompt", source.state.value("prompt")},
                            {"busy", source.commandBusy},
                            {"answered", source.answered},
                            // `connected` is announced with these rows.
                            {"browsable", !source.stateFailed &&
                                              browsable(source.state)}});
  }
  return rows;
}

void MusicRoom::sourceCommand(const QString &id, const QString &action) {
  auto found = m_sources.find(id);
  if (found == m_sources.end())
    return;
  auto &source = found->second;
  static const QStringList housekeeping{"connect", "authorize", "cancel", "scan",
                                        "disconnect"};
  if (source.commandBusy || !housekeeping.contains(action) ||
      !source.state.value("actions").toStringList().contains(action))
    return;
  source.commandBusy = true;
  publishSources();
  const auto generation = source.generation;
  const QJsonObject payload{
      {"request_id", QUuid::createUuid().toString(QUuid::WithoutBraces)},
      {"action", action},
      {"session", QString()},
      {"entity", QString()},
      {"occurrence", QString()},
      {"position_us", 0},
      {"replaces_provider", QString()},
      {"replaces_session", QString()},
      {"text", QString()}};
  auto message = QDBusMessage::createMethodCall(service, source.path,
                                                "org.arinn.Music1", "Command");
  message << source.generation
          << QString::fromUtf8(
                 QJsonDocument(payload).toJson(QJsonDocument::Compact));
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 5500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, id, generation](auto *watch) {
            watch->deleteLater();
            auto found = m_sources.find(id);
            if (found == m_sources.end() ||
                found->second.generation != generation)
              return;
            found->second.commandBusy = false;
            publishSources();
            // The confirmed state, catalogue included, is read again.
            readState(id);
          });
}
