// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "transport.h"
#include "telemetry.h"
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <curl/curl.h>
#include <algorithm>
#include <atomic>
#include <cmath>

namespace {
void copy(QJsonObject &out, const QJsonObject &in, const char *key) {
  const auto value = in.value(key);
  const QString name(key);
  bool valid = false;
  if (name.endsWith("timestamp")) {
    valid = (value.isDouble() && std::isfinite(value.toDouble()) && value.toDouble() >= 0) ||
      (value.isString() && QRegularExpression("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+(Z|[+-][0-9]{2}:[0-9]{2})$").match(value.toString()).hasMatch());
  } else if (name == "image_size") valid = value.isDouble() && value.toDouble() >= 0 && std::isfinite(value.toDouble());
  else if (name == "level") valid = QStringList{"debug", "info", "warning", "error", "fatal"}.contains(value.toString());
  else if (name == "type") valid = QStringList{"elf", "pe", "macho"}.contains(value.toString());
  else if (name == "status") valid = QStringList{"ok", "aborted", "internal_error", "cancelled", "unknown_error"}.contains(value.toString());
  else if (name == "image_addr") valid = QRegularExpression("^0x[0-9a-fA-F]{1,16}$").match(value.toString()).hasMatch();
  else if (name == "event_id") {
    auto id = value.toString();
    if (QRegularExpression("^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$").match(id).hasMatch()) id.remove('-');
    if (QRegularExpression("^[0-9a-fA-F]{32}$").match(id).hasMatch()) out.insert(key, id);
    return;
  }
  else if (name == "trace_id") valid = QRegularExpression("^[0-9a-fA-F]{32}$").match(value.toString()).hasMatch();
  else if (name == "span_id" || name == "parent_span_id") valid = QRegularExpression("^[0-9a-fA-F]{16}$").match(value.toString()).hasMatch();
  else if (name == "debug_id" || name == "code_id") valid = QRegularExpression("^[0-9a-fA-F-]{1,128}$").match(value.toString()).hasMatch();
  if (valid) out.insert(key, value);
}
QJsonObject frames(const QJsonObject &stack) {
  QJsonArray clean;
  const auto original = stack.value("frames").toArray();
  for (auto value : original) {
    if (clean.size() == 128) break;
    QJsonObject frame;
    const auto source = value.toObject();
    for (auto key : {"instruction_addr", "image_addr", "symbol_addr"}) {
      const auto address = source.value(key).toString();
      if (address.startsWith("0x") && address.size() <= 18 &&
          std::all_of(address.begin() + 2, address.end(), [](QChar c) {
            return c.isDigit() || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
          })) frame.insert(key, address);
    }
    if (!frame.isEmpty()) clean.append(frame);
  }
  return {{"frames", clean}};
}
QJsonObject event(const QJsonObject &original) {
  QJsonObject clean{{"platform", "native"}, {"release", ARINN_RELEASE},
    {"message", "Native technical diagnostic; private message omitted"},
    {"tags", QJsonObject{{"component", "arinn-shell"}}}};
  for (auto key : {"event_id", "timestamp", "level"}) copy(clean, original, key);
  const auto tags = original.value("tags").toObject();
  auto cleanTags = clean.value("tags").toObject();
  const auto location = tags.value("qt.location").toString().toLatin1();
  if (ArinnTelemetry::safeLocation(location.constData()))
    cleanTags.insert("qt.location", QString::fromLatin1(location));
  const auto category = tags.value("qt.category").toString().toLatin1();
  if (ArinnTelemetry::safeCategory(category.constData()))
    cleanTags.insert("qt.category", QString::fromLatin1(category));
  clean.insert("tags", cleanTags);
  const auto print = original.value("fingerprint").toArray();
  if (print.size() == 4) {
    QJsonArray cleanPrint;
    for (const auto &part : print) {
      const auto text = part.toString().toLatin1();
      if (!ArinnTelemetry::safeTemplateText(text.constData()) && !ArinnTelemetry::safeLocation(text.constData())) break;
      cleanPrint.append(QString::fromLatin1(text));
    }
    if (cleanPrint.size() == 4) clean.insert("fingerprint", cleanPrint);
  }
  QJsonArray exceptions;
  for (auto value : original.value("exception").toObject().value("values").toArray()) {
    if (exceptions.size() == 4) break;
    auto source = value.toObject();
    auto name = source.value("type").toString();
    if (name != "SIGSEGV" && name != "SIGABRT" && name != "SIGBUS" && name != "SIGILL" && name != "SIGFPE" &&
        name != "QtWarning" && name != "QtCritical" && name != "QtFatal") name = "NativeError";
    // Non-Latin-1 text becomes '?', which the template check rejects.
    const auto text = source.value("value").toString().toLatin1();
    const bool qt = name.startsWith("Qt");
    QJsonObject exception{{"type", name}, {"value", qt && ArinnTelemetry::safeTemplateText(text.constData())
      ? QString::fromLatin1(text) : QStringLiteral("Technical failure; private message omitted")}};
    if (source.contains("stacktrace")) exception.insert("stacktrace", frames(source.value("stacktrace").toObject()));
    exceptions.append(exception);
  }
  if (!exceptions.isEmpty()) clean.insert("exception", QJsonObject{{"values", exceptions}});
  if (original.contains("stacktrace")) clean.insert("stacktrace", frames(original.value("stacktrace").toObject()));
  QJsonArray crumbs;
  for (auto value : original.value("breadcrumbs").toArray()) {
    if (crumbs.size() == 100) break;
    auto source = value.toObject();
    auto message = source.value("message").toString();
    bool allowed = false;
    for (auto known : {"startup", "shutdown", "about-to-quit", "qt.debug", "qt.info", "qt.warning", "qt.critical", "qt.fatal", "qt.noise", "input.key", "input.pointer", "focus.in", "focus.out", "window.show", "window.hide", "window.activate", "window.deactivate"})
      if (message == known) allowed = true;
    if (allowed) {
      QJsonObject crumb{{"type", "default"}, {"category", "shell.lifecycle"}, {"message", message}};
      copy(crumb, source, "timestamp");
      crumbs.append(crumb);
    }
  }
  clean.insert("breadcrumbs", crumbs);
  QJsonArray images;
  for (auto value : original.value("debug_meta").toObject().value("images").toArray()) {
    if (images.size() == 256) break;
    QJsonObject image;
    for (auto key : {"type", "debug_id", "code_id", "image_addr", "image_size"}) copy(image, value.toObject(), key);
    images.append(image);
  }
  if (!images.isEmpty()) clean.insert("debug_meta", QJsonObject{{"images", images}});
  return clean;
}
QJsonObject transaction(const QJsonObject &original) {
  if (original.value("transaction").toString() != "shell.startup") return {};
  QJsonObject clean{{"type", "transaction"}, {"transaction", "shell.startup"},
    {"platform", "native"}, {"release", ARINN_RELEASE},
    {"tags", QJsonObject{{"component", "arinn-shell"}}}, {"spans", QJsonArray{}}};
  for (auto key : {"event_id", "start_timestamp", "timestamp"}) copy(clean, original, key);
  auto source = original.value("contexts").toObject().value("trace").toObject();
  QJsonObject trace{{"op", "shell.startup"}};
  for (auto key : {"trace_id", "span_id", "parent_span_id", "status"}) copy(trace, source, key);
  clean.insert("contexts", QJsonObject{{"trace", trace}});
  return clean;
}
QJsonObject metrics(const QJsonObject &original) {
  QJsonArray items;
  for (auto value : original.value("items").toArray()) {
    if (items.size() == 1000) break;
    auto source = value.toObject();
    auto name = source.value("name").toString();
    const auto number = source.value("value");
    if (!QStringList{"arinn.shell.messages", "arinn.shell.errors", "arinn.shell.messages.benign",
                     "arinn.shell.messages.suppressed", "arinn.shell.startup.duration"}.contains(name) ||
        !number.isDouble() || !std::isfinite(number.toDouble()) || number.toDouble() < 0) continue;
    const auto type = source.value("type").toString();
    if (type != "counter" && type != "distribution" && type != "gauge") continue;
    QJsonObject metric{{"name", name}, {"type", type}, {"value", number},
      {"attributes", QJsonObject{{"component", QJsonObject{{"type", "string"}, {"value", "arinn-shell"}}}}}};
    for (auto key : {"timestamp", "trace_id", "span_id"}) copy(metric, source, key);
    if (name == "arinn.shell.startup.duration") metric.insert("unit", "millisecond");
    items.append(metric);
  }
  if (items.isEmpty()) return {};
  return {{"items", items}};
}
struct Client {
  CURL *curl = nullptr;
  std::atomic_bool stopping{false};
};
sentry_http_client_t *create(void *) {
  if (curl_global_init(CURL_GLOBAL_DEFAULT) != CURLE_OK) return nullptr;
  auto *client = new Client;
  client->curl = curl_easy_init();
  if (!client->curl) { delete client; curl_global_cleanup(); return nullptr; }
  return client;
}
size_t discard(char *, size_t size, size_t count, void *) { return size * count; }
size_t responseHeader(char *data, size_t size, size_t count, void *state) {
  const auto line = QByteArray(data, size * count);
  const auto colon = line.indexOf(':');
  if (colon > 0) {
    const auto key = line.left(colon).trimmed();
    const auto value = line.mid(colon + 1).trimmed();
    sentry_http_response_set_header(static_cast<sentry_http_response_t *>(state), key.constData(), value.constData());
  }
  return size * count;
}
int progress(void *state, curl_off_t, curl_off_t, curl_off_t, curl_off_t) {
  return static_cast<Client *>(state)->stopping.load() ? 1 : 0;
}
int send(sentry_http_client_t *opaque, sentry_http_request_t *request, sentry_http_response_t *response) {
  auto *client = static_cast<Client *>(opaque);
  size_t size = 0;
  const auto *body = sentry_http_request_get_body(request, &size);
  QByteArray clean;
  try {
    if (body && size <= 4 * 1024 * 1024 && QByteArray(sentry_http_request_get_method(request)) == "POST")
      clean = ArinnTelemetry::filterEnvelope(QByteArray(body, size));
  } catch (...) { clean.clear(); }
  if (clean.isEmpty()) {
    // Discard unknown/unfilterable payloads, including attachments, without retry.
    sentry_http_response_set_status_code(response, 204);
    return 1;
  }
  curl_easy_reset(client->curl);
  curl_slist *headers = nullptr;
  for (size_t i = 0; i < sentry_http_request_get_header_count(request); ++i) {
    const char *key = nullptr, *value = nullptr;
    if (sentry_http_request_get_header(request, i, &key, &value)) {
      const auto lower = QByteArray(key).toLower();
      if (lower != "content-type" && lower != "x-sentry-auth" && lower != "user-agent") continue;
      const QByteArray header = QByteArray(key) + ": " + value;
      headers = curl_slist_append(headers, header.constData());
    }
  }
  curl_easy_setopt(client->curl, CURLOPT_URL, sentry_http_request_get_url(request));
  curl_easy_setopt(client->curl, CURLOPT_POST, 1L);
  curl_easy_setopt(client->curl, CURLOPT_POSTFIELDS, clean.constData());
  curl_easy_setopt(client->curl, CURLOPT_POSTFIELDSIZE_LARGE, static_cast<curl_off_t>(clean.size()));
  curl_easy_setopt(client->curl, CURLOPT_HTTPHEADER, headers);
  curl_easy_setopt(client->curl, CURLOPT_CONNECTTIMEOUT_MS, 1000L);
  curl_easy_setopt(client->curl, CURLOPT_TIMEOUT_MS, 2000L);
  curl_easy_setopt(client->curl, CURLOPT_NOSIGNAL, 1L);
  curl_easy_setopt(client->curl, CURLOPT_WRITEFUNCTION, discard);
  curl_easy_setopt(client->curl, CURLOPT_HEADERFUNCTION, responseHeader);
  curl_easy_setopt(client->curl, CURLOPT_HEADERDATA, response);
  curl_easy_setopt(client->curl, CURLOPT_NOPROGRESS, 0L);
  curl_easy_setopt(client->curl, CURLOPT_XFERINFOFUNCTION, progress);
  curl_easy_setopt(client->curl, CURLOPT_XFERINFODATA, client);
  const auto result = curl_easy_perform(client->curl);
  long status = 0;
  curl_easy_getinfo(client->curl, CURLINFO_RESPONSE_CODE, &status);
  curl_slist_free_all(headers);
  sentry_http_response_set_status_code(response, status);
  return result == CURLE_OK ? 1 : 0;
}
void destroy(sentry_http_client_t *opaque) {
  auto *client = static_cast<Client *>(opaque);
  curl_easy_cleanup(client->curl);
  delete client;
  curl_global_cleanup();
}
void shutdown(sentry_http_client_t *opaque) { static_cast<Client *>(opaque)->stopping = true; }
}
namespace ArinnTelemetry {
QByteArray filterEnvelope(const QByteArray &body) {
  if (body.size() > 4 * 1024 * 1024) return {};
  qsizetype position = body.indexOf('\n');
  if (position < 0 || !QJsonDocument::fromJson(body.left(position)).isObject()) return {};
  QByteArray clean;
  QJsonObject envelopeHeader;
  ++position;
  int count = 0;
  while (position < body.size()) {
    if (++count > 64) return {};
    const auto end = body.indexOf('\n', position);
    if (end < 0) return {};
    const auto header = QJsonDocument::fromJson(body.mid(position, end - position));
    if (!header.isObject()) return {};
    position = end + 1;
    const auto length = header.object().value("length").toInteger(-1);
    qsizetype size = length;
    if (size < 0) {
      const auto newline = body.indexOf('\n', position);
      size = (newline < 0 ? body.size() : newline) - position;
    }
    if (size < 0 || size > body.size() - position) return {};
    const auto payload = QJsonDocument::fromJson(body.mid(position, size));
    if (!payload.isObject()) return {};
    position += size;
    if (position < body.size()) {
      if (body.at(position) != '\n') return {};
      ++position;
    }
    const auto type = header.object().value("type").toString();
    QJsonObject filtered;
    if (type == "event") filtered = event(payload.object());
    else if (type == "transaction") filtered = transaction(payload.object());
    else if (type == "trace_metric") filtered = metrics(payload.object());
    else return {};
    if (filtered.isEmpty()) continue;
    if (type == "event" || type == "transaction") {
      if (!filtered.contains("event_id")) continue;
      if (envelopeHeader.contains("event_id")) return {};
      envelopeHeader.insert("event_id", filtered.value("event_id"));
    }
    const auto encoded = QJsonDocument(filtered).toJson(QJsonDocument::Compact);
    QJsonObject item{{"type", type}, {"length", encoded.size()}};
    if (type == "trace_metric") {
      item.insert("item_count", filtered.value("items").toArray().size());
      item.insert("content_type", "application/vnd.sentry.items.trace-metric+json");
    }
    clean += QJsonDocument(item).toJson(QJsonDocument::Compact) + '\n' + encoded + '\n';
  }
  return clean.isEmpty() ? QByteArray{} : QJsonDocument(envelopeHeader).toJson(QJsonDocument::Compact) + '\n' + clean;
}
sentry_transport_t *transport() {
  auto *out = sentry_http_transport_new(create, nullptr, send, destroy);
  if (out) sentry_http_transport_set_client_shutdown_func(out, shutdown);
  return out;
}
}

// Called by the pinned SDK before either queueing or suspended crash persistence.
// Recursive hook-bypass crashes are dropped by the backend before this boundary.
extern "C" sentry_envelope_t *arinn_sentry_filter_envelope(sentry_envelope_t *envelope) {
  sentry_envelope_t *out = nullptr;
  size_t size = 0;
  char *body = sentry_envelope_serialize(envelope, &size);
  try {
    if (body && size <= 4 * 1024 * 1024) {
      const auto clean = ArinnTelemetry::filterEnvelope(QByteArray(body, size));
      if (!clean.isEmpty()) out = sentry_envelope_deserialize(clean.constData(), clean.size());
    }
  } catch (...) { /* Drop payloads on filtering failure. */ }
  sentry_free(body);
  sentry_envelope_free(envelope);
  return out;
}
