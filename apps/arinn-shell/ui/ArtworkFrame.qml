// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "artwork.js" as Artwork

// A presentation-only artwork slot. Feature overlays paint before the row
// shade; status overlays paint after it. Neither changes caption geometry.
Item {
    id: root
    property url source: ""
    property real radius: Theme.stillRadius
    property bool focused: false
    property bool pointerMode: false
    property bool alignStart: false
    property bool transformEnabled: true
    property real focusScale: Theme.contentFocusScale
    property real focusLift: Theme.contentFocusLift
    property bool decorationEnabled: true
    property real rowBrightness: 1
    property bool bounce: loaded
    property bool cache: true
    property bool imageVisible: true
    property color placeholderColor: Theme.background
    default property alias content: contentSlot.data
    property alias statusContent: statusSlot.data
    property alias placeholder: placeholderSlot.data
    readonly property alias image: picture
    readonly property alias canvas: artworkCanvas
    readonly property alias shade: rowShade
    readonly property alias decoration: focusDecoration
    readonly property alias plate: backing
    readonly property alias imageDisplay: imageDisplay
    readonly property bool imageReady: picture.status === Image.Ready
    readonly property bool loaded: imageReady && artworkCanvas.showsArtwork
        && artworkCanvas.loadedSource === picture.source.toString()
    readonly property bool ready: loaded
    readonly property int status: imageReady && !loaded ? Image.Loading : picture.status
    property real imageFocus: focused && transformEnabled ? 1 : 0
    readonly property real imageScale: 1 + (pointerMode ? (focusScale - 1) / 2
        : focusScale - 1) * imageFocus
    readonly property real imageLift: (pointerMode && focusLift < 0 ? -2 : focusLift) * imageFocus
    Behavior on imageFocus {
        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easing }
    }
    transform: [
        Scale {
            origin.x: root.alignStart ? 0 : root.width / 2
            origin.y: root.height
            xScale: root.imageScale; yScale: root.imageScale
        },
        Translate {
            x: root.alignStart ? (Theme.matThickness + Theme.rimThickness) * root.imageScale * root.imageFocus : 0
            y: root.imageLift
        }
    ]
    onRadiusChanged: artworkCanvas.requestPaint()
    Rectangle {
        id: backing
        anchors.fill: parent
        radius: root.radius
        color: root.placeholderColor
        visible: !root.loaded
    }
    Image {
        id: picture
        anchors.fill: parent
        source: root.source
        asynchronous: true
        cache: root.cache
        visible: false
        fillMode: Image.PreserveAspectCrop
        sourceSize: Artwork.sourceSize(source, width, height)
        onStatusChanged: artworkCanvas.syncSource()
        onSourceChanged: Qt.callLater(artworkCanvas.syncSource)
        onImplicitWidthChanged: Qt.callLater(artworkCanvas.syncSource)
        onImplicitHeightChanged: Qt.callLater(artworkCanvas.syncSource)
    }
    Item {
        id: placeholderSlot
        anchors.fill: parent
        visible: !root.loaded
    }
    Item {
        id: imageDisplay
        anchors.fill: parent
        visible: root.imageReady
        readonly property bool ready: root.loaded
        readonly property int status: root.status
        property alias source: root.source
        Canvas {
            id: artworkCanvas
            anchors.fill: parent
            visible: root.imageVisible
            property string loadedSource: ""
            property size loadedSize: Qt.size(0, 0)
            property bool showsArtwork: false
            function syncSource() {
                const ready = picture.status === Image.Ready;
                const next = ready ? picture.source.toString() : "";
                const size = ready ? Qt.size(picture.implicitWidth, picture.implicitHeight) : Qt.size(0, 0);
                if (loadedSource !== next || loadedSize.width !== size.width || loadedSize.height !== size.height) {
                    showsArtwork = false;
                    if (loadedSource.length)
                        unloadImage(loadedSource);
                    loadedSource = next;
                    loadedSize = size;
                }
                if (available && next.length && !isImageLoaded(next)
                        && !isImageLoading(next) && !isImageError(next))
                    Artwork.loadCopy(artworkCanvas, picture);
                requestPaint();
            }
            onAvailableChanged: syncSource()
            onImageLoaded: requestPaint()
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const url = loadedSource;
                if (!url.length || picture.status !== Image.Ready
                        || url !== picture.source.toString() || !isImageLoaded(url)) {
                    showsArtwork = false;
                    return;
                }
                Artwork.clipRounded(ctx, width, height, root.radius);
                showsArtwork = Artwork.drawCover(ctx, url, picture.implicitWidth,
                    picture.implicitHeight, width, height);
            }
        }
    }
    Item { id: contentSlot; anchors.fill: parent }
    Rectangle {
        id: rowShade
        objectName: "rowShade"
        anchors.fill: parent
        radius: root.radius
        color: "black"
        opacity: root.focused ? 0 : Math.max(0, Math.min(1, 1 - root.rowBrightness))
        Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
    }
    ArtworkFocus {
        id: focusDecoration
        shown: root.focused && root.decorationEnabled
        bounce: root.bounce
        artworkRadius: root.radius
    }
    Item { id: statusSlot; anchors.fill: parent; z: 31 }
}
