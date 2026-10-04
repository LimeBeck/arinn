// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "input_bindings.h"

#include <algorithm>

#include <QCoreApplication>
#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QKeySequence>
#include <QWindow>

namespace {
constexpr int kMaxEvdev = 0x2ff;
// Keys every screen already handles. They keep their own meaning: a
// binding never takes one over for another action.
bool builtIn(int key) {
  switch (key) {
  case Qt::Key_Up: case Qt::Key_Down: case Qt::Key_Left: case Qt::Key_Right:
  case Qt::Key_Return: case Qt::Key_Enter: case Qt::Key_Space:
  case Qt::Key_Escape: case Qt::Key_Backspace: case Qt::Key_Back:
  case Qt::Key_Home: case Qt::Key_Menu: case Qt::Key_Tab: case Qt::Key_Backtab:
  case Qt::Key_MediaPlay: case Qt::Key_MediaPause:
  case Qt::Key_MediaTogglePlayPause: case Qt::Key_MediaStop:
    return true;
  default:
    return false;
  }
}
// Qt's xcb and Wayland backends report the XKB keycode: evdev code + 8.
constexpr int kXkbOffset = 8;
} // namespace

InputBindings::InputBindings(QObject *parent) : QObject(parent) {
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this,
          [this](Qt::ApplicationState state) {
            if (state == Qt::ApplicationActive)
              reload();
          });
}

int InputBindings::canonicalKey(const QString &action) {
  if (action == QLatin1String("up")) return Qt::Key_Up;
  if (action == QLatin1String("down")) return Qt::Key_Down;
  if (action == QLatin1String("left")) return Qt::Key_Left;
  if (action == QLatin1String("right")) return Qt::Key_Right;
  if (action == QLatin1String("select")) return Qt::Key_Return;
  if (action == QLatin1String("back")) return Qt::Key_Escape;
  if (action == QLatin1String("home")) return Qt::Key_Home;
  if (action == QLatin1String("menu")) return Qt::Key_Menu;
  if (action == QLatin1String("play_pause")) return Qt::Key_MediaTogglePlayPause;
  return 0;
}

void InputBindings::setBindings(
    const QList<QPair<QString, QStringList>> &bindings) {
  QList<Rule> rules;
  for (const auto &[action, keys] : bindings) {
    const auto target = canonicalKey(action);
    if (!target)
      continue;
    for (const auto &name : keys) {
      Rule rule;
      rule.target = target;
      if (name.startsWith(QLatin1String("evdev:"))) {
        bool ok = false;
        const auto code = name.mid(6).toInt(&ok);
        if (!ok || code < 1 || code > kMaxEvdev)
          continue;
        rule.evdev = code;
      } else {
        const QKeySequence sequence(name, QKeySequence::PortableText);
        if (sequence.count() != 1)
          continue;
        const auto combination = sequence[0];
        // Named, known, non-text keys only, without modifiers, and never a
        // key that already has a meaning in the shell.
        if (combination.keyboardModifiers() != Qt::NoModifier ||
            combination.key() < Qt::Key_Escape ||
            combination.key() == Qt::Key_unknown || builtIn(combination.key()))
          continue;
        rule.key = combination.key();
      }
      rules.append(rule);
    }
  }
  // A key bound to two actions is ambiguous, whatever its spelling: drop it
  // from both.
  QList<Rule> unique;
  for (const auto &rule : rules) {
    const auto same = [&rule](const Rule &other) {
      return rule.evdev ? other.evdev == rule.evdev : other.key == rule.key;
    };
    const auto clash = std::any_of(rules.cbegin(), rules.cend(), [&](const Rule &other) {
      return same(other) && other.target != rule.target;
    });
    if (!clash && std::none_of(unique.cbegin(), unique.cend(), same))
      unique.append(rule);
  }
  m_rules = unique;
}

void InputBindings::reload() {
  auto call = QDBusMessage::createMethodCall(
      QStringLiteral("org.arinn.Daemon1"), QStringLiteral("/org/arinn/Daemon1"),
      QStringLiteral("org.arinn.Daemon1"), QStringLiteral("GetInputBindings"));
  const auto ticket = ++m_ticket;
  auto *watcher = new QDBusPendingCallWatcher(
      QDBusConnection::sessionBus().asyncCall(call, 3000), this);
  connect(watcher, &QDBusPendingCallWatcher::finished, this,
          [this, ticket](QDBusPendingCallWatcher *watcher) {
            watcher->deleteLater();
            const auto reply = watcher->reply();
            // A failed call keeps the current bindings; a newer call wins.
            if (ticket != m_ticket ||
                reply.type() != QDBusMessage::ReplyMessage ||
                reply.signature() != QLatin1String("a(sas)") ||
                reply.arguments().isEmpty())
              return;
            QList<QPair<QString, QStringList>> bindings;
            const auto argument =
                qvariant_cast<QDBusArgument>(reply.arguments().constFirst());
            argument.beginArray();
            while (!argument.atEnd()) {
              QString action;
              QStringList keys;
              argument.beginStructure();
              argument >> action >> keys;
              argument.endStructure();
              bindings.append({action, keys});
            }
            argument.endArray();
            setBindings(bindings);
          });
}

int InputBindings::targetFor(const QKeyEvent *event) const {
  // Text keys keep typing: a key that produces printable text is never
  // rebound, so text fields work whatever the configuration says.
  const auto text = event->text();
  if ((!text.isEmpty() && text.at(0).isPrint()) || builtIn(event->key()) ||
      (event->modifiers() & ~Qt::KeypadModifier) != Qt::NoModifier)
    return 0;
  const auto evdev = static_cast<int>(event->nativeScanCode()) - kXkbOffset;
  // A key can match by name and by code. Rules for two different actions
  // make it ambiguous: it stays unchanged, as for two spellings of a name.
  int target = 0;
  for (const auto &rule : m_rules) {
    if ((rule.evdev && rule.evdev == evdev) ||
        (rule.key && rule.key == event->key())) {
      if (target && target != rule.target)
        return 0;
      target = rule.target;
    }
  }
  return target;
}

bool InputBindings::eventFilter(QObject *watched, QEvent *event) {
  if (m_sending || m_rules.isEmpty() ||
      (event->type() != QEvent::KeyPress &&
       event->type() != QEvent::KeyRelease) ||
      !watched->isWindowType())
    return false;
  const auto *key = static_cast<QKeyEvent *>(event);
  const auto target = targetFor(key);
  if (!target || target == key->key())
    return false;
  QKeyEvent mapped(key->type(), target, Qt::NoModifier, QString(),
                   key->isAutoRepeat(), static_cast<quint16>(key->count()));
  m_sending = true;
  QCoreApplication::sendEvent(watched, &mapped);
  m_sending = false;
  return true;
}
