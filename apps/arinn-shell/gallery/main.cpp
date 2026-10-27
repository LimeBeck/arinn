// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QStringList>
#include <QTimer>

#include <cstdio>

int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QQmlApplicationEngine engine;
  const auto args = app.arguments();
  // Screen previews were removed (#567); fail loudly instead of silently
  // showing the component catalogue.
  static const QStringList removed = {
      "--home", "--applications", "--nav", "--state",
      "--browser", "--now-playing", "--library", "--settings"};
  for (int i = 1; i < args.size(); ++i) {
    const auto &arg = args.at(i);
    if (arg == "--keyboard" || arg == "--4k")
      continue;
    if (arg == "--capture") {
      if (i + 1 >= args.size()) {
        std::fprintf(stderr, "arinn-ui-gallery: --capture needs a PNG path\n");
        return 2;
      }
      ++i;
      continue;
    }
    if (removed.contains(arg)) {
      std::fprintf(stderr,
                   "arinn-ui-gallery: %s was removed; the gallery shows components only. "
                   "Screens and their states are captured from tests/ui/visual with "
                   "scripts/ui-compare.py.\n",
                   qPrintable(arg));
      return 2;
    }
    std::fprintf(stderr,
                 "arinn-ui-gallery: unknown option %s\n"
                 "usage: arinn-ui-gallery [--keyboard] [--4k] [--capture FILE.png]\n",
                 qPrintable(arg));
    return 2;
  }
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
