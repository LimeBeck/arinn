// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QTimer>

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QQmlApplicationEngine engine;
  const auto args = app.arguments();
  // Component catalogue only. Screens and their states are captured from the
  // fixtures in tests/ui/visual (scripts/ui-compare.py, #567).
  const QString qml = args.contains("--keyboard")
      ? QStringLiteral("qrc:/gallery/KeyboardGallery.qml")
      : QStringLiteral("qrc:/gallery/Gallery.qml");
  engine.load(QUrl(qml));
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
