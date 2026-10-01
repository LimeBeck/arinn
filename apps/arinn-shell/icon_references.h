// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QCryptographicHash>
#include <QHash>
#include <QMutex>
#include <QMutexLocker>
#include <QRegularExpression>
#include <QString>

// Private, bounded indirection: local paths never enter QML or image errors.
class IconReferences {
public:
  static bool valid(const QString &value) {
    if (value.isEmpty() || value.toUtf8().size() > 1024) return false;
    for (const auto c : value)
      if (c.category() == QChar::Other_Control) return false;
    if (value.startsWith('/')) {
      const auto parts = value.mid(1).split('/');
      for (const auto &part : parts)
        if (part.isEmpty() || part == "." || part == "..") return false;
      return true;
    }
    static const QRegularExpression name("^[A-Za-z0-9._-]*[A-Za-z0-9][A-Za-z0-9._-]*$");
    return name.match(value).hasMatch();
  }
  static QString imageUrl(const QString &reference) {
    if (!valid(reference)) return {};
    auto &s = state(); QMutexLocker lock(&s.mutex);
    const auto token = QString::fromLatin1(QCryptographicHash::hash(reference.toUtf8(), QCryptographicHash::Sha256).toHex());
    if (!s.references.contains(token) && s.references.size() >= 4096) return {};
    s.references.insert(token, reference);
    return "image://arinn-icons/" + token;
  }
  static QString lookup(const QString &token) {
    auto &s = state(); QMutexLocker lock(&s.mutex);
    return s.references.value(token);
  }
private:
  struct State { QMutex mutex; QHash<QString, QString> references; };
  static State &state() { static State s; return s; }
};
