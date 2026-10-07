// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick

// The shell's one input mode (spec §6.1, ADR-0095).
//
// D-pad mode hides the cursor; pointer mode shows it and lets hover move
// focus. A key press enters D-pad mode and starts a cooldown during which
// pointer motion cannot leave it, so an air mouse resting in the hand does
// not take focus from the remote. After the cooldown the cursor stays hidden
// until the pointer genuinely moves; time alone never reveals it.
//
// Screens bind their pointerMode to pointerActive and report input here.
// In the shell, InputModeSource (input_mode.h) reports every key press and
// pointer movement in its windows, including keys a control consumes.
QtObject {
    id: root

    // Pointer mode. False is D-pad mode.
    property bool pointerActive: false
    // How long after a key press pointer motion is ignored.
    property int keyCooldownMs: 2000
    readonly property bool cooldownActive: cooldown.running
    property point pointerPosition
    property bool hasPointerPosition: false

    // A key press in the shell: D-pad mode, and the cooldown restarts.
    function keyPressed() {
        pointerActive = false;
        cooldown.restart();
    }

    // Pointer motion. With a scene position, only a position that differs
    // from the last reported one is movement: hover re-evaluated under a
    // resting pointer (scrolling, animations, model refreshes, a screen
    // shown under it) reports the same point. The first position after
    // forgetPointer() only sets the reference. Without a position the
    // caller has already established movement. Returns whether pointer mode
    // is active afterwards.
    function pointerMoved(position) {
        if (position !== undefined) {
            const known = hasPointerPosition;
            const same = known && position.x === pointerPosition.x && position.y === pointerPosition.y;
            pointerPosition = position;
            hasPointerPosition = true;
            if (!known || same)
                return pointerActive;
        }
        if (!cooldown.running)
            pointerActive = true;
        return pointerActive;
    }

    // The pointer entered a window, or a window appeared or was activated
    // under it: its next position is a reference, not movement.
    function forgetPointer() {
        hasPointerPosition = false;
    }

    // D-pad mode without a cooldown, for test fixtures.
    function reset() {
        cooldown.stop();
        pointerActive = false;
    }

    property Timer cooldown: Timer {
        interval: root.keyCooldownMs
    }
}
