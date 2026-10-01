// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    identityGlow: true
    property string monogram: text.trim().slice(0, 1).toUpperCase()
    property url iconSource: ""
    readonly property bool iconReady: icon.status === Image.Ready
    property string subtitle: ""
    property bool running: false
    property bool serviceOpen: false
    property bool unavailable: false
    implicitWidth: 264
    implicitHeight: 132
    focusScale: 1.08
    opacity: !enabled || unavailable ? 0.4 : 1
    contentItem: Item {
        Rectangle {
            objectName: "applicationChip"
            x: 24; y: (parent.height - height) / 2
            width: 60; height: 60; radius: 18
            color: root.iconReady ? Qt.rgba(1, 1, 1, 0.1) : root.unavailable ? Qt.rgba(1, 1, 1, 0.06) : root.accentColor
            Image {
                id: icon
                objectName: "applicationIcon"
                anchors.fill: parent
                sourceSize: Qt.size(128, 128)
                source: root.iconSource
                asynchronous: true
                fillMode: Image.PreserveAspectFit
                visible: false
                onStatusChanged: {
                    if (status === Image.Ready) iconFrame.loadImage(source);
                    iconFrame.requestPaint();
                }
            }
            Canvas {
                id: iconFrame
                // Rasterize at 2x for the 4K logical canvas, then fit the chip.
                width: parent.width * 2; height: parent.height * 2
                scale: 0.5
                transformOrigin: Item.TopLeft
                onImageLoaded: requestPaint()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    if (!root.iconReady || !isImageLoaded(icon.source)) return;
                    ctx.scale(2, 2);
                    const w = parent.width, h = parent.height, r = parent.radius;
                    ctx.beginPath();
                    ctx.moveTo(r, 0);
                    ctx.lineTo(w-r, 0); ctx.quadraticCurveTo(w, 0, w, r);
                    ctx.lineTo(w, h-r); ctx.quadraticCurveTo(w, h, w-r, h);
                    ctx.lineTo(r, h); ctx.quadraticCurveTo(0, h, 0, h-r);
                    ctx.lineTo(0, r); ctx.quadraticCurveTo(0, 0, r, 0);
                    ctx.closePath(); ctx.clip();
                    ctx.drawImage(icon.source, (w-icon.paintedWidth)/2,
                                  (h-icon.paintedHeight)/2,
                                  icon.paintedWidth, icon.paintedHeight);
                }
            }
            UiText {
                objectName: "applicationFallback"
                visible: !root.iconReady
                anchors.centerIn: parent
                text: root.monogram
                role: "control"
                color: root.unavailable ? Theme.textHint : Theme.textPrimary
            }
        }
        Column {
            x: 100; width: parent.width - 120
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            UiText {
                width: parent.width
                role: "control"; font.pixelSize: 28
                text: root.busy ? qsTr("Loading…") : root.text
                maximumLineCount: root.subtitle.length ? 1 : 2
                elide: Text.ElideRight
                color: root.unavailable ? Theme.textHint : Theme.textPrimary
            }
            UiText {
                width: parent.width
                visible: root.subtitle.length > 0
                text: root.subtitle
                role: "caption"
                color: Theme.textHint
                maximumLineCount: 1
                elide: Text.ElideRight
            }
        }
        Rectangle {
            objectName: "emberDot"
            anchors.top: parent.top; anchors.right: parent.right; anchors.margins: 16
            width: 12; height: 12; radius: 6; color: Theme.ember
            visible: root.running || root.serviceOpen
        }
    }
}
