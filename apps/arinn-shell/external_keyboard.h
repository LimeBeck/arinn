// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QString>

class QWindow;

// The shell's side of the global on-screen keyboard (ADR-0105,
// docs/architecture/external-keyboard.md): org.arinn.ExternalKeyboard1.
//
// The daemon opens a session when the keyboard key is pressed over an
// application; the shell shows the panel while `open`. The platform routes
// the remote's panel keys here (`input`), and every typed key goes back to
// the daemon (`type`, `press`), which hands it to the trusted platform
// deliverer. The shell never sends input to an application, never logs or
// keeps the text, and calls never block the UI thread.
class ExternalKeyboard : public QObject {
  Q_OBJECT
  Q_PROPERTY(bool open READ open NOTIFY openChanged)
public:
  explicit ExternalKeyboard(QObject *parent = nullptr);
  bool open() const { return !m_session.isEmpty(); }
  // Text the panel typed: 1–256 UTF-8 bytes without control characters.
  Q_INVOKABLE void type(const QString &text);
  // "backspace", "return", "left" or "right".
  Q_INVOKABLE void press(const QString &key);
  // The panel was hidden in the shell (Back, Home, lock).
  Q_INVOKABLE void close();
  // Keeps the panel window from ever taking focus (platform layer).
  Q_INVOKABLE void preparePanel(QWindow *window);
signals:
  void openChanged();
  // A panel key: "up", "down", "left", "right", "select" or "back".
  void input(const QString &key);
private slots:
  void opened(const QString &session);
  void received(const QString &session, const QString &key);
  void closed(const QString &session);

private:
  void call(const char *method, const QString &value);
  void finish();
  QString m_session;
  // Sessions the daemon closed before this client saw them open: a late
  // Opened for one of them is ignored (the daemon also sends Opened before
  // Closed). Session IDs are fresh UUIDs; the last few are enough.
  QStringList m_ended;
};
