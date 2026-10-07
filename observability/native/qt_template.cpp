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
QString scrub(QString text) {
  struct Rule { QRegularExpression pattern; QString replacement; };
  static const Rule rules[] = {
    // Non-printable or non-ASCII text (titles in any script, control characters).
    {QRegularExpression(QStringLiteral("[^\\x20-\\x7E]+(?:\\s+[^\\x20-\\x7E]+)*")), QStringLiteral(" <text> ")},
    // Quoted text, closed or not: titles, names, property values.
    {QRegularExpression(QStringLiteral("\"[^\"]*\"?|'[^']*'?|`[^`]*`?")), QStringLiteral("<str>")},
    // URLs of any scheme, hierarchical or opaque (qrc:, file:, data:, magnet:, urn:),
    // but not a C++ scope such as QQuickItem::stackAfter.
    {QRegularExpression(QStringLiteral("\\b[A-Za-z][A-Za-z0-9+.-]{0,31}:(?!:)[^\\s<>]+")), QStringLiteral("<url>")},
    // Any token with a slash or backslash is a path, whatever precedes it.
    {QRegularExpression(QStringLiteral("[^\\s<>()\\[\\]{}]*[/\\\\][^\\s<>()\\[\\]{}]*")), QStringLiteral("<path>")},
    // Addresses and accounts.
    {QRegularExpression(QStringLiteral("[^\\s<>]+@[^\\s<>]+")), QStringLiteral("<str>")},
    // Any dotted name, whatever its extension: file names, hosts, dotted identifiers.
    // A sentence-ending dot is kept.
    {QRegularExpression(QStringLiteral("[^\\s<>()\\[\\]{}]*\\w\\.\\w[^\\s<>()\\[\\]{}]*")), QStringLiteral("<file>")},
    // A token that still holds a query or address separator is dropped whole,
    // never split into its parts.
    {QRegularExpression(QStringLiteral("[^\\s<>]*[\\\\/?&#%=@][^\\s<>]*")), QStringLiteral("<str>")},
    {QRegularExpression(QStringLiteral("0x[0-9A-Fa-f]+|[0-9]+")), QStringLiteral("<n>")},
    {QRegularExpression(QStringLiteral("\\s+")), QStringLiteral(" ")},
  };
  for (const auto &rule : rules) text.replace(rule.pattern, rule.replacement);
  text = text.trimmed();
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
bool safeLocation(const char *text) {
  if (!text || !*text) return false;
  size_t size = 0;
  bool dot = false;
  for (const char *c = text; *c; ++c, ++size) {
    if (size >= 96) return false;
    if (*c == '.') dot = true;
    else if (!isWordChar(*c) && *c != ':') return false;
  }
  return dot && isWordChar(text[0]);
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
