// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pragma Singleton
import QtQuick

// The only place in the shell that calls forceActiveFocus() (ADR-0076).
//
// Qt moves active focus to the window's root item when forceActiveFocus()
// targets an item that is hidden or disabled, directly or through an
// ancestor. No key handler runs there, so the remote stops working. take()
// refuses such items: a screen that is not on display cannot take focus,
// and the screen on display keeps it.
QtObject {
    // Moves active focus to item when it can hold it. Returns whether it did.
    function take(item, reason) {
        // `visible` and `enabled` read the effective state, which includes
        // every ancestor.
        if (!item || !item.visible || !item.enabled)
            return false;
        if (reason === undefined)
            item.forceActiveFocus();
        else
            item.forceActiveFocus(reason);
        return true;
    }
}
