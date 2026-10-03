// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
// Test-only browser provider. Never connects to an application or compositor.
#include "browse_catalog.h"
#include <QCoreApplication>
#include <QFile>
#include <QTimer>
#include <QtDBus>
struct Registration {
  QString plugin, provider;
  uint major = 1, minor = 0;
  QString version;
  QStringList capabilities{"browser"};
  QDBusObjectPath path{"/org/arinn/Provider1"};
  QString token;
};
struct Window {
  QString id, token;
  bool focused;
};
struct Tab {
  QString id, window, title, url;
  bool active, audible;
};
struct Snapshot {
  QString generation;
};
Q_DECLARE_METATYPE(Registration)
Q_DECLARE_METATYPE(Window)
Q_DECLARE_METATYPE(Tab)
Q_DECLARE_METATYPE(Snapshot)
QDBusArgument &operator<<(QDBusArgument &a, const Registration &v) {
  a.beginStructure();
  a << v.plugin << v.provider << v.major << v.minor << v.version
    << v.capabilities << v.path << v.token;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, Registration &) {
  return a;
}
QDBusArgument &operator<<(QDBusArgument &a, const Window &v) {
  a.beginStructure();
  a << v.id << v.token << v.focused;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, Window &) { return a; }
QDBusArgument &operator<<(QDBusArgument &a, const Tab &v) {
  a.beginStructure();
  a << v.id << v.window << v.title << v.url << v.active << v.audible;
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, Tab &) { return a; }
QDBusArgument &operator<<(QDBusArgument &a, const Snapshot &v) {
  a.beginStructure();
  a << v.generation << qulonglong(1) << QList<Window>{} << QList<Tab>{}
    << QString();
  a.endStructure();
  return a;
}
const QDBusArgument &operator>>(const QDBusArgument &a, Snapshot &) {
  return a;
}
class Provider : public QDBusVirtualObject {
public:
  QString generation;
  QString introspect(const QString &) const override { return {}; }
  bool handleMessage(const QDBusMessage &m,
                     const QDBusConnection &bus) override {
    if (m.member() == "BackendState")
      bus.send(m.createReply(QString("available")));
    else if (m.member() == "GetTabs")
      bus.send(m.createReply(QVariant::fromValue(Snapshot{generation})));
    else if (m.member() == "OpenUrl") {
      if (m.arguments().at(0).toString() != generation) {
        bus.send(m.createErrorReply("org.arinn.Error.Conflict",
                                    "Stale fixture generation"));
        return true;
      }
      QFile output(qEnvironmentVariable("ARINN_PROVIDER_DATA_DIR") +
                   "/received-url");
      if (!output.open(QIODevice::WriteOnly))
        return false;
      output.write(m.arguments().at(1).toString().toUtf8());
      output.close();
      bus.send(m.createReply());
    } else
      bus.send(m.createErrorReply("org.arinn.Error.Unsupported",
                                  "Fixture method unsupported"));
    return true;
  }
};
int main(int argc, char **argv) {
  QCoreApplication app(argc, argv);
  if (argc > 1 && QString::fromLocal8Bit(argv[1]) == "--client") {
    BrowseCatalog client;
    bool sent = false;
    QObject::connect(&client, &BrowseCatalog::changed, &app, [&] {
      if (!sent && client.kind() == "address") {
        sent = true;
        client.open();
      } else if (client.operationState() == "succeeded")
        app.exit(0);
      else if (sent && !client.operationState().isEmpty() &&
               client.operationState() != "running")
        app.exit(1);
    });
    QTimer::singleShot(10000, &app, [&] { app.exit(2); });
    client.setText("pages.example/path");
    return app.exec();
  }
  qDBusRegisterMetaType<Registration>();
  qDBusRegisterMetaType<Window>();
  qDBusRegisterMetaType<Tab>();
  qDBusRegisterMetaType<QList<Window>>();
  qDBusRegisterMetaType<QList<Tab>>();
  qDBusRegisterMetaType<Snapshot>();
  Provider provider;
  auto bus = QDBusConnection::sessionBus();
  if (!bus.registerVirtualObject("/org/arinn/Provider1", &provider))
    return 1;
  QTimer registration;
  QObject::connect(&registration, &QTimer::timeout, &app, [&] {
    if (!provider.generation.isEmpty())
      return;
    QFile token(qEnvironmentVariable("ARINN_TOKEN_FILE"));
    if (!token.open(QIODevice::ReadOnly))
      return;
    Registration r;
    r.plugin = qEnvironmentVariable("ARINN_PLUGIN_ID");
    r.provider = qEnvironmentVariable("ARINN_PROVIDER_ID");
    r.version = qEnvironmentVariable("ARINN_PLUGIN_VERSION");
    r.token = QString::fromUtf8(token.readAll());
    auto message = QDBusMessage::createMethodCall(
        "org.arinn.Daemon1", "/org/arinn/Daemon1", "org.arinn.Daemon1",
        "Register");
    message << QVariant::fromValue(r);
    const auto reply = bus.call(message, QDBus::Block, 3000);
    if (reply.signature() != "(ssuuuu)" ||
        reply.type() == QDBusMessage::ErrorMessage)
      return;
    const auto arg = qvariant_cast<QDBusArgument>(reply.arguments().at(0));
    QString epoch;
    arg.beginStructure();
    arg >> epoch >> provider.generation;
  });
  registration.start(100);
  return app.exec();
}
