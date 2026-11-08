// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Effects
import "../.." as Ui

// The album, playlist or artist detail opened from the room (brief mh-02,
// #801), built like the movie detail: the blurred cover as backdrop with a
// left scrim, the cover, an eyebrow that names the source in words, the
// title in Playfair Display and the actions as pills by capability. Opening
// it never plays. There is no Shuffle: the protocol has no shuffle action.
Item {
    id: root
    property bool pointerMode: Ui.InputMode.pointerActive
    property bool hoverFocusEnabled: true
    // The room entry the detail was opened from.
    property var entry: ({})
    // The tracks the detail lists, for the length in the meta line.
    property var tracks: []
    property bool canListen: false
    property bool canRadio: false
    readonly property string kind: entry.kind || ""
    readonly property Item firstPill: listen.visible ? listen : radio.visible ? radio : null
    readonly property Item lastPill: radio.visible ? radio : listen.visible ? listen : null
    readonly property real pillsEnd: pills.x + pills.width
    readonly property real pillsY: pills.y
    // Where the tracks may start: below the pills, which move down under a
    // title on three lines (review of #819).
    readonly property real contentBottom: pills.y + pills.height
    // The control right of the last pill (the playlist actions), so the
    // pills' line is one row for the D-pad (review of #819).
    property Item rightNeighbour: null
    signal listenRequested()
    signal radioRequested()
    signal downRequested()
    anchors.fill: parent
    function kindName() {
        return kind === "album" ? qsTr("Album") : kind === "artist" ? qsTr("Artist") : qsTr("Playlist");
    }
    function sourceName() {
        return entry.local === true ? qsTr("This computer") : (entry.providerName || "");
    }
    function length() {
        const count = tracks.filter(track => track.kind === "track").length;
        let total = 0;
        for (const track of tracks)
            total += Math.max(0, track.duration_us || 0);
        const minutes = Math.round(total / 60000000);
        const parts = [];
        if (count === 1)
            parts.push(qsTr("1 track"));
        else if (count)
            parts.push(qsTr("%n tracks", "", count));
        if (minutes >= 60)
            parts.push(qsTr("%1 h %2 min").arg(Math.floor(minutes / 60)).arg(minutes % 60));
        else if (minutes > 0)
            parts.push(qsTr("%1 min").arg(minutes));
        return parts.join(" · ");
    }
    Image {
        id: backdropSource
        anchors.fill: parent
        source: root.entry.image || ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        visible: false
        sourceSize: Qt.size(480, 270)
    }
    MultiEffect {
        objectName: "musicDetailBackdrop"
        anchors.fill: parent
        source: backdropSource
        visible: backdropSource.status === Image.Ready
        blurEnabled: true
        blur: 1
        blurMax: 64
        opacity: 0.45
    }
    // The left scrim keeps the text on its ground.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Ui.Theme.background }
            GradientStop { position: 0.55; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.85) }
            GradientStop { position: 1; color: Qt.rgba(Ui.Theme.background.r, Ui.Theme.background.g, Ui.Theme.background.b, 0.6) }
        }
    }
    MusicArtwork {
        objectName: "musicDetailCover"
        x: Ui.Theme.safeLeft
        y: 120
        width: 420
        height: 420
        radius: root.kind === "artist" ? 210 : Ui.Theme.stillRadius
        source: root.entry.image || ""
    }
    Column {
        id: titleColumn
        x: 600
        y: 140
        width: Ui.Theme.canvasWidth - x - Ui.Theme.safeRight
        spacing: 12
        Ui.UiText {
            objectName: "musicDetailEyebrow"
            width: parent.width
            role: "eyebrow"
            color: Ui.Theme.textSecondary
            textFormat: Text.PlainText
            text: [root.kindName(), root.sourceName()].filter(part => part.length).join(" · ")
        }
        Ui.UiText {
            objectName: "musicDetailTitle"
            width: parent.width
            role: "detailTitle"
            fitSteps: [[104, 2], [80, 3]]
            textFormat: Text.PlainText
            text: root.entry.title || ""
        }
        Ui.UiText {
            objectName: "musicDetailMeta"
            width: parent.width
            role: "meta"
            font.pixelSize: 28
            color: Ui.Theme.textSecondary
            textFormat: Text.PlainText
            maximumLineCount: 1
            elide: Text.ElideRight
            text: [root.kind === "artist" ? "" : (root.entry.artist || ""), root.length()]
                .filter(part => part.length).join(" · ")
        }
    }
    Row {
        id: pills
        x: 600
        y: Math.max(540, titleColumn.y + titleColumn.height + 32)
        spacing: 16
        Ui.ActionButton {
            id: listen
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled
            hoverMotionRevision: Ui.InputMode.pointerRevision
            onDpadRequested: Ui.InputMode.keyPressed()
            objectName: "musicDetailListen"
            primary: true
            visible: root.canListen
            text: qsTr("Listen")
            KeyNavigation.right: radio.visible ? radio : root.rightNeighbour
            Keys.onDownPressed: root.downRequested()
            onActivated: root.listenRequested()
        }
        Ui.ActionButton {
            id: radio
            pointerMode: root.pointerMode
            hoverFocusEnabled: root.hoverFocusEnabled
            hoverMotionRevision: Ui.InputMode.pointerRevision
            onDpadRequested: Ui.InputMode.keyPressed()
            objectName: "musicDetailRadio"
            visible: root.canRadio
            text: root.kind === "artist" ? qsTr("Radio from artist")
                : root.kind === "playlist" ? qsTr("Radio from playlist")
                : qsTr("Radio from album")
            KeyNavigation.left: listen.visible ? listen : null
            KeyNavigation.right: root.rightNeighbour
            Keys.onDownPressed: root.downRequested()
            onActivated: root.radioRequested()
        }
    }
}
