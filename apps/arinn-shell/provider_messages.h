// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include "local_translations.h"
#include <QJsonArray>
#include <QMap>
#include <QSet>
#include <QTranslator>
#include <cmath>
#include <limits>
#include <memory>
#include <optional>

// Client-only fixed text. JSON comes through the daemon's validated Texts1
// boundary (including duplicate-key rejection); local QMs never join the
// application's translator stack. Keep snapshot plaintext for every failure.
namespace ProviderMessages {
inline constexpr qsizetype maxJsonBytes = 240 * 1024;
inline constexpr qsizetype maxQmBytes = 240 * 1024;
inline constexpr qsizetype maxTextBytes = 8192;

inline bool text(const QString &value) {
  return value.toUtf8().size() <= maxTextBytes &&
         std::none_of(value.begin(), value.end(), [](QChar c) {
           return c.category() == QChar::Other_Control &&
                  c != QLatin1Char('\n') && c != QLatin1Char('\t');
         });
}
inline bool token(const QString &value) {
  static const QRegularExpression pattern(QStringLiteral("^[A-Za-z0-9_.-]{1,128}$"));
  return pattern.match(value).hasMatch();
}
inline bool keys(const QJsonObject &object, const QStringList &allowed) {
  for (auto it = object.begin(); it != object.end(); ++it)
    if (!allowed.contains(it.key())) return false;
  return true;
}
inline std::optional<qint64> integer(const QJsonValue &value) {
  if (!value.isDouble()) return {};
  // QJson preserves signed 64-bit integers. Avoid converting their limits
  // through double, which would round INT64_MAX up to 2^63.
  const auto result = value.toInteger(std::numeric_limits<qint64>::min());
  if (value != QJsonValue(result)) return {};
  return result;
}
struct Placeholder {
  qsizetype offset = 0, length = 0;
  int position = 0; // zero is numerus
  bool localized = false;
};
inline std::optional<QList<Placeholder>> placeholders(const QString &pattern) {
  QList<Placeholder> result;
  for (qsizetype i = 0; i < pattern.size(); ++i) {
    if (pattern.at(i) != QLatin1Char('%')) continue;
    const auto start = i++;
    const bool localized = i < pattern.size() && pattern.at(i) == QLatin1Char('L');
    if (localized) ++i;
    if (i < pattern.size() && pattern.at(i) == QLatin1Char('n')) {
      result.append({start, i - start + 1, 0, localized});
    } else if (i < pattern.size() && pattern.at(i) >= QLatin1Char('0') && pattern.at(i) <= QLatin1Char('9')) {
      const auto first = i;
      while (i < pattern.size() && pattern.at(i) >= QLatin1Char('0') && pattern.at(i) <= QLatin1Char('9')) ++i;
      if (i - first > 2 || pattern.at(first) == QLatin1Char('0')) return {};
      const auto position = pattern.mid(first, i - first).toInt();
      result.append({start, i - start, position, localized});
      --i;
    } else {
      if (localized) return {};
      --i; // an ordinary percent is literal, including a following percent
    }
  }
  return result;
}
enum class Kind { Text, Integer, Number };
struct Argument { int position = 0; Kind kind = Kind::Text; };
struct Message {
  quint32 revision = 0;
  QString source;
  QMap<QString, Argument> arguments;
  bool numerus = false;
};
struct Catalog {
  QString pluginId;
  QStringList locales;
  QMap<QString, Message> messages;
};
inline bool validPattern(const QString &pattern, const Message &message) {
  if (pattern.isEmpty() || !text(pattern)) return false;
  const auto parts = placeholders(pattern);
  if (!parts) return false;
  QMap<int, Kind> declared;
  for (const auto &argument : message.arguments) declared.insert(argument.position, argument.kind);
  QSet<int> positions;
  bool numerus = false;
  for (const auto &part : *parts) {
    if (part.position == 0) { numerus = true; continue; }
    if (!declared.contains(part.position) || (part.localized && declared.value(part.position) == Kind::Text)) return false;
    positions.insert(part.position);
  }
  return numerus == message.numerus && positions.size() == declared.size();
}
inline std::optional<Catalog> catalog(const QByteArray &bytes, const QString &expectedPlugin) {
  if (bytes.isEmpty() || bytes.size() > maxJsonBytes || LocalTranslations::pluginDirectory(expectedPlugin).isEmpty()) return {};
  QJsonParseError error;
  const auto document = QJsonDocument::fromJson(bytes, &error);
  if (error.error != QJsonParseError::NoError || !document.isObject()) return {};
  const auto root = document.object();
  if (!keys(root, {QStringLiteral("formatVersion"), QStringLiteral("pluginId"), QStringLiteral("sourceLocale"), QStringLiteral("locales"), QStringLiteral("messages")}) ||
      integer(root.value("formatVersion")) != 1 || root.value("pluginId").toString() != expectedPlugin ||
      root.value("sourceLocale") != QJsonValue(QStringLiteral("en")) ||
      !root.value("locales").isArray() || !root.value("messages").isObject()) return {};
  Catalog result;
  result.pluginId = expectedPlugin;
  const auto locales = root.value("locales").toArray();
  if (locales.size() > 64) return {};
  for (const auto &value : locales) {
    if (!value.isString() || !LocalTranslations::validLocale(value.toString()) || result.locales.contains(value.toString())) return {};
    result.locales.append(value.toString());
  }
  const auto messages = root.value("messages").toObject();
  if (messages.size() > 1024) return {};
  for (auto it = messages.begin(); it != messages.end(); ++it) {
    if (!token(it.key()) || !it.value().isObject()) return {};
    const auto object = it.value().toObject();
    if (!keys(object, {QStringLiteral("revision"), QStringLiteral("source"), QStringLiteral("arguments"), QStringLiteral("numerus"), QStringLiteral("comment")})) return {};
    const auto revision = integer(object.value("revision"));
    if (!revision || *revision < 1 || *revision > std::numeric_limits<quint32>::max() || !object.value("source").isString() ||
        (object.contains("numerus") && !object.value("numerus").isBool()) ||
        (object.contains("comment") && (!object.value("comment").isString() || !text(object.value("comment").toString()))) ||
        (object.contains("arguments") && !object.value("arguments").isObject())) return {};
    Message message{quint32(*revision), object.value("source").toString(), {}, object.value("numerus").toBool()};
    const auto arguments = object.value("arguments").toObject();
    if (arguments.size() > 16) return {};
    QSet<int> positions;
    for (auto arg = arguments.begin(); arg != arguments.end(); ++arg) {
      if (!token(arg.key()) || !arg.value().isObject()) return {};
      const auto declaration = arg.value().toObject();
      const auto position = integer(declaration.value("position"));
      const auto type = declaration.value("type").toString();
      if (!keys(declaration, {QStringLiteral("position"), QStringLiteral("type")}) || !position || *position < 1 || *position > arguments.size() || positions.contains(int(*position))) return {};
      Kind kind;
      if (type == QLatin1String("text")) kind = Kind::Text;
      else if (type == QLatin1String("integer")) kind = Kind::Integer;
      else if (type == QLatin1String("number")) kind = Kind::Number;
      else return {};
      positions.insert(int(*position));
      message.arguments.insert(arg.key(), {int(*position), kind});
    }
    if (!validPattern(message.source, message)) return {};
    result.messages.insert(it.key(), message);
  }
  return result;
}
struct Reference {
  QString key;
  quint32 revision = 0;
  QJsonObject arguments;
  std::optional<int> count;
};
inline std::optional<Reference> reference(const QJsonObject &object, const Catalog &catalog) {
  if (!keys(object, {QStringLiteral("messageKey"), QStringLiteral("messageRevision"), QStringLiteral("arguments"), QStringLiteral("count")}) ||
      !object.value("messageKey").isString() || (object.contains("arguments") && !object.value("arguments").isObject())) return {};
  const auto revision = integer(object.value("messageRevision"));
  const auto key = object.value("messageKey").toString();
  const auto entry = catalog.messages.constFind(key);
  if (!revision || entry == catalog.messages.end() || *revision != entry->revision) return {};
  Reference result{key, quint32(*revision), object.value("arguments").toObject(), {}};
  if (object.contains("count")) {
    const auto count = integer(object.value("count"));
    if (!count || *count < 0 || *count > std::numeric_limits<int>::max()) return {};
    result.count = int(*count);
  }
  if (result.count.has_value() != entry->numerus || result.arguments.size() != entry->arguments.size()) return {};
  for (auto it = entry->arguments.begin(); it != entry->arguments.end(); ++it) {
    const auto value = result.arguments.value(it.key());
    if (it->kind == Kind::Text ? !value.isString() || !text(value.toString()) :
        it->kind == Kind::Integer ? !integer(value).has_value() : !value.isDouble() || !std::isfinite(value.toDouble())) return {};
  }
  return result;
}
inline std::optional<QString> format(const QString &pattern, const Message &message,
                                     const Reference &reference, const QLocale &locale) {
  if (!validPattern(pattern, message)) return {};
  const auto parts = placeholders(pattern);
  QMap<int, QPair<Kind, QJsonValue>> values;
  for (auto it = message.arguments.begin(); it != message.arguments.end(); ++it)
    values.insert(it->position, {it->kind, reference.arguments.value(it.key())});
  QString result;
  qsizetype offset = 0;
  for (const auto &part : *parts) {
    result += pattern.mid(offset, part.offset - offset);
    if (!part.position) {
      if (!reference.count) return {};
      result += part.localized ? locale.toString(*reference.count) : QString::number(*reference.count);
    } else {
      const auto value = values.value(part.position);
      if (value.first == Kind::Text) result += value.second.toString();
      else if (value.first == Kind::Integer) result += part.localized ? locale.toString(value.second.toInteger()) : QString::number(value.second.toInteger());
      else result += part.localized ? locale.toString(value.second.toDouble(), 'g', QLocale::FloatingPointShortest) : QString::number(value.second.toDouble(), 'g', 17);
    }
    if (result.toUtf8().size() > maxTextBytes) return {};
    offset = part.offset + part.length;
  }
  result += pattern.mid(offset);
  if (result.toUtf8().size() > maxTextBytes) return {};
  return result;
}

// QTranslator::load(data) borrows data; keep it alive until after destruction.
class Dictionary {
  QString m_pluginId;
  QByteArray m_bytes;
  std::unique_ptr<QTranslator> m_translator;
public:
  explicit Dictionary(const QString &pluginId) : m_pluginId(pluginId) {}
  QString pluginId() const { return m_pluginId; }
  qsizetype byteSize() const { return m_bytes.size(); }
  bool load(const QByteArray &bytes) {
    if (LocalTranslations::pluginDirectory(m_pluginId).isEmpty() || bytes.isEmpty() || bytes.size() > maxQmBytes || !LocalTranslations::selfContainedQm(bytes)) return false;
    // Also own input made with QByteArray::fromRawData; its shallow copy
    // does not extend the external backing buffer's lifetime.
    QByteArray owned(bytes.constData(), bytes.size());
    auto candidate = std::make_unique<QTranslator>();
    if (!candidate->load(reinterpret_cast<const uchar *>(owned.constData()), int(owned.size()))) return false;
    m_translator.reset();
    m_bytes = std::move(owned);
    m_translator = std::move(candidate);
    return true;
  }
  QString lookup(const QString &id, int count) const {
    const auto utf8 = id.toUtf8();
    return m_translator ? m_translator->translate("", utf8.constData(), nullptr, count) : QString();
  }
};
inline QString render(const Catalog &catalog, const Reference &reference,
                      const QList<const Dictionary *> &dictionaries,
                      const QLocale &locale, const QString &fallback) {
  // Public callers can retain references, but cannot apply them to a changed
  // source revision or skip the argument validation at the render boundary.
  QJsonObject object{{QStringLiteral("messageKey"), reference.key}, {QStringLiteral("messageRevision"), qint64(reference.revision)},
                     {QStringLiteral("arguments"), reference.arguments}};
  if (reference.count) object.insert(QStringLiteral("count"), *reference.count);
  if (!ProviderMessages::reference(object, catalog)) return fallback;
  const auto &message = catalog.messages[reference.key];
  const auto id = reference.key + QLatin1Char('@') + QString::number(message.revision);
  for (const auto *dictionary : dictionaries) {
    if (!dictionary || dictionary->pluginId() != catalog.pluginId) continue;
    const auto pattern = dictionary->lookup(id, reference.count.value_or(-1));
    // lrelease without -nounfinished may return its id for an absent label.
    if (pattern == id) continue;
    if (const auto result = format(pattern, message, reference, locale)) return *result;
  }
  return fallback;
}
} // namespace ProviderMessages
