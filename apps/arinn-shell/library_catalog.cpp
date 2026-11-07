// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "library_catalog.h"
#include "language_change.h"
#include <QLocale>
#include "asset_images.h"
#include <QDeadlineTimer>
#include <QHash>
#include <QSet>
#include <QUuid>
#include <QtDBus>
#include <utility>
#include <QtQml>
#include <algorithm>
#include <limits>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
constexpr auto libraryPage = "(sta(sss)a(sssssbibubuxssb)s)";
constexpr auto resumePage = "(sta(sxxbt)s)";
// Launch waits out the 30 s launch timeout (spec §9.5) plus one second.
constexpr int operationWaitMs = 31000;
constexpr int fallbackPollMs = 30000;
QDBusMessage daemonCall(const char *interface, const char *method) {
  return QDBusMessage::createMethodCall(service, path, interface, method);
}
QString presentationId(const QString &provider, const QString &local) {
  return QUuid::createUuidV5(QUuid(), provider + QLatin1Char('\n') + local)
      .toString(QUuid::WithoutBraces);
}
QString folderPresentationId(const QString &provider, const QString &local) {
  // Catalogue identities are UUID strings; this disjoint namespace cannot
  // collide with any valid provider-controlled content ID.
  return QStringLiteral("folder:") + presentationId(provider, local);
}
QString displayKind(const QString &kind, bool episode) {
  if (kind == QLatin1String("film") || kind == QLatin1String("series") ||
      kind == QLatin1String("other"))
    return kind;
  if (episode && kind == QLatin1String("episode"))
    return kind;
  return QStringLiteral("other");
}
QString formatDuration(qint64 us) {
  if (us < 0)
    return {};
  const auto seconds = us / 1000000;
  // The unit through the catalogue and the number in the locale's format
  // (§15).
  if (seconds < 60)
    return QCoreApplication::translate("LibraryCatalog", "%1 s")
        .arg(QLocale().toString(seconds));
  return QCoreApplication::translate("LibraryCatalog", "%1 min")
      .arg(QLocale().toString(seconds / 60));
}
QString errorCode(const QDBusMessage &reply) {
  auto name = reply.errorName();
  const auto dot = name.lastIndexOf(QLatin1Char('.'));
  if (dot >= 0)
    name = name.mid(dot + 1);
  return name.isEmpty() ? QStringLiteral("Unavailable") : name;
}
double resumeFraction(qint64 position, qint64 resumeDuration, qint64 itemDuration,
                      bool completed, bool hasResume) {
  if (!hasResume || completed || position <= 0)
    return 0;
  const auto duration = resumeDuration >= 0 ? resumeDuration : itemDuration;
  if (duration <= 0)
    return 0.01;
  const auto fraction =
      static_cast<double>(position) / static_cast<double>(duration);
  if (fraction > 1)
    return 1;
  if (fraction <= 0)
    return 0.01;
  return fraction;
}
bool optionalYear(bool has, int year) {
  return has ? year >= 1 && year <= 9999 : year == 0;
}
bool optionalIndex(bool has, quint32 value) {
  return has ? value >= 1 && value < 10000 : value == 0;
}
void registerLibraryCatalog() {
  qmlRegisterType<LibraryCatalog>("Arinn.Shell", 1, 0, "LibraryCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerLibraryCatalog)
} // namespace

struct LibraryCatalog::Catalog {
  quint64 epoch = 0;
  QString snapshot;
  bool sawLibrary = false;
  QVariantList providers;
  QList<CollectionRow> collections;
  QList<ItemRow> items;
  QList<ResumeRow> resumes;
  QSet<QString> tokens;
  qsizetype providerStartItems = 0;
  qsizetype providerStartCollections = 0;
  int pages = 0;
  int failed = 0;
  QString pageGeneration;
  quint64 pageRevision = 0;
  bool resumeFeature = false;
  quint64 libraryRevision = 0;
  int additionsIndex = -1;
  int hierarchyIndex = -1;
  QVariantList hierarchies, hierarchyFolders, hierarchyMembers;
  QDeadlineTimer hierarchyDeadline;
  QSet<QString> hierarchyTokens;
};

LibraryCatalog::LibraryCatalog(QObject *parent) : QObject(parent) {
  new LanguageChangeObserver(this, [this] { publish(true); });
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, service, "Changed", this, SLOT(refresh()));
  bus.connect(service, path, service, "ProvidersChanged", this, SLOT(refresh()));
  bus.connect(service, path, service, "StateChanged", this, SLOT(refresh()));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &) {
            ++m_epoch;
            ++m_commandTicket;
            m_connected = false;
            m_loaded = false;
            m_error = false;
            m_unreachable = false;
            m_items.clear();
    m_hierarchies.clear();
            m_collections.clear();
            setOperation(QString());
            publish();
            refresh();
          });
  m_timer.setInterval(fallbackPollMs);
  connect(&m_timer, &QTimer::timeout, this, &LibraryCatalog::refresh);
  m_timer.start();
  refresh();
}

void LibraryCatalog::requestDescription(const QString &entryId) {
  if (entryId.isEmpty() && m_descriptionEntryId.isEmpty() && !m_descriptionPending) return;
  const auto *found = findId(entryId);
  const bool supported = found && found->descriptions;
  const auto generation = found ? found->generation : QString();
  const auto revision = found ? found->revision : 0;
  if (entryId == m_descriptionEntryId && generation == m_descriptionGeneration
      && revision == m_descriptionRevision && supported == m_descriptionSupported
      && (m_descriptionLoaded || m_descriptionPending))
    return;
  const auto ticket = ++m_descriptionTicket;
  m_descriptionEntryId = entryId;
  m_descriptionGeneration = generation;
  m_descriptionRevision = revision;
  m_descriptionSupported = supported;
  m_descriptionText.clear();
  m_descriptionLoaded = false;
  m_descriptionPending = false;
  emit descriptionChanged();
  if (!found || !found->descriptions || !m_connected) {
    m_descriptionLoaded = found && !found->descriptions;
    return;
  }
  const auto item = *found;
  const auto epoch = m_epoch;
  m_descriptionPending = true;
  auto msg = QDBusMessage::createMethodCall(service, item.path, "org.arinn.Library1", "GetDescription");
  msg << item.generation << qulonglong(item.revision) << item.localId;
  auto pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(msg, 1000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
      [this, ticket, epoch, item](auto *call) {
    const auto reply = call->reply();
    call->deleteLater();
    if (ticket != m_descriptionTicket || epoch != m_epoch) return;
    m_descriptionPending = false;
    const auto *current = findId(item.id);
    if (!current || current->generation != item.generation || current->revision != item.revision
        || reply.type() == QDBusMessage::ErrorMessage || reply.signature() != "(stss)"
        || reply.arguments().size() != 1) return;
    const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
    QString generation, id, text; qulonglong revision = 0;
    arg.beginStructure(); arg >> generation >> revision >> id >> text; arg.endStructure();
    if (generation != item.generation || revision != item.revision || id != item.localId
        || text.toUtf8().size() > 8192 || text.contains(QChar(0))) return;
    m_descriptionLoaded = true;
    m_descriptionText = text;
    emit descriptionChanged();
  });
}

QString LibraryCatalog::libraryState() const {
  if (!m_loaded)
    return QStringLiteral("loading");
  if (!m_connected || m_unreachable)
    return QStringLiteral("unavailable");
  if (m_error)
    return QStringLiteral("error");
  if (m_entries.isEmpty() && m_folderEntries.isEmpty())
    return QStringLiteral("empty");
  return QStringLiteral("ready");
}
const LibraryCatalog::ItemRow *LibraryCatalog::findId(const QString &id) const {
  for (const auto &row : m_items)
    if (row.id == id)
      return &row;
  return nullptr;
}
void LibraryCatalog::setOperation(const QString &state) {
  const auto terminal = state != QLatin1String("pending") &&
                        state != QLatin1String("queued") &&
                        state != QLatin1String("running");
  const auto hadLaunch = !m_launchingId.isEmpty();
  if (terminal)
    m_launchingId.clear();
  if (m_operation == state) {
    if (terminal && hadLaunch)
      emit operationChanged();
    return;
  }
  m_operation = state;
  emit operationChanged();
}
void LibraryCatalog::publish(bool presentationOnly) {
  if (!presentationOnly) requestDescription(m_descriptionEntryId);
  QVariantList tabs;
  tabs.reserve(m_collections.size());
  for (const auto &collection : m_collections) {
    // The shell names the contract's collection kinds in the user's
    // language; a provider title is shown only for any other kind (#431).
    QString title = collection.title;
    if (collection.kind == QLatin1String("film"))
      title = tr("Films");
    else if (collection.kind == QLatin1String("series"))
      title = tr("Series");
    else if (collection.kind == QLatin1String("other"))
      title = tr("Other");
    tabs.append(QVariantMap{{QStringLiteral("id"), collection.id},
                            {QStringLiteral("title"), title},
                            {QStringLiteral("kind"), collection.kind}});
  }
  QHash<QString, QList<int>> children;
  for (int i = 0; i < m_items.size(); ++i) {
    const auto &item = m_items.at(i);
    if (!item.parentLocal.isEmpty())
      children[item.providerId + QLatin1Char('\n') + item.parentLocal].append(i);
  }
  QVariantList entries;
  QHash<QString, QVariantMap> allEntries;
  QList<QPair<const ItemRow *, const ItemRow *>> resumable;
  for (const auto &item : m_items) {
    auto kids = children.value(item.providerId + QLatin1Char('\n') + item.localId);
    std::sort(kids.begin(), kids.end(), [this](int a, int b) {
      const auto &x = m_items.at(a); const auto &y = m_items.at(b);
      const auto unknown = std::numeric_limits<quint32>::max();
      if ((x.hasSeason ? x.season : unknown) != (y.hasSeason ? y.season : unknown))
        return (x.hasSeason ? x.season : unknown) < (y.hasSeason ? y.season : unknown);
      if ((x.hasEpisode ? x.episode : unknown) != (y.hasEpisode ? y.episode : unknown))
        return (x.hasEpisode ? x.episode : unknown) < (y.hasEpisode ? y.episode : unknown);
      return x.id < y.id;
    });
    QList<quint32> seasonOrder;
    QSet<quint32> seenSeasons;
    QVariantList episodes;
    bool unknownSeason = false;
    for (const auto index : kids) {
      const auto &kid = m_items.at(index);
      unknownSeason |= !kid.hasSeason;
      if (kid.hasSeason && !seenSeasons.contains(kid.season)) {
        seenSeasons.insert(kid.season);
        seasonOrder.append(kid.season);
      }
      QVariantMap episode{{QStringLiteral("id"), kid.id},
                          {QStringLiteral("title"), kid.title},
                          {QStringLiteral("duration"), formatDuration(kid.durationUs)},
                          {QStringLiteral("missing"), !kid.available},
                          {QStringLiteral("enabled"), true},
                          {QStringLiteral("progress"),
                           resumeFraction(kid.resumePosition, kid.resumeDuration,
                                          kid.durationUs, kid.resumeCompleted,
                                          kid.hasResume)}};
      const auto still = AssetImages::imageUrl(kid.providerId, kid.path,
                                               kid.generation, kid.poster);
      if (!still.isEmpty())
        episode.insert(QStringLiteral("artwork"), still);
      episode.insert(QStringLiteral("seasonId"),
                     kid.hasSeason ? QStringLiteral("s") + QString::number(kid.season)
                                   : QStringLiteral("unknown"));
      if (kid.hasEpisode)
        episode.insert(QStringLiteral("number"), kid.episode);
      episodes.append(episode);
    }
    std::sort(seasonOrder.begin(), seasonOrder.end());
    QVariantList seasons;
    for (const auto season : seasonOrder) {
      seasons.append(QVariantMap{
          {QStringLiteral("id"), QStringLiteral("s") + QString::number(season)},
          {QStringLiteral("title"), tr("Season %1").arg(season)}});
    }
    if (unknownSeason && !seasonOrder.isEmpty())
      seasons.append(QVariantMap{{QStringLiteral("id"), QStringLiteral("unknown")},
                                 {QStringLiteral("title"), tr("Without season")}});
    auto progress = resumeFraction(item.resumePosition, item.resumeDuration,
                                   item.durationUs, item.resumeCompleted,
                                   item.hasResume);
    const ItemRow *resume = &item;
    if (item.kind == QLatin1String("series") || !kids.isEmpty()) {
      resume = seriesTarget(item);
      if (const auto *target = resume)
        progress = resumeFraction(target->resumePosition, target->resumeDuration,
                                  target->durationUs, target->resumeCompleted,
                                  target->hasResume);
      else
        progress = 0;
    }
    if (item.parentLocal.isEmpty() && item.available && resume && resume->available &&
        (item.kind == QLatin1String("film") || item.kind == QLatin1String("series")) &&
        resume->hasResume && !resume->resumeCompleted && resume->resumePosition > 0)
      resumable.append(QPair<const ItemRow *, const ItemRow *>{&item, resume});
    const auto poster = AssetImages::imageUrl(item.providerId, item.path,
                                              item.generation, item.poster);
    const auto fanart = AssetImages::imageUrl(item.providerId, item.path,
                                              item.generation, item.fanart);
    QVariantMap entry{{QStringLiteral("id"), item.id},
                      {QStringLiteral("tabId"), item.tabId},
                      {QStringLiteral("title"), item.title},
                      {QStringLiteral("kind"), item.kind},
                      {QStringLiteral("noArt"), poster.isEmpty()},
                      {QStringLiteral("missing"), !item.available},
                      {QStringLiteral("enabled"), true},
                      {QStringLiteral("progress"), progress}};
    // Detail shows the time left and where Continue starts (#728): the
    // resume of the item Play opens, an episode for a series.
    if (progress > 0 && resume && resume->hasResume && resume->resumePosition > 0) {
      const auto duration = resume->resumeDuration > 0 ? resume->resumeDuration
                                                       : resume->durationUs;
      entry.insert(QStringLiteral("resumePositionUs"),
                   static_cast<double>(resume->resumePosition));
      if (duration > 0)
        entry.insert(QStringLiteral("resumeDurationUs"), static_cast<double>(duration));
      if (resume != &item && resume->hasSeason)
        entry.insert(QStringLiteral("resumeSeason"), resume->season);
      if (resume != &item && resume->hasEpisode)
        entry.insert(QStringLiteral("resumeEpisode"), resume->episode);
    }
    if (!poster.isEmpty())
      entry.insert(QStringLiteral("artwork"), poster);
    if (!fanart.isEmpty())
      entry.insert(QStringLiteral("fanart"), fanart);
    if (item.hasYear)
      entry.insert(QStringLiteral("year"), item.year);
    if (!seasons.isEmpty())
      entry.insert(QStringLiteral("seasons"), seasons);
    if (!episodes.isEmpty())
      entry.insert(QStringLiteral("episodes"), episodes);
    allEntries.insert(item.id, entry);
    if (item.parentLocal.isEmpty()) entries.append(entry);
  }
  QVariantList folderEntries;
  for (const auto &raw : std::as_const(m_hierarchies)) {
    const auto hierarchy = raw.toMap();
    const auto provider = hierarchy.value(QStringLiteral("providerId")).toString();
    const auto folders = hierarchy.value(QStringLiteral("folders")).toList();
    for (const auto &value : folders) {
      const auto f = value.toMap();
      const auto local = f.value(QStringLiteral("id")).toString();
      const auto parent = f.value(QStringLiteral("parentId")).toString();
      folderEntries.append(QVariantMap{
        {QStringLiteral("id"), folderPresentationId(provider, local)},
        {QStringLiteral("folderId"), parent.isEmpty() ? QString() : folderPresentationId(provider, parent)},
        {QStringLiteral("title"), f.value(QStringLiteral("title"))},
        {QStringLiteral("kind"), QStringLiteral("folder")},
        {QStringLiteral("missing"), !f.value(QStringLiteral("available")).toBool()},
        {QStringLiteral("enabled"), true}, {QStringLiteral("noArt"), true}});
    }
    const auto members = hierarchy.value(QStringLiteral("members")).toList();
    for (const auto &value : members) {
      const auto m = value.toMap();
      const auto itemId = presentationId(provider, m.value(QStringLiteral("itemId")).toString());
      if (!allEntries.contains(itemId)) continue;
      auto entry = allEntries.value(itemId);
      entry.insert(QStringLiteral("folderId"), folderPresentationId(provider, m.value(QStringLiteral("folderId")).toString()));
      folderEntries.append(entry);
    }
  }
  std::stable_sort(folderEntries.begin(), folderEntries.end(), [](const QVariant &a, const QVariant &b) {
    const auto x = a.toMap(), y = b.toMap();
    const bool xf = x.value(QStringLiteral("kind")) == QStringLiteral("folder");
    const bool yf = y.value(QStringLiteral("kind")) == QStringLiteral("folder");
    if (xf != yf) return xf;
    const auto title = QString::localeAwareCompare(x.value(QStringLiteral("title")).toString(), y.value(QStringLiteral("title")).toString());
    return title == 0 ? x.value(QStringLiteral("id")).toString() < y.value(QStringLiteral("id")).toString() : title < 0;
  });
  if (!folderEntries.isEmpty())
    tabs.append(QVariantMap{{QStringLiteral("id"), QStringLiteral("@folders")}, {QStringLiteral("title"), tr("Folders")}});
  // A reload follows every ProvidersChanged. Announcing an unchanged catalog
  // makes every view bound to it rebuild its cards (#441).
  QList<const ItemRow *> recent;
  for (const auto &item : std::as_const(m_items))
    if (item.addedAtUs > 0 && item.available && item.parentLocal.isEmpty() &&
        (item.kind == QLatin1String("film") || item.kind == QLatin1String("series")))
      recent.append(&item);
  std::sort(recent.begin(), recent.end(), [](const auto *a, const auto *b) {
    if (a->addedAtUs != b->addedAtUs) return a->addedAtUs > b->addedAtUs;
    if (a->providerId != b->providerId) return a->providerId.toUtf8() < b->providerId.toUtf8();
    return a->localId.toUtf8() < b->localId.toUtf8();
  });
  QVariantList newEntries;
  for (const auto *item : std::as_const(recent)) {
    if (newEntries.size() == 12) break;
    for (const auto &entry : std::as_const(entries)) {
      if (entry.toMap().value(QStringLiteral("id")).toString() == item->id) {
        auto card = entry.toMap();
        if (item->hasYear)
          card.insert(QStringLiteral("subtitle"), QString::number(item->year));
        newEntries.append(card);
        break;
      }
    }
  }
  std::sort(resumable.begin(), resumable.end(), [](const auto &a, const auto &b) {
    if (a.second->resumeObserved != b.second->resumeObserved)
      return a.second->resumeObserved > b.second->resumeObserved;
    return a.first->id < b.first->id;
  });
  QVariantList continued;
  for (const auto &[title, target] : std::as_const(resumable)) {
    if (continued.size() == 4) break;
    auto card = allEntries.value(title->id);
    // Home needs one bounded card, not the series' complete episode rail.
    card.remove(QStringLiteral("episodes"));
    card.remove(QStringLiteral("seasons"));
    // Distinct from the same title's New/detail card. No live session ID or
    // runtime artwork reference is persisted: Resume1 remains authoritative.
    card.insert(QStringLiteral("id"), QStringLiteral("resume:") + title->id);
    card.insert(QStringLiteral("libraryEntryId"), title->id);
    card.insert(QStringLiteral("resumeContentId"), target->id);
    // The hero names the kind, the year or the episode, never the row
    // (Hearth light brief, Home; #756 C20).
    card.insert(QStringLiteral("mediaKind"), card.value(QStringLiteral("kind")));
    if (title->hasYear)
      card.insert(QStringLiteral("year"), title->year);
    if (target->hasSeason)
      card.insert(QStringLiteral("season"), target->season);
    if (target->hasEpisode)
      card.insert(QStringLiteral("episode"), target->episode);
    card.insert(QStringLiteral("kind"), QStringLiteral("media"));
    card.insert(QStringLiteral("state"), QStringLiteral("stopped"));
    card.insert(QStringLiteral("running"), false);
    card.insert(QStringLiteral("positionUs"), static_cast<double>(target->resumePosition));
    card.insert(QStringLiteral("durationUs"), static_cast<double>(target->resumeDuration > 0 ? target->resumeDuration : target->durationUs));
    card.insert(QStringLiteral("subtitle"), target == title ? QString() : target->title);
    const auto art = artworkFor(target->providerId, target->localId);
    if (!art.isEmpty()) card.insert(QStringLiteral("artwork"), art);
    continued.append(card);
  }
  const auto changed = tabs != m_tabs || entries != m_entries || folderEntries != m_folderEntries || newEntries != m_newEntries || continued != m_continueEntries ||
                       m_items != m_publishedItems;
  m_tabs = tabs;
  m_folderEntries = folderEntries;
  m_entries = entries;
  m_newEntries = newEntries;
  m_continueEntries = continued;
  if (!changed && libraryState() == m_publishedState)
    return;
  // A deep copy: sharing would let the next non-const pass over m_items
  // detach it and free rows a caller still points to.
  m_publishedItems = m_items;
  m_publishedItems.detach();
  m_publishedState = libraryState();
  emit catalogChanged();
}
void LibraryCatalog::refresh() {
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
              m_loaded = true;
              m_error = false;
              m_unreachable = false;
              m_items.clear();
    m_hierarchies.clear();
              m_collections.clear();
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
void LibraryCatalog::readProviders(const std::shared_ptr<Catalog> &catalog,
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
          if (!capabilities.contains(QLatin1String("library")))
            continue;
          catalog->sawLibrary = true;
          if (runtime == QLatin1String("ready") && !id.isEmpty() &&
              !endpoint.path().isEmpty() && !generation.isEmpty())
            catalog->providers.append(QVariantMap{
                {QStringLiteral("id"), id},
                {QStringLiteral("path"), endpoint.path()},
                {QStringLiteral("generation"), generation},
                {QStringLiteral("backend"), backend}});
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
          readLibrary(catalog, 0);
        }
      });
}
void LibraryCatalog::failProvider(const std::shared_ptr<Catalog> &catalog,
                                  int index) {
  ++catalog->failed;
  catalog->items.resize(catalog->providerStartItems);
  catalog->collections.resize(catalog->providerStartCollections);
  catalog->resumes.clear();
  nextProvider(catalog, index);
}
void LibraryCatalog::dropResumes(const std::shared_ptr<Catalog> &catalog,
                                 int index) {
  catalog->resumes.clear();
  nextProvider(catalog, index);
}
void LibraryCatalog::nextProvider(const std::shared_ptr<Catalog> &catalog,
                                  int index) {
  if (catalog->additionsIndex == index) {
    catalog->additionsIndex = -1;
    readAdditions(catalog, index);
    return;
  }
  if (catalog->hierarchyIndex == index) {
    catalog->hierarchyIndex = -1;
    catalog->hierarchyFolders.clear(); catalog->hierarchyMembers.clear();
    catalog->hierarchyTokens.clear(); catalog->hierarchyDeadline = QDeadlineTimer(5000);
    readHierarchy(catalog, index);
    return;
  }
  catalog->tokens.clear();
  catalog->pageGeneration.clear();
  catalog->pageRevision = 0;
  catalog->resumeFeature = false;
  readLibrary(catalog, index + 1);
}
bool LibraryCatalog::acceptProvider(const std::shared_ptr<Catalog> &catalog,
                                    int index) {
  QSet<QString> collectionIds;
  const auto providerId =
      catalog->providers.at(index).toMap().value(QStringLiteral("id")).toString();
  for (int i = catalog->providerStartCollections; i < catalog->collections.size();
       ++i) {
    const auto &collection = catalog->collections.at(i);
    if (collection.providerId != providerId || collection.localId.isEmpty() ||
        collectionIds.contains(collection.localId))
      return false;
    collectionIds.insert(collection.localId);
  }
  QHash<QString, QString> parents;
  QSet<QString> itemIds;
  for (int i = catalog->providerStartItems; i < catalog->items.size(); ++i) {
    const auto &item = catalog->items.at(i);
    if (item.providerId != providerId || item.localId.isEmpty() ||
        itemIds.contains(item.localId) ||
        !collectionIds.contains(item.collectionLocal))
      return false;
    itemIds.insert(item.localId);
    parents.insert(item.localId, item.parentLocal);
  }
  for (auto it = parents.cbegin(); it != parents.cend(); ++it) {
    if (it.value().isEmpty())
      continue;
    if (it.value() == it.key() || !itemIds.contains(it.value()))
      return false;
    QSet<QString> seen;
    auto cursor = it.key();
    while (!cursor.isEmpty()) {
      if (seen.contains(cursor))
        return false;
      seen.insert(cursor);
      cursor = parents.value(cursor);
    }
  }
  return true;
}
void LibraryCatalog::applyResumes(const std::shared_ptr<Catalog> &catalog,
                                  int index) {
  const auto providerId =
      catalog->providers.at(index).toMap().value(QStringLiteral("id")).toString();
  for (auto &item : catalog->items) {
    if (item.providerId != providerId)
      continue;
    for (const auto &resume : catalog->resumes) {
      if (resume.localId != item.localId)
        continue;
      item.hasResume = true;
      item.resumePosition = resume.position;
      item.resumeDuration = resume.duration;
      item.resumeCompleted = resume.completed;
      item.resumeObserved = resume.observed;
    }
  }
  catalog->resumes.clear();
}
void LibraryCatalog::readLibrary(const std::shared_ptr<Catalog> &catalog,
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
    catalog->providerStartItems = catalog->items.size();
    catalog->providerStartCollections = catalog->collections.size();
    catalog->pageGeneration.clear();
    catalog->pageRevision = 0;
    catalog->resumes.clear();
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value(QStringLiteral("path")).toString(),
      "org.arinn.Library1", "GetLibrary");
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
            reply.arguments().isEmpty() || reply.signature() != libraryPage) {
          failProvider(catalog, index);
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString generation, next;
        qulonglong revision = 0;
        arg.beginStructure();
        arg >> generation >> revision;
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
        const auto providerId = provider.value(QStringLiteral("id")).toString();
        const auto providerPath = provider.value(QStringLiteral("path")).toString();
        const auto backend = provider.value(QStringLiteral("backend")).toString();
        arg.beginArray();
        while (!arg.atEnd()) {
          CollectionRow collection;
          collection.providerId = providerId;
          arg.beginStructure();
          arg >> collection.localId >> collection.title >> collection.kind;
          arg.endStructure();
          if (collection.localId.isEmpty()) {
            failProvider(catalog, index);
            return;
          }
          collection.kind = displayKind(collection.kind, false);
          collection.id = presentationId(providerId, collection.localId);
          catalog->collections.append(collection);
        }
        arg.endArray();
        arg.beginArray();
        while (!arg.atEnd()) {
          ItemRow row;
          row.providerId = providerId;
          row.path = providerPath;
          row.generation = generation;
          row.revision = revision;
          row.backend = backend;
          bool hasYear = false, hasSeason = false, hasEpisode = false,
               available = false;
          int year = 0;
          quint32 season = 0, episode = 0;
          qlonglong duration = -1;
          QString poster, fanart, kind;
          arg.beginStructure();
          arg >> row.localId >> row.collectionLocal >> row.parentLocal >>
              row.title >> kind >> hasYear >> year >> hasSeason >> season >>
              hasEpisode >> episode >> duration >> poster >> fanart >> available;
          arg.endStructure();
          row.poster = poster;
          row.fanart = fanart;
          if (row.localId.isEmpty() || row.collectionLocal.isEmpty() ||
              !optionalYear(hasYear, year) || !optionalIndex(hasSeason, season) ||
              !optionalIndex(hasEpisode, episode) || duration < -1) {
            failProvider(catalog, index);
            return;
          }
          row.kind = displayKind(kind, true);
          row.hasYear = hasYear;
          row.year = year;
          row.hasSeason = hasSeason;
          row.season = season;
          row.hasEpisode = hasEpisode;
          row.episode = episode;
          row.durationUs = duration;
          row.available = available;
          row.tabId = presentationId(providerId, row.collectionLocal);
          row.id = presentationId(providerId, row.localId);
          catalog->items.append(row);
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty())
          readLibrary(catalog, index, next);
        else if (!acceptProvider(catalog, index))
          failProvider(catalog, index);
        else {
          catalog->libraryRevision = catalog->pageRevision;
          catalog->tokens.clear();
          catalog->pageGeneration.clear();
          catalog->pageRevision = 0;
          readFeatures(catalog, index);
        }
      });
}
void LibraryCatalog::readHierarchy(const std::shared_ptr<Catalog> &catalog, int index, const QString &token) {
  if (catalog->epoch != m_epoch) { finishCatalog(catalog, false); return; }
  if (catalog->hierarchyDeadline.hasExpired() || catalog->hierarchyTokens.size() >= 1024 || catalog->hierarchyTokens.contains(token)) {
    nextProvider(catalog, index); return;
  }
  catalog->hierarchyTokens.insert(token);
  const auto provider = catalog->providers.at(index).toMap();
  const auto generation = provider.value(QStringLiteral("generation")).toString();
  const auto providerId = provider.value(QStringLiteral("id")).toString();
  auto msg = QDBusMessage::createMethodCall(service, provider.value(QStringLiteral("path")).toString(), "org.arinn.Library1", "GetHierarchy");
  msg << generation << qulonglong(catalog->libraryRevision) << token;
  auto pending = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(msg, int(std::min<qint64>(3000, catalog->hierarchyDeadline.remainingTime()))), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this, [this, catalog, index, generation, providerId](auto *call) {
    const auto reply = call->reply(); call->deleteLater();
    if (catalog->epoch != m_epoch) { finishCatalog(catalog, false); return; }
    if (reply.type() == QDBusMessage::ErrorMessage || reply.signature() != "(sta(sssb)a(ss)s)" || reply.arguments().isEmpty()) { nextProvider(catalog, index); return; }
    const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
    QString gen, next; qulonglong revision = 0; int count = 0;
    arg.beginStructure(); arg >> gen >> revision;
    bool valid = gen == generation && revision == catalog->libraryRevision;
    auto text = [](const QString &s, bool empty = false) { return (empty || !s.isEmpty()) && s.toUtf8().size() <= 8192 && !s.contains(QChar(0)); };
    arg.beginArray();
    while (!arg.atEnd()) {
      QString id, parent, title; bool available;
      arg.beginStructure(); arg >> id >> parent >> title >> available; arg.endStructure();
      valid &= text(id) && text(parent, true) && text(title, true);
      catalog->hierarchyFolders.append(QVariantMap{{QStringLiteral("id"), id}, {QStringLiteral("parentId"), parent}, {QStringLiteral("title"), title}, {QStringLiteral("available"), available}}); ++count;
    }
    arg.endArray(); arg.beginArray();
    while (!arg.atEnd()) {
      QString folder, item; arg.beginStructure(); arg >> folder >> item; arg.endStructure();
      valid &= text(folder) && text(item);
      catalog->hierarchyMembers.append(QVariantMap{{QStringLiteral("folderId"), folder}, {QStringLiteral("itemId"), item}}); ++count;
    }
    arg.endArray(); arg >> next; arg.endStructure();
    valid &= count <= 200 && catalog->hierarchyFolders.size() + catalog->hierarchyMembers.size() <= 100000 && text(next, true);
    if (!valid) { nextProvider(catalog, index); return; }
    if (!next.isEmpty()) { readHierarchy(catalog, index, next); return; }
    QHash<QString, QString> parents;
    for (const auto &value : std::as_const(catalog->hierarchyFolders)) {
      const auto row = value.toMap(); const auto id = row.value(QStringLiteral("id")).toString();
      if (parents.contains(id)) { nextProvider(catalog, index); return; }
      parents.insert(id, row.value(QStringLiteral("parentId")).toString());
    }
    for (auto it = parents.cbegin(); it != parents.cend(); ++it) {
      auto parent = it.value(); int depth = 0;
      while (!parent.isEmpty()) {
        if (++depth > 64 || !parents.contains(parent) || parent == it.key()) { nextProvider(catalog, index); return; }
        parent = parents.value(parent);
      }
    }
    QSet<QString> items, pairs;
    for (qsizetype i = catalog->providerStartItems; i < catalog->items.size(); ++i) items.insert(catalog->items.at(i).localId);
    for (const auto &value : std::as_const(catalog->hierarchyMembers)) {
      const auto row = value.toMap(); const auto folder = row.value(QStringLiteral("folderId")).toString(), item = row.value(QStringLiteral("itemId")).toString();
      const auto pair = folder + QChar(0) + item;
      if (!parents.contains(folder) || !items.contains(item) || pairs.contains(pair)) { nextProvider(catalog, index); return; }
      pairs.insert(pair);
    }
    catalog->hierarchies.append(QVariantMap{{QStringLiteral("providerId"), providerId}, {QStringLiteral("folders"), catalog->hierarchyFolders}, {QStringLiteral("members"), catalog->hierarchyMembers}});
    nextProvider(catalog, index);
  });
}
void LibraryCatalog::readAdditions(const std::shared_ptr<Catalog> &catalog,
                                   int index) {
  if (catalog->epoch != m_epoch) {
    finishCatalog(catalog, false);
    return;
  }
  const auto provider = catalog->providers.at(index).toMap();
  const auto generation = provider.value(QStringLiteral("generation")).toString();
  const auto revision = catalog->libraryRevision;
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value(QStringLiteral("path")).toString(),
      "org.arinn.Library1", "GetAdditions");
  msg << generation << qulonglong(revision);
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 500), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, catalog, index, generation, revision](auto *call) {
    const auto reply = call->reply();
    call->deleteLater();
    if (catalog->epoch != m_epoch) {
      finishCatalog(catalog, false);
      return;
    }
    // Optional metadata is all-or-nothing and cannot invalidate the library.
    if (reply.type() == QDBusMessage::ErrorMessage ||
        reply.signature() != "(sta(st))" || reply.arguments().isEmpty()) {
      nextProvider(catalog, index);
      return;
    }
    const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
    QString receivedGeneration;
    qulonglong receivedRevision = 0;
    arg.beginStructure();
    arg >> receivedGeneration >> receivedRevision;
    bool valid = receivedGeneration == generation && receivedRevision == revision;
    QHash<QString, quint64> times;
    QString previousId;
    quint64 previousTime = std::numeric_limits<quint64>::max();
    arg.beginArray();
    while (!arg.atEnd()) {
      QString id;
      qulonglong time = 0;
      arg.beginStructure();
      arg >> id >> time;
      arg.endStructure();
      if (times.size() >= 64 || time == 0 ||
          time > quint64(std::numeric_limits<qint64>::max()) ||
          times.contains(id) || time > previousTime ||
          (time == previousTime && id.toUtf8() <= previousId.toUtf8())) {
        valid = false;
        break;
      }
      bool known = false;
      for (qsizetype i = catalog->providerStartItems; i < catalog->items.size(); ++i) {
        const auto &item = catalog->items.at(i);
        if (item.localId == id && item.parentLocal.isEmpty() && item.available &&
            (item.kind == QLatin1String("film") || item.kind == QLatin1String("series"))) {
          known = true;
          break;
        }
      }
      if (!known) valid = false;
      times.insert(id, time);
      previousId = id;
      previousTime = time;
    }
    arg.endArray();
    arg.endStructure();
    if (valid)
      for (qsizetype i = catalog->providerStartItems; i < catalog->items.size(); ++i)
        catalog->items[i].addedAtUs = times.value(catalog->items.at(i).localId);
    nextProvider(catalog, index);
  });
}
void LibraryCatalog::readFeatures(const std::shared_ptr<Catalog> &catalog,
                                  int index) {
  if (catalog->epoch != m_epoch) {
    finishCatalog(catalog, false);
    return;
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value(QStringLiteral("path")).toString(),
      "org.arinn.Library1", "Features");
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, catalog, index](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (catalog->epoch != m_epoch) {
              finishCatalog(catalog, false);
              return;
            }
            catalog->resumeFeature = false;
            if (reply.type() != QDBusMessage::ErrorMessage &&
                reply.signature() == "as" && !reply.arguments().isEmpty()) {
              if (reply.arguments().at(0).toStringList().contains(
                      QLatin1String("LibraryAdditions1")))
                catalog->additionsIndex = index;
              if (reply.arguments().at(0).toStringList().contains(QLatin1String("LibraryHierarchy1")))
                catalog->hierarchyIndex = index;
              catalog->resumeFeature =
                  reply.arguments().at(0).toStringList().contains(
                      QLatin1String("LibraryResume1"));
            }
            const bool descriptions = reply.type() != QDBusMessage::ErrorMessage
                && reply.signature() == "as" && !reply.arguments().isEmpty()
                && reply.arguments().at(0).toStringList().contains(QLatin1String("LibraryDescriptions1"));
            for (qsizetype i = catalog->providerStartItems; i < catalog->items.size(); ++i)
              catalog->items[i].descriptions = descriptions;
            if (catalog->resumeFeature)
              readResumes(catalog, index);
            else
              nextProvider(catalog, index);
          });
}
void LibraryCatalog::readResumes(const std::shared_ptr<Catalog> &catalog,
                                 int index, const QString &token) {
  if (catalog->epoch != m_epoch) {
    finishCatalog(catalog, false);
    return;
  }
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    dropResumes(catalog, index);
    return;
  }
  catalog->tokens.insert(token);
  if (token.isEmpty()) {
    catalog->pageGeneration.clear();
    catalog->pageRevision = 0;
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value(QStringLiteral("path")).toString(),
      "org.arinn.Library1", "GetResumes");
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
            reply.arguments().isEmpty() || reply.signature() != resumePage) {
          dropResumes(catalog, index);
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString generation, next;
        qulonglong revision = 0;
        arg.beginStructure();
        arg >> generation >> revision;
        if (generation !=
                provider.value(QStringLiteral("generation")).toString() ||
            revision == 0 ||
            (!token.isEmpty() &&
             (generation != catalog->pageGeneration ||
              revision != catalog->pageRevision))) {
          dropResumes(catalog, index);
          return;
        }
        catalog->pageGeneration = generation;
        catalog->pageRevision = revision;
        QList<ResumeRow> page;
        arg.beginArray();
        while (!arg.atEnd()) {
          ResumeRow row;
          qlonglong position = 0, duration = -1;
          bool completed = false;
          qulonglong observed = 0;
          arg.beginStructure();
          arg >> row.localId >> position >> duration >> completed >> observed;
          arg.endStructure();
          row.observed = observed;
          if (row.localId.isEmpty() || position < 0 || duration < -1 ||
              (duration >= 0 && position > duration)) {
            dropResumes(catalog, index);
            return;
          }
          row.position = position;
          row.duration = duration;
          row.completed = completed;
          page.append(row);
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        for (const auto &row : page) {
          auto duplicate = false;
          for (const auto &kept : catalog->resumes)
            if (kept.localId == row.localId)
              duplicate = true;
          if (!duplicate)
            catalog->resumes.append(row);
        }
        if (!next.isEmpty())
          readResumes(catalog, index, next);
        else {
          applyResumes(catalog, index);
          nextProvider(catalog, index);
        }
      });
}
void LibraryCatalog::finishCatalog(const std::shared_ptr<Catalog> &catalog,
                                   bool success) {
  m_refreshing = false;
  if (catalog->epoch != m_epoch) {
    refresh();
    return;
  }
  m_loaded = true;
  m_connected = true;
  const auto everyProviderFailed =
      !catalog->providers.isEmpty() &&
      catalog->failed >= catalog->providers.size();
  if (!success || everyProviderFailed) {
    m_error = true;
    m_unreachable = false;
    m_items.clear();
    m_hierarchies.clear();
    m_collections.clear();
  } else if (catalog->providers.isEmpty() && catalog->sawLibrary) {
    m_error = false;
    m_unreachable = true;
    m_items.clear();
    m_hierarchies.clear();
    m_collections.clear();
  } else {
    m_error = false;
    m_unreachable = false;
    m_items = catalog->items;
    m_collections = catalog->collections;
    m_hierarchies = catalog->hierarchies;
  }
  publish();
  if (!m_pendingLocal.isEmpty()) {
    const auto provider = std::exchange(m_pendingProvider, QString());
    const auto local = std::exchange(m_pendingLocal, QString());
    setOperation(QString());
    launchContent(provider, local);
  }
  if (m_dirty)
    QTimer::singleShot(0, this, &LibraryCatalog::refresh);
}
const LibraryCatalog::ItemRow *
LibraryCatalog::seriesTarget(const ItemRow &series) const {
  QList<const ItemRow *> kids;
  kids.reserve(m_items.size());
  for (const auto &item : m_items) {
    if (item.providerId == series.providerId && item.parentLocal == series.localId)
      kids.append(&item);
  }
  std::stable_sort(
      kids.begin(), kids.end(), [](const ItemRow *left, const ItemRow *right) {
        const auto seasonLeft =
            left->hasSeason ? left->season : std::numeric_limits<quint32>::max();
        const auto seasonRight = right->hasSeason
                                     ? right->season
                                     : std::numeric_limits<quint32>::max();
        if (seasonLeft != seasonRight)
          return seasonLeft < seasonRight;
        const auto episodeLeft = left->hasEpisode
                                     ? left->episode
                                     : std::numeric_limits<quint32>::max();
        const auto episodeRight = right->hasEpisode
                                      ? right->episode
                                      : std::numeric_limits<quint32>::max();
        return episodeLeft < episodeRight;
      });
  const ItemRow *resume = nullptr;
  const ItemRow *firstAvailable = nullptr;
  for (const auto *kid : kids) {
    if (!kid->available)
      continue;
    if (!firstAvailable)
      firstAvailable = kid;
    if (kid->hasResume && !kid->resumeCompleted && kid->resumePosition > 0 &&
        (!resume || kid->resumeObserved > resume->resumeObserved))
      resume = kid;
  }
  return resume ? resume : firstAvailable;
}
const LibraryCatalog::ItemRow *
LibraryCatalog::launchTarget(const ItemRow &row) const {
  auto series = row.kind == QLatin1String("series");
  if (!series) {
    for (const auto &item : m_items) {
      if (item.providerId == row.providerId && item.parentLocal == row.localId) {
        series = true;
        break;
      }
    }
  }
  if (series)
    return seriesTarget(row);
  return &row;
}
QString LibraryCatalog::entryIdFor(const QString &providerId,
                                   const QString &localId) const {
  auto local = localId;
  // Bounded walk: acceptProvider already rejects parent cycles.
  for (int depth = 0; depth <= m_items.size(); ++depth) {
    const ItemRow *found = nullptr;
    for (const auto &item : m_items)
      if (item.providerId == providerId && item.localId == local)
        found = &item;
    if (!found)
      return {};
    if (found->parentLocal.isEmpty())
      return found->id;
    local = found->parentLocal;
  }
  return {};
}
QString LibraryCatalog::posterFor(const QString &providerId,
                                 const QString &localId) const {
  const auto *entry = findId(entryIdFor(providerId, localId));
  if (!entry)
    return {};
  // The same URL as the entry's `artwork`: the library card's poster.
  return AssetImages::imageUrl(entry->providerId, entry->path,
                               entry->generation, entry->poster);
}
QString LibraryCatalog::artworkFor(const QString &providerId,
                                  const QString &localId) const {
  auto local = localId;
  // Bounded walk as in entryIdFor: an item's own fanart, then its own
  // poster (an episode's still), then the nearest parent's artwork.
  for (int depth = 0; depth <= m_items.size(); ++depth) {
    const ItemRow *found = nullptr;
    for (const auto &item : m_items)
      if (item.providerId == providerId && item.localId == local)
        found = &item;
    if (!found)
      break;
    const auto fanart = AssetImages::imageUrl(found->providerId, found->path,
                                              found->generation, found->fanart);
    if (!fanart.isEmpty())
      return fanart;
    const auto poster = AssetImages::imageUrl(found->providerId, found->path,
                                              found->generation, found->poster);
    if (!poster.isEmpty())
      return poster;
    if (found->parentLocal.isEmpty())
      break;
    local = found->parentLocal;
  }
  return {};
}
QVariantMap LibraryCatalog::titleFor(const QString &providerId,
                                     const QString &localId) const {
  const ItemRow *item = nullptr;
  for (const auto &row : m_items)
    if (row.providerId == providerId && row.localId == localId)
      item = &row;
  if (!item)
    return {};
  // Bounded walk to the top-level entry, as in entryIdFor.
  const ItemRow *top = item;
  for (int depth = 0; depth <= m_items.size() && !top->parentLocal.isEmpty(); ++depth) {
    const ItemRow *parent = nullptr;
    for (const auto &row : m_items)
      if (row.providerId == providerId && row.localId == top->parentLocal)
        parent = &row;
    if (!parent)
      break;
    top = parent;
  }
  QVariantMap names{{QStringLiteral("title"), top->title}};
  if (top != item) {
    QStringList line;
    if (item->hasSeason && item->hasEpisode)
      line << tr("S%1 E%2").arg(item->season).arg(item->episode);
    if (!item->title.isEmpty() && item->title != top->title)
      line << item->title;
    names.insert(QStringLiteral("episodeLine"), line.join(QStringLiteral(" · ")));
  }
  return names;
}
void LibraryCatalog::launchContent(const QString &providerId,
                                   const QString &localId) {
  for (const auto &item : m_items) {
    if (item.providerId == providerId && item.localId == localId) {
      launch(item.id);
      return;
    }
  }
  // Home can show a stopped film before the first catalog read finishes.
  // Launch it when that read completes instead of reporting NotFound.
  if (!m_loaded || m_refreshing) {
    m_pendingProvider = providerId;
    m_pendingLocal = localId;
    setOperation(QStringLiteral("queued"));
    return;
  }
  setOperation(QStringLiteral("NotFound"));
}
void LibraryCatalog::launch(const QString &id) {
  if (m_operation == QLatin1String("pending") ||
      m_operation == QLatin1String("queued") ||
      m_operation == QLatin1String("running"))
    return;
  ++m_commandTicket;
  const auto ticket = m_commandTicket;
  const auto epoch = m_epoch;
  const auto *row = findId(id);
  if (libraryState() != QLatin1String("ready") || !row) {
    m_launchingId.clear();
    setOperation(row ? QStringLiteral("Unavailable") : QStringLiteral("NotFound"));
    return;
  }
  const auto *target = launchTarget(*row);
  if (!target || !target->available) {
    m_launchingId.clear();
    setOperation(QStringLiteral("Unavailable"));
    return;
  }
  const auto targetId = target->id;
  const auto targetPath = target->path;
  const auto generation = target->generation;
  const auto localId = target->localId;
  m_launchingId = targetId;
  setOperation(QStringLiteral("pending"));
  auto msg = QDBusMessage::createMethodCall(service, targetPath,
                                            "org.arinn.Library1", "Launch");
  msg << generation << localId
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
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
void LibraryCatalog::watchOperation(const QString &objectPath, quint64 ticket,
                                    quint64 epoch) {
  auto timer = new QTimer(this);
  timer->setInterval(100);
  const auto deadline = QDeadlineTimer(operationWaitMs);
  connect(
      timer, &QTimer::timeout, this,
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
        connect(
            pending, &QDBusPendingCallWatcher::finished, this,
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
              QString state, error, sessionProvider, sessionLocal;
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
                if (sessionLocal.isEmpty() &&
                    capability == QLatin1String("media") && !provider.isEmpty() &&
                    !entity.isEmpty()) {
                  sessionProvider = provider;
                  sessionLocal = entity;
                }
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
              if (state == QLatin1String("succeeded")) {
                setOperation(QStringLiteral("succeeded"));
                if (!sessionLocal.isEmpty())
                  emit sessionLaunched(
                      presentationId(sessionProvider, sessionLocal));
              } else {
                setOperation(error.isEmpty() ? state : error);
              }
              timer->deleteLater();
            });
      });
  timer->start();
}
