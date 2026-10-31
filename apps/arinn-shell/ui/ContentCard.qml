// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "artwork.js" as Artwork

Surface {
    id: root
    identityGlow: true
    property string subtitle: ""
    property url artwork: ""
    property bool running: false
    property bool showProgress: false
    property real progress: 0
    // Chips of further items ({id, letter, icon}), drawn overlapping in the
    // top-left corner instead of this card's own monogram (handoff H-1).
    property var stack: []
    readonly property bool artworkReady: picture.status === Image.Ready
    implicitWidth: 400
    implicitHeight: 225
    contentItem: Item {
        Rectangle {
            objectName: "cardPlate"
            anchors.fill: parent
            radius: root.cornerRadius
            color: "transparent"
            visible: picture.status !== Image.Ready
        }
        Image {
            id: picture
            objectName: "cardArtwork"
            anchors.fill: parent
            asynchronous: true
            fillMode: Image.PreserveAspectCrop
            source: root.artwork
            visible: false
            onStatusChanged: frame.syncSource()
        }
        Canvas {
            id: frame
            objectName: "cardArtworkCanvas"
            anchors.fill: parent
            // The URL the canvas holds decoded. A card lives across row
            // updates (#441), so the previous artwork is released when the
            // card shows another one.
            property string loadedSource: ""
            // Whether the last paint drew the artwork.
            property bool showsArtwork: false
            function syncSource() {
                const next = picture.status === Image.Ready ? picture.source.toString() : "";
                if (loadedSource !== next) {
                    if (loadedSource.length)
                        unloadImage(loadedSource);
                    loadedSource = next;
                }
                if (available && next.length && !isImageLoaded(next)
                        && !isImageLoading(next) && !isImageError(next))
                    loadImage(next);
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
        // Handoff 02: rgba(10,11,13) from .9 at the bottom to 0 at 70% height.
        Rectangle {
            objectName: "artworkScrim"
            anchors.fill: parent
            radius: root.cornerRadius
            visible: root.artworkReady
            gradient: Gradient {
                GradientStop { position: 0; color: "#000a0b0d" }
                GradientStop { position: 0.3; color: "#000a0b0d" }
                GradientStop { position: 0.55; color: "#660a0b0d" }
                GradientStop { position: 1; color: "#e60a0b0d" }
            }
        }
        Row {
            objectName: "cardStack"
            x: 26; y: 24
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
                    color: "#3a3c40"
                    border.width: 4; border.color: "#2a2c2f"
                    UiText {
                        anchors.centerIn: parent
                        visible: stackIcon.status !== Image.Ready
                        text: modelData.letter || ""
                        font.pixelSize: 28; font.weight: 700
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
            visible: !root.artworkReady && root.stack.length === 0
            anchors.right: parent.right; anchors.top: parent.top
            anchors.margins: 24
            width: 60; height: 60; radius: 18
            color: root.accentColor
            UiText { anchors.centerIn: parent; text: root.text.trim().slice(0, 1).toUpperCase(); role: "control" }
        }
        Rectangle {
            x: 24; y: 34; width: 10; height: 10; radius: 5
            color: Theme.ember; visible: root.running && !root.busy
        }
        UiText {
            x: root.running && !root.busy ? 44 : 24; y: 20
            width: parent.width - 108
            role: "caption"
            text: root.busy ? qsTr("Loading…") : root.running ? qsTr("Now running") : ""
            color: Theme.textSecondary
        }
        Column {
            x: 24
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 24
            width: parent.width - 48
            spacing: 8
            UiText { width: parent.width; text: root.text; role: "section"; maximumLineCount: 2; elide: Text.ElideRight }
            UiText {
                objectName: "cardSubtitle"
                width: parent.width; text: root.subtitle; maximumLineCount: 1; elide: Text.ElideRight
                // Media cards use the 24 sub style of handoff 02.
                role: root.artworkReady || root.showProgress ? "caption" : "meta"
                color: Theme.textSecondary
                visible: text.length > 0
            }
            ProgressTrack { objectName: "cardProgress"; width: parent.width; visible: root.showProgress; value: root.progress }
        }
    }
}
