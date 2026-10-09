// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "browse_catalog.h"
#include <QRegularExpression>
#include <QTimer>
#include <QUrl>
#include <QUuid>
#include <QtDBus>
#include <QtQml>
#include <algorithm>
namespace {
constexpr auto service = "org.arinn.Daemon1";
constexpr auto path = "/org/arinn/Daemon1";
QDBusMessage call(const char *method,
                  const char *interface = "org.arinn.Browse1") {
  return QDBusMessage::createMethodCall(service, path, interface, method);
}
QString error(const QDBusMessage &reply) {
  return reply.type() == QDBusMessage::ErrorMessage
             ? reply.errorName().section('.', -1)
             : QStringLiteral("Unavailable");
}
QString origin(const QString &text) {
  const QUrl url(text);
  if (!url.isValid() || url.host().isEmpty() ||
      (url.scheme() != "http" && url.scheme() != "https"))
    return {};
  QString host = url.host().toLower();
  if (host.startsWith("www."))
    host.remove(0, 4);
  return url.scheme() + "|" + host + "|" +
         QString::number(url.port(url.scheme() == "https" ? 443 : 80));
}
// Word prefixes, as on the address and search screen: "med" finds
// "Media Library" and "https://media.example".
bool matchesQuery(const QString &query, const QStringList &texts) {
  if (query.isEmpty())
    return true;
  for (const auto &text : texts) {
    const auto folded = text.toCaseFolded();
    auto words = folded.split(QRegularExpression("[^\\p{L}\\p{N}]+"),
                              Qt::SkipEmptyParts);
    words << folded;
    if (std::any_of(
            words.cbegin(), words.cend(),
            [&query](const QString &word) { return word.startsWith(query); }))
      return true;
  }
  return false;
}
// Host with an explicit port, without "www.", for display and matching.
QString hostOf(const QUrl &url) {
  auto host = url.host();
  if (host.startsWith("www."))
    host.remove(0, 4);
  if (url.port() != -1)
    host += ":" + QString::number(url.port());
  return host;
}
void registerBrowse() {
  qmlRegisterType<BrowseCatalog>("Arinn.Shell", 1, 0, "BrowseCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerBrowse)
} // namespace
BrowseCatalog::BrowseCatalog(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, "org.arinn.Browse1", "RecentChanged", this,
              SLOT(refreshRecent()));
  auto *owner = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(owner, &QDBusServiceWatcher::serviceOwnerChanged, this, [this] {
    reset();
    ++m_recentRead;
    m_recent.clear();
    m_tabs.clear();
    emit changed();
    refreshRecent();
  });
  refreshRecent();
}
void BrowseCatalog::reset() {
  ++m_resolve;
  ++m_command;
  m_text.clear();
  m_kind.clear();
  m_url.clear();
  m_engine.clear();
  m_operation.clear();
  m_busy = false;
  emit changed();
}
void BrowseCatalog::setTabs(const QVariantList &tabs) {
  m_tabs = tabs;
  emit changed();
}
QString BrowseCatalog::openTitle() const {
  const auto wanted = origin(m_url);
  if (wanted.isEmpty())
    return {};
  for (const auto &row : m_tabs) {
    const auto tab = row.toMap();
    if (tab.value("active").toBool() &&
        origin(tab.value("url").toString()) == wanted)
      return tab.value("title").toString().isEmpty()
                 ? QUrl(m_url).host()
                 : tab.value("title").toString();
  }
  return {};
}
// Recent entries, then the open services, that match the text: up to four
// for the on-screen keyboard's suggestion row (spec §7.1, ADR-0103). Web
// shortcuts are not suggested; their tiles are on Home.
QVariantList BrowseCatalog::suggestions() const {
  QVariantList result;
  const auto query = m_text.trimmed().toCaseFolded();
  for (const auto &entry : m_recent) {
    if (result.size() == 4)
      return result;
    if (entry.trimmed().toCaseFolded() == query ||
        !matchesQuery(query, {entry}))
      continue;
    result.append(QVariantMap{{"kind", "recent"},
                              {"icon", "history"},
                              {"title", entry},
                              {"subtitle", QString()},
                              {"value", entry},
                              {"open", false}});
  }
  for (const auto &service : openServices(m_text)) {
    if (result.size() == 4)
      break;
    result.append(service);
  }
  return result;
}
QVariantList BrowseCatalog::openServices(const QString &text) const {
  QVariantList result;
  const auto query = text.trimmed().toCaseFolded();
  QStringList origins;
  for (const auto &row : m_tabs) {
    if (result.size() == 4)
      break;
    const auto tab = row.toMap();
    const QUrl url(tab.value("url").toString());
    const auto key = origin(url.toString());
    if (!tab.value("active").toBool() || key.isEmpty() || origins.contains(key))
      continue;
    origins << key;
    const auto host = hostOf(url);
    const auto title = tab.value("title").toString();
    // The origin with its scheme: Browse1 and the web shortcut rules take
    // it as typed.
    const auto value = url.adjusted(QUrl::RemovePath | QUrl::RemoveQuery |
                                    QUrl::RemoveFragment | QUrl::RemoveUserInfo)
                           .toString();
    if (value.toCaseFolded() == query || !matchesQuery(query, {title, host}))
      continue;
    result.append(QVariantMap{{"kind", "open"},
                              {"icon", "public"},
                              {"title", title.isEmpty() ? host : title},
                              {"subtitle", host},
                              {"value", value},
                              {"open", true}});
  }
  return result;
}
QVariantList BrowseCatalog::nameSuggestions(const QString &address,
                                            const QString &text) const {
  const auto url = QUrl::fromUserInput(address.trimmed());
  const auto host = hostOf(url);
  if (address.trimmed().isEmpty() || url.host().isEmpty())
    return {};
  QStringList names;
  for (const auto &row : m_tabs) {
    const auto tab = row.toMap();
    const QUrl open(tab.value("url").toString());
    const auto title = tab.value("title").toString().trimmed();
    if (tab.value("active").toBool() && !title.isEmpty() &&
        hostOf(open) == host) {
      names << title;
      break;
    }
  }
  QVariantList result;
  const auto query = text.trimmed().toCaseFolded();
  const QStringList kinds{"title", "domain"};
  names << host;
  for (int i = 0; i < names.size(); ++i) {
    const auto folded = names[i].toCaseFolded();
    const auto kind = i + 1 == names.size() ? kinds[1] : kinds[0];
    if (folded == query || !folded.startsWith(query))
      continue;
    result.append(QVariantMap{
        {"kind", kind},
        {"icon", kind == "domain" ? "public" : QString()},
        {"title", names[i]},
        {"subtitle", kind == "domain" ? tr("domain") : tr("page title")},
        {"value", names[i]},
        {"open", false}});
  }
  return result;
}
void BrowseCatalog::setText(const QString &text) {
  if (text == m_text)
    return;
  m_text = text;
  m_kind.clear();
  m_url.clear();
  m_engine.clear();
  if (!m_busy)
    m_operation.clear();
  const auto ticket = ++m_resolve;
  emit changed();
  if (text.isEmpty())
    return;
  auto msg = call("Resolve");
  msg << text;
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, ticket](auto *watcher) {
        const auto reply = watcher->reply();
        watcher->deleteLater();
        if (ticket != m_resolve)
          return;
        if (reply.signature() != "(sss)" ||
            reply.type() == QDBusMessage::ErrorMessage) {
          m_operation = error(reply);
          emit changed();
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        arg.beginStructure();
        arg >> m_kind >> m_url >> m_engine;
        arg.endStructure();
        if (m_kind != "address" && m_kind != "search" && m_kind != "invalid") {
          m_kind = "invalid";
          m_url.clear();
        }
        emit changed();
      });
}
void BrowseCatalog::refreshRecent() {
  const auto ticket = ++m_recentRead;
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call("GetRecent"), 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, ticket](auto *watcher) {
            const auto reply = watcher->reply();
            watcher->deleteLater();
            if (ticket != m_recentRead)
              return;
            if (reply.signature() == "as" &&
                reply.type() != QDBusMessage::ErrorMessage) {
              const auto entries =
                  qdbus_cast<QStringList>(reply.arguments().at(0));
              if (entries.size() <= 5)
                m_recent = entries;
              emit changed();
            }
          });
}
void BrowseCatalog::dismissError() {
  if (m_busy)
    return;
  m_operation.clear();
  emit changed();
}
void BrowseCatalog::clearRecent() {
  const auto ticket = m_command;
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call("ClearRecent"), 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, ticket](auto *watcher) {
            const auto reply = watcher->reply();
            watcher->deleteLater();
            if (ticket != m_command)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage)
              m_operation = error(reply);
            else
              refreshRecent();
            emit changed();
          });
}
void BrowseCatalog::open() {
  if (m_busy || (m_kind != "address" && m_kind != "search"))
    return;
  m_busy = true;
  m_operation = "running";
  const auto ticket = ++m_command;
  QTimer::singleShot(30000, this, [this, ticket] {
    if (ticket != m_command || !m_busy)
      return;
    ++m_command;
    m_busy = false;
    m_operation = "Timeout";
    emit changed();
  });
  emit changed();
  auto msg = call("Open");
  msg << m_text << QUuid::createUuid().toString(QUuid::WithoutBraces);
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, ticket](auto *watcher) {
            const auto reply = watcher->reply();
            watcher->deleteLater();
            if (ticket != m_command)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "o") {
              m_busy = false;
              m_operation = error(reply);
              emit changed();
              return;
            }
            watchOperation(
                qvariant_cast<QDBusObjectPath>(reply.arguments().at(0)).path(),
                ticket);
          });
}
void BrowseCatalog::watchOperation(const QString &objectPath, quint64 ticket,
                                   int polls) {
  if (ticket != m_command)
    return;
  if (polls >= 150) {
    m_busy = false;
    m_operation = "Timeout";
    emit changed();
    return;
  }
  auto msg = QDBusMessage::createMethodCall(service, objectPath,
                                            "org.arinn.Operation1", "Get");
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(msg, 3000), this);
  connect(pending, &QDBusPendingCallWatcher::finished, this,
          [this, objectPath, ticket, polls](auto *watcher) {
            const auto reply = watcher->reply();
            watcher->deleteLater();
            if (ticket != m_command)
              return;
            if (reply.type() == QDBusMessage::ErrorMessage ||
                reply.signature() != "(ssbtta(sss)b)") {
              m_busy = false;
              m_operation = error(reply);
              emit changed();
              return;
            }
            const auto arg =
                qvariant_cast<QDBusArgument>(reply.arguments().at(0));
            QString state, code;
            bool retryable;
            qulonglong started, ended;
            arg.beginStructure();
            arg >> state >> code >> retryable >> started >> ended;
            arg.beginArray();
            while (!arg.atEnd()) {
              QString provider, capability, entity;
              arg.beginStructure();
              arg >> provider >> capability >> entity;
              arg.endStructure();
            }
            arg.endArray();
            bool cancellable;
            arg >> cancellable;
            arg.endStructure();
            if (state == "queued" || state == "running") {
              QTimer::singleShot(200, this, [this, objectPath, ticket, polls] {
                watchOperation(objectPath, ticket, polls + 1);
              });
              return;
            }
            m_busy = false;
            m_operation =
                state == "succeeded"
                    ? state
                    : (code.isEmpty() ? QStringLiteral("Unavailable") : code);
            refreshRecent();
            emit changed();
          });
}
