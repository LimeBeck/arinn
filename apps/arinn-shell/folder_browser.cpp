// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "folder_browser.h"
#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QtQml>
#include <algorithm>

namespace {
const auto kService = QStringLiteral("org.arinn.Daemon1");
const auto kPath = QStringLiteral("/org/arinn/Daemon1");
const auto kInterface = QStringLiteral("org.arinn.Folders1");
// The daemon's own limit on a listing is 3 s (ADR-0071).
constexpr int kCallMs = 5000;
constexpr quint32 kPage = 256;
QString lastSegment(const QString &path) {
  if (path == QLatin1String("/"))
    return path;
  return path.section(QLatin1Char('/'), -1, -1, QString::SectionSkipEmpty);
}
QString stateFor(const QString &errorName) {
  const auto code = errorName.section(QLatin1Char('.'), -1);
  if (code == QLatin1String("PermissionDenied"))
    return QStringLiteral("denied");
  if (code == QLatin1String("NotFound"))
    return QStringLiteral("gone");
  return QStringLiteral("unavailable");
}
// Case-insensitive, with digit runs compared by value ("series 9" before
// "series 10"), independent of the locale's collation (PK-3).
bool naturalLess(const QString &a, const QString &b) {
  qsizetype i = 0, j = 0;
  while (i < a.size() && j < b.size()) {
    if (a.at(i).isDigit() && b.at(j).isDigit()) {
      qsizetype ei = i, ej = j;
      while (ei < a.size() && a.at(ei).isDigit()) ++ei;
      while (ej < b.size() && b.at(ej).isDigit()) ++ej;
      auto na = a.mid(i, ei - i), nb = b.mid(j, ej - j);
      while (na.size() > 1 && na.startsWith(QLatin1Char('0'))) na.remove(0, 1);
      while (nb.size() > 1 && nb.startsWith(QLatin1Char('0'))) nb.remove(0, 1);
      if (na.size() != nb.size())
        return na.size() < nb.size();
      if (const auto c = na.compare(nb); c != 0)
        return c < 0;
      i = ei;
      j = ej;
      continue;
    }
    const auto ca = a.at(i).toCaseFolded(), cb = b.at(j).toCaseFolded();
    if (ca != cb)
      return ca < cb;
    ++i;
    ++j;
  }
  if ((a.size() - i) != (b.size() - j))
    return (a.size() - i) < (b.size() - j);
  return a < b;
}
void registerFolderBrowser() {
  qmlRegisterType<FolderBrowser>("Arinn.Shell", 1, 0, "FolderBrowser");
}
Q_COREAPP_STARTUP_FUNCTION(registerFolderBrowser)
} // namespace

FolderBrowser::FolderBrowser(QObject *parent) : QObject(parent) {}

QString FolderBrowser::child(const QString &parent, const QString &name) {
  if (parent.isEmpty())
    return {};
  return parent.endsWith(QLatin1Char('/')) ? parent + name
                                           : parent + QLatin1Char('/') + name;
}

QString FolderBrowser::parentOf(const QString &path) {
  if (path.isEmpty() || path == QLatin1String("/"))
    return {};
  auto trimmed = path;
  while (trimmed.size() > 1 && trimmed.endsWith(QLatin1Char('/')))
    trimmed.chop(1);
  const auto at = trimmed.lastIndexOf(QLatin1Char('/'));
  return at <= 0 ? QStringLiteral("/") : trimmed.left(at);
}

void FolderBrowser::refreshPlaces() {
  const auto request = ++m_placesRequest;
  // Before the first folder opens, the picker waits for the places.
  if (m_path.isEmpty() && m_state != QLatin1String("loading")) {
    m_state = QStringLiteral("loading");
    emit listingChanged();
  }
  auto call = QDBusMessage::createMethodCall(kService, kPath, kInterface,
                                             QStringLiteral("Places"));
  auto *watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, kCallMs), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, request](QDBusPendingCallWatcher *watcher) {
            watcher->deleteLater();
            if (request != m_placesRequest)
              return;
            const auto reply = watcher->reply();
            QVariantList places;
            bool read = false;
            if (reply.type() == QDBusMessage::ReplyMessage &&
                reply.signature() == QLatin1String("a(ss)")) {
              const auto argument =
                  qvariant_cast<QDBusArgument>(reply.arguments().constFirst());
              argument.beginArray();
              while (!argument.atEnd()) {
                QString kind, path;
                argument.beginStructure();
                argument >> kind >> path;
                argument.endStructure();
                if (!path.startsWith(QLatin1Char('/')))
                  continue;
                places.append(QVariantMap{{QStringLiteral("kind"), kind},
                                          {QStringLiteral("path"), path},
                                          {QStringLiteral("title"), lastSegment(path)}});
              }
              argument.endArray();
              read = true;
            }
            // A failed read, or no places, before the first folder opens
            // leaves the picker nothing to show: unavailable, so it offers
            // Retry (review of #780). A failed refresh keeps the places
            // read before.
            if (m_path.isEmpty() && (!read || places.isEmpty())) {
              m_state = QStringLiteral("unavailable");
              emit listingChanged();
            }
            if (!read || places == m_places)
              return;
            m_places = places;
            emit placesChanged();
          });
}

void FolderBrowser::open(const QString &path) {
  if (!path.startsWith(QLatin1Char('/')))
    return;
  const auto request = ++m_request;
  m_path = path;
  m_folders.clear();
  m_truncated = false;
  m_state = QStringLiteral("loading");
  emit listingChanged();
  readPage(request, path, {}, {});
}

void FolderBrowser::up() {
  const auto parent = parentOf(m_path);
  if (!parent.isEmpty())
    open(parent);
}

void FolderBrowser::readPage(quint64 request, const QString &path,
                             const QString &after, QStringList names) {
  auto call = QDBusMessage::createMethodCall(kService, kPath, kInterface,
                                             QStringLiteral("List"));
  call << path << after << kPage;
  auto *watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, kCallMs), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, request, path, names](QDBusPendingCallWatcher *watcher) mutable {
            watcher->deleteLater();
            if (request != m_request)
              return;
            const auto reply = watcher->reply();
            if (reply.type() == QDBusMessage::ErrorMessage) {
              finish(request, stateFor(reply.errorName()), {}, false);
              return;
            }
            if (reply.signature() != QLatin1String("asb")) {
              finish(request, QStringLiteral("unavailable"), {}, false);
              return;
            }
            const auto page = reply.arguments().at(0).toStringList();
            const auto more = reply.arguments().at(1).toBool();
            names.append(page);
            if (more && !page.isEmpty() && names.size() < kMaxFolders) {
              readPage(request, path, page.constLast(), names);
              return;
            }
            finish(request, names.isEmpty() ? QStringLiteral("empty")
                                             : QStringLiteral("ready"),
                   names, more);
          });
}

void FolderBrowser::finish(quint64 request, const QString &state,
                           QStringList names, bool truncated) {
  if (request != m_request)
    return;
  std::sort(names.begin(), names.end(), [](const QString &a, const QString &b) {
    return naturalLess(a, b);
  });
  if (names.size() > kMaxFolders)
    names = names.mid(0, kMaxFolders);
  m_folders = names;
  m_truncated = truncated;
  m_state = state;
  emit listingChanged();
}
