// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Text {
    // Named roles keep action labels at least 26 logical pixels.
    property string role: "body"
    readonly property var typography: ({
        display: [112, 700, -3.36], hero: [84, 700, -2.1],
        title: [56, 700, -1.12], section: [30, 600, 0],
        body: [30, 400, 0], control: [30, 600, 0],
        meta: [26, 500, 0], eyebrow: [24, 600, 1.92], caption: [24, 500, 0]
    })
    readonly property var metrics: typography[role] || typography.body
    font.family: Theme.fontFamily
    font.pixelSize: metrics[0]
    font.weight: metrics[1]
    font.letterSpacing: metrics[2]
    font.features: ({ "tnum": 1 })
    color: Theme.textPrimary
    wrapMode: Text.WordWrap
}
