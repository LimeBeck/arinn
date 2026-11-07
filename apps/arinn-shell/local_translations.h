// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QDir>
#include <QCryptographicHash>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QHash>
#include <QLocale>
#include <QRegularExpression>
#include <QStandardPaths>
#include <QtEndian>
#include <algorithm>

// Data-only local overlays, shared by SettingsCatalog and the main loader.
// No provider callbacks or application-specific ids are involved.
namespace LocalTranslations {
inline constexpr qint64 maxCatalogBytes = 1024 * 1024;
inline constexpr qint64 maxSettingsBytes = 256 * 1024;
// Qt may follow a QM dependency block while loading. Local resources must
// be self-contained; bounds on the outer file alone would not bound those reads.
inline bool selfContainedQm(const QByteArray &bytes) {
  static const auto magic = QByteArray::fromHex("3cb86418caef9c95cd211cbf60a1bddd");
  if (!bytes.startsWith(magic)) return false;
  qsizetype offset = magic.size();
  while (offset < bytes.size()) {
    if (bytes.size() - offset < 5) return false;
    const auto tag = static_cast<uchar>(bytes.at(offset));
    const auto length = qFromBigEndian<quint32>(bytes.constData() + offset + 1);
    offset += 5;
    if (tag == 0x96 || length > static_cast<quint64>(bytes.size() - offset)) return false;
    offset += length;
  }
  return true;
}
inline bool validLocale(const QString &tag) {
  static const QRegularExpression pattern(
      QStringLiteral("^[a-z]{2,3}(?:-[A-Z][a-z]{3})?(?:-(?:[A-Z]{2}|[0-9]{3}))?$"));
  return pattern.match(tag).hasMatch();
}
inline QStringList roots() {
  QStringList result;
  const auto locations = QStandardPaths::standardLocations(QStandardPaths::GenericDataLocation);
  for (const auto &location : locations.mid(0, 16)) {
    const auto path = QDir(location).filePath(QStringLiteral("arinn/translations"));
    if (!result.contains(path)) result.append(path);
  }
  return result;
}
inline QString currentLocaleTag() {
  const QLocale locale;
  const auto full = locale.name().split(QLatin1Char('_'));
  const auto minimal = locale.bcp47Name().split(QLatin1Char('-'));
  if (minimal.size() > 1 && minimal.at(1).size() == 4 && full.size() == 2)
    return full.first() + QLatin1Char('-') + minimal.at(1) + QLatin1Char('-') + full.last();
  return locale.name().replace(QLatin1Char('_'), QLatin1Char('-'));
}
inline QString pluginDirectory(const QString &pluginId) {
  const auto bytes = pluginId.toUtf8();
  if (bytes.isEmpty() || bytes.size() > 256 || bytes.contains('\0')) return {};
  static const QRegularExpression portable(QStringLiteral("^[A-Za-z0-9_.-]{1,128}$"));
  if (pluginId != QLatin1String(".") && pluginId != QLatin1String("..") && portable.match(pluginId).hasMatch())
    return pluginId;
  return QStringLiteral("by-id/%1").arg(QString::fromLatin1(
      QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex()));
}
inline QStringList locales(const QString &tag) {
  QStringList result{tag};
  const auto parts = tag.split(QLatin1Char('-'));
  const bool script = parts.size() > 1 && parts.at(1).size() == 4;
  if (script && QLocale::scriptToCode(QLocale(tag).script()) != parts.at(1)) {
    if (parts.size() == 3) result.append(parts.mid(0, 2).join(QLatin1Char('-')));
    return result;
  }
  if (script && parts.size() == 3) {
    const auto regional = parts.first() + QLatin1Char('-') + parts.last();
    if (QLocale(regional).script() == QLocale(tag).script()) result.append(regional);
  }
  const auto expanded = QLocale(tag).uiLanguages();
  for (const auto &candidate : expanded) {
    const auto segments = candidate.split(QLatin1Char('-'));
    if (segments.size() > 1 && segments.at(1).size() == 4 &&
        segments.first() == parts.first() &&
        QLocale(candidate).script() == QLocale(tag).script()) {
      // Only the requested territory and its script parent may be used.
      if (segments.size() == 2 || (parts.size() > 1 && segments.last() == parts.last()))
        result.append(candidate);
      if (segments.size() == 3) result.append(segments.mid(0, 2).join(QLatin1Char('-')));
    }
  }
  if (script && parts.size() == 3) result.append(parts.mid(0, 2).join(QLatin1Char('-')));
  // A default-script parent is safe; sr-Latn must not select sr-Cyrl.
  if (parts.size() > 1 && QLocale(parts.first()).script() == QLocale(tag).script())
    result.append(parts.first());
  result.removeDuplicates();
  return result;
}
inline bool safeFile(const QString &root, const QString &file, qint64 maximum) {
  const QFileInfo base(root), entry(file);
  const auto canonicalRoot = base.canonicalFilePath();
  const auto canonicalFile = entry.canonicalFilePath();
  return !canonicalRoot.isEmpty() && entry.isFile() && entry.isReadable() &&
         entry.size() > 0 && entry.size() <= maximum &&
         canonicalFile.startsWith(canonicalRoot + QLatin1Char('/'));
}
inline QJsonObject readObject(const QString &root, const QString &path, qint64 maximum) {
  if (!safeFile(root, path, maximum)) return {};
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) return {};
  const auto bytes = file.read(maximum + 1);
  if (bytes.size() > maximum) return {};
  QJsonParseError error;
  const auto document = QJsonDocument::fromJson(bytes, &error);
  return error.error == QJsonParseError::NoError && document.isObject()
             ? document.object() : QJsonObject{};
}
inline QStringList catalogPaths(const QStringList &directories,
                                const QString &domain, const QString &tag) {
  QStringList result;
  if (!validLocale(tag) || (domain != QLatin1String("arinn") && domain != QLatin1String("music")))
    return result;
  // Source priority precedes locale specificity: a personal ru overlay
  // deliberately wins over a lower-priority ru-RU catalogue.
  for (const auto &root : directories) {
    for (const auto &locale : locales(tag)) {
      const auto structured = QDir(root).filePath(
          QStringLiteral("overrides/%1/shell/%2.qm").arg(locale, domain));
      if (safeFile(root, structured, maxCatalogBytes)) result.append(structured);
      const auto flat = QDir(root).filePath(
          QStringLiteral("%1_%2.qm").arg(domain, QString(locale).replace(QLatin1Char('-'), QLatin1Char('_'))));
      if (safeFile(root, flat, maxCatalogBytes)) result.append(flat);
    }
  }
  return result;
}
inline QString nativeName(const QStringList &directories, const QString &tag) {
  for (const auto &root : directories) {
    const auto object = readObject(root, QDir(root).filePath(
        QStringLiteral("overrides/%1/language.json").arg(tag)), 4096);
    const auto name = object.value(QStringLiteral("nativeName")).toString();
    if (object.value(QStringLiteral("formatVersion")).toInt() == 1 &&
        object.value(QStringLiteral("locale")).toString() == tag &&
        !name.isEmpty() && name.toUtf8().size() <= 128 &&
        std::none_of(name.begin(), name.end(), [](QChar c) { return c.category() == QChar::Other_Control; }))
      return name;
  }
  if (tag == QLatin1String("en")) return QStringLiteral("English");
  if (tag == QLatin1String("ru")) return QStringLiteral("Русский");
  const QLocale locale(tag);
  if (locale.language() == QLocale::C || locale.nativeLanguageName().isEmpty()) return tag;
  auto name = locale.nativeLanguageName();
  const auto parts = tag.split(QLatin1Char('-'));
  if (parts.size() > 1 && parts.last().size() != 4 && !locale.nativeTerritoryName().isEmpty())
    name += QStringLiteral(" (%1)").arg(locale.nativeTerritoryName());
  return name;
}

// The local envelope is not a wire table. A field applies only if its
// expected English source still matches the provider's current schema.
inline bool settingText(const QStringList &directories, const QString &tag,
                        const QString &pluginId, const QString &setting,
                        const QString &field, const QString &source,
                        const QString &option, QString *translated,
                        QHash<QString, QJsonObject> *parsedFiles = nullptr) {
  const auto pluginPath = pluginDirectory(pluginId);
  if (!validLocale(tag) || pluginPath.isEmpty()) return false;
  for (const auto &root : directories) {
    for (const auto &locale : locales(tag)) {
      const auto path = QDir(root).filePath(
          QStringLiteral("overrides/%1/providers/%2/settings.json").arg(locale, pluginPath));
      QJsonObject object;
      if (parsedFiles && parsedFiles->contains(path)) object = parsedFiles->value(path);
      else {
        object = readObject(root, path, maxSettingsBytes);
        if (parsedFiles) parsedFiles->insert(path, object);
      }
      if (object.value(QStringLiteral("formatVersion")).toInt() != 1 ||
          object.value(QStringLiteral("pluginId")).toString() != pluginId ||
          object.value(QStringLiteral("locale")).toString() != locale) continue;
      const auto entry = object.value(QStringLiteral("settings")).toObject().value(setting).toObject();
      const auto value = option.isEmpty() ? entry.value(field).toObject()
          : entry.value(QStringLiteral("options")).toObject().value(option).toObject();
      if (!value.value(QStringLiteral("source")).isString() ||
          value.value(QStringLiteral("source")).toString() != source ||
          !value.value(QStringLiteral("translation")).isString()) continue;
      const auto text = value.value(QStringLiteral("translation")).toString();
      if (text.isEmpty() && field != QLatin1String("description")) continue;
      if (text.toUtf8().size() > 8192 || std::any_of(text.begin(), text.end(), [](QChar c) {
            return c.category() == QChar::Other_Control && c != QLatin1Char('\n') && c != QLatin1Char('\t');
          })) continue;
      *translated = text;
      return true;
    }
  }
  return false;
}
} // namespace LocalTranslations
