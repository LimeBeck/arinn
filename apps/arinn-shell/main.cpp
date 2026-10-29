// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "asset_images.h"
#include "backend.h"
#include "input_bindings.h"
#include "sessionlifecycle.h"
#include "shell_window.h"
#include "startup_home.h"
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QGuiApplication>
#include <QIcon>
#include <QLocale>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QScreen>
#include <QTranslator>
#include <cstdio>
int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  app.setApplicationName("arinn-shell");
  app.setDesktopFileName("org.arinn.Shell");
  // The interface follows the system locale; English strings are the source
  // and the fallback (#363).
  QTranslator translator;
  if (translator.load(QLocale(), QStringLiteral("arinn"), QStringLiteral("_"),
                      QStringLiteral(":/i18n")))
    app.installTranslator(&translator);
  // The desktop entry names the icon (#289); X11 also needs it on the window.
  app.setWindowIcon(QIcon::fromTheme(QStringLiteral("org.arinn.Shell"),
                                     QIcon(QStringLiteral(":/brand/arinn-mark.svg"))));
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
  Backend backend(nullptr, {QGuiApplication::platformName(), app.desktopFileName()});
  // Extra keys from config.toml (ADR-0079), applied before any screen.
  InputBindings bindings;
  app.installEventFilter(&bindings);
  bindings.reload();
  auto *sessionGuard = createSessionLifecycle(&app);
  bus.registerObject("/org/arinn/Shell1", &backend,
                     QDBusConnection::ExportAllProperties);
  bus.registerObject("/org/arinn/Session1", sessionGuard,
                     QDBusConnection::ExportAllProperties);
  configureShellWindows();
  QQmlApplicationEngine engine;
  installAssetImageProvider(&engine);
  engine.rootContext()->setContextProperty("backend", &backend);
  engine.rootContext()->setContextProperty("sessionGuard", sessionGuard);
  // The home folder for `~` in a Settings path (#479), from the environment
  // only: the shell never reads the file system itself (R-05).
  engine.rootContext()->setContextProperty("shellHomeFolder",
                                           qEnvironmentVariable("HOME"));
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
  createHiddenShellWindow(window);
  StartupHome startupHome(sessionGuard);
  return app.exec();
}
