// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Effects
import "artwork.js" as Artwork

Surface {
    id: root
    shapeRole: "content"
    property string subtitle: ""
    property string statusText: ""
    readonly property bool focusWithin: activeFocus || secondaryAction.activeFocus
    property string secondaryIcon: ""
    property string secondaryText: ""
    property bool secondaryEnabled: true
    signal secondaryActivated()
    Keys.onRightPressed: event => {
        if (secondaryAction.visible && secondaryAction.enabled) Focus.take(secondaryAction);
        else event.accepted = false;
    }
    property url artwork: ""
    property bool running: false
    property bool showProgress: false
    property real progress: 0
    // Chips of further items ({id, letter, icon}), drawn overlapping in the
    // top-left corner instead of this card's own monogram (handoff H-1).
    property var stack: []
    // Hue of the cast under the image. Home passes the pool hue.
    // A playing or paused music session (variant 1 of the music card, brief
    // *Presence*, MH-2, #803): the blurred cover behind the centred cover,
    // the playing chip and the Ember line. Empty for any other card.
    property string musicState: ""
    // The source's mark in the neutral chip when there is no cover.
    property string musicSourceIcon: "music_note"
    readonly property bool musicFace: musicState.length > 0
    // The bars move only while playing; the shell has no reduced-motion
    // setting yet, and one would stop them here.
    property bool motionEnabled: true
    // Set by a row so the image stays 16:9 (or a poster) and the caption
    // sits below it. 0 uses the role's own image height.
    property real artworkHeight: 0
    readonly property bool artworkReady: picture.status === Image.Ready
    // A poster (Home's New row, 180 px wide) has a narrow text column.
    readonly property bool compact: width < 300
    readonly property real frameHeight: artworkHeight > 0 ? artworkHeight : (compact ? 270 : 225)
    readonly property real captionHeight: compact ? Theme.posterCaptionHeight : Theme.contentCaptionHeight
    readonly property real progressFraction: isFinite(progress) ? Math.max(0, Math.min(1, progress)) : 0
    implicitWidth: 400
    implicitHeight: frameHeight + captionHeight
    cornerRadius: compact ? Theme.posterRadius : Theme.stillRadius
    // Only the image lifts (ADR-0136): the caption keeps its place, so its
    // text never jumps when focus moves (#837).
    focusScale: 1
    focusLift: 0
    // Set by a row for its first card: the image grows from its left edge
    // and the rim stays inside the row's start, the safe edge (#837).
    property bool alignStart: false
    property real imageFocus: activeFocus ? 1 : 0
    Behavior on imageFocus {
        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easing }
    }
    readonly property real imageScale: 1 + (pointerMode ? (Theme.contentFocusScale - 1) / 2 : Theme.contentFocusScale - 1) * imageFocus
    readonly property real imageLift: (pointerMode ? -2 : Theme.contentFocusLift) * imageFocus
    contentItem: Item {
        Item {
            id: frame
            objectName: "cardFrame"
            z: 1
            width: parent.width
            height: root.frameHeight
            transform: [
                Scale {
                    origin.x: root.alignStart ? 0 : frame.width / 2
                    origin.y: frame.height
                    xScale: root.imageScale
                    yScale: root.imageScale
                },
                Translate {
                    // The mat and the rim, scaled with the image.
                    x: root.alignStart ? (Theme.matThickness + Theme.rimThickness) * root.imageScale * root.imageFocus : 0
                    y: root.imageLift
                }
            ]
            Rectangle {
                objectName: "cardPlate"
                anchors.fill: parent
                radius: root.cornerRadius
                color: Theme.background
                visible: picture.status !== Image.Ready
            }
            Image {
                id: picture
                objectName: "cardArtwork"
                anchors.fill: parent
                asynchronous: true
                fillMode: Image.PreserveAspectCrop
                // Decode only what the card covers, not the full poster (#611).
                sourceSize: Artwork.sourceSize(source, width, height)
                source: root.artwork
                visible: false
                onStatusChanged: frameCanvas.syncSource()
                // A cached image can stay Ready across a URL change without a
                // status change. Sync once the new source has started loading.
                onSourceChanged: Qt.callLater(frameCanvas.syncSource)
            }
            Canvas {
                id: frameCanvas
                objectName: "cardArtworkCanvas"
                anchors.fill: parent
                // Under the music face the blurred backdrop replaces the sharp
                // cover; the software renderer draws no shader effect, so it
                // keeps the sharp one (review of #827).
                visible: !root.musicFace || GraphicsInfo.api === GraphicsInfo.Software
                // The URL the canvas holds decoded. A card lives across row
                // updates (#441), so the previous artwork is released when the
                // card shows another one.
                property string loadedSource: ""
                // Whether the last paint drew the artwork.
                property bool showsArtwork: false
                // The canvas copy is decoded at the Image's decoded size, which
                // covers the card (#611), so the crop uses the same pixels.
                property size loadedSize: Qt.size(0, 0)
                function syncSource() {
                    const ready = picture.status === Image.Ready;
                    const next = ready ? picture.source.toString() : "";
                    const size = ready ? Qt.size(picture.implicitWidth, picture.implicitHeight) : Qt.size(0, 0);
                    if (loadedSource !== next || loadedSize.width !== size.width
                            || loadedSize.height !== size.height) {
                        if (loadedSource.length)
                            unloadImage(loadedSource);
                        loadedSource = next;
                        loadedSize = size;
                    }
                    if (available && next.length && !isImageLoaded(next)
                            && !isImageLoading(next) && !isImageError(next))
                        Artwork.loadCopy(frameCanvas, picture);
                    requestPaint();
                }
                onAvailableChanged: syncSource()
                onImageLoaded: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    const image = loadedSource;
                    if (!image.length || picture.status !== Image.Ready || !isImageLoaded(image)) {
                        showsArtwork = false;
                        return;
                    }
                    Artwork.clipRounded(ctx, width, height, root.cornerRadius);
                    // Centre crop, as Image.PreserveAspectCrop draws it (#609).
                    showsArtwork = Artwork.drawCover(ctx, image, picture.implicitWidth,
                                                     picture.implicitHeight, width, height);
                }
            }
            // A card without artwork, such as a web service, is an identity-
            // light tile as in Applications: a glow from below in its own
            // hue, half at rest and full in focus (#837).
            Canvas {
                id: identityGlow
                objectName: "cardIdentityGlow"
                anchors.fill: parent
                visible: !root.artworkReady && !root.musicFace
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
                }
            }
            // The chip, or the stack of chips, always sits top-left (#837).
            Row {
                id: chipStack
                objectName: "cardStack"
                x: 24; y: 24
                spacing: -18
                visible: root.stack.length > 0
                Repeater {
                    model: root.stack
                    Rectangle {
                        required property var modelData
                        required property int index
                        objectName: "stackChip-" + index
                        z: root.stack.length - index
                        width: 64; height: 64; radius: 20
                        color: Theme.chip
                        border.width: 1; border.color: Theme.chipLine
                        UiText {
                            anchors.centerIn: parent
                            visible: stackIcon.status !== Image.Ready
                            text: modelData.letter || ""
                            font.pixelSize: 28; font.weight: 600
                            color: Theme.textSecondary
                        }
                        Image {
                            id: stackIcon
                            anchors.fill: parent; anchors.margins: 8
                            source: modelData.icon || ""
                            sourceSize.width: 96; sourceSize.height: 96
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            visible: status === Image.Ready
                        }
                    }
                }
            }
            Rectangle {
                id: fallbackChip
                visible: !root.artworkReady && root.stack.length === 0 && !root.musicFace
                x: 24; y: 24
                width: 60; height: 60; radius: 18
                objectName: "cardChip"
                // Neutral chip, never a hashed pastel (brief *Monograms*, #756 C19).
                color: Theme.chip
                border.width: 1; border.color: Theme.chipLine
                UiText { anchors.centerIn: parent; text: root.text.trim().slice(0, 1).toUpperCase(); role: "control"; color: Theme.textSecondary }
            }
            // Variant 1 of the music card (mh-04, mh-05): the cover blurred
            // by 28 px with a 30–55 % Charcoal shade, the cover centred at
            // full height minus 22 px with a hairline and a cast shadow, the
            // playing chip at 16 px.
            Item {
                id: musicFaceItem
                objectName: "musicCardFace"
                anchors.fill: parent
                visible: root.musicFace
                // The card's rounded outline masks the blurred backdrop, so no
                // artwork shows past its corners (Codex review of #827).
                Item {
                    id: backdropMask
                    objectName: "musicCardBackdropMask"
                    anchors.fill: parent
                    visible: false
                    layer.enabled: root.musicFace
                    Rectangle { anchors.fill: parent; radius: root.cornerRadius; color: "white" }
                }
                MultiEffect {
                    objectName: "musicCardBackdrop"
                    anchors.fill: parent
                    source: picture
                    visible: root.artworkReady
                    blurEnabled: true
                    blurMax: 28
                    blur: 1
                    // No padding: the blur, its source and the r12 mask share
                    // the frame's rectangle, so nothing spills past the card
                    // (review of #827).
                    autoPaddingEnabled: false
                    maskEnabled: true
                    maskSource: backdropMask
                }
                Rectangle {
                    objectName: "musicCardShade"
                    anchors.fill: parent
                    radius: root.cornerRadius
                    gradient: Gradient {
                        GradientStop { position: 0; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0.3) }
                        GradientStop { position: 1; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0.55) }
                    }
                }
                Rectangle {
                    // The cast shadow under the cover; none under the source
                    // chip that stands in for a missing one (Codex review of
                    // #827).
                    objectName: "musicCardShadow"
                    visible: root.artworkReady
                    x: musicCover.x + 4; y: musicCover.y + 10
                    width: musicCover.width; height: musicCover.height
                    radius: 8
                    color: "black"
                    opacity: 0.35
                }
                Canvas {
                    id: musicCover
                    objectName: "musicCardCover"
                    width: parent.height - 44; height: width
                    x: (parent.width - width) / 2; y: 22
                    property string loadedSource: ""
                    function syncSource() {
                        const next = root.musicFace && picture.status === Image.Ready ? picture.source.toString() : "";
                        if (loadedSource !== next) {
                            if (loadedSource.length)
                                unloadImage(loadedSource);
                            loadedSource = next;
                        }
                        if (available && next.length && !isImageLoaded(next) && !isImageLoading(next) && !isImageError(next))
                            Artwork.loadCopy(musicCover, picture);
                        requestPaint();
                    }
                    Connections {
                        target: picture
                        function onStatusChanged() { musicCover.syncSource(); }
                        // A cached cover stays Ready across a new source
                        // (Codex review of #827).
                        function onSourceChanged() { Qt.callLater(musicCover.syncSource); }
                    }
                    Connections {
                        target: root
                        function onMusicFaceChanged() { musicCover.syncSource(); }
                    }
                    Component.onCompleted: syncSource()
                    onAvailableChanged: syncSource()
                    onImageLoaded: requestPaint()
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.reset();
                        if (!loadedSource.length || !isImageLoaded(loadedSource))
                            return;
                        Artwork.clipRounded(ctx, width, height, 8);
                        Artwork.drawCover(ctx, loadedSource, picture.implicitWidth, picture.implicitHeight, width, height);
                    }
                }
                // Without a cover (the station's included): the source's mark
                // in a neutral chip.
                Rectangle {
                    objectName: "musicCardFallback"
                    anchors.centerIn: musicCover
                    visible: !root.artworkReady
                    width: 96; height: 96; radius: 24
                    color: Theme.chip
                    border.width: 1; border.color: Theme.chipLine
                    LineIcon { objectName: "musicCardSourceIcon"; anchors.centerIn: parent; width: 48; height: 48; name: root.musicSourceIcon || "music_note"; color: Theme.textSecondary }
                }
                Rectangle {
                    objectName: "musicCardHairline"
                    anchors.fill: musicCover
                    visible: root.artworkReady
                    radius: 8
                    color: "transparent"
                    border.width: 1
                    border.color: Qt.rgba(1, 1, 1, 0.14)
                }
                // The playing chip: three Ember bars that move while playing,
                // and stand still in text-3 on pause.
                Rectangle {
                    objectName: "musicCardChip"
                    x: 16; y: 16
                    width: 56; height: 42; radius: 14
                    color: Theme.chip
                    border.width: 1; border.color: Theme.chipLine
                    Row {
                        anchors.centerIn: parent
                        spacing: 4
                        height: 20
                        Repeater {
                            objectName: "musicCardBars"
                            model: [{rest: 12, period: 1100}, {rest: 18, period: 900}, {rest: 9, period: 1300}]
                            delegate: Rectangle {
                                required property var modelData
                                objectName: "musicCardBar"
                                width: 5; radius: 2
                                anchors.bottom: parent.bottom
                                color: root.musicState === "playing" ? Theme.ember : Theme.textHint
                                height: modelData.rest
                                readonly property bool moving: barMotion.running
                                SequentialAnimation on height {
                                    id: barMotion
                                    running: root.musicState === "playing" && root.motionEnabled && root.visible
                                    loops: Animation.Infinite
                                    NumberAnimation { to: 20; duration: modelData.period / 2; easing.type: Easing.InOutQuad }
                                    NumberAnimation { to: 6; duration: modelData.period / 2; easing.type: Easing.InOutQuad }
                                    onStopped: height = modelData.rest
                                }
                            }
                        }
                    }
                }
            }
            ProgressTrack {
                objectName: "cardProgress"
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                line: true
                visible: root.showProgress
                value: root.progress
            }
            // The rest of a lit row is 0.82; other rows are 0.62. The caption
            // below the image is not shaded. A black veil at (1 − brightness)
            // is a multiply on the software renderer.
            Rectangle {
                objectName: "rowShade"
                anchors.fill: parent
                radius: root.cornerRadius
                color: "black"
                opacity: Math.max(0, 1 - root.rowBrightness)
                z: 8
                Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
            }
            // The warm bounce lights artwork; on a card without any it read
            // as a bright bar (#837).
            ArtworkFocus {
                shown: root.activeFocus
                bounce: root.artworkReady || root.musicFace
                artworkRadius: root.cornerRadius
            }
        }
        // No cast light under a captioned card: between the image and its
        // caption it read as a coloured bar, and the floor pool already
        // lights the focused card (#837).
        // Status text starts after the top-left chip or chip stack, never
        // across it (Codex review of #852).
        UiText {
            readonly property real markRight: chipStack.visible ? chipStack.x + chipStack.width
                : fallbackChip.visible ? fallbackChip.x + fallbackChip.width : 0
            x: markRight > 0 ? markRight + 16 : 24
            y: markRight > 0 ? 24 + (60 - height) / 2 : 20
            width: parent.width - x - 96
            objectName: "cardStatus"
            text: root.statusText; visible: text.length > 0 && !root.musicFace
            role: "caption"; textFormat: Text.PlainText; z: 2
        }
        Surface {
            id: secondaryAction; objectName: "cardSecondaryAction"
            x: parent.width - width - 20; y: root.frameHeight - height - 20; z: 3
            width: 72; height: 72; cornerRadius: 36
            // The music card's variant 1 has only the playing chip: Play/Pause
            // on Home acts on the session (Codex review of #827).
            visible: root.secondaryIcon.length > 0 && !root.musicFace
            focusedColor: Theme.textPrimary
            enabled: root.secondaryEnabled || activeFocus
            opacity: root.busy ? 0.6 : 1
            text: root.secondaryText
            contentItem: Item {
                LineIcon {anchors.centerIn:parent;width:32;height:32;name:root.secondaryIcon;color:secondaryAction.activeFocus ? Theme.ink : Theme.textPrimary}
            }
            onActivated: if(!root.busy && root.secondaryEnabled)root.secondaryActivated()
            Keys.onLeftPressed: Focus.take(root)
            onDpadRequested: root.dpadRequested()
        }
        Column {
            objectName: "cardCaption"
            z: 1
            x: 0
            y: root.frameHeight + (root.compact ? 14 : 18)
            width: parent.width
            spacing: 4
            // A running entry's Ember dot sits before its caption, not
            // floating on the image (#837).
            Row {
                width: parent.width
                spacing: 10
                Rectangle {
                    objectName: "cardRunningDot"
                    anchors.verticalCenter: parent.verticalCenter
                    width: 12; height: 12; radius: 6
                    color: Theme.ember
                    visible: root.running && !root.busy && !root.musicFace
                }
                UiText {
                    objectName: "cardTitle"
                    width: parent.width - (x > 0 ? x : 0)
                    textFormat: Text.PlainText
                    text: root.busy ? qsTr("Loading…") : root.text
                    role: "caption"
                    font.pixelSize: root.compact ? 24 : 28
                    font.weight: root.compact ? 500 : 600
                    color: root.compact && !root.activeFocus ? Theme.textSecondary : Theme.textPrimary
                    wrapMode: Text.NoWrap
                    maximumLineCount: 1
                    elide: Text.ElideRight
                }
            }
            UiText {
                objectName: "cardSubtitle"
                width: parent.width
                textFormat: Text.PlainText
                text: root.subtitle
                role: "caption"
                color: Theme.textSecondary
                wrapMode: Text.NoWrap
                maximumLineCount: 1
                elide: Text.ElideRight
                visible: text.length > 0
            }
        }
    }
}
