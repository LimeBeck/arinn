// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Surface {
    id: root
    identityGlow: true
    property string subtitle: ""
    property url artwork: ""
    property bool running: false
    property bool showProgress: false
    property real progress: 0
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
            onStatusChanged: {
                if (status === Image.Ready)
                    frame.loadImage(source);
                frame.requestPaint();
            }
        }
        Canvas {
            id: frame
            anchors.fill: parent
            onImageLoaded: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                if (picture.status !== Image.Ready || !isImageLoaded(picture.source))
                    return;
                const w = width, h = height, r = root.cornerRadius;
                ctx.beginPath();
                ctx.moveTo(r, 0);
                ctx.lineTo(w - r, 0); ctx.quadraticCurveTo(w, 0, w, r);
                ctx.lineTo(w, h - r); ctx.quadraticCurveTo(w, h, w - r, h);
                ctx.lineTo(r, h); ctx.quadraticCurveTo(0, h, 0, h - r);
                ctx.lineTo(0, r); ctx.quadraticCurveTo(0, 0, r, 0);
                ctx.closePath();
                ctx.clip();
                ctx.drawImage(picture.source, 0, 0, w, h);
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: root.cornerRadius
            visible: root.artworkReady
            gradient: Gradient {
                GradientStop { position: 0; color: "#10131416" }
                GradientStop { position: 1; color: "#ee0a0b0d" }
            }
        }
        Rectangle {
            visible: !root.artworkReady
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
            UiText { width: parent.width; text: root.subtitle; role: "meta"; maximumLineCount: 1; elide: Text.ElideRight; color: Theme.textSecondary }
            ProgressTrack { width: parent.width; visible: root.showProgress; value: root.progress }
        }
    }
}
