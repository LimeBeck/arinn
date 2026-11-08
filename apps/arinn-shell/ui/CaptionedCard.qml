// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

// Feature wrappers fill artwork/caption slots and route activation intents.
// Only the artwork transforms; the Surface hit area and caption stay fixed.
Surface {
    id: root
    shapeRole: "content"
    focusScale: 1
    focusLift: 0
    property real artworkHeight: 0
    property real defaultArtworkHeight: 225
    readonly property real frameHeight: artworkHeight > 0 ? artworkHeight : defaultArtworkHeight
    property real captionHeight: Theme.contentCaptionHeight
    property real captionGap: 18
    property alias artwork: frame.source
    property alias alignStart: frame.alignStart
    property alias artworkContent: frame.content
    property alias artworkStatus: frame.statusContent
    property alias caption: captionSlot.data
    readonly property alias artworkFrame: frame
    readonly property alias captionItem: captionSlot
    readonly property alias imageScale: frame.imageScale
    readonly property alias imageLift: frame.imageLift
    readonly property alias imageFocus: frame.imageFocus
    readonly property alias artworkReady: frame.imageReady
    implicitWidth: 400
    implicitHeight: frameHeight + captionHeight
    contentItem: Item {
        ArtworkFrame {
            id: frame
            objectName: "cardFrame"
            z: 1
            width: parent.width
            height: root.frameHeight
            radius: root.cornerRadius
            focused: root.activeFocus
            pointerMode: root.pointerMode
            rowBrightness: root.rowBrightness
        }
        Item {
            id: captionSlot
            objectName: "cardCaption"
            z: 1
            y: root.frameHeight + root.captionGap
            width: parent.width
            height: Math.max(0, root.captionHeight - root.captionGap)
        }
    }
}
