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
    // Home left the menu while a request was in flight (spec §6.3, owner
    // decision 2026-10-23): the request goes on, and a refusal or timeout
    // that arrives later reopens the power scope on that refusal.
    property bool awaitingAnswer: false
    // The menu is open only to show such a late refusal: Back closes it.
    property bool lateRefusal: false
    // True while a dialog above this overlay is on screen (the launch
    // overlay): a late refusal waits for it to close instead of taking focus
    // behind it.
    property bool holdLateAnswer: false
    // The item that had focus when a late refusal opened; focus returns there.
    property Item lateFocusItem: null
    readonly property string controlPending: control ? control.pendingAction : ""
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
             : action === "power-off" ? "shutdown" : action === "switch-to-desktop" ? "desktop" : "";
    }
    function openMenu() {
        if (menuOpen)
            return;
        if (control) {
            control.reset();
            control.readPower();
        }
        awaitingAnswer = false;
        lateRefusal = false;
        lateFocusItem = null;
        powerMenu.step = "menu";
        powerMenu.pendingAction = "";
        powerMenu.menuFocus = "sleep";
        menuOpen = true;
        // The menu's scope remembers its last row and restores it as soon as
        // it is shown; a new opening starts on the first row again.
        // A request left in flight by Home is shown as it was left.
        Qt.callLater(() => {
            const pending = control ? idFor(control.pendingAction) : "";
            if (pending.length > 0) {
                powerMenu.showPending(pending);
                return;
            }
            powerMenu.menuFocus = "sleep";
            powerMenu.restoreMenuFocus();
        });
    }
    function closeMenu() {
        if (!menuOpen)
            return;
        menuOpen = false;
        lateRefusal = false;
        if (control)
            control.reset();
        closed();
    }
    function leaveHome() {
        // The request is not cancelled; its answer is shown later.
        awaitingAnswer = controlPending.length > 0;
        lateFocusItem = null;
        closeMenu();
        homeRequested();
    }
    // The answer to a request left behind by Home. Success needs nothing:
    // the system suspends or powers off. A refusal or timeout reopens the
    // power scope on the refusal pane, which stays until Back, so it also
    // reaches a user who is in another application when it arrives.
    onControlPendingChanged: if (controlPending.length === 0) Qt.callLater(settleLateAnswer)
    onHoldLateAnswerChanged: if (!holdLateAnswer) Qt.callLater(settleLateAnswer)
    function settleLateAnswer() {
        if (!awaitingAnswer || !control || controlPending.length > 0)
            return;
        if (menuOpen || control.refusal.length === 0) {
            awaitingAnswer = false;
            return;
        }
        // Shown once the dialog above has closed.
        if (holdLateAnswer)
            return;
        awaitingAnswer = false;
        lateFocusItem = Window.activeFocusItem;
        powerMenu.step = "menu";
        powerMenu.pendingAction = "";
        lateRefusal = true;
        menuOpen = true;
    }
    // A lock or sleep hides the shell. After an accepted request the menu
    // has done its job: wake returns to the screen underneath (§5.7).
    function sessionBlocked() {
        // The lifecycle signal and the daemon's reply travel on different
        // buses: a request still in flight counts too.
        // A pending Suspend may be the reason for the block; its reply can
        // arrive after it. A pending Restart or Shut down blocked by an
        // unrelated lock stays open, so its answer is shown after unlock.
        if (menuOpen && control && (control.acceptedAction.length > 0 || control.pendingAction === "suspend"))
            closeMenu();
    }
    // After a late refusal is dismissed with Back, focus returns to the item
    // the user was on. Returns false when there is none to return to.
    function restoreLateFocus() {
        const item = lateFocusItem;
        lateFocusItem = null;
        return item !== null && Ui.Focus.take(item);
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
        // A control without the session switch (an older double) offers none.
        desktopSwitchAvailability: root.control && root.control.sessionSwitch ? root.control.sessionSwitch : "unknown"
        inhibitors: root.control ? root.control.inhibitors : []
        busyAction: root.control ? root.idFor(root.control.pendingAction) : ""
        acceptedAction: root.control ? root.idFor(root.control.acceptedAction) : ""
        refusalCode: root.control ? root.control.refusal : ""
        refusalAction: root.control ? root.control.refusalAction : ""
        onSleepRequested: if (root.control) root.control.request("suspend")
        onRestartRequested: if (root.control) root.control.request("reboot")
        onShutdownRequested: if (root.control) root.control.request("power-off")
        onDesktopSwitchRequested: if (root.control) root.control.request("switch-to-desktop")
        onRefusalDismissed: {
            if (root.lateRefusal)
                root.closeMenu();
            else if (root.control)
                root.control.reset();
        }
        onAcceptedDismissed: root.closeMenu()
        onDismissRequested: root.closeMenu()
        onHomeRequested: root.leaveHome()
        onAmbientRequested: root.ambientRequested()
    }

    Ui.VolumeHud {
        id: volumeHud
        z: 10
        anchors.horizontalCenter: parent.horizontalCenter
        y: 48
    }
}
