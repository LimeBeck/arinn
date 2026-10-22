// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "frame_gate.h"
#include "pixels.h"
#include <cstdlib>
#include <iostream>
#include <limits>

using namespace arinn::background;
namespace {
void check(bool ok, const char *message) {
  if (!ok) { std::cerr << message << '\n'; std::exit(1); }
}
Context context() {
  return {"11111111-1111-4111-8111-111111111111",
          "22222222-2222-4222-8222-222222222222", "fixture-return", "fixture-output",
          "33333333-3333-4333-8333-333333333333", "fixture-window"};
}
Evidence evidence() {
  const auto c = context();
  return {":1.42", c.epoch, c.target, c.output, c.generation, c.token,
          true, true, true, true, true};
}
Snapshot snapshot(std::uint64_t revision = 1) {
  return {context().epoch, "ready", revision, context()};
}
const std::vector<std::uint8_t> rgba(16, 42);
FrameGate::Ticket prepare(FrameGate &gate) {
  gate.observe(evidence());
  check(gate.accept(snapshot()), "accept ready metadata");
  auto ticket = gate.beginRead();
  check(ticket.has_value(), "start one off-thread read");
  return *ticket;
}
void install(FrameGate &gate) {
  auto ticket = prepare(gate);
  check(gate.completeRead(ticket, context(), 2, 2, rgba), "install current bounded frame");
  check(gate.ready() && gate.pixels().size() == 16, "frame is ready");
}
}
int main() {
  FrameGate gate;
  check(!gate.beginRead(), "unknown guard/evidence fails closed");
  install(gate);
  check(!gate.beginRead(), "repeated Home/metadata never rereads ready pixels");
  gate.observe(evidence());
  check(gate.accept(snapshot()) && gate.ready(), "same context is stable");
  // Every authoritative lifetime loss clears immediately; a late copy cannot
  // install pixels, and cancellation does not queue another worker.
  for (int change = 0; change < 11; ++change) {
    FrameGate current;
    const auto ticket = prepare(current);
    auto e = evidence();
    switch (change) {
    case 0: e.home = false; break;
    case 1: e.sessionSafe = false; break; // includes unknown, lock and sleep
    case 2: e.providerLive = false; break;
    case 3: e.producerLive = false; break;
    case 4: e.shellLive = false; break;
    case 5: e.owner = ":1.43"; break;
    case 6: e.epoch = "44444444-4444-4444-8444-444444444444"; break;
    case 7: e.target = "different-return"; break;
    case 8: e.output = "different-output"; break;
    case 9: e.generation = "55555555-5555-4555-8555-555555555555"; break;
    case 10: e.token = "remapped-window"; break;
    }
    current.observe(e);
    check(!current.ready() && current.busy(), "invalidate while preserving occupied worker slot");
    check(!current.beginRead(), "no replacements queue behind cancelled worker");
    check(!current.completeRead(ticket, context(), 2, 2, rgba), "reject late result after lifetime loss");
    check(!current.busy() && !current.ready(), "late result releases resources");
    FrameGate installed; install(installed); installed.observe(e);
    check(!installed.ready() && installed.pixels().empty(), "release installed pixels on lifetime loss");
  }
  FrameGate notified;
  auto old = prepare(notified);
  notified.notified(context().epoch, 2);
  check(!notified.beginRead(), "notification invalidates before IPC");
  check(!notified.accept(snapshot(1)), "reply older than signal rejected");
  check(notified.accept(snapshot(2)), "current reconciliation accepted");
  check(!notified.beginRead(), "old worker prevents unbounded replacements");
  check(!notified.completeRead(old, context(), 2, 2, rgba), "old worker rejected after revision");
  auto fresh = notified.beginRead();
  check(fresh.has_value(), "one replacement after cancelled completion");
  check(!notified.completeRead(old, context(), 2, 2, rgba) && notified.busy(), "duplicate callback cannot free newer worker");
  check(notified.completeRead(*fresh, context(), 2, 2, rgba), "install fresh result");
  notified.notified(context().epoch, 1);
  check(notified.ready(), "old notification does not clear fresh frame");
  notified.failed();
  check(!notified.ready() && !notified.beginRead(), "failed IPC requires fresh reconciliation");
  check(!notified.accept(snapshot(1)), "failure retains metadata anti-regression floor");
  check(notified.accept(snapshot(2)), "same immutable snapshot can reconcile after failure");
  for (int loss = 0; loss < 3; ++loss) {
    FrameGate restored; install(restored);
    auto blocked = evidence();
    if (loss == 0) blocked.sessionSafe = false;
    if (loss == 1) blocked.home = false;
    if (loss == 2) blocked.providerLive = false;
    restored.observe(blocked); restored.observe(evidence());
    check(!restored.accept(snapshot()) && !restored.beginRead(), "restored evidence never resurrects retired capture");
    check(!restored.accept(snapshot(2)), "revision alone cannot revive retired request");
    auto replacement = snapshot(3);
    replacement.context->request = "66666666-6666-4666-8666-666666666666";
    check(restored.accept(replacement) && restored.beginRead().has_value(), "new explicit capture lifetime can replace retired request");
    FrameGate inflight;
    auto retired = prepare(inflight);
    inflight.observe(blocked); inflight.observe(evidence());
    check(!inflight.completeRead(retired, context(), 2, 2, rgba), "unlock cannot admit late pre-lock copy");
    check(!inflight.accept(snapshot()), "in-flight request also stays retired");
  }
  {
    // The Return window resolves after the ready metadata: evidence that
    // only completes never admitted the request and cannot retire it.
    FrameGate completing;
    auto partial = evidence();
    partial.generation.clear(); partial.token.clear();
    completing.observe(partial);
    check(completing.accept(snapshot()) && !completing.beginRead(), "incomplete evidence admits no read");
    completing.observe(evidence());
    check(completing.accept(snapshot()) && completing.beginRead().has_value(), "completed evidence admits the read");
    // A guard that was never safe is restored: the request is retired.
    FrameGate guarded;
    auto locked = evidence();
    locked.sessionSafe = false;
    guarded.observe(locked);
    check(guarded.accept(snapshot()) && !guarded.beginRead(), "blocked guard admits no read");
    guarded.observe(evidence());
    check(!guarded.accept(snapshot()) && !guarded.beginRead(), "unlock retires a request seen while blocked");
  }
  for (const auto &epoch : {std::string("bad"), std::string("44444444-4444-4444-8444-444444444444")}) {
    FrameGate recovering; install(recovering);
    recovering.notified(epoch, 2);
    check(!recovering.ready() && !recovering.beginRead(), "bad signal immediately clears pixels");
    check(recovering.accept(snapshot()) && recovering.beginRead().has_value(), "bad signal cannot poison current-owner reconciliation");
  }
  for (int mutation = 0; mutation < 8; ++mutation) {
    FrameGate invalid; install(invalid);
    auto s = snapshot();
    switch (mutation) {
    case 0: s.revision = 0; break;
    case 1: s.context.reset(); break;
    case 2: s.state = "invalid"; break;
    case 3: s.context->request = "invalid"; break;
    case 4: s.context->token = std::string(257, 'x'); break;
    case 5: s.context->output = std::string("has\0nul", 7); break;
    case 6: s.epoch = "44444444-4444-4444-8444-444444444444"; break;
    case 7: s.context->request = "66666666-6666-4666-8666-666666666666"; break; // same revision, conflicting contents
    }
    check(!invalid.accept(s) && !invalid.ready() && !invalid.beginRead(), "reject malformed/conflicting metadata");
  }
  for (int mutation = 0; mutation < 5; ++mutation) {
    FrameGate invalid;
    auto ticket = prepare(invalid), echoed = ticket;
    auto data = std::optional(rgba); std::uint32_t w = 2, h = 2;
    switch (mutation) {
    case 0: echoed.context.target = "wrong-window"; break;
    case 1: w = std::numeric_limits<std::uint32_t>::max(); break;
    case 2: h = 0; break;
    case 3: data->pop_back(); break;
    case 4: data.reset(); break;
    }
    check(!invalid.completeRead(ticket, echoed.context, w, h, std::move(data)) && !invalid.ready(), "reject invalid read result");
  }
  for (const auto &state : {"idle", "pending", "unavailable"}) {
    FrameGate empty; install(empty);
    auto s = snapshot(2); s.state = state; s.context.reset();
    check(empty.accept(s) && !empty.ready() && !empty.beginRead(), "non-ready metadata releases frame");
  }
  FrameGate endToEnd;
  const auto ticket = prepare(endToEnd);
  auto fd = Pixels::create(2, 2, rgba);
  auto received = Pixels::duplicate(fd.descriptor(), 2, 2);
  fd.reset();
  check(endToEnd.completeRead(ticket, context(), 2, 2, received.read(2, 2)), "sealed producer to consumer round trip");
  std::cout << "background consumer lifecycle checks passed\n";
}
