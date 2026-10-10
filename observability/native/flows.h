// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
// The fixed vocabulary of shell flow transactions (ADR-0093). The SDK
// callback and the transport filter keep only these values; adding one is a
// technical change that updates docs/architecture/telemetry.md and the tests.
#include <cstring>

namespace ArinnFlows {
inline constexpr const char *flows[] = {"home.load",   "downloads.read",  "browse.open",
                                        "shortcut.open", "activity.return", "playback.present"};
inline constexpr const char *steps[] = {"request", "operation.wait", "window.resolve", "platform.present",
                                        "window.check"};
inline constexpr const char *outcomes[] = {"ok",      "failed",          "updated",  "NotFound",
                                           "Unavailable", "Unsupported", "PermissionDenied",
                                           "Timeout", "InvalidArgument", "Conflict", "Internal", "Cancelled"};
// Sentry span statuses an outcome maps to, and the only statuses kept.
inline constexpr const char *statuses[] = {"ok",          "unknown_error",     "not_found",
                                           "unavailable", "unimplemented",     "permission_denied",
                                           "deadline_exceeded", "invalid_argument", "aborted",
                                           "internal_error", "cancelled"};
// The span op of every flow transaction and of its steps.
inline constexpr const char *flowOp = "shell.flow";
inline constexpr const char *stepOp = "shell.step";
// At most this many step spans are kept per flow.
inline constexpr int maxSteps = 8;

template <typename List> bool contains(const List &list, const char *value) {
  if (!value) return false;
  for (const char *known : list)
    if (std::strcmp(known, value) == 0) return true;
  return false;
}
inline bool knownFlow(const char *value) { return contains(flows, value); }
inline bool knownStep(const char *value) { return contains(steps, value); }
inline bool knownOutcome(const char *value) { return contains(outcomes, value); }
inline bool knownStatus(const char *value) { return contains(statuses, value); }
inline const char *status(const char *outcome) {
  struct Pair { const char *outcome, *status; };
  static constexpr Pair pairs[] = {{"ok", "ok"}, {"updated", "ok"}, {"NotFound", "not_found"},
                                   {"Unavailable", "unavailable"}, {"Unsupported", "unimplemented"},
                                   {"PermissionDenied", "permission_denied"}, {"Timeout", "deadline_exceeded"},
                                   {"InvalidArgument", "invalid_argument"}, {"Conflict", "aborted"},
                                   {"Internal", "internal_error"}, {"Cancelled", "cancelled"}};
  for (const auto &pair : pairs)
    if (outcome && std::strcmp(pair.outcome, outcome) == 0) return pair.status;
  return "unknown_error";
}
}
