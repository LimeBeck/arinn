// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "backend.h"
#include "sessionlifecycle.h"
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QScreen>
#include <cstdio>
int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  app.setApplicationName("arinn-shell");
  app.setDesktopFileName("org.arinn.Shell");
  auto bus = QDBusConnection::sessionBus();
  if (!bus.registerService("org.arinn.Shell1")) {
    // Exit status 2 means "another shell owns the name": the systemd unit
    // does not restart on it (RestartPreventExitStatus), so a leftover
    // instance cannot burn the start limit.
    const auto owner =
        bus.interface()->servicePid("org.arinn.Shell1").value();
    // Plain stderr, not qCritical: Qt may route its logging to journald
    // when stderr is not a terminal. systemd captures stderr either way.
    std::fprintf(stderr,
                 "arinn-shell: org.arinn.Shell1 is already owned%s; another "
                 "Arinn shell is running. Stop it before starting this one.\n",
                 owner ? qPrintable(QString(" by PID %1").arg(owner)) : "");
    return 2;
  }
  Backend backend;
  auto *sessionGuard = createSessionLifecycle(&app);
  bus.registerObject("/org/arinn/Shell1", &backend,
                     QDBusConnection::ExportAllProperties);
  bus.registerObject("/org/arinn/Session1", sessionGuard,
                     QDBusConnection::ExportAllProperties);
  QQmlApplicationEngine engine;
  engine.rootContext()->setContextProperty("backend", &backend);
  engine.rootContext()->setContextProperty("sessionGuard", sessionGuard);
  engine.load(QUrl("qrc:/Main.qml"));
  if (engine.rootObjects().isEmpty())
    return 1;
  auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
  QObject::connect(&backend, &Backend::screenRequested, window,
                   [window](const QString &name) {
                     for (auto screen : QGuiApplication::screens()) {
                       if (screen->name() == name &&
                           window->screen() != screen) {
                         window->setScreen(screen);
                         break;
                       }
                     }
                   });
  return app.exec();
}
