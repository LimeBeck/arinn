// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick

// The same activation/state contract as ActionButton; text is its accessible
// name, while a feature may compose a focus caption outside the round plate.
ActionButton {
    iconOnly: true
    minimumWidth: 0
    leftPadding: 0
    rightPadding: 0
}
