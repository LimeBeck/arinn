// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    shapeRole: "object"
    // Home favourites: a name on a shelf, a provider icon in a neutral chip,
    // and no letter (ADR-0136 D-3, D-10). Applications: the same object tile
    // with identity light from below (D-9).
    property bool shelf: false
    property bool identityLight: false
    property string monogram: text.trim().slice(0, 1).toUpperCase()
    property url iconSource: ""
    readonly property bool iconReady: iconFrame.pixelsReady
    readonly property bool shelfHasChip: shelf && iconSource.toString().length > 0
    onIconSourceChanged: if (iconFrame) iconFrame.syncSource()
    property string subtitle: ""
    property bool running: false
    property bool serviceOpen: false
    property bool unavailable: false
    implicitWidth: {
        if (!shelf)
            return identityLight ? 300 : 264;
        const textW = Math.min(360, shelfTitle.implicitWidth);
        const left = shelfHasChip ? 28 + 56 + 18 : 16;
        const dot = (running || serviceOpen) ? 18 + 12 : 0;
        return Math.ceil(left + textW + dot + 28);
    }
    implicitHeight: shelf ? 88 : identityLight ? 168 : 132
    // Objects rise from the bottom edge. A container tile takes the plate and
    // the rim without that lift. The shelf is a container that rises 4 px.
    focusScale: shelf || shapeRole === "container" ? 1 : Theme.contentFocusScale
    focusLift: shelf ? -4 : shapeRole === "container" ? 0 : Theme.contentFocusLift
    liftFromBottom: shelf || shapeRole !== "container"
    cornerRadius: shapeRole === "container" ? Theme.containerRadius : Theme.objectRadius
    // A shelf name recedes with its row. The container shade would draw a
    // rounded plate under that name; at rest there is none (ADR-0136 D-3).
    opacity: !enabled || unavailable ? 0.4 : shelf ? rowBrightness : 1
    Component.onCompleted: {
        if (!shelf)
            return;
        for (let i = 0; i < children.length; ++i)
            if (children[i].objectName === "rowShade")
                children[i].opacity = 0;
    }
    contentItem: Item {
        // Identity light: a radial glow from below the tile, half at rest and
        // full in focus. The hue is the application's, not a pastel chip.
        Canvas {
            id: identityGlow
            objectName: "identityGlow"
            anchors.fill: parent
            visible: root.identityLight
            opacity: root.activeFocus ? 1 : 0.5
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            onVisibleChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                if (width < 1 || height < 1)
                    return;
                const c = root.accentColor;
                const cx = width / 2;
                const cy = height * 1.28;
                const glow = ctx.createRadialGradient(cx, cy, 0, cx, cy, Math.max(width, height) * 0.95);
                glow.addColorStop(0, Qt.rgba(c.r, c.g, c.b, 0.8));
                glow.addColorStop(0.72, Qt.rgba(c.r, c.g, c.b, 0));
                ctx.beginPath();
                ctx.roundedRect(0, 0, width, height, root.cornerRadius, root.cornerRadius);
                ctx.clip();
                ctx.fillStyle = glow;
                ctx.fillRect(0, 0, width, height);
            }
            Connections {
                target: root
                function onAccentColorChanged() { identityGlow.requestPaint(); }
                function onActiveFocusChanged() { identityGlow.requestPaint(); }
            }
        }
        Rectangle {
            id: chip
            objectName: "applicationChip"
            visible: !root.shelf || root.shelfHasChip
            x: root.shelf ? 28 : root.identityLight ? 26 : 24
            y: root.shelf ? (parent.height - height) / 2 : root.identityLight ? 22 : (parent.height - height) / 2
            width: root.shelf ? 56 : root.identityLight ? 64 : 60
            height: width
            radius: root.identityLight || root.shelf ? 16 : 18
            color: root.iconReady ? Qt.rgba(1, 1, 1, 0.1)
                    : root.unavailable ? Qt.rgba(1, 1, 1, 0.06)
                    : (root.shelf || root.identityLight) ? Theme.chip
                    : root.accentColor
            border.width: (root.shelf || root.identityLight) && !root.iconReady ? 1 : 0
            border.color: Theme.chipLine
            Image {
                id: icon
                objectName: "applicationIcon"
                anchors.fill: parent
                sourceSize: Qt.size(128, 128)
                source: root.iconSource
                asynchronous: true
                fillMode: Image.PreserveAspectFit
                visible: false
                onStatusChanged: iconFrame.syncSource()
            }
            Canvas {
                id: iconFrame
                objectName: "applicationIconCanvas"
                property string requestedSource: ""
                property bool pixelsReady: false
                function updateReady() {
                    pixelsReady = available && requestedSource.length > 0
                        && requestedSource === root.iconSource.toString()
                        && requestedSource === icon.source.toString()
                        && icon.status === Image.Ready && isImageLoaded(requestedSource);
                    requestPaint();
                }
                function syncSource() {
                    const next = root.iconSource.toString();
                    if (requestedSource !== next) {
                        const previous = requestedSource;
                        requestedSource = next;
                        pixelsReady = false;
                        if (previous.length) unloadImage(previous);
                        requestPaint();
                    }
                    if (available && next.length && icon.source.toString() === next
                            && icon.status === Image.Ready && !isImageLoaded(next)
                            && !isImageLoading(next) && !isImageError(next))
                        loadImage(next);
                    updateReady();
                }
                onAvailableChanged: syncSource()
                // Rasterize at 2x for the 4K logical canvas, then fit the chip.
                width: parent.width * 2; height: parent.height * 2
                scale: 0.5
                transformOrigin: Item.TopLeft
                onImageLoaded: updateReady()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    if (!root.iconReady || !isImageLoaded(icon.source)) return;
                    ctx.scale(2, 2);
                    const w = parent.width, h = parent.height, r = parent.radius;
                    ctx.beginPath();
                    ctx.roundedRect(0, 0, w, h, r, r);
                    ctx.clip();
                    ctx.drawImage(icon.source, (w-icon.paintedWidth)/2,
                                  (h-icon.paintedHeight)/2,
                                  icon.paintedWidth, icon.paintedHeight);
                }
            }
            UiText {
                objectName: "applicationFallback"
                visible: !root.iconReady && !root.shelf
                anchors.centerIn: parent
                text: root.monogram
                role: "control"
                color: root.unavailable ? Theme.textHint
                        : root.shapeRole === "container" && !root.activeFocus ? Theme.textSecondary
                        : Theme.textPrimary
            }
        }
        Column {
            visible: !root.shelf && !root.identityLight
            x: 100; width: parent.width - 120
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            // Up to two lines at 26 px, so "Site or search" fits whole
            // (owner decision 2026-11-01, #656); Text.Wrap breaks a word
            // wider than the column instead of letting it overflow.
            UiText {
                objectName: (!root.shelf && !root.identityLight) ? "tileTitle" : ""
                width: parent.width
                role: "control"; font.pixelSize: 26
                lineHeight: 1.1
                text: root.busy ? qsTr("Loading…") : root.text
                maximumLineCount: 2
                elide: Text.ElideRight
                wrapMode: Text.Wrap
                color: root.unavailable ? Theme.textHint
                        : root.shapeRole === "container" && !root.activeFocus ? Theme.textSecondary
                        : Theme.textPrimary
            }
            UiText {
                width: parent.width
                visible: root.subtitle.length > 0
                text: root.subtitle
                role: "caption"
                color: Theme.textHint
                maximumLineCount: 1
                elide: Text.ElideRight
                wrapMode: Text.Wrap
            }
        }
        UiText {
            id: shelfTitle
            visible: root.shelf
            objectName: root.shelf ? "tileTitle" : ""
            x: root.shelfHasChip ? 28 + 56 + 18 : 16
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(360, implicitWidth)
            text: root.busy ? qsTr("Loading…") : root.text
            role: "control"
            font.pixelSize: 32
            maximumLineCount: 1
            elide: Text.ElideRight
            wrapMode: Text.NoWrap
            color: root.unavailable ? Theme.textHint
                    : root.activeFocus ? Theme.textPrimary : Theme.textSecondary
        }
        UiText {
            visible: root.identityLight && !root.shelf
            objectName: (root.identityLight && !root.shelf) ? "tileTitle" : ""
            x: 26
            width: parent.width - 52
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 18
            text: root.busy ? qsTr("Loading…") : root.text
            role: "control"
            font.pixelSize: 30
            maximumLineCount: 1
            elide: Text.ElideRight
            wrapMode: Text.NoWrap
            color: root.unavailable ? Theme.textHint : Theme.textPrimary
        }
        Rectangle {
            objectName: "emberDot"
            x: root.shelf ? shelfTitle.x + Math.min(360, shelfTitle.implicitWidth) + 18
                    : parent.width - width - (root.identityLight ? 24 : 16)
            y: root.shelf ? (parent.height - height) / 2 : root.identityLight ? 24 : 16
            width: 12; height: 12; radius: 6; color: Theme.ember
            visible: root.running || root.serviceOpen
        }
    }
}
