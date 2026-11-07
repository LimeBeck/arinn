// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../../../apps/arinn-shell/ui" as Ui
import "../../../apps/arinn-shell/ui/screens/music" as Music

// Current production room/player. All catalogue, identity and artwork data are fictional.
Item {
    id: root
    width: 1920; height: 1080
    property alias view: music
    readonly property string artwork: String(Qt.resolvedUrl("../../../apps/arinn-shell/gallery/fixture-artwork.svg"))
    function entry(provider, id, kind, title, artist) {
        return {key: provider + "\n" + id, provider: provider, id: id, kind: kind, title: title,
                artist: artist || "", image: artwork, available: true, pinnable: kind === "station",
                local: provider === "music-local", providerName: provider === "music-local" ? "Local music" : "Harbor Radio"};
    }
    property var catalog: QtObject {
        property var sources: [{id: "music-local", name: "Local music"}]
        property string sourceId: "music-local"
        property var entries: []
        property var state: ({account: "local", facets: ["tracks", "albums", "playlists"], actions: ["play", "radio"]})
        property var activeState: ({session: "fixture-session", playback: "playing", continuous: true,
            station: {id: "station:library", kind: "station", title: "From this library", pinnable: true},
            current: {occurrence: "1", track: {id: "track:low-tide", kind: "track", title: "Low tide",
                artist: "Mira Vale", album: "Small hours", image: root.artwork, duration_us: 245000000}},
            position_us: 82000000, actions: ["pause", "previous", "next", "seek", "stop"], upcoming: []})
        property string activeSourceName: "Local music"
        property string activeSourceId: "music-local"
        property string errorText: ""
        property string notice: ""
        property string playbackProblem: ""
        property bool busy: false
        property bool transportBusy: false
        property bool more: false
        signal pageFailed()
        function choose(id) { sourceId = id; }
        function browse() {}
        function loadMore() {}
        function command() {}
        function transport() {}
    }
    property var room: QtObject {
        property var sources: [{id: "music-local", name: "Local music", local: true, status: "connected", actions: ["scan"], browsable: true},
                               {id: "fixture-radio", name: "Harbor Radio", local: false, status: "connected", actions: ["scan"], browsable: true}]
        property var shelves: [
            {id: "stations", title: "Stations", kind: "station", entries: [
                root.entry("music-local", "station:library", "station", "From this library"),
                root.entry("fixture-radio", "station:evening", "station", "Evening currents"),
                root.entry("fixture-radio", "station:quiet", "station", "Quiet hours")]},
            {id: "albums", title: "Albums", kind: "album", entries: [
                root.entry("music-local", "album:small-hours", "album", "Small hours", "Mira Vale"),
                root.entry("music-local", "album:coastlines", "album", "Coastlines", "The Lanterns"),
                root.entry("fixture-radio", "album:open-water", "album", "Open water", "June Atlas")]}
        ]
        property bool connected: true
        property bool settled: true
        property bool loading: false
        property bool dated: false
        property bool sourcesSwitchable: true
        property var disabled: []
        property var searchShelves: []
        property string searchQuery: ""
        signal listChanged()
        function list(shelf, source) {
            const found = shelves.find(item => item.id === shelf);
            return (found ? found.entries : []).filter(entry => !source || source === entry.provider);
        }
        function more() { return false; }
        function loadMore() {}
        function search(query) { searchQuery = query; }
        function sourceCommand() {}
        function stable(shown, desired) {
            const keys = desired.map(entry => entry.key);
            const kept = shown.filter(entry => keys.includes(entry.key)).map(entry => desired[keys.indexOf(entry.key)]);
            return kept.concat(desired.filter(entry => !kept.some(item => item.key === entry.key)));
        }
    }
    Music.MusicView { id: music; anchors.fill: parent; catalog: root.catalog; room: root.room }
    Ui.StatusArea { anchors.right: parent.right; anchors.rightMargin: 96; y: 42; clockText: "12:00"; z: 20 }
}
