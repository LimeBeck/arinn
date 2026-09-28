// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QVariantMap>
#include <QTimer>

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QQmlApplicationEngine engine;
  const auto args = app.arguments();
  const auto fixture = args.indexOf("--state");
  if (args.contains("--home")) {
    engine.setInitialProperties({
        {"fixture", fixture >= 0 && fixture + 1 < args.size() ? args.at(fixture + 1) : QString("populated")},
        {"showNavigation", args.contains("--nav")}});
  }
  engine.load(QUrl(args.contains("--home") ? "qrc:/gallery/HomeGallery.qml" : "qrc:/gallery/Gallery.qml"));
  if (engine.rootObjects().isEmpty())
    return 1;
  auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
  if (args.contains("--4k"))
    window->resize(3840, 2160);
  const auto capture = args.indexOf("--capture");
  if (capture >= 0 && capture + 1 < args.size()) {
    const auto path = args.at(capture + 1);
    QTimer::singleShot(1500, window, [window, path, &app]() {
      app.exit(window->grabWindow().save(path) ? 0 : 2);
    });
  }
  return app.exec();
}
