// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QList>
#include <QObject>
#include <QPair>
#include <QStringList>

class QKeyEvent;

// Extra keys for semantic actions (ADR-0079). The daemon reads them from
// config.toml; this filter turns a bound key into the action's canonical key
// before the shell window delivers it, so every screen keeps handling the
// keys it already knows. Keys that type text are never rebound.
class InputBindings : public QObject {
  Q_OBJECT
public:
  explicit InputBindings(QObject *parent = nullptr);
  // Ask the daemon again; also done whenever the application becomes active.
  void reload();
  // Replace the bindings directly: (action, keys) in the wire shape.
  void setBindings(const QList<QPair<QString, QStringList>> &bindings);
  // The canonical Qt key for an action, or 0 for an unknown action.
  static int canonicalKey(const QString &action);

protected:
  bool eventFilter(QObject *watched, QEvent *event) override;

private:
  struct Rule {
    int evdev = 0;  // Linux key code; matches nativeScanCode - 8.
    int key = 0;    // Qt key, when the binding names one.
    int target = 0; // Canonical key of the action.
  };
  int targetFor(const QKeyEvent *event) const;
  QList<Rule> m_rules;
  bool m_sending = false;
  quint64 m_ticket = 0;
};
