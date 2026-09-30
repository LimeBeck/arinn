// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/library" as Library
import "../../apps/arinn-shell/ui/screens/player" as Player
import "../../apps/arinn-shell/ui/screens/session" as Session

// Driven by arinn-asset-images-test. assetUrl is an Assets1 image URL whose
// reply is held until the test checks the generic plate.
Window {
    width: 1920
    height: 1080
    visible: true
    color: Ui.Theme.background
    Session.SessionView {
        objectName: "session"
        sessionState: "ready"
        title: "Piece"
        artwork: assetUrl
    }
    Player.PlayerView {
        objectName: "player"
        playbackState: "ready"
        osdVisible: true
        playing: false
        position: 10
        duration: 100
        thumbnail: assetUrl
    }
    Library.LibraryView {
        objectName: "library"
        columns: 1
        libraryState: "ready"
        entries: [{
            id: "north",
            title: "North",
            kind: "film",
            year: "1999",
            noArt: false,
            artwork: assetUrl
        }]
    }
    Ui.ContentCard {
        objectName: "card"
        x: 120
        y: 120
        text: "Piece"
        artwork: assetUrl
    }
}
