// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "library_catalog.h"
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
  if (seconds < 60)
    return QString::number(seconds) + QStringLiteral(" s");
  return QString::number(seconds / 60) + QStringLiteral(" min");
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
};

LibraryCatalog::LibraryCatalog(QObject *parent) : QObject(parent) {
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

QString LibraryCatalog::libraryState() const {
  if (!m_loaded)
    return QStringLiteral("loading");
  if (!m_connected || m_unreachable)
    return QStringLiteral("unavailable");
  if (m_error)
    return QStringLiteral("error");
  if (m_entries.isEmpty())
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
void LibraryCatalog::publish() {
  QVariantList tabs;
  tabs.reserve(m_collections.size());
  for (const auto &collection : m_collections) {
    tabs.append(QVariantMap{{QStringLiteral("id"), collection.id},
                            {QStringLiteral("title"), collection.title},
                            {QStringLiteral("kind"), collection.kind}});
  }
  QHash<QString, QList<int>> children;
  for (int i = 0; i < m_items.size(); ++i) {
    const auto &item = m_items.at(i);
    if (!item.parentLocal.isEmpty())
      children[item.providerId + QLatin1Char('\n') + item.parentLocal].append(i);
  }
  QVariantList entries;
  for (const auto &item : m_items) {
    if (!item.parentLocal.isEmpty())
      continue;
    const auto kids =
        children.value(item.providerId + QLatin1Char('\n') + item.localId);
    QList<quint32> seasonOrder;
    QSet<quint32> seenSeasons;
    QVariantList episodes;
    for (const auto index : kids) {
      const auto &kid = m_items.at(index);
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
                                   : QString());
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
    auto progress = resumeFraction(item.resumePosition, item.resumeDuration,
                                   item.durationUs, item.resumeCompleted,
                                   item.hasResume);
    if (item.kind == QLatin1String("series") || !kids.isEmpty()) {
      if (const auto *target = seriesTarget(item))
        progress = resumeFraction(target->resumePosition, target->resumeDuration,
                                  target->durationUs, target->resumeCompleted,
                                  target->hasResume);
      else
        progress = 0;
    }
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
    entries.append(entry);
  }
  m_tabs = tabs;
  m_entries = entries;
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
          catalog->tokens.clear();
          catalog->pageGeneration.clear();
          catalog->pageRevision = 0;
          readFeatures(catalog, index);
        }
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
              catalog->resumeFeature =
                  reply.arguments().at(0).toStringList().contains(
                      QLatin1String("LibraryResume1"));
            }
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
    m_collections.clear();
  } else if (catalog->providers.isEmpty() && catalog->sawLibrary) {
    m_error = false;
    m_unreachable = true;
    m_items.clear();
    m_collections.clear();
  } else {
    m_error = false;
    m_unreachable = false;
    m_items = catalog->items;
    m_collections = catalog->collections;
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
