// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "input_mode.h"

#include <QCursor>
#include <QEnterEvent>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QQmlEngine>

namespace {
void registerInputMode() {
  qmlRegisterType<InputModeSource>("Arinn.Shell", 1, 0, "InputModeSource");
}
Q_COREAPP_STARTUP_FUNCTION(registerInputMode)

// A modifier or lock key on its own changes no focus and types nothing.
bool modifierOnly(int key) {
  switch (key) {
  case Qt::Key_Shift:
  case Qt::Key_Control:
  case Qt::Key_Meta:
  case Qt::Key_Alt:
  case Qt::Key_AltGr:
  case Qt::Key_Super_L:
  case Qt::Key_Super_R:
  case Qt::Key_Hyper_L:
  case Qt::Key_Hyper_R:
  case Qt::Key_CapsLock:
  case Qt::Key_NumLock:
  case Qt::Key_ScrollLock:
    return true;
  default:
    return false;
  }
}
} // namespace

InputModeSource::InputModeSource(QObject *parent) : QObject(parent) {
  qApp->installEventFilter(this);
  updateCursor();
}

InputModeSource::~InputModeSource() {
  if (m_cursorHidden)
    QGuiApplication::restoreOverrideCursor();
}

void InputModeSource::setPointerActive(bool active) {
  if (m_pointerActive == active)
    return;
  m_pointerActive = active;
  updateCursor();
  emit pointerActiveChanged();
}

void InputModeSource::updateCursor() {
  if (m_pointerActive == !m_cursorHidden)
    return;
  m_cursorHidden = !m_pointerActive;
  if (m_cursorHidden)
    QGuiApplication::setOverrideCursor(QCursor(Qt::BlankCursor));
  else
    QGuiApplication::restoreOverrideCursor();
}

bool InputModeSource::eventFilter(QObject *watched, QEvent *event) {
  // Events reach a window first and its items afterwards; one count each.
  if (!watched->isWindowType())
    return false;
  switch (event->type()) {
  case QEvent::KeyPress:
    if (!modifierOnly(static_cast<QKeyEvent *>(event)->key()))
      emit keyPressed();
    break;
  case QEvent::MouseMove: {
    const auto position =
        static_cast<QMouseEvent *>(event)->globalPosition();
    const bool moved = m_hasPosition && position != m_position;
    m_position = position;
    m_hasPosition = true;
    if (moved)
      emit pointerMoved();
    break;
  }
  case QEvent::Enter:
    // The pointer arrived in a window; that is not movement in the shell.
    m_position = static_cast<QEnterEvent *>(event)->globalPosition();
    m_hasPosition = true;
    emit pointerArrived();
    break;
  case QEvent::Leave:
  case QEvent::Show:
  case QEvent::Hide:
  case QEvent::WindowActivate:
  case QEvent::WindowDeactivate:
    // A window shown or raised under a resting pointer may report its
    // position again; the next motion event only sets the reference.
    m_hasPosition = false;
    emit pointerArrived();
    break;
  default:
    break;
  }
  return false;
}
