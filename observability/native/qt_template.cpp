// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Reduce Qt and QML messages to fixed templates before they reach Sentry (ADR-0092).
#include "telemetry.h"
#include <QHash>
#include <QMutex>
#include <QRegularExpression>
#include <cstring>

namespace {
constexpr int maxTemplate = 160;
constexpr int maxTemplates = 256;
constexpr qint64 templateWindowMs = 10 * 60 * 1000;

bool isWordChar(char c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
}
// Only Arinn's own source names: qrc QML/JS resources and, from the compile-time
// message context only, C++ sources. Text inside a message never names C++ code.
QByteArray sourceName(const QString &source, bool fromContext) {
  static const QRegularExpression name(QStringLiteral("^[A-Za-z_][A-Za-z0-9_]{0,63}\\.(qml|js|mjs|cpp|h)$"));
  QString file = source;
  const bool resource = file.startsWith(QLatin1String("qrc:/")) || file.startsWith(QLatin1String(":/"));
  const bool code = fromContext && !file.contains(QLatin1String("://")) &&
    (file.endsWith(QLatin1String(".cpp")) || file.endsWith(QLatin1String(".h")));
  if (!resource && !code) return {};
  file = file.mid(file.lastIndexOf(QLatin1Char('/')) + 1);
  return name.match(file).hasMatch() ? file.toLatin1() : QByteArray();
}
// Replace everything from the first to the last delimiter of a quote class with
// <str>, or to the end when only one delimiter is present. Embedded or
// mismatched quotes therefore never leave a quoted word between two matches.
void replaceQuoted(QString &text, const QString &delimiters) {
  static const QString opening = QStringLiteral("\u201C\u201E\u00AB\u2018\u201A\u2039");
  qsizetype first = -1, last = -1, count = 0;
  for (qsizetype i = 0; i < text.size(); ++i) {
    if (!delimiters.contains(text.at(i))) continue;
    if (first < 0) first = i;
    last = i;
    ++count;
  }
  if (first < 0) return;
  // An unpaired or still-open quote hides everything after it.
  const bool open = count % 2 == 1 || opening.contains(text.at(last));
  const qsizetype end = open ? text.size() : last + 1;
  text.replace(first, end - first, QStringLiteral(" <str> "));
}
QString scrub(QString text) {
  // Quoted text first: ASCII quotes and backticks, then typographic quotes
  // (titles, names, property values), each class as one private span.
  for (const auto &delimiters : {QStringLiteral("\""), QStringLiteral("'"), QStringLiteral("`"),
                                 QStringLiteral("\u201C\u201D\u201E\u00AB\u00BB\u2018\u2019\u201A\u2039\u203A")})
    replaceQuoted(text, delimiters);
  struct Rule { QRegularExpression pattern; QString replacement; };
  static const Rule rules[] = {
    // Any token holding non-printable or non-ASCII text (titles in any script,
    // accented names, emoji, control characters) is replaced whole.
    {QRegularExpression(QStringLiteral("\\S*[^\\x20-\\x7E\\s]\\S*(?:\\s+\\S*[^\\x20-\\x7E\\s]\\S*)*")), QStringLiteral(" <text> ")},
    // URLs of any scheme, hierarchical or opaque (qrc:, file:, data:, magnet:, urn:),
    // but not a C++ scope such as QQuickItem::stackAfter.
    {QRegularExpression(QStringLiteral("\\S*\\b[A-Za-z][A-Za-z0-9+.-]*:(?!:)\\S+")), QStringLiteral("<url>")},
    // From here on a match is always the whole whitespace-delimited token, brackets
    // included, so no part of a path or name survives next to its placeholder.
    // Any token with a slash or backslash is a path, whatever precedes it.
    {QRegularExpression(QStringLiteral("\\S*[/\\\\]\\S*")), QStringLiteral("<path>")},
    // Addresses and accounts.
    {QRegularExpression(QStringLiteral("\\S*@\\S*")), QStringLiteral("<str>")},
    // Any dotted name, whatever its extension: file names, hosts, dotted identifiers.
    // A sentence-ending dot is kept.
    {QRegularExpression(QStringLiteral("\\S*\\w\\.\\w\\S*")), QStringLiteral("<file>")},
    // A token that still holds a query or address separator is dropped whole,
    // never split into its parts.
    {QRegularExpression(QStringLiteral("\\S*[\\\\/?&#%=@]\\S*")), QStringLiteral("<str>")},
    {QRegularExpression(QStringLiteral("0x[0-9A-Fa-f]+|[0-9]+")), QStringLiteral("<n>")},
    {QRegularExpression(QStringLiteral("\\s+")), QStringLiteral(" ")},
  };
  for (const auto &rule : rules) text.replace(rule.pattern, rule.replacement);
  // The rules above name what to remove; this pass decides what may stay. A token
  // survives only as words, C++ scopes, placeholders and punctuation; anything
  // else (a leading-dot name, an unusual URL) becomes <str>.
  static const QString atom = QStringLiteral(
    "(?:<(?:url|str|path|file|n|text)>|[A-Za-z_][A-Za-z0-9_-]*(?:::[A-Za-z_][A-Za-z0-9_]*)*)");
  static const QRegularExpression allowed(QStringLiteral("^[(\\[{]*") + atom +
    QStringLiteral("(?:[()\\[\\]{},]+") + atom + QStringLiteral(")*[()\\[\\]{},;:!.]*$"));
  static const QRegularExpression punctuation(QStringLiteral("^[()\\[\\]{},;:!.-]+$"));
  // Hexadecimal groups joined by colons are IPv6 addresses (`dead::beef`), not
  // C++ scopes, even when no digit is left in them.
  static const QRegularExpression address(QStringLiteral("^[(\\[{]*[0-9A-Fa-f]*(?::+[0-9A-Fa-f]*){2,}[)\\]},;:!.]*$"));
  QStringList tokens = text.split(QLatin1Char(' '), Qt::SkipEmptyParts);
  for (auto &token : tokens)
    if (address.match(token).hasMatch() ||
        (!allowed.match(token).hasMatch() && !punctuation.match(token).hasMatch())) token = QStringLiteral("<str>");
  text = tokens.join(QLatin1Char(' '));
  if (text.size() > maxTemplate) {
    text.truncate(maxTemplate);
    const auto open = text.lastIndexOf(QLatin1Char('<'));
    if (open > text.lastIndexOf(QLatin1Char('>'))) text.truncate(open);
    text = text.trimmed();
  }
  return text;
}
bool benign(const QString &text, const QByteArray &category) {
  return text.contains(QLatin1String("icon is unavailable"), Qt::CaseInsensitive) ||
    text.contains(QLatin1String("Artwork is unavailable")) ||
    text.contains(QLatin1String("QWaylandDataOffer")) || category.contains("QWaylandDataOffer");
}
QMutex limiterLock;
// Messages arrive from any thread; the shared expressions are used under this lock.
QMutex templateLock;
QHash<QByteArray, qint64> lastReported;
}

namespace ArinnTelemetry {
QtTemplate qtTemplate(const QMessageLogContext &context, const QString &message) {
  QMutexLocker lock(&templateLock);
  QtTemplate out;
  QString text = message.left(4096);
  // QML prefixes its messages with "<url>:<line>[:<column>]: ".
  static const QRegularExpression prefix(QStringLiteral("^([A-Za-z][A-Za-z0-9+.-]*:[^\\s]*?|/[^\\s]*?):([0-9]{1,6})(?::[0-9]{1,6})?:\\s*"));
  const auto match = prefix.match(text);
  QString source = context.file ? QString::fromUtf8(context.file) : QString();
  int line = context.line;
  bool fromContext = true;
  if (match.hasMatch()) {
    source = match.captured(1);
    fromContext = false;
    line = match.captured(2).toInt();
    text = text.mid(match.capturedLength());
  }
  out.location = sourceName(source, fromContext);
  if (!out.location.isEmpty() && line > 0 && line < 1000000)
    out.location += ':' + QByteArray::number(line);
  if (context.category && safeCategory(context.category) && std::strcmp(context.category, "default") != 0)
    out.category = context.category;
  out.benign = benign(text, out.category);
  out.text = scrub(text).toLatin1();
  if (!safeTemplateText(out.text.constData())) out.text = "Qt message";
  return out;
}
QByteArray scrubTemplate(const QString &text) {
  QMutexLocker lock(&templateLock);
  auto out = scrub(text.left(4096)).toLatin1();
  return safeTemplateText(out.constData()) ? out : QByteArrayLiteral("Qt message");
}
// No allocation or locale access: also called from the native crash callback.
bool safeTemplateText(const char *text) {
  if (!text || !*text) return false;
  size_t size = 0;
  for (const char *c = text; *c; ++c, ++size) {
    if (size >= maxTemplate) return false;
    if (*c < 0x20 || *c > 0x7E || (*c >= '0' && *c <= '9')) return false;
    if (std::strchr("/\\\"'`@?&#%=", *c)) return false;
  }
  return true;
}
// The full location grammar: <name>.<qml|js|mjs|cpp|h>[:<line>], or the fixed
// sentinel for an unknown location. Any other file name is rejected.
bool safeLocation(const char *text) {
  if (!text) return false;
  if (std::strcmp(text, "unknown.location") == 0) return true;
  const char *c = text;
  if (!((*c >= 'a' && *c <= 'z') || (*c >= 'A' && *c <= 'Z') || *c == '_')) return false;
  size_t name = 0;
  while (isWordChar(*c)) {
    ++c;
    if (++name > 64) return false;
  }
  if (*c++ != '.') return false;
  bool known = false;
  for (const char *extension : {"qml", "js", "mjs", "cpp", "h"}) {
    const size_t size = std::strlen(extension);
    if (std::strncmp(c, extension, size) == 0 && (c[size] == '\0' || c[size] == ':')) {
      c += size;
      known = true;
      break;
    }
  }
  if (!known) return false;
  if (*c == '\0') return true;
  if (*c++ != ':') return false;
  size_t digits = 0;
  while (*c >= '0' && *c <= '9') {
    ++c;
    if (++digits > 6) return false;
  }
  return digits > 0 && *c == '\0';
}
bool safeCategory(const char *text) {
  if (!text || !*text) return false;
  size_t size = 0;
  for (const char *c = text; *c; ++c, ++size) {
    if (size >= 64) return false;
    if (!isWordChar(*c) && *c != '.' && *c != '-') return false;
  }
  return true;
}
bool admitTemplate(const QByteArray &key, qint64 nowMs) {
  QMutexLocker lock(&limiterLock);
  const auto found = lastReported.constFind(key);
  if (found != lastReported.constEnd() && nowMs - *found < templateWindowMs) return false;
  if (found == lastReported.constEnd() && lastReported.size() >= maxTemplates) {
    for (auto it = lastReported.begin(); it != lastReported.end();)
      it = nowMs - *it >= templateWindowMs ? lastReported.erase(it) : std::next(it);
    if (lastReported.size() >= maxTemplates) return false;
  }
  lastReported.insert(key, nowMs);
  return true;
}
}
