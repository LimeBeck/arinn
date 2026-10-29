// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#include "external_keyboard.h"
#include "keyboardpanel.h"
#include <QCoreApplication>
#include <QQmlEngine>
#include <QStringList>
#include <QUuid>
#include <QWindow>
#include <QtDBus>

namespace {
const QString service = QStringLiteral("org.arinn.Daemon1");
const QString path = QStringLiteral("/org/arinn/Daemon1");
const QString interface = QStringLiteral("org.arinn.ExternalKeyboard1");
const QStringList inputKeys = {QStringLiteral("up"),    QStringLiteral("down"),
                               QStringLiteral("left"),  QStringLiteral("right"),
                               QStringLiteral("select"), QStringLiteral("back")};
const QStringList pressKeys = {QStringLiteral("backspace"), QStringLiteral("return"),
                               QStringLiteral("left"), QStringLiteral("right")};
constexpr int maxTextBytes = 256;

bool validSession(const QString &session) {
  const QUuid uuid(session);
  return !uuid.isNull() && uuid.toString(QUuid::WithoutBraces) == session;
}
} // namespace

void registerExternalKeyboard() {
  qmlRegisterType<ExternalKeyboard>("Arinn.Shell", 1, 0, "ExternalKeyboard");
}
Q_COREAPP_STARTUP_FUNCTION(registerExternalKeyboard)

ExternalKeyboard::ExternalKeyboard(QObject *parent) : QObject(parent) {
  auto bus = QDBusConnection::sessionBus();
  // Bound to the daemon's well-known name: QtDBus delivers only signals of
  // its current owner.
  bus.connect(service, path, interface, QStringLiteral("Opened"), this,
              SLOT(opened(QString)));
  bus.connect(service, path, interface, QStringLiteral("Input"), this,
              SLOT(received(QString, QString)));
  bus.connect(service, path, interface, QStringLiteral("Closed"), this,
              SLOT(closed(QString)));
  auto watcher = new QDBusServiceWatcher(
      service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
  // A new or lost daemon owner ends any session of the old one.
  connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
          [this] { finish(); });
}

void ExternalKeyboard::opened(const QString &session) {
  if (!validSession(session) || session == m_session || m_ended.contains(session))
    return;
  m_session = session;
  emit openChanged();
}

void ExternalKeyboard::received(const QString &session, const QString &key) {
  if (session.isEmpty() || session != m_session || !inputKeys.contains(key))
    return;
  emit input(key);
}

void ExternalKeyboard::closed(const QString &session) {
  if (!session.isEmpty() && session == m_session) {
    finish();
    return;
  }
  // A Closed for another session is stale, or overtook its own Opened:
  // remember it so that a late Opened never shows the panel.
  if (validSession(session) && !m_ended.contains(session)) {
    m_ended.append(session);
    while (m_ended.size() > 8)
      m_ended.removeFirst();
  }
}

void ExternalKeyboard::finish() {
  if (m_session.isEmpty())
    return;
  m_session.clear();
  emit openChanged();
}

void ExternalKeyboard::call(const char *method, const QString &value) {
  const QString session = m_session;
  QDBusMessage message = QDBusMessage::createMethodCall(
      service, path, interface, QString::fromLatin1(method));
  message.setArguments(value.isNull() ? QVariantList{session}
                                      : QVariantList{session, value});
  auto watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(message, 3000), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, session](QDBusPendingCallWatcher *call) {
            const QDBusMessage reply = call->reply();
            call->deleteLater();
            // The session ended under the panel: hide it. Other errors
            // (a full queue, a slow daemon) lose only this key.
            if (reply.type() == QDBusMessage::ErrorMessage &&
                reply.errorName() == QLatin1String("org.arinn.Error1.Conflict") &&
                session == m_session)
              finish();
          });
}

void ExternalKeyboard::type(const QString &text) {
  if (m_session.isEmpty() || text.isEmpty() ||
      text.toUtf8().size() > maxTextBytes)
    return;
  for (const QChar c : text)
    if (c.category() == QChar::Other_Control)
      return;
  call("Type", text);
}

void ExternalKeyboard::press(const QString &key) {
  if (m_session.isEmpty() || !pressKeys.contains(key))
    return;
  call("Press", key);
}

void ExternalKeyboard::close() {
  if (m_session.isEmpty())
    return;
  call("Close", QString());
  finish();
}

void ExternalKeyboard::preparePanel(QWindow *window) {
  prepareKeyboardPanel(window);
}
