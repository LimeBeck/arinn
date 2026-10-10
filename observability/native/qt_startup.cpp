// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "telemetry.h"
#include "../../apps/arinn-shell/flow_trace.h"
#include <QCoreApplication>
#include <algorithm>
#include <QEvent>
#include <cstdio>
namespace {
QtMessageHandler previous = nullptr;
class Recorder final : public QObject {
public:
  using QObject::QObject;
  bool eventFilter(QObject *, QEvent *event) override {
    const char *operation = nullptr;
    switch (event->type()) {
    case QEvent::KeyPress: operation = "input.key"; break;
    case QEvent::MouseButtonPress: operation = "input.pointer"; break;
    case QEvent::FocusIn: operation = "focus.in"; break;
    case QEvent::FocusOut: operation = "focus.out"; break;
    case QEvent::Show: ArinnTelemetry::finishStartup(); operation = "window.show"; break;
    case QEvent::Hide: operation = "window.hide"; break;
    case QEvent::WindowActivate: operation = "window.activate"; break;
    case QEvent::WindowDeactivate: operation = "window.deactivate"; break;
    default: break;
    }
    if (operation) ArinnTelemetry::breadcrumb(operation);
    return false;
  }
};
void handler(QtMsgType type, const QMessageLogContext &context, const QString &message) {
  ArinnTelemetry::qtMessage(type, context, message);
  if (previous) previous(type, context, message);
  else {
    const auto formatted = qFormatLogMessage(type, context, message).toLocal8Bit();
    std::fprintf(stderr, "%s\n", formatted.constData());
  }
}
// Shell flows reach Sentry only through this hook (ADR-0093).
void recordFlow(const FlowTrace::Record &record) {
  ArinnTelemetry::FlowStep steps[FlowTrace::Flow::MaxSteps];
  const int count = std::clamp(record.stepCount, 0, FlowTrace::Flow::MaxSteps);
  for (int i = 0; i < count; ++i)
    steps[i] = {record.steps[i].name, record.steps[i].startMs, record.steps[i].endMs, record.steps[i].outcome};
  ArinnTelemetry::flow(record.flow, record.startedAtMs, record.durationMs, record.outcome, steps, count);
}
void shutdown() {
  FlowTrace::sink = nullptr;
  qInstallMessageHandler(previous);
  ArinnTelemetry::stop();
}
void startup() {
  if (!ArinnTelemetry::start()) return;
  auto *app = QCoreApplication::instance();
  app->installEventFilter(new Recorder(app));
  previous = qInstallMessageHandler(handler);
  FlowTrace::sink = recordFlow;
  qAddPostRoutine(shutdown);
  QObject::connect(QCoreApplication::instance(), &QCoreApplication::aboutToQuit,
                   [] { ArinnTelemetry::breadcrumb("about-to-quit"); });
}
}
Q_COREAPP_STARTUP_FUNCTION(startup)
