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
  QString qml = QStringLiteral("qrc:/gallery/Gallery.qml");
  if (args.contains("--browser")) {
    qml = QStringLiteral("qrc:/gallery/BrowserGallery.qml");
  } else if (args.contains("--now-playing")) {
    qml = QStringLiteral("qrc:/gallery/SessionGallery.qml");
  } else if (args.contains("--library")) {
    qml = QStringLiteral("qrc:/gallery/LibraryGallery.qml");
  } else if (args.contains("--settings")) {
    qml = QStringLiteral("qrc:/gallery/SettingsGallery.qml");
  } else if (args.contains("--home") || args.contains("--applications")) {
    qml = QStringLiteral("qrc:/gallery/HomeGallery.qml");
    engine.setInitialProperties({
        {"fixture", fixture >= 0 && fixture + 1 < args.size() ? args.at(fixture + 1) : QString("populated")},
        {"showNavigation", args.contains("--nav")},
        {"previewScreen", args.contains("--applications") ? QString("applications") : QString("home")}});
  }
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
