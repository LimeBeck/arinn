// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/library" as Library
import "../../apps/arinn-shell/ui/screens/player" as Player
import "../../apps/arinn-shell/ui/screens/power" as Power
import "../../apps/arinn-shell/ui/screens/settings" as Settings

// Specification §6.3 rows that the shell fixtures own, plus the §6.2 text-field
// exception. The external-application row is the platform shortcut and Main.qml.
TestCase {
    id: suite
    name: "HomeBackTable"
    width: 1920
    height: 1080
    visible: true
    when: windowShown

    property var owned: []

    Component {
        id: spyFactory
        SignalSpy {}
    }
    Component {
        id: homeFactory
        Ui.HomeView {
            continueEntries: [
                {id: "a", title: "Activity", subtitle: "Return", running: true},
                {id: "b", title: "Second"}
            ]
            favoriteEntries: [{id: "f", title: "Favorite", monogram: "F"}]
            newEntries: [{id: "n", title: "New"}]
            applicationEntries: [{id: "app", title: "Editor", monogram: "E"}]
        }
    }
    Component {
        id: powerFactory
        Power.PowerMenu {
            clockText: "12:00"
            dateText: "Monday"
            closingApplications: ["Playback"]
        }
    }
    Component {
        id: playerFactory
        Player.PlayerView {
            width: 1920
            height: 1080
            title: "North"
            position: 30
            duration: 120
            playing: false
            seekSupported: true
            audioTracks: [{id: "a1", label: "One"}, {id: "a2", label: "Two"}]
            subtitleTracks: [{id: "s1", label: "Notes"}]
            audioTrackId: "a1"
            subtitleTrackId: "s1"
        }
    }
    Component {
        id: launchFactory
        Ui.LaunchOverlay {
            anchors.fill: parent
            serviceTitle: "Example"
            failure: "failed"
        }
    }
    Component {
        id: libraryFactory
        Library.LibraryView {}
    }
    Component {
        id: settingsFactory
        Settings.SettingsView {
            categories: [{id: "applications", title: "Applications", icon: "apps"}]
            rows: [{
                id: "label", categoryId: "applications", title: "Label",
                description: "Shown under the icon", type: "string", currentValue: "note",
                constraints: {maximumLength: 40}, enabled: true
            }]
        }
    }
    function init() {
        // Qt Quick Test keeps the pointer between fixtures. Park it before a
        // view exists so a queued hover cannot steal the first focus.
        mouseMove(suite, 10, 10)
    }
    function cleanup() {
        releaseAll()
    }
    function own(item) {
        owned.push(item)
        return item
    }
    function releaseAll() {
        // Spies are created after their targets, so they are released first.
        while (owned.length) {
            const item = owned.pop()
            if (item)
                item.destroy()
        }
    }
    function make(factory, properties) {
        mouseMove(suite, 10, 10)
        const item = own(createTemporaryObject(factory, suite, properties || {}))
        verify(item)
        return item
    }
    function watch(item, signal) {
        const spy = own(createTemporaryObject(spyFactory, suite, {signalName: signal}))
        verify(spy)
        spy.target = item
        spy.clear()
        return spy
    }
    function childOf(root, name) {
        const item = findChild(root, name)
        verify(item !== null, name)
        return item
    }
    function focused(root, name) {
        tryCompare(childOf(root, name), "activeFocus", true, 5000, name)
    }
    function catalogue() {
        return {
            columns: 4,
            tabs: [{id: "films", title: "Films"}],
            entries: [
                {id: "a", tabId: "films", title: "Alpha", subtitle: "2001", kind: "film"},
                {id: "b", tabId: "films", title: "Bravo", subtitle: "2002", kind: "film"}
            ]
        }
    }

    function test_row_modal_power_sheet_navigation() {
        const home = make(homeFactory, {returnActivityId: "a"})
        const returns = watch(home, "returnRequested")
        focused(home, "entry-a")
        keyClick(Qt.Key_Right)
        focused(home, "entry-b")
        keyClick(Qt.Key_Menu)
        focused(home, "nav-0")
        keyClick(Qt.Key_Backspace)
        compare(home.navOpen, false)
        compare(home.screen, "home")
        focused(home, "entry-b")
        compare(returns.count, 0)
        keyClick(Qt.Key_Menu)
        focused(home, "nav-0")
        keyClick(Qt.Key_Escape)
        compare(home.navOpen, false)
        focused(home, "entry-b")
        keyClick(Qt.Key_Menu)
        focused(home, "nav-0")
        keyClick(Qt.Key_Home)
        compare(home.navOpen, false)
        compare(home.screen, "home")
        focused(home, "entry-a")
        compare(returns.count, 0)
        releaseAll()

        const menu = make(powerFactory)
        const shutdowns = watch(menu, "shutdownRequested")
        const restarts = watch(menu, "restartRequested")
        const homes = watch(menu, "homeRequested")
        focused(menu, "sleepButton")
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Down)
        focused(menu, "shutdownButton")
        keyClick(Qt.Key_Return)
        compare(menu.step, "confirm")
        focused(menu, "cancelButton")
        keyClick(Qt.Key_Right)
        focused(menu, "confirmButton")
        keyClick(Qt.Key_Backspace)
        compare(menu.step, "menu")
        compare(shutdowns.count, 0)
        focused(menu, "shutdownButton")
        keyClick(Qt.Key_Return)
        focused(menu, "cancelButton")
        keyClick(Qt.Key_Right)
        focused(menu, "confirmButton")
        keyClick(Qt.Key_Escape)
        compare(menu.step, "menu")
        compare(shutdowns.count, 0)
        focused(menu, "shutdownButton")
        keyClick(Qt.Key_Return)
        focused(menu, "cancelButton")
        keyClick(Qt.Key_Right)
        focused(menu, "confirmButton")
        keyClick(Qt.Key_Home)
        compare(menu.step, "menu")
        compare(shutdowns.count, 0)
        compare(restarts.count, 0)
        compare(homes.count, 1)
        focused(menu, "shutdownButton")
        releaseAll()

        const player = make(playerFactory)
        const subtitles = watch(player, "selectSubtitle")
        const audios = watch(player, "selectAudio")
        const leaves = watch(player, "leave")
        const stops = watch(player, "stop")
        const playerHomes = watch(player, "homeRequested")
        player.forceActiveFocus()
        keyClick(Qt.Key_Down)
        compare(player.osdFocus, "subtitles")
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "subtitles")
        compare(childOf(player, "trackSheet").visible, true)
        keyClick(Qt.Key_Backspace)
        compare(player.sheetKind, "")
        compare(player.osdVisible, true)
        compare(player.osdFocus, "subtitles")
        compare(subtitles.count, 0)
        compare(leaves.count, 0)
        compare(stops.count, 0)
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "subtitles")
        keyClick(Qt.Key_Escape)
        compare(player.sheetKind, "")
        compare(player.osdFocus, "subtitles")
        compare(subtitles.count, 0)
        compare(leaves.count, 0)
        keyClick(Qt.Key_Return)
        compare(player.sheetKind, "subtitles")
        compare(player.subtitleTrackId, "s1")
        keyClick(Qt.Key_Home)
        compare(player.sheetKind, "")
        compare(player.osdVisible, true)
        compare(player.osdFocus, "subtitles")
        compare(player.playing, false)
        compare(player.subtitleTrackId, "s1")
        compare(playerHomes.count, 1)
        compare(subtitles.count, 0)
        compare(audios.count, 0)
        compare(leaves.count, 0)
        compare(stops.count, 0)
        releaseAll()

        const launch = make(launchFactory)
        const retries = watch(launch, "retryRequested")
        const dismisses = watch(launch, "dismissRequested")
        const launchHomes = watch(launch, "homeRequested")
        const cancels = watch(launch, "cancelRequested")
        focused(launch, "launchRetry")
        keyClick(Qt.Key_Backspace)
        compare(dismisses.count, 1)
        compare(retries.count, 0)
        compare(cancels.count, 0)
        compare(launchHomes.count, 0)
        releaseAll()
        const launchAgain = make(launchFactory)
        const retriesAgain = watch(launchAgain, "retryRequested")
        const dismissesAgain = watch(launchAgain, "dismissRequested")
        const homesAgain = watch(launchAgain, "homeRequested")
        focused(launchAgain, "launchRetry")
        keyClick(Qt.Key_Escape)
        compare(dismissesAgain.count, 1)
        compare(retriesAgain.count, 0)
        compare(homesAgain.count, 0)
        releaseAll()
        const launchHome = make(launchFactory)
        const retriesHome = watch(launchHome, "retryRequested")
        const dismissesHome = watch(launchHome, "dismissRequested")
        const homesHome = watch(launchHome, "homeRequested")
        const cancelsHome = watch(launchHome, "cancelRequested")
        focused(launchHome, "launchRetry")
        keyClick(Qt.Key_Home)
        compare(dismissesHome.count, 1)
        compare(retriesHome.count, 0)
        compare(cancelsHome.count, 0)
        compare(homesHome.count, 1)
    }

    function test_row_nested_screen() {
        const home = make(homeFactory)
        focused(home, "entry-a")
        keyClick(Qt.Key_Menu)
        focused(home, "nav-0")
        keyClick(Qt.Key_Down)
        focused(home, "nav-1")
        compare(childOf(home, "nav-1").text, "Library")
        keyClick(Qt.Key_Return)
        compare(home.screen, "library")
        compare(home.navOpen, false)
        focused(home, "sectionBack")
        keyClick(Qt.Key_Backspace)
        compare(home.screen, "home")
        compare(home.navOpen, false)
        focused(home, "entry-a")
        keyClick(Qt.Key_Menu)
        for (let step = 0; step < 3; ++step)
            keyClick(Qt.Key_Down)
        focused(home, "nav-3")
        compare(childOf(home, "nav-3").text, "Applications")
        keyClick(Qt.Key_Return)
        compare(home.screen, "applications")
        focused(home, "entry-app")
        keyClick(Qt.Key_Home)
        compare(home.screen, "home")
        compare(home.navOpen, false)
        focused(home, "entry-a")
        releaseAll()

        const library = make(libraryFactory, catalogue())
        const backs = watch(library, "backRequested")
        const homes = watch(library, "homeRequested")
        focused(library, "tab-films")
        keyClick(Qt.Key_Down)
        focused(library, "poster-a")
        keyClick(Qt.Key_Return)
        focused(library, "detailPlay")
        compare(library.detailOpen, true)
        keyClick(Qt.Key_Backspace)
        compare(library.detailOpen, false)
        compare(backs.count, 0)
        focused(library, "poster-a")
        keyClick(Qt.Key_Escape)
        compare(backs.count, 1)
        focused(library, "poster-a")
        keyClick(Qt.Key_Return)
        focused(library, "detailPlay")
        keyClick(Qt.Key_Home)
        compare(homes.count, 1)
        compare(library.detailOpen, false)
        focused(library, "poster-a")
        compare(backs.count, 1)
    }

    function test_row_home_with_return_activity() {
        const home = make(homeFactory, {returnActivityId: "a"})
        const returns = watch(home, "returnRequested")
        focused(home, "entry-a")
        keyClick(Qt.Key_Right)
        focused(home, "entry-b")
        keyClick(Qt.Key_Home)
        compare(home.screen, "home")
        compare(home.navOpen, false)
        focused(home, "entry-a")
        compare(returns.count, 0)
        keyClick(Qt.Key_Home)
        focused(home, "entry-a")
        compare(home.navOpen, false)
        compare(returns.count, 0)
        keyClick(Qt.Key_Right)
        focused(home, "entry-b")
        keyClick(Qt.Key_Backspace)
        compare(returns.count, 1)
        compare(returns.signalArguments[0][0], "a")
        compare(home.navOpen, false)
        keyClick(Qt.Key_Escape)
        compare(returns.count, 2)
        compare(returns.signalArguments[1][0], "a")
        compare(home.navOpen, false)
    }

    function test_row_home_without_return_activity() {
        const home = make(homeFactory, {returnActivityId: ""})
        const returns = watch(home, "returnRequested")
        focused(home, "entry-a")
        keyClick(Qt.Key_Right)
        focused(home, "entry-b")
        keyClick(Qt.Key_Home)
        compare(home.screen, "home")
        compare(home.navOpen, false)
        focused(home, "entry-b")
        compare(returns.count, 0)
        keyClick(Qt.Key_Home)
        compare(home.screen, "home")
        compare(home.navOpen, false)
        focused(home, "entry-b")
        compare(returns.count, 0)
        keyClick(Qt.Key_Backspace)
        compare(home.navOpen, true)
        compare(home.screen, "home")
        focused(home, "nav-0")
        compare(returns.count, 0)
        keyClick(Qt.Key_Escape)
        compare(home.navOpen, false)
        focused(home, "entry-b")
        keyClick(Qt.Key_Escape)
        compare(home.navOpen, true)
        focused(home, "nav-0")
        compare(returns.count, 0)
    }

    function test_backspace_deletes_and_escape_closes_scope() {
        const settings = make(settingsFactory)
        const changes = watch(settings, "valueChangeRequested")
        const dismissals = watch(settings, "dismissRequested")
        focused(settings, "category-applications")
        keyClick(Qt.Key_Right)
        focused(settings, "setting-label")
        keyClick(Qt.Key_Return)
        const editor = childOf(settings, "editor-label")
        tryCompare(editor, "visible", true)
        tryCompare(editor, "activeFocus", true)
        compare(editor.text, "note")
        keyClick(Qt.Key_Backspace)
        compare(editor.text, "not")
        compare(settings.editingKey, "label")
        tryCompare(editor, "activeFocus", true)
        compare(changes.count, 0)
        compare(dismissals.count, 0)
        keyClick(Qt.Key_Escape)
        compare(settings.editingKey, "")
        compare(changes.count, 0)
        compare(dismissals.count, 0)
        focused(settings, "setting-label")
        compare(childOf(settings, "value-label").text, "note")
        keyClick(Qt.Key_Escape)
        compare(dismissals.count, 0)
        focused(settings, "category-applications")
        keyClick(Qt.Key_Escape)
        compare(dismissals.count, 1)
        compare(changes.count, 0)
        focused(settings, "category-applications")
    }
}
