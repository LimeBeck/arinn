// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "input_mode.h"

#include <QEnterEvent>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QSignalSpy>
#include <QTest>
#include <QWindow>

// InputModeSource reports the shell's key presses and genuine pointer
// movement and hides the cursor outside pointer mode (#308, ADR-0095).
class InputModeTest : public QObject {
  Q_OBJECT

  static void key(QObject *target, int code) {
    QKeyEvent press(QEvent::KeyPress, code, Qt::NoModifier);
    QCoreApplication::sendEvent(target, &press);
    QKeyEvent release(QEvent::KeyRelease, code, Qt::NoModifier);
    QCoreApplication::sendEvent(target, &release);
  }
  static void move(QObject *target, QPointF global) {
    QMouseEvent event(QEvent::MouseMove, global, global, global, Qt::NoButton,
                      Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(target, &event);
  }
  static void enter(QObject *target, QPointF global) {
    QEnterEvent event(global, global, global);
    QCoreApplication::sendEvent(target, &event);
  }

private slots:
  void everyKeyPressInAWindowCounts() {
    QWindow window;
    InputModeSource source;
    QSignalSpy keys(&source, &InputModeSource::keyPressed);
    for (int code : {int(Qt::Key_Down), int(Qt::Key_Return), int(Qt::Key_A),
                     int(Qt::Key_Escape), 0})
      key(&window, code);
    QCOMPARE(keys.count(), 5);
    // Modifiers and locks alone change nothing.
    for (int code : {int(Qt::Key_Shift), int(Qt::Key_Control),
                     int(Qt::Key_Meta), int(Qt::Key_CapsLock)})
      key(&window, code);
    QCOMPARE(keys.count(), 5);
    // Items receive the same event after the window: one count only.
    QObject item;
    key(&item, Qt::Key_Down);
    QCOMPARE(keys.count(), 5);
  }

  void onlyAChangedPositionIsMovement() {
    QWindow window;
    InputModeSource source;
    QSignalSpy moves(&source, &InputModeSource::pointerMoved);
    QSignalSpy arrivals(&source, &InputModeSource::pointerArrived);
    // The first event only sets the reference position.
    move(&window, {10, 10});
    QCOMPARE(moves.count(), 0);
    // A repeated position, as a window or hover update can report, is not.
    move(&window, {10, 10});
    QCOMPARE(moves.count(), 0);
    move(&window, {11, 10});
    QCOMPARE(moves.count(), 1);
    // Showing a window under a resting pointer resets the reference.
    QEvent show(QEvent::Show);
    QCoreApplication::sendEvent(&window, &show);
    QCOMPARE(arrivals.count(), 1);
    move(&window, {200, 200});
    QCOMPARE(moves.count(), 1);
    move(&window, {201, 200});
    QCOMPARE(moves.count(), 2);
    // Entering a window is arrival, not movement.
    enter(&window, {500, 500});
    QCOMPARE(arrivals.count(), 2);
    move(&window, {500, 500});
    QCOMPARE(moves.count(), 2);
    move(&window, {502, 500});
    QCOMPARE(moves.count(), 3);
  }

  void cursorIsHiddenOutsidePointerMode() {
    QVERIFY(!QGuiApplication::overrideCursor());
    {
      InputModeSource source;
      QVERIFY(source.cursorHidden());
      QCOMPARE(QGuiApplication::overrideCursor()->shape(), Qt::BlankCursor);
      source.setPointerActive(true);
      QVERIFY(!source.cursorHidden());
      QVERIFY(!QGuiApplication::overrideCursor());
      source.setPointerActive(false);
      QCOMPARE(QGuiApplication::overrideCursor()->shape(), Qt::BlankCursor);
    }
    // Destroying the source restores the cursor it hid.
    QVERIFY(!QGuiApplication::overrideCursor());
  }

  void eventsAreNeverConsumed() {
    struct Window : QWindow {
      int keys = 0, moves = 0;
      void keyPressEvent(QKeyEvent *) override { ++keys; }
      void mouseMoveEvent(QMouseEvent *) override { ++moves; }
    } window;
    InputModeSource source;
    key(&window, Qt::Key_Down);
    move(&window, {1, 1});
    move(&window, {2, 1});
    QCOMPARE(window.keys, 1);
    QCOMPARE(window.moves, 2);
  }
};

QTEST_MAIN(InputModeTest)
#include "input_mode_test.moc"
