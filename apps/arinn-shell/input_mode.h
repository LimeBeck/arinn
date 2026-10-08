// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QObject>
#include <QPointF>

class QEvent;

// Reports shell input to the QML InputMode singleton and shows or hides the
// shell's cursor (spec §6.1, ADR-0095).
//
// An application event filter sees every key press sent to a shell window,
// including keys a control consumes, and every pointer movement. It never
// consumes an event. Movement counts only when the pointer position changed
// since the last event in a window; entering a window, or a window that
// appears under a resting pointer, sets the reference position without
// counting. The cursor is hidden with an application override cursor, so it
// affects only Arinn's own windows.
class InputModeSource : public QObject {
  Q_OBJECT
  // Pointer mode: the cursor is visible. Main.qml binds InputMode to it.
  Q_PROPERTY(bool pointerActive READ pointerActive WRITE setPointerActive
                 NOTIFY pointerActiveChanged)
public:
  explicit InputModeSource(QObject *parent = nullptr);
  ~InputModeSource() override;
  bool pointerActive() const { return m_pointerActive; }
  void setPointerActive(bool active);
  // Whether the override cursor currently hides the pointer.
  bool cursorHidden() const { return m_cursorHidden; }

signals:
  void keyPressed();
  void pointerMoved();
  // The pointer entered a window, or a window appeared or was activated
  // under it; its next position is a reference, not movement.
  void pointerArrived();
  void pointerActiveChanged();

protected:
  bool eventFilter(QObject *watched, QEvent *event) override;

private:
  void updateCursor();
  bool m_pointerActive = false;
  bool m_cursorHidden = false;
  bool m_hasPosition = false;
  QPointF m_position;
};
