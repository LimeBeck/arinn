// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QHash>
#include <QMutex>
#include <QMutexLocker>
#include <QString>
#include <QUuid>

// Only opaque tokens leave C++. This bounded table is invalidated before a
// changed list is read, so delayed images cannot install under a new context.
class WebIconReferences {
public:
  struct Request { QString owner, epoch, id; quint64 revision = 0; };
  static void clear() { QMutexLocker lock(&mutex()); refs().clear(); }
  static QString add(const QString &owner, const QString &epoch,
                     quint64 revision, const QString &id, const QString &explicitIcon) {
    // No site artwork is packaged today. The packaged allowlist is empty;
    // unknown explicit references must never become paths or network URLs.
    if (!explicitIcon.isEmpty() || !owner.startsWith(':') ||
        QUuid(epoch).isNull() || revision == 0 || id.isEmpty()) return {};
    QMutexLocker lock(&mutex());
    for (auto it = refs().cbegin(); it != refs().cend(); ++it) {
      const auto &r = it.value();
      if (r.owner == owner && r.epoch == epoch && r.revision == revision && r.id == id)
        return "image://arinn-web-icons/" + it.key();
    }
    if (refs().size() >= 64) return {};
    const auto token = QUuid::createUuid().toString(QUuid::WithoutBraces);
    refs().insert(token, {owner, epoch, id, revision});
    return "image://arinn-web-icons/" + token;
  }
  static Request lookup(const QString &token) { QMutexLocker lock(&mutex()); return refs().value(token); }
  static bool current(const QString &token) { QMutexLocker lock(&mutex()); return refs().contains(token); }
private:
  static QMutex &mutex() { static QMutex value; return value; }
  static QHash<QString,Request> &refs() { static QHash<QString,Request> value; return value; }
};
