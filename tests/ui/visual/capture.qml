// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import QtTest
import "../../../apps/arinn-shell/ui" as Ui
import "../../../apps/arinn-shell/ui/screens/browse" as Browse
import "../../../apps/arinn-shell/ui/screens/browser" as Browser
import "../../../apps/arinn-shell/ui/screens/downloads" as Downloads
import "../../../apps/arinn-shell/ui/screens/library" as Library
import "../../../apps/arinn-shell/ui/screens/player" as Player
import "../../../apps/arinn-shell/ui/screens/power" as Power
import "../../../apps/arinn-shell/ui/screens/session" as Session
import "../../../apps/arinn-shell/ui/screens/settings" as Settings
// Generated under the output directory and supplied with arinn-ui-test -import.
import CaptureRequest 1.0 as CaptureRequest

// Offscreen capture of fixture-driven screens. Not part of the ui-components
// CTest input: that runner only loads tst_*.qml from tests/ui.
TestCase {
    id: suite
    name: "VisualCapture"
    when: windowShown
    visible: true
    width: job.wide ? 3840 : 1920
    height: job.wide ? 2160 : 1080

    readonly property var job: ({
        output: CaptureRequest.Request.output(),
        scale: CaptureRequest.Request.scale(),
        wide: CaptureRequest.Request.scale() === "4k-layout"
    })
    function readManifest() {
        const xhr = new XMLHttpRequest();
        xhr.open("GET", Qt.resolvedUrl("manifest.json"), false);
        xhr.send();
        verify(xhr.responseText.length > 0, "manifest.json did not load");
        return JSON.parse(xhr.responseText);
    }
    function art() {
        return Qt.resolvedUrl("../../../apps/arinn-shell/gallery/fixture-artwork.svg");
    }
    function apply(item, props) {
        const keys = Object.keys(props);
        for (let i = 0; i < keys.length; ++i)
            item[keys[i]] = props[keys[i]];
    }
    function homeProps(extra) {
        const props = {
            width: 1920, height: 1080, clockText: "22:57",
            continueEntries: [
                {id: "film", title: "The Quiet Harbor", kind: "media", positionUs: 4422e6, durationUs: 9960e6, artwork: art()},
                {id: "second", title: "Lighthouse Keepers", subtitle: "S1 E4 · The Fog Bell", kind: "media", positionUs: 300e6, durationUs: 2520e6, artwork: art()},
                {id: "third", title: "Glass Valley", subtitle: "S2 E1 · Thaw", kind: "media", positionUs: 240e6, durationUs: 2400e6, artwork: art()}
            ],
            downloadActive: true, downloadProgress: 0.82,
            favoriteEntries: [
                {id: "f1", title: "Northwind TV", monogram: "N"},
                {id: "f2", title: "Harbor Radio", monogram: "H"},
                {id: "f3", title: "Lumen Video", monogram: "L"},
                {id: "f4", title: "Atlas Maps", monogram: "A"}
            ],
            newEntries: [
                {id: "n1", title: "Paper Moons", subtitle: "2024", artwork: art()},
                {id: "n2", title: "Salt Roads", subtitle: "2023", artwork: art()}
            ]
        };
        const keys = Object.keys(extra || {});
        for (let i = 0; i < keys.length; ++i)
            props[keys[i]] = extra[keys[i]];
        return props;
    }
    function films() {
        const titles = ["The Quiet Harbor", "Paper Moons", "Salt Roads", "Copper Sky", "Winter Orchard", "The Last Ferry", "Lanterns"];
        const entries = [];
        for (let i = 0; i < titles.length; ++i)
            entries.push({id: "p" + i, tabId: "films", title: titles[i], year: String(2001 + i), kind: "film"});
        return entries;
    }
    // Open services: fictional services on .example domains (the frames are
    // CC-BY-SA, so no real brands), chips are monograms.
    readonly property var services: [
        {title: "Northwind TV · Live", url: "https://tv.north.example/live", letter: "N"},
        {title: "Harbor Radio · Morning Tide", url: "https://harbor-radio.example/live", letter: "H"},
        {title: "Lumen Video · Watch later", url: "https://video.lumen.example/later", letter: "L"},
        {title: "Meadow Weather · Week ahead", url: "https://weather.meadow.example/week", letter: "M"},
        {title: "Atlas Maps · Coast road", url: "https://maps.atlas.example/route", letter: "A"},
        {title: "Quill News · Front page", url: "https://news.quill.example/", letter: "Q"},
        {title: "Pinecone Kitchen · Fish soup", url: "https://pinecone.example/recipes/fish-soup", letter: "P"},
        {title: "Cinder Arcade", url: "https://play.cinder.example/", letter: "C"}
    ]
    function tabs(count, extra) {
        const rows = [];
        for (let i = 0; i < count; ++i) {
            const service = services[i % services.length];
            rows.push({
                id: "t" + i, windowId: "w1", title: service.title, url: service.url, letter: service.letter,
                active: i === 0, audible: i === 1
            });
        }
        if (extra)
            for (let i = 0; i < extra.length; ++i)
                rows.push(extra[i]);
        return rows;
    }
    function jobs() {
        return [
            {id: "active", title: "Salt Roads", subtitle: "2023 · 1080p", state: "downloading", progress: 0.82, statusText: "82%", availableActions: ["pause"], importState: "pending"},
            {id: "waiting", title: "Lighthouse Keepers S1 E5", state: "queued", availableActions: ["start_now"], importState: "pending"},
            {id: "held", title: "Copper Sky", state: "paused", progress: 0.4, availableActions: ["resume"], importState: "pending"},
            {id: "failed", title: "Winter Orchard", state: "error", errorText: "Could not download", availableActions: ["retry"], importState: "pending"}
        ];
    }
    function playerProps(extra) {
        const props = {
            width: 1920, height: 1080, title: "Lighthouse Keepers", episodeLine: "S1 E4 · The Fog Bell", clockText: "22:57",
            position: 754, duration: 5400, playing: false, seekSupported: true,
            audioTracks: [{id: "a1", label: "English · 5.1"}, {id: "a2", label: "English · Stereo"}],
            subtitleTracks: [{id: "s1", label: "English"}, {id: "s2", label: "English (SDH)"}],
            audioTrackId: "a1", subtitleTrackId: "s1"
        };
        const keys = Object.keys(extra || {});
        for (let i = 0; i < keys.length; ++i)
            props[keys[i]] = extra[keys[i]];
        return props;
    }
    function settingsProps() {
        return {
            width: 1920, height: 1080,
            categories: [
                {id: "media", title: "Media", icon: "video_library"},
                {id: "playback", title: "Playback", icon: "play_circle"},
                {id: "sound", title: "Sound", icon: "volume_up"},
                {id: "network", title: "Network", icon: "wifi"},
                {id: "applications", title: "Applications", icon: "apps"},
                {id: "appearance", title: "Appearance", icon: "palette"},
                {id: "system", title: "System", icon: "memory"}
            ],
            // Settings › Media holds more rows than fit on the screen, so
            // the frame shows how the list ends at the bottom edge.
            rows: settingsRows().concat(withProviders([
                {id: "sortfilms", categoryId: "media", title: "Sort films by", type: "enum", currentValue: "added", constraints: {options: [
                    {value: "title", title: "Title"}, {value: "year", title: "Year"}, {value: "added", title: "Recently added"}
                ]}, enabled: true},
                {id: "watched", categoryId: "media", title: "Show watched", description: "Keep finished films in the lists", type: "boolean", currentValue: true, constraints: {}, enabled: true},
                {id: "artwork", categoryId: "media", title: "Artwork from files", description: "Use posters stored next to the video", type: "boolean", currentValue: true, constraints: {}, enabled: true},
                {id: "metalang", categoryId: "media", title: "Title language", type: "enum", currentValue: "en", constraints: {options: [
                    {value: "en", title: "English"}, {value: "ru", title: "Russian"}, {value: "original", title: "Original"}
                ]}, enabled: true},
                {id: "minlength", categoryId: "media", title: "Skip short videos", description: "Shorter files stay out of the library", type: "integer", currentValue: 2, constraints: {min: 0, max: 30, unit: "min"}, enabled: true},
                {id: "collections", categoryId: "media", title: "Group films into collections", type: "boolean", currentValue: false, constraints: {}, enabled: true},
                {id: "hidden", categoryId: "media", title: "Hidden folders", description: "Scan folders whose names start with a dot", type: "boolean", currentValue: false, constraints: {}, enabled: true},
                {id: "libname", categoryId: "media", title: "Library name", description: "Shown in arinnctl and on the home network", type: "string", currentValue: "Home films", constraints: {maxLength: 32}, enabled: true}
            ]))
        };
    }
    // The provider that declares each row, shown as the row's third line
    // (settings_catalog.cpp fills providerName for every row). Only the name:
    // a providerId would change the row keys the scenes focus by.
    readonly property var settingsProviders: ({
        media: "Local library", playback: "Player", sound: "Audio", network: "Arinn shell",
        applications: "Arinn shell", appearance: "Arinn shell", system: "Arinn shell"
    })
    function withProviders(rows) {
        for (let i = 0; i < rows.length; ++i)
            if (rows[i].providerName === undefined)
                rows[i].providerName = settingsProviders[rows[i].categoryId] || "";
        return rows;
    }
    // Settings fixture for the editor states (handoff se-*): one row of each
    // kind the editors draw, in the categories of the handoff frames.
    function settingsRows(mutate) {
        const languages = [];
        for (let i = 0; i < 64; ++i)
            languages.push({value: "l" + i, title: "Language " + (i + 1)});
        const rows = [
            {id: "folders", categoryId: "media", title: "Library folders", description: "Places to scan", type: "path", currentValue: ["/srv/films", "/srv/series"], constraints: {directory: true, multiple: true}, enabled: true},
            {id: "autoscan", categoryId: "media", title: "Update automatically", description: "New files appear in the library", type: "boolean", currentValue: true, constraints: {}, enabled: true},
            {id: "download", categoryId: "media", title: "Download folder", description: "Where finished downloads go", type: "path", currentValue: "/srv/downloads", constraints: {directory: true}, enabled: true},
            {id: "autoplay", categoryId: "playback", title: "Play the next episode", description: "Ten seconds after the credits", type: "boolean", currentValue: true, constraints: {}, enabled: true},
            {id: "subtitles", categoryId: "playback", title: "Default subtitles", type: "enum", currentValue: "one", constraints: {options: [
                {value: "off", title: "Off"}, {value: "one", title: "First language"},
                {value: "signs", title: "First language (signs)"}, {value: "two", title: "Second language"},
                {value: "file", title: "As in the file"}
            ]}, enabled: true},
            {id: "audio", categoryId: "playback", title: "Default audio", type: "enum", currentValue: "l44", constraints: {options: languages}, enabled: true},
            {id: "intros", categoryId: "playback", title: "Skip intros", type: "boolean", currentValue: false, constraints: {}, enabled: true},
            {id: "output", categoryId: "sound", title: "Sound output", type: "enum", currentValue: "hdmi1", constraints: {options: [
                {value: "hdmi1", title: "HDMI · Television"}, {value: "hdmi2", title: "HDMI 2 · Receiver"},
                {value: "phones", title: "Headphones"}, {value: "speakers", title: "Built-in speakers"}
            ]}, enabled: true},
            {id: "night", categoryId: "sound", title: "Night mode", description: "Quieter explosions, louder dialogue", type: "boolean", currentValue: true, constraints: {}, enabled: true},
            {id: "delay", categoryId: "sound", title: "Audio delay", type: "integer", currentValue: 0, constraints: {min: 0, max: 500, unit: "ms"}, enabled: false},
            {id: "clicks", categoryId: "sound", title: "Interface sounds", type: "boolean", currentValue: true, constraints: {}, enabled: true},
            {id: "glow", categoryId: "appearance", title: "Light field", description: "Artwork colour lights the background", type: "boolean", currentValue: true, constraints: {}, enabled: true},
            {id: "ambient", categoryId: "appearance", title: "Ambient screen", description: "Minutes without input before the ambient screen starts", type: "integer", currentValue: 10, constraints: {min: 1, max: 60, unit: "min"}, enabled: true},
            {id: "textsize", categoryId: "appearance", title: "Text size", type: "enum", currentValue: "regular", constraints: {options: [
                {value: "regular", title: "Regular"}, {value: "large", title: "Large"}, {value: "larger", title: "Very large"}
            ]}, enabled: true},
            {id: "name", categoryId: "system", title: "Device name", description: "How Arinn shows on the home network and in arinnctl", type: "string", currentValue: "Arinn living room", constraints: {maxLength: 32}, enabled: true}
        ];
        withProviders(rows);
        if (mutate)
            for (let i = 0; i < rows.length; ++i)
                mutate(rows[i]);
        return rows;
    }
    function editorProps(mutate, extra) {
        const props = settingsProps();
        props.rows = settingsRows(mutate);
        const keys = Object.keys(extra || {});
        for (let i = 0; i < keys.length; ++i)
            props[keys[i]] = extra[keys[i]];
        return props;
    }
    readonly property var categoryIndex: ({media: 0, playback: 1, sound: 2, network: 3, applications: 4, appearance: 5, system: 6})
    // Focus a row the way the remote reaches it: its category, then the row.
    // `part` picks a control inside the row, such as "remove:0" or "add" on
    // a multiple path row; by default the row's first control.
    function settingsRow(view, id, part) {
        const row = view.rowByKey(id);
        verify(row !== null, id);
        view.focusCategory(categoryIndex[row.categoryId]);
        wait(0);
        const name = part || view.partsFor(row)[0];
        verify(view.focusPart(view.activeRowIndex(id), name), id + " " + name);
        return row;
    }
    function settingsScene(mutate, act, extra, expect) {
        return {factory: settingsFactory, props: editorProps(mutate, extra), tune: act, settle: 500, expect: expect};
    }
    function setRow(id, values) {
        return row => {
            if (row.id !== id)
                return;
            const keys = Object.keys(values);
            for (let i = 0; i < keys.length; ++i)
                row[keys[i]] = values[keys[i]];
        };
    }
    // Application tiles behind the tile action sheet (handoff cl-*).
    function appTiles() {
        return [
            {id: "a1", providerId: "fixture", title: "Northwind TV", monogram: "N", group: "online", pinned: true},
            {id: "a2", providerId: "fixture", title: "Harbor Radio", monogram: "H", group: "online"},
            {id: "g1", providerId: "fixture", title: "Cinder Rally", monogram: "C", group: "games", pinned: true, runningId: "w1", running: true},
            {id: "g2", providerId: "fixture", title: "Tidepool", monogram: "T", group: "games", pinned: true},
            {id: "o1", providerId: "fixture", title: "Text Editor", monogram: "E", group: "other", runningId: "w2", running: true}
        ];
    }
    function appScene(act, expect) {
        return {factory: homeFactory, props: homeProps({
            screen: "applications", sectionDescription: "Applications", actionsEnabled: true,
            applicationEntries: appTiles(),
            runningEntries: [{id: "w1", title: "Cinder Rally", monogram: "C", running: true}]
        }), tune: act, settle: 500, expect: expect};
    }
    // The on-screen keyboard: its language, layer and Shift, set through its
    // own keys as a remote would.
    function keyAt(keyboard, action) {
        const rows = keyboard.model.rows;
        for (let r = 0; r < rows.length; ++r)
            for (let c = 0; c < rows[r].length; ++c)
                if (rows[r][c].action === action)
                    return [r, c];
        fail("no " + action + " key");
        return [0, 0];
    }
    function pressKey(keyboard, action) {
        const at = keyAt(keyboard, action);
        keyboard.model.focusKey(at[0], at[1]);
        keyboard.apply(keyboard.model.activate());
    }
    function setLayer(keyboard, language, symbols, shift) {
        if (keyboard.model.language !== language)
            pressKey(keyboard, "language");
        if (keyboard.model.symbols !== symbols)
            pressKey(keyboard, "layer");
        if (keyboard.model.shift !== shift)
            pressKey(keyboard, "shift");
        compare(keyboard.model.language, language);
        compare(keyboard.model.symbols, symbols);
        compare(keyboard.model.shift, shift);
    }
    // A key in the letter rows (0-based), skipping the context row.
    function letterKey(keyboard, row, column) {
        const first = keyboard.model.rows[0][0].kind === "context" ? 1 : 0;
        keyboard.model.focusKey(first + row, column);
    }
    function browseScene(props, act) {
        const base = {width: 1920, height: 1080, engine: "Example Search"};
        const keys = Object.keys(props);
        for (let i = 0; i < keys.length; ++i)
            base[keys[i]] = props[keys[i]];
        return {factory: browseFactory, props: base, tune: act, settle: 500, expect: "keyboardPanel"};
    }
    function browseKeyboard(view, language, symbols, shift, row, column) {
        view.showKeyboard();
        wait(0);
        const keyboard = findChild(view, "browseOnScreenKeyboard");
        verify(keyboard !== null, "browseOnScreenKeyboard");
        setLayer(keyboard, language, symbols, shift);
        letterKey(keyboard, row, column);
    }
    function quickMenuProps(overActivity) {
        return {
            overActivity: overActivity,
            nowPlaying: {id: "song", title: "Low Tide", subtitle: "Harbor Radio", state: "playing", playPause: true},
            audio: "unmuted", volumeControl: true, volumeKnown: true, volumeLevel: 40,
            network: "wifi", wifiLevel: 3
        };
    }
    function scene(id) {
        if (id === "home-populated")
            return {factory: homeFactory, props: homeProps(), tune: "menu"};
        if (id === "home-content-focus")
            return {factory: homeFactory, props: homeProps(), tune: "right"};
        if (id === "home-activity")
            return {factory: homeFactory, props: homeProps({
                returnActivityId: "activity",
                continueEntries: [
                    {id: "activity", title: "Cinder Rally", subtitle: "Return", running: true},
                    {id: "film", title: "The Quiet Harbor", kind: "media", positionUs: 4422e6, durationUs: 9960e6, artwork: art()}
                ]
            })};
        if (id === "home-nav")
            return {factory: homeFactory, props: homeProps(), tune: "nav"};
        if (id === "home-loading")
            return {factory: homeFactory, props: homeProps({
                homeState: "loading", continueEntries: [], favoriteEntries: [], newEntries: [],
                statusTitle: "Loading Home", statusDescription: "Navigation stays available."
            })};
        if (id === "home-empty")
            return {factory: homeFactory, props: homeProps({
                homeState: "empty", continueEntries: [], favoriteEntries: [], newEntries: [],
                statusTitle: "Your Home starts here", statusDescription: "Browse sections to find something to open."
            })};
        if (id === "home-error")
            return {factory: homeFactory, props: homeProps({
                homeState: "error", continueEntries: [], favoriteEntries: [], newEntries: [],
                statusTitle: "Home could not be loaded", statusDescription: "Try again when you are ready."
            })};
        if (id === "home-unavailable")
            return {factory: homeFactory, props: homeProps({
                homeState: "unavailable", continueEntries: [], favoriteEntries: [], newEntries: [],
                statusTitle: "Home is unavailable", statusDescription: "The service is not connected."
            })};
        if (id === "home-over-service")
            return {factory: homeFactory, props: homeProps({
                currentServiceId: "svc", currentServiceTitle: "Northwind TV", currentServiceSubtitle: "tv.north.example",
                openServices: [{id: "svc", title: "Northwind TV", letter: "N"}, {id: "svc-2", title: "Harbor Radio", letter: "H"}]
            })};
        if (id === "home-nav-service")
            return {factory: homeFactory, props: homeProps({
                currentServiceId: "svc", currentServiceTitle: "Northwind TV", currentServiceSubtitle: "tv.north.example",
                openServices: [{id: "svc", title: "Northwind TV", letter: "N"}, {id: "svc-2", title: "Harbor Radio", letter: "H"},
                               {id: "svc-3", title: "Lumen Video", letter: "L"}]
            }), tune: "nav"};
        if (id === "home-browser-unavailable")
            return {factory: homeFactory, props: homeProps({
                browserUnavailable: true,
                favoriteEntries: [
                    {id: "f1", title: "Northwind TV", monogram: "N", webShortcut: true},
                    {id: "f2", title: "Harbor Radio", monogram: "H"}
                ]
            })};
        if (id === "applications")
            return {factory: homeFactory, props: homeProps({
                screen: "applications", sectionDescription: "Applications",
                applicationEntries: [
                    {id: "a", title: "Northwind TV", monogram: "N", group: "online"},
                    {id: "b", title: "Cinder Rally", monogram: "C", group: "games"},
                    {id: "c", title: "Music", monogram: "M", group: "media"},
                    {id: "d", title: "Text Editor", monogram: "T", group: "other"}
                ],
                runningEntries: [{id: "r", title: "Cinder Rally", monogram: "C", running: true}]
            })};
        if (id === "library-grid")
            return {factory: libraryFactory, props: {
                width: 1920, height: 1080,
                tabs: [{id: "films", title: "Films"}, {id: "series", title: "Series"}],
                entries: films()
            }};
        if (id === "library-detail")
            return {factory: libraryFactory, props: {
                width: 1920, height: 1080, detailEntryId: "p0",
                tabs: [{id: "films", title: "Films"}],
                entries: [{id: "p0", tabId: "films", title: "The Quiet Harbor", year: "2001", kind: "film", description: "A ferry captain spends one last winter on a northern island before the route closes.", progress: 0.4}]
            }};
        if (id === "library-series")
            return {factory: libraryFactory, props: {
                width: 1920, height: 1080, detailEntryId: "show", currentSeasonId: "s1",
                tabs: [{id: "series", title: "Series"}],
                entries: [{
                    id: "show", tabId: "series", title: "Lighthouse Keepers", subtitle: "2011", kind: "series",
                    description: "Two keepers, one rock and a light that must not go out.",
                    seasons: [{id: "s1", title: "Season 1"}, {id: "s2", title: "Season 2"}],
                    episodes: [
                        {id: "p1", seasonId: "s1", title: "First Watch", duration: "40 min", number: 1},
                        {id: "p2", seasonId: "s1", title: "Supply Boat", duration: "41 min", number: 2}
                    ]
                }]
            }};
        if (id === "library-empty")
            return {factory: libraryFactory, props: {width: 1920, height: 1080, libraryState: "empty", tabs: [{id: "films", title: "Films"}]}};
        if (id === "library-loading")
            return {factory: libraryFactory, props: {width: 1920, height: 1080, libraryState: "loading", tabs: [{id: "films", title: "Films"}]}};
        if (id === "library-error")
            return {factory: libraryFactory, props: {width: 1920, height: 1080, libraryState: "error", tabs: [{id: "films", title: "Films"}]}};
        if (id === "library-unavailable")
            return {factory: libraryFactory, props: {width: 1920, height: 1080, libraryState: "unavailable", tabs: [{id: "films", title: "Films"}]}};
        if (id === "player-hidden")
            return {factory: playerFactory, props: playerProps({osdVisible: false, playing: true})};
        if (id === "player-osd")
            return {factory: playerFactory, props: playerProps()};
        if (id === "player-subtitles")
            return {factory: playerFactory, props: playerProps({sheetKind: "subtitles"})};
        if (id === "player-audio")
            return {factory: playerFactory, props: playerProps({sheetKind: "audio"})};
        if (id === "player-loading")
            return {factory: playerFactory, props: playerProps({playbackState: "loading"})};
        if (id === "player-error")
            return {factory: playerFactory, props: playerProps({playbackState: "error", statusTitle: "Playback stopped", statusDescription: "The file could not be read."})};
        if (id === "player-unavailable")
            return {factory: playerFactory, props: playerProps({playbackState: "unavailable", statusTitle: "Playback unavailable", statusDescription: "No player is connected."})};
        if (id === "downloads-queue")
            return {factory: downloadsFactory, props: {width: 1920, height: 1080, jobs: jobs()}};
        if (id === "downloads-error")
            return {factory: downloadsFactory, props: {width: 1920, height: 1080, viewState: "error", jobs: []}};
        if (id === "downloads-empty")
            return {factory: downloadsFactory, props: {width: 1920, height: 1080, viewState: "empty", jobs: []}};
        if (id === "downloads-loading")
            return {factory: downloadsFactory, props: {width: 1920, height: 1080, viewState: "loading", jobs: []}};
        if (id === "downloads-unavailable")
            return {factory: downloadsFactory, props: {width: 1920, height: 1080, viewState: "unavailable", jobs: []}};
        if (id === "settings")
            return {factory: settingsFactory, props: settingsProps()};
        if (id === "power-menu")
            return {factory: powerFactory, props: {width: 1920, height: 1080, closingApplications: ["Cinder Rally", "Text Editor"]}};
        if (id === "power-confirm")
            return {factory: powerFactory, props: {width: 1920, height: 1080, step: "confirm", pendingAction: "shutdown", closingApplications: ["Cinder Rally"]}};
        if (id === "power-ambient")
            return {factory: ambientFactory, props: {width: 1920, height: 1080, clockText: "22:57", dateText: "Monday", driftInterval: 600000},
                    tune: view => view.start(), expect: ["ambientClock", "ambientDate"]};
        if (id === "power-desktop")
            return {factory: powerFactory, props: {
                width: 1920, height: 1080, closingApplications: ["Cinder Rally", "Text Editor"],
                sleepAvailability: "available", restartAvailability: "available", shutdownAvailability: "available",
                desktopSwitchAvailability: "available", menuFocus: "desktop"
            }, tune: view => view.restoreMenuFocus(), expect: "powerMenu"};
        if (id === "power-desktop-confirm")
            return {factory: powerFactory, props: {
                width: 1920, height: 1080, closingApplications: ["Cinder Rally", "Text Editor"],
                sleepAvailability: "available", restartAvailability: "available", shutdownAvailability: "available",
                desktopSwitchAvailability: "available", step: "confirm", pendingAction: "desktop", menuFocus: "desktop"
            }, expect: ["powerConfirm", "confirmDetail"]};
        if (id === "quick-menu-shell" || id === "quick-menu-activity")
            return {factory: quickMenuFactory, props: {overActivity: id === "quick-menu-activity"}, tune: view => {
                apply(view.menu, quickMenuProps(view.overActivity));
                view.menu.start();
            }, settle: 400, expect: "quickMenu"};
        if (id === "volume-hud")
            return {factory: volumeFactory, props: {}, tune: view => {
                view.hud.hideAfter = 600000;
                view.hud.show(40, false);
            }, settle: 400, expect: "volumeHud"};
        if (id === "launch-starting" || id === "launch-stop-confirm" || id === "launch-stopping")
            return {factory: launchFactory, props: {
                width: 1920, height: 1080, kind: "application", active: true, serviceTitle: "Cinder Rally",
                revealDelay: 600000, slowDelay: 600000, timeoutDelay: 0
            }, tune: view => {
                view.revealed = true;
                view.slow = true;
                if (id === "launch-stop-confirm") {
                    view.requestCancel();
                } else if (id === "launch-stopping") {
                    view.cancellable = false;
                    view.stopping = true;
                }
            }, settle: 400, expect: id === "launch-stop-confirm" ? "launchConfirm" : "launchTitle"};
        if (id === "apps-tile-running")
            return appScene(view => view.openTileActions("g1"), ["tileActions", "closeAppAction"]);
        if (id === "apps-close-confirm")
            return appScene(view => {
                view.openTileActions("g1");
                wait(50);
                view.confirmAppClose = true;
                Ui.Focus.take(findChild(view, "closeAppCancel"));
            }, "closeAppConfirmation");
        if (id === "home-tile-actions")
            return {factory: homeFactory, props: homeProps({
                actionsEnabled: true,
                favoriteEntries: [
                    {id: "f1", providerId: "fixture", title: "Northwind TV", monogram: "N", pinned: true, runningId: "w1", running: true},
                    {id: "f2", providerId: "fixture", title: "Harbor Radio", monogram: "H", pinned: true},
                    {id: "f3", providerId: "fixture", title: "Lumen Video", monogram: "L", pinned: true},
                    {id: "f4", providerId: "fixture", title: "Atlas Maps", monogram: "A", pinned: true}
                ]
            }), tune: view => view.openTileActions("f2"), settle: 500, expect: "tileActions"};
        if (id === "keyboard-browse")
            return browseScene({text: "northern", kind: "search", suggestions: [
                {icon: "history", title: "northern lights tonight", value: "northern lights tonight"},
                {icon: "history", title: "northern trails map", value: "northern trails map"}
            ]}, view => browseKeyboard(view, "en", false, false, 1, 4));
        if (id === "keyboard-symbols")
            return browseScene({text: "192.168.1.10:", kind: "address", resolvedUrl: "http://192.168.1.10:", suggestions: [
                {icon: "public", title: "Media", subtitle: "192.168.1.10:8096", value: "http://192.168.1.10:8096"}
            ]}, view => browseKeyboard(view, "en", true, false, 0, 7));
        if (id === "keyboard-shift")
            return browseScene({text: "", suggestions: []}, view => browseKeyboard(view, "en", false, true, 1, 4));
        if (id === "keyboard-shortcut-address" || id === "keyboard-shortcut-name")
            return {factory: homeFactory, props: homeProps({
                screen: "applications", sectionDescription: "Applications", shortcutManagement: true, keyboardAvailable: false,
                applicationEntries: appTiles(),
                shortcutNameSuggestions: [
                    {icon: "title", title: "Media", subtitle: "Page title", value: "Media"},
                    {icon: "public", title: "media.local", subtitle: "Domain", value: "media.local"}
                ]
            }), tune: view => {
                view.shortcutEditor = {id: "", title: id === "keyboard-shortcut-name" ? "Med" : "", url: "media.local:8096"};
                wait(50);
                view.startShortcutTyping(id === "keyboard-shortcut-name" ? "title" : "url");
            }, settle: 500, expect: "keyboardPanel"};
        if (id === "keyboard-setting-string")
            return settingsScene(null, view => {
                view.openString(settingsRow(view, "name"));
                wait(50);
                view.startStringTyping();
            }, {keyboardAvailable: false}, "stringOnScreenKeyboard");
        if (id === "keyboard-setting-path")
            return settingsScene(null, view => {
                const row = settingsRow(view, "download");
                view.openEditor(row, "main", row.currentValue);
                wait(50);
                view.startPathTyping();
            }, {keyboardAvailable: false}, "pathOnScreenKeyboard");
        // Library folders (handoff se-lf-*): the shell lists the folders of a
        // multiple path setting inline in Settings › Media.
        if (id === "library-folders-row")
            return settingsScene(null, view => settingsRow(view, "folders"), null, ["path-folders-0", "remove-folders-0", "add-folders"]);
        if (id === "library-folders-empty")
            return settingsScene(setRow("folders", {currentValue: []}), view => settingsRow(view, "folders", "add"), null, "add-folders");
        if (id === "library-folders-list")
            return settingsScene(null, view => settingsRow(view, "folders", "remove:0"), null, ["path-folders-1", "remove-folders-0"]);
        // A folder the library provider reports unavailable (#540): it stays
        // in the list with its note. Its removal (se-lf-7) is a confirmation
        // dialog the shell does not draw, like se-lf-6.
        if (id === "library-folders-4")
            return settingsScene(setRow("folders", {
                currentValue: ["/srv/films", "/srv/series", "/mnt/nas/archive"],
                unavailablePaths: ["/mnt/nas/archive"]
            }), view => settingsRow(view, "folders", "path:0"), null, ["path-folders-0", "pathNote-folders-2"]);
        if (id === "library-folders-limit")
            return settingsScene(setRow("folders", {
                currentValue: ["/srv/archive-2019", "/srv/archive-2020", "/srv/archive-2021", "/srv/films"],
                constraints: {directory: true, multiple: true, maxItems: 4}
            }), view => settingsRow(view, "folders", "add"), null, ["path-folders-3", "add-folders"]);
        if (id === "settings-rows")
            return settingsScene(null, view => settingsRow(view, "ambient"));
        if (id === "settings-disabled")
            return settingsScene(null, view => settingsRow(view, "delay"));
        if (id === "settings-enum")
            return settingsScene(null, view => view.openOptions(settingsRow(view, "subtitles")), null, "option-0");
        if (id === "settings-enum-long")
            return settingsScene(null, view => view.openOptions(settingsRow(view, "audio")), null, "optionPosition");
        if (id === "settings-enum-short")
            return settingsScene(null, view => view.openOptions(settingsRow(view, "textsize")), null, "option-0");
        if (id === "settings-integer")
            return settingsScene(null, view => {
                view.openInteger(settingsRow(view, "ambient"));
                for (let i = 0; i < 5; ++i)
                    view.nudgeInteger(1, false);
            }, null, "integerStepper");
        if (id === "settings-integer-max")
            return settingsScene(null, view => {
                view.openInteger(settingsRow(view, "ambient"));
                // Was 10: step up to the maximum, then once more against it.
                while (view.integerDraft < 60)
                    view.nudgeInteger(1, false);
                view.nudgeInteger(1, false);
            }, null, ["integerStepper", "integerNote"]);
        if (id === "settings-string")
            return settingsScene(null, view => view.openString(settingsRow(view, "name")), null, "stringCounter");
        if (id === "settings-saving")
            return settingsScene(null, view => {
                view.openOptions(settingsRow(view, "subtitles"));
                wait(50);
                view.optionIndex = 3;
                view.focusOption(3);
                view.chooseOption();
                // The daemon accepted the request and has not answered yet.
                view.rows = settingsRows(setRow("subtitles", {pending: true}));
            }, null, "optionSpinner");
        if (id === "settings-saved")
            return settingsScene(setRow("subtitles", {currentValue: "two"}), view => {
                settingsRow(view, "subtitles");
                view.showToast("Saved", "Default subtitles: Second language", false);
            }, null, "settingsToast");
        if (id === "settings-saved-restart")
            return settingsScene(setRow("ambient", {currentValue: 15, deferred: true}), view => {
                settingsRow(view, "ambient");
                view.showToast("Saved", "Applies after Arinn restarts", false);
            }, null, "settingsToast");
        if (id === "settings-string-rejected")
            return settingsScene(setRow("name", {error: "Use letters, digits, spaces and hyphens."}),
                                 view => view.openString(settingsRow(view, "name")), null, "stringError");
        if (id === "settings-enum-rejected")
            return settingsScene(setRow("output", {error: "HDMI 2 is not connected."}), view => {
                view.openOptions(settingsRow(view, "output"));
                wait(50);
                view.focusOption(1);
            }, null, "enumError");
        if (id === "settings-toggle-rejected")
            return settingsScene(setRow("night", {currentValue: false, error: "The HDMI output does not support it."}), view => {
                settingsRow(view, "night");
                view.showToast("Could not save", "The HDMI output does not support it.", true);
            }, null, "settingsToast");
        if (id === "power-refusal")
            return {factory: powerFactory, props: {width: 1920, height: 1080, refusalMessage: "The system did not suspend."}};
        if (id === "browser-list")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", tabs: tabs(3)}};
        if (id === "browser-close-focus")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", tabs: tabs(3)}, tune: "close"};
        if (id === "browser-closing")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", tabs: tabs(3), closingId: "t0"}};
        if (id === "browser-many")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", tabs: tabs(8).map((tab, i) => {
                // One service already saved: its button shows bookmark_added.
                tab.saved = i === 2;
                return tab;
            })}};
        if (id === "browser-long")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", tabs: [
                {id: "t0", windowId: "w1", title: "Quill News · Coastal towns vote on the new ferry timetable after a long winter of cancelled crossings", url: "https://news.quill.example/2026/10/ferry-timetable-vote", letter: "Q", active: true, audible: false},
                {id: "t1", windowId: "w1", title: "", url: "https://harbor-radio.example/archive", letter: "H", active: false, audible: false}
            ]}};
        if (id === "browser-empty")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", browserState: "empty", tabs: []}};
        if (id === "browser-unavailable")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", browserState: "unavailable", tabs: []}};
        if (id === "browser-error")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", browserState: "error", tabs: []}};
        if (id === "browser-loading")
            return {factory: browserFactory, props: {width: 1920, height: 1080, clockText: "22:57", browserState: "loading", tabs: []}};
        if (id === "launch-opening" || id === "launch-slow")
            return {factory: launchFactory, props: {
                width: 1920, height: 1080, active: true, serviceTitle: "Northwind TV",
                revealDelay: 600000, slowDelay: 600000, timeoutDelay: 600000
            }, tune: id === "launch-slow" ? "slow" : "opening"};
        if (id === "launch-failed")
            return {factory: launchFactory, props: {width: 1920, height: 1080, serviceTitle: "Northwind TV", failure: "unavailable"}};
        if (id === "session-ready")
            return {factory: sessionFactory, props: {
                width: 1920, height: 1080, title: "Low Tide", subtitle: "Harbor Radio",
                playbackState: "playing", durationUs: 125000000, positionUs: 65000000, artworkHue: "#336699"
            }};
        if (id === "session-empty")
            return {factory: sessionFactory, props: {width: 1920, height: 1080, sessionState: "empty"}};
        if (id === "session-unavailable")
            return {factory: sessionFactory, props: {width: 1920, height: 1080, sessionState: "unavailable"}};
        return null;
    }
    function tune(kind, item) {
        if (typeof kind === "function") {
            wait(50);
            kind(item);
        } else if (kind === "nav")
            item.navOpen = true;
        else if (kind === "menu") {
            wait(50);
            const menu = findChild(item, "menuButton");
            if (menu)
                menu.forceActiveFocus();
        } else if (kind === "right") {
            wait(50);
            keyClick(Qt.Key_Right);
        } else if (kind === "close") {
            const id = item.orderedTabs.length ? item.orderedTabs[0].id : "";
            item.focusedEntryId = id;
            item.listSide = "close";
            item.settleFocus();
        } else if (kind === "opening" || kind === "slow") {
            item.revealed = true;
            item.slow = kind === "slow";
        }
    }
    function meanAbs(a, b) {
        const cols = 64;
        const rows = 36;
        let sum = 0;
        let n = 0;
        for (let row = 0; row < rows; ++row) {
            const ay = Math.min(a.height - 1, Math.floor((row + 0.5) * a.height / rows));
            const by = Math.min(b.height - 1, Math.floor((row + 0.5) * b.height / rows));
            for (let col = 0; col < cols; ++col) {
                const ax = Math.min(a.width - 1, Math.floor((col + 0.5) * a.width / cols));
                const bx = Math.min(b.width - 1, Math.floor((col + 0.5) * b.width / cols));
                sum += Math.abs(a.red(ax, ay) - b.red(bx, by));
                sum += Math.abs(a.green(ax, ay) - b.green(bx, by));
                sum += Math.abs(a.blue(ax, ay) - b.blue(bx, by));
                n += 3;
            }
        }
        return n ? sum / n / 255 : 0;
    }
    function emit(record) {
        console.log("ARINN_CAPTURE " + JSON.stringify(record));
    }

    Rectangle { anchors.fill: parent; color: Ui.Theme.background; z: -1 }
    Item {
        id: stageHost
        anchors.fill: parent
        Item {
            id: stage
            width: 1920
            height: 1080
            scale: stageHost.width / 1920
            transformOrigin: Item.TopLeft
        }
    }
    Image {
        id: reference
        anchors.fill: parent
        visible: false
        z: 2
        fillMode: Image.Stretch
        asynchronous: false
        cache: false
    }

    Component { id: homeFactory; Ui.HomeView { width: 1920; height: 1080 } }
    Component { id: libraryFactory; Library.LibraryView { width: 1920; height: 1080 } }
    Component { id: playerFactory; Player.PlayerView { width: 1920; height: 1080 } }
    Component { id: downloadsFactory; Downloads.DownloadsView { width: 1920; height: 1080 } }
    Component { id: settingsFactory; Settings.SettingsView { width: 1920; height: 1080 } }
    Component { id: powerFactory; Power.PowerMenu { width: 1920; height: 1080 } }
    Component { id: browserFactory; Browser.BrowserView { width: 1920; height: 1080 } }
    Component { id: launchFactory; Ui.LaunchOverlay { width: 1920; height: 1080 } }
    Component { id: sessionFactory; Session.SessionView { width: 1920; height: 1080 } }
    Component { id: ambientFactory; Power.AmbientScreen { width: 1920; height: 1080 } }
    Component { id: browseFactory; Browse.BrowseView { width: 1920; height: 1080 } }
    // The quick menu over the shell (Home behind it), or over an Activity.
    // The shell window is transparent over an Activity, so a fixture image
    // stands in for the application window behind it.
    Component {
        id: quickMenuFactory
        Item {
            id: quickScene
            width: 1920; height: 1080
            property bool overActivity: false
            readonly property alias menu: quickMenu
            Image {
                anchors.fill: parent
                visible: quickScene.overActivity
                source: suite.art()
                fillMode: Image.PreserveAspectCrop
            }
            Ui.HomeView {
                visible: !quickScene.overActivity
                Component.onCompleted: suite.apply(this, suite.homeProps())
            }
            Ui.QuickMenu { id: quickMenu; anchors.fill: parent; clockText: "22:57" }
        }
    }
    // The volume HUD over Home, placed as PowerOverlay places it.
    Component {
        id: volumeFactory
        Item {
            width: 1920; height: 1080
            readonly property alias hud: volumeHud
            Ui.HomeView { Component.onCompleted: suite.apply(this, suite.homeProps()) }
            Ui.VolumeHud { id: volumeHud; z: 10; anchors.horizontalCenter: parent.horizontalCenter; y: 48 }
        }
    }

    function test_capture() {
        if (!job.output) {
            skip("No capture request. Run scripts/ui-compare.py.");
            return;
        }
        tryVerify(() => Ui.Theme.onest.status === FontLoader.Ready || Ui.Theme.onest.status === FontLoader.Error, 5000);
        const manifest = readManifest();
        let shown = null;
        const window = suite.Window.window;
        const dpr = window ? window.devicePixelRatio : 1;
        for (let i = 0; i < manifest.states.length; ++i) {
            const state = manifest.states[i];
            if (!state.render)
                continue;
            const plan = scene(state.id);
            verify(plan !== null, "No scene for " + state.id);
            mouseMove(suite, 8, 8);
            reference.visible = false;
            stageHost.visible = true;
            if (shown) {
                shown.destroy();
                shown = null;
                wait(0);
            }
            shown = plan.factory.createObject(stage, plan.props);
            verify(shown !== null, state.id);
            if (plan.tune)
                tune(plan.tune, shown);
            wait(0);
            if (!shown.Window.window)
                fail(state.id + " has no window (" + shown.width + "x" + shown.height + ")");
            // A settled frame, such as a static player plate, never produces
            // another frame, so waitForRendering returns false. That is not a
            // failure: wait, then require the item to still be visible.
            waitForRendering(shown, 5000);
            wait(plan.settle !== undefined ? plan.settle : plan.tune === "nav" ? 450 : 200);
            if (!shown || !shown.visible)
                fail("not visible: " + state.id);
            // A scene names the parts it must show, so a stale fixture (a
            // removed step, a renamed screen) fails instead of saving an
            // empty frame.
            const expected = plan.expect ? [].concat(plan.expect) : [];
            for (let e = 0; e < expected.length; ++e) {
                const part = shown.objectName === expected[e] ? shown : findChild(shown, expected[e]);
                verify(part !== null && part.visible, state.id + " does not show " + expected[e]);
            }
            waitForRendering(shown, 1000);
            const shot = grabImage(suite);
            const path = job.output + "/" + state.id + ".png";
            shot.save(path);
            let mae = null;
            if (state.reference) {
                stageHost.visible = false;
                reference.source = Qt.resolvedUrl("../../../docs/design/handoff/screenshots/" + state.reference);
                reference.visible = true;
                tryCompare(reference, "status", Image.Ready, 5000, state.reference);
                verify(waitForRendering(reference, 5000), state.reference);
                mae = meanAbs(shot, grabImage(reference));
                reference.visible = false;
                stageHost.visible = true;
            }
            emit({
                id: state.id,
                scale: job.scale,
                png: state.id + ".png",
                width: shot.width,
                height: shot.height,
                dpr: dpr,
                reference: state.reference || null,
                mae: mae === null ? null : Math.round(mae * 10000) / 10000,
                rendered: true
            });
        }
        if (shown)
            shown.destroy();
        console.log("ARINN_CAPTURE_DONE");
    }
}
