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
void registerBrowse() {
  qmlRegisterType<BrowseCatalog>("Arinn.Shell", 1, 0, "BrowseCatalog");
}
Q_COREAPP_STARTUP_FUNCTION(registerBrowse)
} // namespace
BrowseCatalog::BrowseCatalog(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  bus.connect(service, path, "org.arinn.Browse1", "RecentChanged", this,
              SLOT(refreshRecent()));
  bus.connect(service, path, "org.arinn.WebShortcuts1", "Changed", this,
              SLOT(refreshShortcuts()));
  auto *owner = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  connect(owner, &QDBusServiceWatcher::serviceOwnerChanged, this, [this] {
    reset();
    ++m_recentRead;
    ++m_shortcutRead;
    m_recent.clear();
    m_shortcuts.clear();
    m_tabs.clear();
    emit changed();
    refreshRecent();
    refreshShortcuts();
  });
  refreshRecent();
  refreshShortcuts();
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
QVariantList BrowseCatalog::suggestions() const {
  QVariantList result;
  const auto query = m_text.trimmed().toCaseFolded();
  for (const auto &row : m_shortcuts) {
    const auto shortcut = row.toMap();
    const auto title = shortcut.value("title").toString().toCaseFolded();
    const auto address = shortcut.value("url").toString().toCaseFolded();
    auto words =
        (title + " " + address)
            .split(QRegularExpression("[^\\p{L}\\p{N}]+"), Qt::SkipEmptyParts);
    words << title << address << address.section("://", 1);
    if (!query.isEmpty() && std::none_of(words.cbegin(), words.cend(),
                                         [&query](const QString &word) {
                                           return word.startsWith(query);
                                         }))
      continue;
    bool opened = false;
    const auto wanted = origin(shortcut.value("url").toString());
    for (const auto &tabRow : m_tabs) {
      const auto tab = tabRow.toMap();
      if (!wanted.isEmpty() && tab.value("active").toBool() &&
          origin(tab.value("url").toString()) == wanted)
        opened = true;
    }
    result.append(QVariantMap{
        {"id", shortcut.value("id")},
        {"title", shortcut.value("title")},
        {"open", opened},
        {"subtitle", QUrl(shortcut.value("url").toString()).host()}});
    if (result.size() == 3)
      break;
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
void BrowseCatalog::refreshShortcuts() {
  const auto ticket = ++m_shortcutRead;
  auto *pending = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(
          call("List", "org.arinn.WebShortcuts1"), 3000),
      this);
  connect(
      pending, &QDBusPendingCallWatcher::finished, this,
      [this, ticket](auto *watcher) {
        const auto reply = watcher->reply();
        watcher->deleteLater();
        if (ticket != m_shortcutRead)
          return;
        if (reply.signature() != "(ta(ssss))" ||
            reply.type() == QDBusMessage::ErrorMessage) {
          m_shortcuts.clear();
          emit changed();
          return;
        }
        const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
        qulonglong revision;
        QVariantList rows;
        QString id, title, url, icon;
        arg.beginStructure();
        arg >> revision;
        arg.beginArray();
        while (!arg.atEnd()) {
          arg.beginStructure();
          arg >> id >> title >> url >> icon;
          arg.endStructure();
          rows.append(QVariantMap{{"id", id}, {"title", title}, {"url", url}});
        }
        arg.endArray();
        arg.endStructure();
        m_shortcuts = rows;
        emit changed();
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
