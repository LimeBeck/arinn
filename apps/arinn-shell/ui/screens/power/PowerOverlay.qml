// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui

// The shell's system overlay for power and volume (spec §5.7): the power
// scope over every screen and the non-modal volume HUD. `control` is the
// SystemControl model (ADR-0108) or a test double with the same members; this
// file only maps its state to the menu and never reports an action as done.
FocusScope {
    id: root
    objectName: "powerOverlay"
    width: 1920
    height: 1080

    property var control: null
    // Running applications from the live registry ({title}).
    property var runningApplications: []
    // False while the registry cannot be read (no daemon).
    property bool registryKnown: true
    // Volume and mute keys work only while the shell is on screen (OQ-04).
    property bool keysEnabled: false
    readonly property bool open: menuOpen
    property bool menuOpen: false
    // The volume HUD and the power menu read nothing else.
    readonly property alias menu: powerMenu
    readonly property alias hud: volumeHud

    // The menu closed; the caller returns focus to the opener.
    signal closed()
    signal homeRequested()
    signal ambientRequested()
    signal volumeUnavailable()

    function idFor(action) {
        return action === "suspend" ? "sleep" : action === "reboot" ? "restart"
             : action === "power-off" ? "shutdown" : "";
    }
    function openMenu() {
        if (menuOpen)
            return;
        if (control) {
            control.reset();
            control.readPower();
        }
        powerMenu.step = "menu";
        powerMenu.pendingAction = "";
        powerMenu.menuFocus = "sleep";
        menuOpen = true;
        // The menu's scope remembers its last row and restores it as soon as
        // it is shown; a new opening starts on the first row again.
        Qt.callLater(() => {
            powerMenu.menuFocus = "sleep";
            powerMenu.restoreMenuFocus();
        });
    }
    function closeMenu() {
        if (!menuOpen)
            return;
        menuOpen = false;
        if (control)
            control.reset();
        closed();
    }
    // A lock or sleep hides the shell. After an accepted request the menu
    // has done its job: wake returns to the screen underneath (§5.7).
    function sessionBlocked() {
        // The lifecycle signal and the daemon's reply travel on different
        // buses: a request still in flight counts too.
        if (menuOpen && control && (control.acceptedAction.length > 0 || control.pendingAction.length > 0))
            closeMenu();
    }
    function volumeKey(key) {
        if (!control)
            return;
        if (key === Qt.Key_VolumeUp)
            control.raiseVolume();
        else if (key === Qt.Key_VolumeDown)
            control.stepVolume(-1);
        else if (key === Qt.Key_VolumeMute)
            control.toggleMuted();
    }

    Connections {
        target: root.control
        ignoreUnknownSignals: true
        function onVolumeShown() { volumeHud.show(root.control.volumeLevel, root.control.volumeMuted); }
        // The HUD says so itself, so a failure shows over managed playback
        // too, where the feedback strip is hidden.
        function onVolumeFailed(refusal) { volumeHud.showUnavailable(); root.volumeUnavailable(); }
    }

    // Window shortcuts: they reach the shell whatever screen holds focus,
    // and never while another application is active.
    Shortcut {
        sequences: ["Volume Up"]
        context: Qt.WindowShortcut
        enabled: root.keysEnabled
        onActivated: root.volumeKey(Qt.Key_VolumeUp)
    }
    Shortcut {
        sequences: ["Volume Down"]
        context: Qt.WindowShortcut
        enabled: root.keysEnabled
        onActivated: root.volumeKey(Qt.Key_VolumeDown)
    }
    Shortcut {
        sequences: ["Volume Mute"]
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: root.keysEnabled
        onActivated: root.volumeKey(Qt.Key_VolumeMute)
    }

    PowerMenu {
        id: powerMenu
        anchors.fill: parent
        visible: root.menuOpen
        enabled: root.menuOpen
        focus: root.menuOpen
        closingApplications: root.runningApplications
        closingKnown: root.registryKnown
        sleepAvailability: root.control ? root.control.suspend : "unknown"
        restartAvailability: root.control ? root.control.reboot : "unknown"
        shutdownAvailability: root.control ? root.control.powerOff : "unknown"
        inhibitors: root.control ? root.control.inhibitors : []
        busyAction: root.control ? root.idFor(root.control.pendingAction) : ""
        acceptedAction: root.control ? root.idFor(root.control.acceptedAction) : ""
        refusalCode: root.control ? root.control.refusal : ""
        refusalAction: root.control ? root.control.refusalAction : ""
        onSleepRequested: if (root.control) root.control.request("suspend")
        onRestartRequested: if (root.control) root.control.request("reboot")
        onShutdownRequested: if (root.control) root.control.request("power-off")
        onRefusalDismissed: if (root.control) root.control.reset()
        onAcceptedDismissed: root.closeMenu()
        onDismissRequested: root.closeMenu()
        onHomeRequested: { root.closeMenu(); root.homeRequested(); }
        onAmbientRequested: root.ambientRequested()
    }

    Ui.VolumeHud {
        id: volumeHud
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        y: 48
    }
}
