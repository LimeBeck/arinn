// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include "build_identity.h"
#include "icon_references.h"
#include "web_icon_references.h"
#include <QDateTime>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSet>
#include <QTextBoundaryFinder>
#include <QUrl>
#include <QUuid>
#include <QtDBus>
#include <algorithm>
#include <arpa/inet.h>
// Local transport types; Favorites1 stores identities, never presentation.
struct FavoriteWire {
  QString provider, capability, local;
};
Q_DECLARE_METATYPE(FavoriteWire)
Q_DECLARE_METATYPE(QList<FavoriteWire>)
QDBusArgument &operator<<(QDBusArgument &arg, const FavoriteWire &v) {
  arg.beginStructure();
  arg << v.provider << v.capability << v.local;
  arg.endStructure();
  return arg;
}
const QDBusArgument &operator>>(const QDBusArgument &arg, FavoriteWire &v) {
  arg.beginStructure();
  arg >> v.provider >> v.capability >> v.local;
  arg.endStructure();
  return arg;
}
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
QDBusMessage request(const char *method) {
  return QDBusMessage::createMethodCall(service, path, service, method);
}
// First grapheme, uppercased. A leading unpaired surrogate is not a letter:
// the tile keeps the neutral placeholder.
QString monogramFor(const QString &title) {
  const auto trimmed = title.trimmed();
  if (trimmed.isEmpty())
    return {};
  const auto first = trimmed.at(0);
  if (first.isSurrogate() &&
      !(first.isHighSurrogate() && trimmed.size() > 1 &&
        trimmed.at(1).isLowSurrogate()))
    return {};
  QTextBoundaryFinder finder(QTextBoundaryFinder::Grapheme, trimmed);
  const int end = finder.toNextBoundary();
  if (end <= 0)
    return {};
  const auto grapheme = trimmed.left(end);
  if (grapheme.size() == 1 && grapheme.at(0).isSurrogate())
    return {};
  return grapheme.toUpper();
}
bool shortcutUrlValid(const QString &value) {
  if (std::any_of(value.begin(), value.end(), [](QChar c) { return c.isSpace() || c == '\\' || c.category() == QChar::Other_Control; }))
    return false;
  const QUrl url(value, QUrl::StrictMode);
  if (!url.isValid() || url.host().isEmpty() || !url.userInfo().isEmpty() || value.section('/', 2, 2).contains('@') || url.port() == 0 || value.section('/', 2, 2).endsWith(':'))
    return false;
  if (url.scheme() == "https") return true;
  if (url.scheme() != "http") return false;
  const auto host = url.host().toLower();
  in_addr v4{};
  const auto bytes = host.toLatin1();
  if (inet_pton(AF_INET, bytes.constData(), &v4) == 1) {
    const auto address = ntohl(v4.s_addr);
    return (address >> 24) == 10 || (address >> 24) == 127 ||
        (address >> 20) == 0xac1 || (address >> 16) == 0xc0a8 || (address >> 16) == 0xa9fe;
  }
  in6_addr v6{};
  if (inet_pton(AF_INET6, bytes.constData(), &v6) == 1)
    return IN6_IS_ADDR_LOOPBACK(&v6) || (v6.s6_addr[0] & 0xfe) == 0xfc ||
        (v6.s6_addr[0] == 0xfe && (v6.s6_addr[1] & 0xc0) == 0x80);
  if (host.contains(':')) return false;
  if (std::all_of(host.begin(), host.end(), [](QChar c) { return c.isDigit() || c == '.'; })) return false;
  return !host.contains('.') || host.endsWith(".local") || host.endsWith(".lan") ||
      host.endsWith(".home.arpa") || host.endsWith(".internal");
}
} // namespace
struct Backend::Catalog {
  quint64 epoch;
  QString snapshot;
  QVariantList providers, applications, running;
  QSet<QString> tokens;
  QString generation;
  quint64 revision = 0;
  int pages = 0;
  qsizetype providerStart = 0, runningStart = 0;
  bool hadError = false;
};
Backend::Backend(QObject *parent) : QObject(parent) {
  qDBusRegisterMetaType<FavoriteWire>();
  qDBusRegisterMetaType<QList<FavoriteWire>>();
  // An action's result is shown briefly; connection states stay (#258).
  m_resultTimer.setSingleShot(true);
  m_resultTimer.setInterval(resultMessageMs);
  connect(&m_resultTimer, &QTimer::timeout, this, [this] {
    if (m_message != m_resultMessage)
      return;
    m_message.clear();
    m_resultMessage.clear();
    publishState();
  });
  auto bus = QDBusConnection::sessionBus();
  // Presentation intent is transient: subscribe before the first Status.
  bus.connect(service, path, service, "HomeRequested", this,
              SLOT(presentHome(QString, QString, QString)));
  bus.connect(service, path, service, "QuickMenuRequested", this,
              SLOT(presentQuickMenu(QString, QString, QString)));
  bus.connect(service, path, service, "PowerMenuRequested", this,
              SLOT(presentPowerMenu(QString, QString, QString)));
  bus.connect(service, path, service, "Changed", this, SLOT(reloadCatalog()));
  bus.connect(service, path, service, "ProvidersChanged", this,
              SLOT(reloadCatalog()));
  // Focus and provider StateChanged refresh the catalog only. Listing
  // shortcuts here would repeat List on every focus change.
  bus.connect(service, path, service, "StateChanged", this,
              SLOT(reloadCatalog()));
  // Subscribe before the first List. This signal re-reads shortcuts only.
  bus.connect(service, path, "org.arinn.WebShortcuts1", "Changed", this,
              SLOT(reloadShortcuts()));
  bus.connect(service, path, "org.arinn.Favorites1", "Changed", this,
              SLOT(reloadFavorites()));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this](const QString &, const QString &, const QString &owner) {
            ++m_favoriteTicket;
            m_favoriteRevision = 0;
            m_favoritesLoaded = m_favoritesError = m_favoritesRequested =
                m_favoriteSaving = false;
            m_favoriteReconcile = false;
            m_favoriteTargets.clear();
            m_favoriteError.clear();
            ++m_connectionEpoch;
            m_daemonOwner = owner;
            // A new daemon owner may be another build: ask it again.
            m_buildRequested = m_buildMismatch = false;
            m_pendingIds.clear();
            m_launches.clear();
            m_shownLaunch.clear();
            m_connected = false;
            m_available = false;
            m_generation.clear();
            m_snapshotRevision = 0;
            m_items.clear();
            m_applications.clear();
            m_return.clear();
            m_loaded = false;
            m_snapshotError = false;
            clearShortcuts();
            publishState();
            refresh();
          });
  connect(&m_timer, &QTimer::timeout, this, &Backend::reloadCatalog);
  m_timer.start(1000);
  const auto ownerTicket = m_connectionEpoch;
  auto lookup = QDBusMessage::createMethodCall(
      "org.freedesktop.DBus", "/org/freedesktop/DBus",
      "org.freedesktop.DBus", "GetNameOwner");
  lookup << QString::fromLatin1(service);
  auto *ownerCall = new QDBusPendingCallWatcher(bus.asyncCall(lookup, 3000), this);
  connect(ownerCall, &QDBusPendingCallWatcher::finished, this,
          [this, ownerTicket](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (ownerTicket != m_connectionEpoch)
              return;
            if (reply.type() != QDBusMessage::ErrorMessage && reply.signature() == "s")
              m_daemonOwner = reply.arguments().first().toString();
            refresh();
          });
}
// A request counts only for the daemon this shell is connected to; one from
// another epoch or before the first Status is dropped, never replayed.
void Backend::presentHome(const QString &epoch, const QString &,
                          const QString &) {
  if (m_connected && !epoch.isEmpty() && epoch == m_daemonEpoch)
    emit homeRequested();
}
void Backend::presentQuickMenu(const QString &epoch, const QString &target,
                               const QString &) {
  if (m_connected && !epoch.isEmpty() && epoch == m_daemonEpoch)
    emit quickMenuRequested(target);
}
void Backend::presentPowerMenu(const QString &epoch, const QString &,
                               const QString &) {
  if (m_connected && !epoch.isEmpty() && epoch == m_daemonEpoch)
    emit powerMenuRequested();
}
void Backend::publishState() {
  if (m_publishedItems != m_items) {
    m_publishedItems = m_items;
    emit itemsChanged();
  }
  const auto entries = continueEntries();
  if (m_publishedContinue != entries) {
    m_publishedContinue = entries;
    emit continueEntriesChanged();
  }
  if (m_publishedApplications != applications()) {
    m_publishedApplications = applications();
    emit applicationsChanged();
  }
  if (m_publishedShortcuts != m_webShortcuts) {
    m_publishedShortcuts = m_webShortcuts;
    emit webShortcutsChanged();
    emit applicationsChanged();
  }
  traceFlows();
  emit changed();
}
void Backend::traceFlows() {
  // Home load: from "loading" to the first state Home settles in.
  const auto state = homeState();
  if (state == QLatin1String("loading")) {
    if (!m_homeFlow.active())
      m_homeFlow.start("home.load");
  } else if (m_homeFlow.active()) {
    m_homeFlow.finish(state == QLatin1String("ready") || state == QLatin1String("empty") ? "ok"
                      : state == QLatin1String("unavailable")                         ? "Unavailable"
                      : state == QLatin1String("updated")                             ? "updated"
                                                                                      : "failed");
  }
  // Return: done once the daemon hides Home.
  if (m_returnFlow.active() && !m_home)
    m_returnFlow.finish("ok");
  else if (m_returnFlow.active() && m_returnFlow.elapsedMs() > returnTraceMs)
    m_returnFlow.finish("Timeout");
}
QVariantList Backend::continueEntries() const {
  if (!m_connected || m_return.isEmpty())
    return {};
  return {QVariantMap{{"id", m_return},
                      {"title", tr("Return to activity")},
                      {"subtitle", tr("Continue where you left off")},
                      {"running", true}}};
}
QVariantList Backend::applications() const {
  // Applications › Online is browsers and web shortcuts (spec §5.5). Shortcuts
  // follow the catalog entries so provider indexes stay stable. The URL stays
  // out of this model.
  auto rows = m_applications;
  for (auto &value : rows) {
    auto row = value.toMap();
    row["pinned"] = isFavorite(row);
    value = row;
  }
  for (const auto &item : m_webShortcuts) {
    auto map = item.toMap();
    map["pinned"] = isFavorite(map);
    map.insert(QStringLiteral("group"), QStringLiteral("online"));
    map.insert(QStringLiteral("enabled"), true);
    rows.append(map);
  }
  return rows;
}
QVariantList Backend::favorites() const {
  QVariantList rows;
  const auto catalog = applications();
  for (const auto &value : m_favoriteTargets) {
    const auto target = value.toMap();
    QVariantMap row;
    for (const auto &candidate : catalog) {
      const auto entry = candidate.toMap();
      if (entry.value("providerId") == target.value("providerId") &&
          entry.value("capability") == target.value("capability") &&
          entry.value("localId") == target.value("localId")) {
        row = entry;
        break;
      }
    }
    if (row.isEmpty()) {
      // Retain a removable placeholder without disclosing opaque target IDs.
      const auto key =
          QUuid::createUuidV5(
              QUuid(),
              QJsonDocument(QJsonArray{target.value("providerId").toString(),
                                       target.value("capability").toString(),
                                       target.value("localId").toString()})
                  .toJson(QJsonDocument::Compact))
              .toString();
      row = target;
      row["id"] = key;
      row["title"] = tr("Unavailable favorite");
      row["subtitle"] = tr("Open actions to remove");
      row["enabled"] = true;
      row["unavailable"] = true;
      row["launchable"] = false;
    }
    if (!row.contains("launchable"))
      row["launchable"] = row.value("enabled", true);
    row["enabled"] = true;
    row["pinned"] = true;
    rows.append(row);
  }
  return rows;
}
bool Backend::isFavorite(const QVariantMap &target) const {
  return std::any_of(
      m_favoriteTargets.begin(), m_favoriteTargets.end(),
      [&](const auto &value) {
        const auto entry = value.toMap();
        return entry.value("providerId") == target.value("providerId") &&
               entry.value("capability") == target.value("capability") &&
               entry.value("localId") == target.value("localId");
      });
}
QVariantMap Backend::favoriteTarget(const QString &id) const {
  auto rows = applications();
  rows += favorites();
  for (const auto &value : rows) {
    const auto row = value.toMap();
    if (row.value("id").toString() == id &&
        !row.value("providerId").toString().isEmpty())
      return {{"providerId", row.value("providerId")},
              {"capability", row.value("capability")},
              {"localId", row.value("localId")}};
  }
  return {};
}
QString Backend::homeState() const {
  if (!m_connected)
    return "unavailable";
  // Another daemon build may lack what this shell calls: say so first.
  if (m_buildMismatch)
    return "updated";
  if (!m_return.isEmpty() || !m_favoriteTargets.isEmpty())
    return "ready";
  if (m_favoritesError || m_shortcutsError || m_snapshotError)
    return "error";
  return m_favoritesLoaded && m_shortcutsLoaded && m_loaded ? "empty"
                                                            : "loading";
}
QString Backend::applicationsState() const {
  if (!m_connected)
    return "unavailable";
  if (m_snapshotError)
    return "error";
  if (!m_loaded)
    return "loading";
  return m_applications.isEmpty() ? "empty" : "ready";
}
void Backend::clearShortcuts() {
  WebIconReferences::clear();
  ++m_shortcutTicket;
  m_webShortcuts.clear();
  m_shortcutRecords.clear();
  m_shortcutRevision = 0;
  m_shortcutSaving = false;
  closeShortcutEditor();
  m_shortcutsLoaded = false;
  m_shortcutsError = false;
  m_shortcutsRequested = false;
}
void Backend::refresh() {
  m_forceShortcutReload = true;
  m_favoritesRequested = false;
  m_favoriteError.clear();
  reloadCatalog();
}
void Backend::reloadShortcuts() {
  WebIconReferences::clear();
  readShortcuts(m_connectionEpoch);
}
void Backend::checkBuild(quint64 epoch) {
  m_buildRequested = true;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(request("BuildInfo"), 3000),
      this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_connectionEpoch)
              return;
            bool mismatch = false;
            if (reply.type() == QDBusMessage::ErrorMessage) {
              // A daemon older than BuildInfo is another build. Any other
              // error proves nothing: ask again on the next Status.
              if (reply.errorName() !=
                  QLatin1String("org.freedesktop.DBus.Error.UnknownMethod")) {
                m_buildRequested = false;
                return;
              }
              mismatch = true;
            } else if (reply.signature() == QLatin1String("ss")) {
              mismatch = !BuildIdentity::same(
                  BuildIdentity::shell(), reply.arguments().at(1).toString());
            } else {
              m_buildRequested = false;
              return;
            }
            if (mismatch != m_buildMismatch) {
              m_buildMismatch = mismatch;
              publishState();
            }
          });
}
void Backend::restartArinn() {
  // After the QML call returns; the launcher restarts the daemon and systemd
  // the shell (ADR-0098).
  QTimer::singleShot(0, qApp, [] {
    QCoreApplication::exit(BuildIdentity::RestartExitStatus);
  });
}
void Backend::reloadCatalog() {
  if (m_refreshing) {
    m_dirty = true;
    return;
  }
  m_refreshing = true;
  m_dirty = false;
  const auto epoch = m_connectionEpoch;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(request("Status"), 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_connectionEpoch) {
              m_refreshing = false;
              refresh();
              return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.arguments().isEmpty() ||
                reply.signature() != "(sbstbsssbbss)") {
              m_refreshing = false;
              m_connected = false;
              m_available = false;
              m_items.clear();
              m_applications.clear();
              m_return.clear();
              m_loaded = false;
              m_snapshotError = false;
              clearShortcuts();
              m_message = tr("Daemon is unavailable · reconnecting");
              publishState();
              return;
            }
            const QDBusArgument arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            const auto previousGeneration = m_generation;
            QString daemonEpoch, active, klass, homeOutput, activeOutput;
            qulonglong revision = 0;
            bool full, platform;
            arg.beginStructure();
            arg >> daemonEpoch >> m_available >> m_generation >> revision >>
                m_home >> m_return >> active >> klass >> full >> platform >>
                homeOutput >> activeOutput;
            arg.endStructure();
            if (m_daemonEpoch != daemonEpoch)
              WebIconReferences::clear();
            m_daemonEpoch = daemonEpoch;
            if (previousGeneration != m_generation) {
              m_items.clear();
              m_applications.clear();
              m_snapshotRevision = 0;
            }
            emit screenRequested(homeOutput);
            m_connected = true;
            if (!m_buildRequested)
              checkBuild(epoch);
            // List once per connection, on an explicit refresh, and on
            // WebShortcuts1.Changed. A focus StateChanged must not List.
            // Defer List so Home can publish "loading" first; a same-thread
            // bus would otherwise deliver it before that state.
            if (m_forceShortcutReload || !m_shortcutsRequested) {
              m_forceShortcutReload = false;
              QTimer::singleShot(0, this, [this, epoch] {
                if (epoch == m_connectionEpoch)
                  readShortcuts(epoch);
              });
            }
            if (!m_favoritesRequested)
              readFavorites();
            // Status is polled every second. It may replace a connection
            // banner, and only a connection banner: an operation result has to
            // stay visible when that poll's reply arrives in the same turn.
            const auto connectionBanner =
                m_message.isEmpty() || m_message.startsWith(QLatin1String("Daemon")) ||
                m_message.startsWith(QLatin1String("Provider")) ||
                m_message.startsWith(QLatin1String("KWin"));
            if (!platform) {
              if (connectionBanner)
                m_message = tr("KWin adapter is not connected");
            } else if (!m_available) {
              if (connectionBanner)
                m_message = tr("Provider is unavailable");
            } else if (m_message.startsWith(QLatin1String("Daemon")) ||
                       m_message.startsWith(QLatin1String("Provider")) ||
                       m_message.startsWith(QLatin1String("KWin")))
              m_message.clear();
            publishState();
            auto catalog = std::make_shared<Catalog>();
            catalog->epoch = epoch;
            readProviders(catalog);
          });
}
void Backend::readProviders(const std::shared_ptr<Catalog> &catalog,
                            const QString &token) {
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    finishCatalog(catalog, false);
    return;
  }
  catalog->tokens.insert(token);
  auto msg = request("Providers");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (catalog->epoch != m_connectionEpoch ||
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
          qulonglong revision, errorAt, recovered, restarts, cumulative;
          arg.beginStructure();
          arg >> id >> plugin >> source >> name >> version >> endpoint >>
              capabilities >> runtime >> backend >> generation >> revision >>
              error >> errorAt >> recovered >> restarts >> cumulative;
          arg.endStructure();
          if (runtime == "ready" && backend != "unavailable" &&
              capabilities.contains("application"))
            catalog->providers.append(QVariantMap{{"id", id},
                                                  {"path", endpoint.path()},
                                                  {"generation", generation}});
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty())
          readProviders(catalog, next);
        else {
          catalog->tokens.clear();
          readApplications(catalog);
        }
      });
}
void Backend::readApplications(const std::shared_ptr<Catalog> &catalog,
                               int index, const QString &token) {
  if (index >= catalog->providers.size()) {
    finishCatalog(catalog, true);
    return;
  }
  if (++catalog->pages > 1024 || catalog->tokens.contains(token)) {
    finishCatalog(catalog, false);
    return;
  }
  catalog->tokens.insert(token);
  if (token.isEmpty()) {
    catalog->providerStart = catalog->applications.size();
    catalog->runningStart = catalog->running.size();
  }
  const auto provider = catalog->providers.at(index).toMap();
  auto msg = QDBusMessage::createMethodCall(
      service, provider.value("path").toString(), "org.arinn.Application1",
      "GetApplications");
  msg << token;
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, catalog, provider, index, token](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (catalog->epoch != m_connectionEpoch) {
          finishCatalog(catalog, false);
          return;
        }
        auto unavailable = [this, catalog, index] {
          catalog->hadError = true;
          catalog->applications.resize(catalog->providerStart);
          catalog->running.resize(catalog->runningStart);
          catalog->tokens.clear();
          readApplications(catalog, index + 1);
        };
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != "(sta(ssbss)a(ssssbb)s)") {
          unavailable();
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        QString generation, next;
        qulonglong revision = 0;
        arg.beginStructure();
        arg >> generation >> revision;
        if (generation != provider.value("generation").toString() ||
            (!token.isEmpty() && (generation != catalog->generation ||
                                  revision != catalog->revision))) {
          unavailable();
          return;
        }
        catalog->generation = generation;
        catalog->revision = revision;
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, title, group, icon;
          bool launchable;
          arg.beginStructure();
          arg >> id >> title >> launchable >> group >> icon;
          arg.endStructure();
          // Providers choose the group; unknown groups display as "other".
          static const QStringList groups{"online", "games", "media", "other"};
          if (!groups.contains(group))
            group = "other";
          const auto key =
              QUuid::createUuidV5(
                  QUuid(),
                  QJsonDocument(QJsonArray{provider.value("id").toString(),
                                           generation, "launch", id})
                      .toJson(QJsonDocument::Compact))
                  .toString();
          catalog->applications.append(QVariantMap{
              {"id", key},
              {"title", title},
              {"iconSource", IconReferences::imageUrl(icon)},
              {"visualIdentity", provider.value("id").toString() + ":" + id},
              {"monogram", monogramFor(title)},
              {"enabled", launchable},
              {"group", group},
              {"path", provider.value("path")},
              {"generation", generation},
              {"localId", id},
              {"providerId", provider.value("id")},
              {"capability", QStringLiteral("application")}});
        }
        arg.endArray();
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, application, title, window;
          bool activate, terminate;
          arg.beginStructure();
          arg >> id >> application >> title >> window >> activate >> terminate;
          arg.endStructure();
          const auto key =
              QUuid::createUuidV5(
                  QUuid(),
                  QJsonDocument(QJsonArray{provider.value("id").toString(),
                                           generation, "activate", id})
                      .toJson(QJsonDocument::Compact))
                  .toString();
          catalog->running.append(
              QVariantMap{{"id", key},
                          {"title", title},
                          {"enabled", activate},
                          {"running", true},
                          {"path", provider.value("path")},
                          {"generation", generation},
                          {"localId", id},
                          {"applicationId", application},
                          {"providerId", provider.value("id")},
                          {"terminable", terminate}});
        }
        arg.endArray();
        arg >> next;
        arg.endStructure();
        if (!next.isEmpty())
          readApplications(catalog, index, next);
        else {
          catalog->tokens.clear();
          readApplications(catalog, index + 1);
        }
      });
}
void Backend::finishCatalog(const std::shared_ptr<Catalog> &catalog,
                            bool success) {
  m_refreshing = false;
  if (catalog->epoch != m_connectionEpoch) {
    refresh();
    return;
  }
  m_snapshotError = !success || catalog->hadError;
  m_loaded = success;
  m_applications = success ? catalog->applications : QVariantList{};
  m_items = success ? catalog->running : QVariantList{};
  for (auto &value : m_applications) {
    auto app = value.toMap();
    QVariantList matches;
    for (const auto &running : m_items) {
      const auto row = running.toMap();
      if (row.value("providerId") == app.value("providerId") &&
          row.value("applicationId") == app.value("localId") &&
          row.value("terminable").toBool())
        matches.append(row);
    }
    if (matches.size() == 1)
      app["runningId"] = matches.first().toMap().value("id");
    value = app;
  }
  publishState();
  if (m_dirty)
    QTimer::singleShot(0, this, &Backend::reloadCatalog);
}
void Backend::back() {
  m_returnFlow.start("activity.return");
  auto call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(request("Back"), 3000), this);
  connect(call, &QDBusPendingCallWatcher::finished, this, [this](auto *call) {
    if (call->isError()) {
      m_returnFlow.finish(FlowTrace::outcome(call->error().name().section('.', -1)));
      setResultMessage(call->error().message());
      publishState();
    }
    call->deleteLater();
  });
}
void Backend::readShortcuts(quint64 epoch) {
  if (epoch != m_connectionEpoch || !m_daemonOwner.startsWith(':'))
    return;
  m_shortcutsRequested = true;
  const auto ticket = ++m_shortcutTicket;
  auto msg = QDBusMessage::createMethodCall(m_daemonOwner, path,
                                            "org.arinn.WebShortcuts1", "List");
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, epoch, ticket](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (epoch != m_connectionEpoch || ticket != m_shortcutTicket)
          return;
        auto fail = [this] {
          WebIconReferences::clear();
          m_webShortcuts.clear();
          m_shortcutsLoaded = false;
          m_shortcutsError = true;
          publishState();
        };
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.arguments().isEmpty() || reply.signature() != "(ta(ssss))") {
          fail();
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        qulonglong revision = 0;
        QVariantList entries;
        QHash<QString, QVariantMap> records;
        bool valid = true;
        arg.beginStructure();
        arg >> revision;
        arg.beginArray();
        while (!arg.atEnd()) {
          QString id, title, url, icon;
          arg.beginStructure();
          arg >> id >> title >> url >> icon;
          arg.endStructure();
          // Full user configuration stays private to C++, except the selected
          // editor draft. Tile models never carry URLs or icon references.
          auto bounded = [](const QString &value, int limit, bool empty) {
            return (empty || !value.trimmed().isEmpty()) &&
                   value.toUtf8().size() <= limit &&
                   std::none_of(value.begin(), value.end(), [](QChar c) {
                     return c.category() == QChar::Other_Control;
                   });
          };
          valid = valid && bounded(id, 256, false) && bounded(title, 256, false) &&
                  bounded(url, 2048, false) && bounded(icon, 256, true) &&
                  shortcutUrlValid(url) && !records.contains(id) && records.size() < 64;
          if (!valid) {
            fail();
            return;
          }
          records.insert(
              id, {{"id", id}, {"title", title}, {"url", url}, {"icon", icon}});
          entries.append(
              QVariantMap{{"id", id},
                          {"title", title},
                          {"monogram", monogramFor(title)},
                          {"webShortcut", true},
                          {"providerId", QStringLiteral("arinn")},
                          {"capability", QStringLiteral("web-shortcut")},
                          {"localId", id}});
        }
        arg.endArray();
        arg.endStructure();
        if (!valid || revision == 0 || revision < m_shortcutRevision ||
            (revision == m_shortcutRevision && records != m_shortcutRecords)) {
          fail();
          return;
        }
        if (!reply.service().isEmpty() && reply.service() != m_daemonOwner) {
          fail();
          return;
        }
        if (revision != m_shortcutRevision)
          WebIconReferences::clear();
        for (auto &entry : entries) {
          auto row = entry.toMap();
          const auto id = row.value("id").toString();
          row.insert("iconSource", WebIconReferences::add(m_daemonOwner, m_daemonEpoch,
              revision, id, records.value(id).value("icon").toString()));
          entry = row;
        }
        m_shortcutRevision = revision;
        m_shortcutRecords = records;
        m_webShortcuts = entries;
        m_shortcutsLoaded = true;
        m_shortcutsError = false;
        publishState();
      });
}
void Backend::launch(const QString &id) {
  // Home favorites and Applications share entryRequested. Shortcut ids and
  // application ids do not collide: a shortcut opens through WebShortcuts1,
  // and any other known id is an application launch target.
  for (const auto &item : m_webShortcuts) {
    if (item.toMap().value("id").toString() == id) {
      openShortcut(id);
      return;
    }
  }
  for (const auto &item : m_applications) {
    if (item.toMap().value("id").toString() == id) {
      runApplication(id, m_applications, "Launch");
      return;
    }
  }
  setResultMessage(tr("Shortcut is no longer available"));
  publishState();
}
void Backend::activate(const QString &id) {
  runApplication(id, m_items, "Activate");
}
void Backend::runApplication(const QString &id, const QVariantList &entries,
                             const char *method) {
  const bool launching = QByteArray(method) == "Launch";
  if (m_pendingIds.contains(id)) {
    // Its starting screen comes back, also behind a newer launch (§6.5).
    const auto found = m_launches.constFind(id);
    if (launching && found != m_launches.constEnd() && found->error.isEmpty()) {
      showLaunch(id);
      m_launches[id].hidden = false;
      publishState();
      emit launchShown();
    }
    return;
  }
  QVariantMap selected;
  for (const auto &item : entries) {
    if (item.toMap().value("id").toString() == id) {
      selected = item.toMap();
      break;
    }
  }
  if (selected.isEmpty() ||
      !selected
           .value(QByteArray(method) == "Terminate" ? "terminable" : "enabled",
                  true)
           .toBool()) {
    setResultMessage(selected.isEmpty() ? tr("Shortcut is no longer available")
                                        : QStringLiteral("Unavailable"));
    publishState();
    return;
  }
  auto msg =
      QDBusMessage::createMethodCall(service, selected.value("path").toString(),
                                     "org.arinn.Application1", method);
  msg << selected.value("generation").toString()
      << selected.value("localId").toString()
      << QUuid::createUuid().toString(QUuid::WithoutBraces);
  if (launching) {
    // The newest launch is on screen; earlier ones keep their records.
    Launch launch;
    launch.id = id;
    launch.title = selected.value("title").toString();
    launch.started = QDateTime::currentMSecsSinceEpoch();
    m_launches.insert(id, launch);
    showLaunch(id);
  }
  beginOperation(id, msg);
  if (launching && m_launches.contains(id))
    emit launchStarted();
}
qint64 Backend::launchElapsed() const {
  const auto *launch = shownLaunch();
  return launch && launch->started > 0
             ? QDateTime::currentMSecsSinceEpoch() - launch->started
             : 0;
}
void Backend::showLaunch(const QString &id) {
  if (m_shownLaunch != id) {
    const auto previous = m_launches.find(m_shownLaunch);
    if (previous != m_launches.end())
      previous->hidden = true;
    m_launchConfirming = false;
  }
  m_shownLaunch = id;
}
void Backend::setLaunchHidden(bool hidden) {
  const auto found = m_launches.find(m_shownLaunch);
  if (found != m_launches.end() && found->error.isEmpty())
    found->hidden = hidden;
}
void Backend::setLaunchConfirming(bool confirming) {
  if (m_launchConfirming == confirming)
    return;
  m_launchConfirming = confirming;
  // A failure that waited behind the confirmation comes on screen next. Not
  // within this call: the overlay reports from a property change, and a
  // confirmed Stop is sent in the same turn.
  if (!confirming)
    QMetaObject::invokeMethod(
        this,
        [this] {
          if (showQueuedFailure())
            publishState();
        },
        Qt::QueuedConnection);
}
bool Backend::showQueuedFailure() {
  const auto *shown = shownLaunch();
  if (m_launchConfirming || (shown && !shown->error.isEmpty()))
    return false;
  const Launch *oldest = nullptr;
  for (const auto &launch : std::as_const(m_launches))
    if (!launch.error.isEmpty() && (!oldest || launch.started < oldest->started))
      oldest = &launch;
  if (!oldest)
    return false;
  // The launch it covers keeps its visibility for when the dialog closes.
  m_shownLaunch = oldest->id;
  return true;
}
void Backend::forgetShownLaunch() {
  m_launches.remove(m_shownLaunch);
  m_shownLaunch.clear();
  // A failure that waited behind the one on screen comes next; otherwise the
  // newest launch still in flight.
  qint64 newest = -1;
  bool failed = false;
  for (const auto &launch : std::as_const(m_launches)) {
    const bool error = !launch.error.isEmpty();
    if ((error && !failed) || (error == failed && launch.started > newest)) {
      failed = error;
      newest = launch.started;
      m_shownLaunch = launch.id;
    }
  }
}
void Backend::failLaunch(Launch &launch, const QString &error) {
  launch.error = error.isEmpty() ? QStringLiteral("Unavailable") : error;
  launch.cancellable = launch.cancelling = false;
  // Another failure already on screen stays until it is dismissed, and an
  // open stop confirmation is not interrupted: an OK meant for it must not
  // land on this dialog. The launch it covers keeps its visibility.
  const auto *shown = shownLaunch();
  if (shown && shown != &launch &&
      (!shown->error.isEmpty() || m_launchConfirming))
    return;
  if (shown != &launch)
    m_launchConfirming = false;
  m_shownLaunch = launch.id;
}
void Backend::cancelLaunch() {
  auto found = m_launches.find(m_shownLaunch);
  if (!launchCancellable() || found == m_launches.end() ||
      found->operation.isEmpty())
    return;
  found->cancelling = true;
  publishState();
  const auto epoch = m_connectionEpoch;
  const auto id = found->id;
  const auto operation = found->operation;
  auto msg = QDBusMessage::createMethodCall(service, operation,
                                            "org.arinn.Operation1", "Cancel");
  // The provider stops the launch's processes before it answers.
  auto call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 20000), this);
  connect(call, &QDBusPendingCallWatcher::finished, this,
          [this, epoch, id, operation](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            auto *launch = launchRecord(id, operation);
            if (epoch != m_connectionEpoch || !launch)
              return;
            // Success is read back from the operation, which ends as
            // cancelled. A Timeout means the stop is still under way: the
            // launch reads "Stopping…" until its operation ends. Another
            // refusal leaves the launch starting.
            if (reply.type() == QDBusMessage::ErrorMessage &&
                reply.errorName() == QLatin1String("org.arinn.Error1.Timeout")) {
              launch->stopping = true;
            } else {
              launch->cancelling = false;
              if (reply.type() == QDBusMessage::ErrorMessage)
                setResultMessage(tr("The launch could not be stopped"));
            }
            publishState();
          });
}
void Backend::retryLaunch() {
  const auto *shown = shownLaunch();
  if (!shown || shown->error.isEmpty())
    return;
  const auto id = shown->id;
  m_launches.remove(id);
  m_shownLaunch.clear();
  launch(id);
  // Nothing was started (the entry is gone): show what still runs.
  if (!m_launches.contains(id))
    dismissed();
  // Another failure waited behind this dialog: it comes next, over the
  // new launch.
  else if (showQueuedFailure())
    publishState();
}
void Backend::dismissLaunchFailure() {
  const auto *shown = shownLaunch();
  if (!shown || shown->error.isEmpty())
    return;
  forgetShownLaunch();
  dismissed();
}
void Backend::dismissed() {
  // Read before publishing: the overlay restarts for the launch that takes
  // the screen and reports that as shown.
  const auto *next = shownLaunch();
  const bool hidden = next && next->hidden;
  publishState();
  if (!launchActive())
    return;
  // Closing a dialog returns to what it covered (§6.1): a starting screen
  // that was on screen comes back; one that Home hid stays hidden (§6.5).
  if (hidden) {
    m_launches[m_shownLaunch].hidden = true;
    emit launchHidden();
  } else {
    emit launchShown();
  }
}
void Backend::openShortcut(const QString &id) {
  if (m_pendingIds.contains(id))
    return;
  bool found = false;
  for (const auto &item : m_webShortcuts) {
    if (item.toMap().value("id").toString() == id) {
      found = true;
      break;
    }
  }
  if (!found) {
    setResultMessage(tr("Shortcut is no longer available"));
    publishState();
    return;
  }
  auto msg = QDBusMessage::createMethodCall(service, path,
                                            "org.arinn.WebShortcuts1", "Open");
  msg << id << QUuid::createUuid().toString(QUuid::WithoutBraces);
  beginOperation(id, msg);
}
void Backend::setResultMessage(const QString &text) {
  m_message = text;
  m_resultMessage = text;
  m_resultTimer.start();
}
void Backend::beginOperation(const QString &id, const QDBusMessage &msg) {
  if (m_pendingIds.contains(id))
    return;
  const auto epoch = m_connectionEpoch;
  m_pendingIds.insert(id);
  m_message = tr("Working…");
  publishState();
  auto pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, id, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_connectionEpoch)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "o" || reply.arguments().isEmpty()) {
              m_pendingIds.remove(id);
              if (auto *launch = launchRecord(id, QString()))
                failLaunch(*launch, reply.type() == QDBusMessage::ErrorMessage
                                        ? reply.errorName().section('.', -1)
                                        : QStringLiteral("Unavailable"));
              setResultMessage(reply.type() == QDBusMessage::ErrorMessage
                                   ? reply.errorMessage()
                                   : tr("Operation is unavailable"));
              publishState();
              return;
            }
            const auto operation =
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path();
            if (auto *launch = launchRecord(id, QString()))
              launch->operation = operation;
            watchOperation(id, operation, epoch);
          });
}
void Backend::watchOperation(const QString &id, const QString &operationPath,
                             quint64 epoch) {
  auto timer = new QTimer(this);
  timer->setInterval(100);
  // The launch record of this operation, if it is a launch.
  const auto shown = [this, id, operationPath] {
    return launchRecord(id, operationPath);
  };
  connect(timer, &QTimer::timeout, this,
          [this, timer, operationPath, id, epoch, shown] {
            timer->stop();
            if (epoch != m_connectionEpoch) {
              timer->deleteLater();
              return;
            }
            auto msg = QDBusMessage::createMethodCall(
                service, operationPath, "org.arinn.Operation1", "Get");
            auto p = new QDBusPendingCallWatcher(
                QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
            connect(p, &QDBusPendingCallWatcher::finished, this,
                    [this, timer, id, epoch, shown](auto *call) {
                      const auto reply = call->reply();
                      call->deleteLater();
                      if (epoch != m_connectionEpoch) {
                        timer->deleteLater();
                        return;
                      }
                      if (reply.type() == QDBusMessage::ErrorMessage ||
                          reply.signature() != "(ssbtta(sss)b)" ||
                          reply.arguments().isEmpty()) {
                        m_pendingIds.remove(id);
                        if (auto *launch = shown())
                          failLaunch(*launch, QStringLiteral("Unavailable"));
                        setResultMessage(tr("Operation is unavailable"));
                        timer->deleteLater();
                        publishState();
                        return;
                      }
                      const QDBusArgument arg =
                          qvariant_cast<QDBusArgument>(reply.arguments().at(0));
                      QString state, error;
                      bool retryable, cancellable;
                      qulonglong started, ended;
                      arg.beginStructure();
                      arg >> state >> error >> retryable >> started >> ended;
                      arg.beginArray();
                      while (!arg.atEnd()) {
                        QString provider, capability, resultId;
                        arg.beginStructure();
                        arg >> provider >> capability >> resultId;
                        arg.endStructure();
                      }
                      arg.endArray();
                      arg >> cancellable;
                      arg.endStructure();
                      if (state == "running" || state == "queued") {
                        auto *launch = shown();
                        if (launch && launch->cancellable != cancellable) {
                          launch->cancellable = cancellable;
                          publishState();
                        }
                        timer->start();
                        return;
                      }
                      m_pendingIds.remove(id);
                      bool next = false;
                      if (auto *launch = shown()) {
                        // Success and a confirmed cancel close the launch;
                        // a failure comes on screen and stays until it is
                        // dismissed, also for a launch behind another one.
                        if (state == "failed") {
                          failLaunch(*launch, error);
                        } else if (launch->id == m_shownLaunch) {
                          forgetShownLaunch();
                          next = true;
                        } else {
                          m_launches.remove(launch->id);
                        }
                      }
                      setResultMessage(state == "succeeded" ? tr("Done")
                                       : state == "cancelled"
                                           ? tr("Launch cancelled")
                                           : error);
                      timer->deleteLater();
                      if (next)
                        dismissed();
                      else
                        publishState();
                    });
          });
  timer->start();
}

void Backend::editShortcut(const QString &id) {
  if (m_shortcutSaving)
    return;
  if (!m_connected || !m_shortcutsLoaded || m_shortcutsError ||
      (!id.isEmpty() && !m_shortcutRecords.contains(id))) {
    setResultMessage(tr("Shortcuts are unavailable. Refresh and try again."));
    publishState();
    return;
  }
  ++m_editorTicket;
  m_shortcutError.clear();
  m_editorRevision = m_shortcutRevision;
  m_shortcutEditor = id.isEmpty()
      ? QVariantMap{{"id", QString()}, {"title", QString()}, {"url", QString()}, {"icon", QString()}}
      : m_shortcutRecords.value(id);
  publishState();
}
void Backend::closeShortcutEditor() {
  ++m_editorTicket;
  m_shortcutEditor.clear();
  m_shortcutError.clear();
  publishState();
}
void Backend::saveShortcut(const QString &title, const QString &url) {
  if (m_shortcutEditor.isEmpty() || m_shortcutSaving)
    return;
  // An empty name saves the host (handoff KB-3, DR-43).
  auto name = title;
  if (name.trimmed().isEmpty()) {
    const auto address = QUrl::fromUserInput(url.trimmed());
    name = address.host();
    if (address.port() != -1)
      name += ":" + QString::number(address.port());
  }
  // Preserve the draft on errors. URL policy belongs to the daemon.
  m_shortcutEditor["title"] = name;
  m_shortcutEditor["url"] = url;
  const auto id = m_shortcutEditor.value("id").toString();
  const auto icon = m_shortcutEditor.value("icon").toString();
  if (id.isEmpty())
    mutateShortcut("Add", {name, url, icon, QUuid::createUuid().toString(QUuid::WithoutBraces)});
  else
    mutateShortcut("Update", {id, QVariant::fromValue<qulonglong>(m_editorRevision), name, url, icon});
}
void Backend::removeShortcut() {
  const auto id = m_shortcutEditor.value("id").toString();
  if (!id.isEmpty())
    mutateShortcut("Remove", {id, QVariant::fromValue<qulonglong>(m_editorRevision)});
}
void Backend::mutateShortcut(const char *method, const QVariantList &args) {
  if (m_shortcutSaving || m_shortcutEditor.isEmpty() || !m_connected)
    return;
  auto msg = QDBusMessage::createMethodCall(service, path, "org.arinn.WebShortcuts1", method);
  msg.setArguments(args);
  const auto epoch = m_connectionEpoch;
  const auto ticket = m_editorTicket;
  const bool adding = QByteArray(method) == "Add";
  m_shortcutSaving = true;
  m_shortcutError.clear();
  publishState();
  auto call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      call, &QDBusPendingCallWatcher::finished, this,
      [this, epoch, ticket, adding](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (epoch != m_connectionEpoch)
          return;
        m_shortcutSaving = false;
        if (ticket != m_editorTicket) {
          readShortcuts(epoch);
          publishState();
          return;
        }
        if (reply.type() == QDBusMessage::ErrorMessage) {
          const auto code = reply.errorName().section('.', -1);
          // Never display arbitrary remote messages that may contain user URLs.
          m_shortcutError =
              code == "Conflict" ? tr("Shortcuts changed. Close and reopen the "
                                      "editor before saving.")
              : code == "InvalidArgument"
                  ? tr("Check the title and address. Use HTTPS, or HTTP for a "
                       "local service.")
              : code == "NoReply" || code == "Timeout"
                  ? tr("No reply. Check the shortcut list before trying again.")
                  : tr("The shortcut could not be saved (%1).").arg(code);
        } else if ((adding && (reply.signature() != "s" ||
                               reply.arguments().at(0).toString().isEmpty())) ||
                   (!adding && !reply.signature().isEmpty())) {
          m_shortcutError =
              tr("Invalid reply. Check the shortcut list before trying again.");
        } else {
          closeShortcutEditor();
        }
        // No mutation is retried automatically, including uncertain Add
        // replies.
        readShortcuts(epoch);
        publishState();
      });
}

void Backend::reloadFavorites() { readFavorites(); }
void Backend::readFavorites() {
  if (!m_connected || !m_daemonOwner.startsWith(':'))
    return;
  m_favoritesRequested = true;
  const auto epoch = m_connectionEpoch, ticket = ++m_favoriteTicket;
  auto msg = QDBusMessage::createMethodCall(m_daemonOwner, path,
                                            "org.arinn.Favorites1", "List");
  auto call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      call, &QDBusPendingCallWatcher::finished, this,
      [this, epoch, ticket](auto *call) {
        const auto reply = call->reply();
        call->deleteLater();
        if (epoch != m_connectionEpoch || ticket != m_favoriteTicket)
          return;
        if (m_favoriteReconcile) {
          m_favoriteSaving = false;
          m_favoriteReconcile = false;
        }
        auto fail = [this] {
          m_favoritesError = true;
          m_favoritesLoaded = false;
          m_favoritesRequested = false;
          m_favoriteError = tr("Favorites unavailable. Refresh the list before editing.");
          publishState();
        };
        if (reply.type() == QDBusMessage::ErrorMessage ||
            reply.signature() != "(ta(sss))" ||
            (!reply.service().isEmpty() && reply.service() != m_daemonOwner)) {
          fail();
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        qulonglong revision = 0;
        QVariantList targets;
        QSet<QByteArray> seen;
        qsizetype bytes = 16;
        arg.beginStructure();
        arg >> revision;
        arg.beginArray();
        while (!arg.atEnd()) {
          QString provider, capability, local;
          arg.beginStructure();
          arg >> provider >> capability >> local;
          arg.endStructure();
          auto validId = [](const QString &id) {
            return !id.isEmpty() && id.toUtf8().size() <= 256 &&
                   !id.contains(QChar(0));
          };
          const auto key =
              QJsonDocument(QJsonArray{provider, capability, local})
                  .toJson(QJsonDocument::Compact);
          // Exact D-Bus alignment and UTF-8 string footprint for (ta(sss)).
          bytes = (bytes + 7) & ~qsizetype(7);
          for (const auto &field : {provider, capability, local}) {
            bytes = (bytes + 3) & ~qsizetype(3);
            bytes += 4 + field.toUtf8().size() + 1;
          }
          if (!validId(local) || !validId(provider) ||
              !(capability == "application" ||
                (provider == "arinn" && capability == "web-shortcut")) ||
              seen.contains(key) || targets.size() >= 1024 ||
              bytes > 256 * 1024) {
            fail();
            return;
          }
          seen.insert(key);
          targets.append(QVariantMap{{"providerId", provider},
                                     {"capability", capability},
                                     {"localId", local}});
        }
        arg.endArray();
        arg.endStructure();
        if (!revision || revision < m_favoriteRevision ||
            (revision == m_favoriteRevision && targets != m_favoriteTargets)) {
          fail();
          return;
        }
        m_favoriteRevision = revision;
        m_favoriteTargets = targets;
        if (m_favoritesError) m_favoriteError.clear();
        m_favoritesLoaded = true;
        m_favoritesError = false;
        publishState();
      });
}
void Backend::setFavorite(const QString &id, bool pinned) {
  if (!favoritesReady() || m_favoriteSaving)
    return;
  const auto target = favoriteTarget(id);
  if (target.isEmpty())
    return;
  auto entries = m_favoriteTargets;
  if (pinned == isFavorite(target))
    return;
  if (pinned)
    entries.append(target);
  else
    entries.removeAll(target);
  QList<FavoriteWire> wire;
  for (const auto &value : entries) {
    const auto entry = value.toMap();
    wire.append({entry.value("providerId").toString(),
                 entry.value("capability").toString(),
                 entry.value("localId").toString()});
  }
  auto msg = QDBusMessage::createMethodCall(m_daemonOwner, path,
                                            "org.arinn.Favorites1", "Set");
  msg << QVariant::fromValue<qulonglong>(m_favoriteRevision)
      << QVariant::fromValue(wire);
  const auto epoch = m_connectionEpoch;
  m_favoriteSaving = true;
  m_favoriteError.clear();
  publishState();
  auto call = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(call, &QDBusPendingCallWatcher::finished, this,
          [this, epoch](auto *call) {
            const auto reply = call->reply();
            call->deleteLater();
            if (epoch != m_connectionEpoch)
              return;
            m_favoriteReconcile = true;
            const auto code = reply.errorName().section('.', -1);
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "t") {
              m_favoriteError =
                  code == "Conflict"
                      ? tr("Favorites changed. Refresh before trying again.")
                      : tr("Favorites could not be saved. Refresh the list "
                           "before trying again.");
            } else
              setResultMessage(tr("Favorites saved"));
            // Reconcile after every outcome; never retry a write or change
            // selection optimistically.
            readFavorites();
            publishState();
          });
}
void Backend::terminate(const QString &id) {
  runApplication(id, m_items, "Terminate");
}
