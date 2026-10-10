// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// #433 spike: a fullscreen window that hosts player surfaces through a
// private nested Wayland compositor.
#include <QGuiApplication>
#include <QQmlApplicationEngine>

int main(int argc, char *argv[]) {
  // Client buffers: dmabuf first (zero copy for hardware-decoded video),
  // wl_egl as the fallback. Must be set before the compositor starts.
  if (qEnvironmentVariableIsEmpty("QT_WAYLAND_HARDWARE_INTEGRATION"))
    qputenv("QT_WAYLAND_HARDWARE_INTEGRATION", "linux-dmabuf-v1;wayland-egl");
  QGuiApplication app(argc, argv);
  QQmlApplicationEngine engine;
  engine.loadFromModule("Spike", "Main");
  return engine.rootObjects().isEmpty() ? 1 : app.exec();
}
