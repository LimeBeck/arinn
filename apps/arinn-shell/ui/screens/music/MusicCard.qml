// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

// One entry on a music shelf (brief mh-01): the artwork, then the title and
// a subtitle that starts with the source mark (MH-1, MH-10). A station is a
// 16:9 card, an artist a round portrait, everything else a square cover.
Ui.Surface {
    id: root
    property var entry: ({})
    property real artworkWidth: 240
    property real artworkHeight: 240
    property bool round: false
    // The entry is the one that plays: the playing chip sits on its artwork.
    property bool playing: false
    // The cover of the track that plays: a playing station without its own
    // cover shows it (#843). Then the neutral chip with the station glyph.
    property string playingImage: ""
    readonly property string kind: entry.kind || ""
    shapeRole: "content"
    text: entry.title || ""
    cornerRadius: round ? artworkWidth / 2 : Ui.Theme.stillRadius
    implicitWidth: artworkWidth
    implicitHeight: artworkHeight + Ui.Theme.contentCaptionHeight
    // Dimmed as one layer, so the chip under the glyph never shows through
    // (#843).
    opacity: entry.available === false ? 0.6 : 1
    layer.enabled: opacity < 1
    contentItem: Item {
        MusicArtwork {
            id: art
            objectName: "musicCardArtwork"
            width: root.artworkWidth
            height: root.artworkHeight
            radius: root.cornerRadius
            // The card draws its own placeholder chip below: one glyph (#843).
            placeholder: false
            source: root.entry.image
                || (root.playing && root.kind === "station" ? root.playingImage : "")
            // Focus as on every content card (ADR-0136): the mat, then the
            // rim from below (#838).
            Ui.ArtworkFocus {
                objectName: "musicCardFocus"
                shown: root.activeFocus
                artworkRadius: root.cornerRadius
            }
        }
        // An artist without a portrait: its initial in a neutral chip.
        Rectangle {
            objectName: "musicCardMonogram"
            width: root.artworkWidth
            height: root.artworkHeight
            radius: root.cornerRadius
            color: Ui.Theme.chip
            border.color: Ui.Theme.chipLine
            visible: !art.loaded
            Ui.UiText {
                anchors.centerIn: parent
                role: "title"
                color: Ui.Theme.textSecondary
                text: (root.entry.title || "?").charAt(0).toUpperCase()
                visible: root.kind === "artist"
            }
            Ui.LineIcon {
                anchors.centerIn: parent
                width: 64
                height: 64
                name: root.kind === "station" ? "radio" : root.kind === "playlist" ? "queue_music" : "music_note"
                color: Ui.Theme.textHint
                visible: root.kind !== "artist"
            }
        }
        // The playing chip (brief, Continue card): three Ember bars in a
        // neutral chip, a status mark, never a focus mark.
        Rectangle {
            objectName: "musicCardPlaying"
            x: 16
            y: 16
            width: 56
            height: 42
            radius: 12
            color: Ui.Theme.chip
            visible: root.playing
            Row {
                anchors.centerIn: parent
                spacing: 4
                Repeater {
                    model: [14, 22, 10]
                    Rectangle {
                        required property var modelData
                        width: 5
                        height: modelData
                        radius: 2
                        anchors.bottom: parent.bottom
                        color: Ui.Theme.ember
                    }
                }
            }
        }
        Column {
            y: root.artworkHeight + 12
            width: root.artworkWidth
            spacing: 4
            Ui.UiText {
                objectName: "musicCardTitle"
                width: parent.width
                role: "meta"
                color: root.activeFocus ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                text: root.entry.title || ""
                textFormat: Text.PlainText
                maximumLineCount: 1
                elide: Text.ElideRight
            }
            Row {
                spacing: 8
                width: parent.width
                MusicSourceMark {
                    objectName: "musicCardSource"
                    local: root.entry.local === true
                    name: root.entry.providerName || ""
                }
                Ui.UiText {
                    objectName: "musicCardSubtitle"
                    width: parent.width - 38
                    anchors.verticalCenter: parent.verticalCenter
                    role: "caption"
                    color: Ui.Theme.textHint
                    // A service is named after its mark, so two services and
                    // their unmerged duplicates stay apart (brief, MH-10).
                    text: [root.entry.local === true ? "" : (root.entry.providerName || ""),
                           root.entry.available === false ? qsTr("Unavailable in this source")
                           : root.kind === "album" || root.kind === "track" ? (root.entry.artist || "") : ""]
                        .filter(part => part.length).join(" · ")
                    textFormat: Text.PlainText
                    maximumLineCount: 1
                    elide: Text.ElideRight
                }
            }
        }
    }
}
