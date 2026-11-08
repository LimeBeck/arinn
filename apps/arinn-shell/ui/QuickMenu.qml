// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import "screens/music" as Music

// The quick menu (spec §5.7, ADR-0106): a side sheet over the current
// Activity or over the shell. It is the sheet focus scope (§6.1). Intents
// leave this file; nothing here talks to the daemon, a provider or the
// platform. There is no design handoff for it yet: the layout follows the
// tile action sheet (handoff cl-1) and the power menu rows.
FocusScope {
    id: root
    objectName: "quickMenu"
    width: 1920; height: 1080

    // The live Activity is behind the sheet: dim it less than the shell.
    property bool overActivity: false
    // The daemon is connected. Without it only Open Home and Close Arinn
    // stay available (degraded form).
    property bool connected: true
    // Every media and music session the shell knows ({id, kind: "music" or
    // "media", title, subtitle, artwork, state, playPause, previous, next,
    // open, eyebrow, positionUs, durationUs}). Only playing or paused ones
    // take part (ADR-0142 MH-7, music brief *Presence › Quick menu*, #805).
    property var sessions: []
    // Playback state changes the shell observed: id → {state, at}. `at` is 0
    // for a session first seen while the sheet's data is being primed: at
    // start and after a reconnect, the catalogues report their sessions one
    // after another, and that order is no change (Codex review of #826).
    property var observed: ({})
    property int observedTick: 0
    property int primeInterval: 2000
    Timer { id: primingTimer; interval: root.primeInterval }
    readonly property bool priming: primingTimer.running
    function observe() {
        const next = {};
        let tick = observedTick;
        for (const session of sessions || []) {
            if (!session || !session.id) continue;
            const before = observed[session.id];
            const state = String(session.state || "");
            if (!before)
                next[session.id] = {state: state, at: primingTimer.running ? 0 : ++tick};
            else if (before.state !== state)
                next[session.id] = {state: state, at: ++tick};
            else
                next[session.id] = before;
        }
        observedTick = tick;
        observed = next;
    }
    onSessionsChanged: observe()
    // A lost daemon forgets what was observed; the sessions it reports again
    // are primed anew.
    onConnectedChanged: {
        if (!connected) {
            observed = ({});
            primingTimer.stop();
        } else {
            primingTimer.restart();
            observe();
        }
    }
    Component.onCompleted: {
        primingTimer.restart();
        observe();
    }
    // One precedence order (brief *Two sessions at once*): playing above
    // paused; then the change observed last; with none observed, the native
    // music session; otherwise the order given. No product is preferred.
    readonly property var rankedSessions: {
        if (!connected) return [];
        const list = (sessions || []).filter(session => session && session.id
            && (session.state === "playing" || session.state === "paused"));
        const at = session => observed[session.id] ? observed[session.id].at : 0;
        return list.map((session, index) => ({session: session, index: index}))
            .sort((a, b) => {
                const playing = (b.session.state === "playing") - (a.session.state === "playing");
                if (playing) return playing;
                const changed = at(b.session) - at(a.session);
                if (changed) return changed;
                const music = (b.session.kind === "music") - (a.session.kind === "music");
                if (music) return music;
                return a.index - b.index;
            }).map(entry => entry.session);
    }
    // The session the card shows, and the one in the compact row under it.
    readonly property var nowPlaying: rankedSessions.length ? rankedSessions[0] : null
    readonly property var otherSession: rankedSessions.length > 1 ? rankedSessions[1] : null
    // "muted", "unmuted", or empty when unknown (SystemStatus1).
    property string audio: ""
    // Volume and mute are executed by the platform layer through
    // SystemControl1 (ADR-0108). Without it the row shows the state only.
    property bool volumeControl: false
    // The platform's last volume reply (SystemControl1); while it is not
    // known the row falls back to `audio` and shows no level.
    property bool volumeKnown: false
    property bool volumeMuted: false
    property int volumeLevel: 0
    // "wifi", "wired", "other", "disconnected", or empty when unknown.
    property string network: ""
    property int wifiLevel: 0
    property string clockText: ""
    property bool pointerMode: InputMode.pointerActive
    // A short result line, such as an action that is not available yet.
    property string notice: ""
    function showNotice(text) {
        notice = text;
        noticeTimer.restart();
    }
    Timer { id: noticeTimer; interval: 3000; onTriggered: root.notice = "" }

    signal playPauseRequested(string sessionId)
    signal previousRequested(string sessionId)
    signal nextRequested(string sessionId)
    signal openPlayerRequested(string sessionId)
    signal muteToggleRequested()
    signal volumeStepRequested(int step)
    signal sleepRequested()
    signal powerMenuRequested()
    signal homeRequested()
    signal closeShellRequested()
    signal backRequested()

    readonly property bool mediaAvailable: connected && !!nowPlaying && !!nowPlaying.id
    readonly property bool playing: mediaAvailable && nowPlaying.state === "playing"
    readonly property bool muted: volumeKnown ? volumeMuted : audio === "muted"
    readonly property string audioText: volumeKnown
        ? (volumeMuted ? qsTr("Muted") : qsTr("Sound on · %1%").arg(volumeLevel))
        : audio === "muted" ? qsTr("Muted")
        : audio === "unmuted" ? qsTr("Sound on") : qsTr("Sound state unknown")
    readonly property string networkText: {
        if (!connected || network === "") return qsTr("Network state unknown");
        if (network === "disconnected") return qsTr("Not connected");
        if (network === "wired") return qsTr("Connected by cable");
        if (network === "wifi")
            return wifiLevel > 0 ? qsTr("Wi-Fi · signal %1 of 4").arg(wifiLevel) : qsTr("Wi-Fi");
        return qsTr("Connected");
    }
    // Focusable rows in display order; disabled ones are skipped. The
    // media card counts as one row: moving onto it lands on play/pause, or
    // its first working control.
    readonly property var rows: [mediaCard, otherRow, mediaRow, volumeRow, sleepRow, powerRow, homeRow, closeRow]
    readonly property var cardControls: [cardPrevious, cardPlayPause, cardNext, cardOpen]
    property var lastFocused: null
    function within(item, ancestor) {
        for (let node = item; node; node = node.parent)
            if (node === ancestor) return true;
        return false;
    }
    function focusRow(row) {
        if (!row || !row.visible) return false;
        if (row === mediaCard) {
            for (const control of [cardPlayPause, cardNext, cardPrevious, cardOpen])
                if (Focus.take(control)) { reveal(mediaCard); return true; }
            return false;
        }
        if (!Focus.take(row)) return false;
        reveal(row);
        // A row may change height with focus, and the rows above it as focus
        // leaves them: reveal again once the column has laid out.
        Qt.callLater(revealCurrent);
        return true;
    }
    function revealCurrent() {
        const row = currentRow();
        if (row) reveal(row);
    }
    // Focus the first available row, as when the sheet opens: the column
    // starts at the top, with focus on play/pause.
    function start() {
        lastFocused = null;
        column.contentY = 0;
        restoreFocus();
    }
    function restoreFocus() {
        if (lastFocused && lastFocused.visible && Focus.take(lastFocused)) {
            reveal(within(lastFocused, mediaCard) ? mediaCard : lastFocused);
            return;
        }
        for (const row of rows) if (focusRow(row)) return;
    }
    function currentRow() {
        const window = root.Window.window;
        const item = window ? window.activeFocusItem : null;
        return rows.find(row => row.visible && (row === item || within(item, row))) || null;
    }
    function move(step) {
        const index = rows.indexOf(currentRow());
        for (let i = index + step; i >= 0 && i < rows.length; i += step)
            if (focusRow(rows[i])) return;
    }
    // Left and Right along the card's row of buttons, stopping at its ends.
    function moveInCard(step) {
        const window = root.Window.window;
        const item = window ? window.activeFocusItem : null;
        const index = cardControls.indexOf(item);
        if (index < 0) return;
        for (let i = index + step; i >= 0 && i < cardControls.length; i += step)
            if (Focus.take(cardControls[i])) return;
    }
    // The rows scroll with focus (brief *Fitting the sheet*): the focused row
    // never sits at an edge and the next row stays partly visible.
    readonly property int edgeRoom: 48
    function reveal(row) {
        if (!row) return;
        const top = row.mapToItem(columnContent, 0, 0).y;
        const bottom = top + row.height;
        let next = column.contentY;
        if (bottom + edgeRoom > next + column.height) next = bottom + edgeRoom - column.height;
        if (top - edgeRoom < next) next = top - edgeRoom;
        column.contentY = Math.max(0, Math.min(next, Math.max(0, column.contentHeight - column.height)));
    }
    // A row that becomes unavailable hands focus to the next available one,
    // or the previous one (§6.1).
    function settle(row) {
        if ((row.enabled && row.visible) || lastFocused !== row || !root.activeFocus) return;
        const owner = within(row, mediaCard) ? mediaCard : row;
        if (owner === mediaCard && mediaCard.visible && focusRow(mediaCard)) return;
        const index = rows.indexOf(owner);
        for (let i = index + 1; i < rows.length; ++i) if (focusRow(rows[i])) return;
        for (let i = index - 1; i >= 0; --i) if (focusRow(rows[i])) return;
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Up) move(-1);
        else if (event.key === Qt.Key_Down) move(1);
        else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            if (!event.isAutoRepeat) backRequested();
        } else if (event.key === Qt.Key_Home) {
            if (!event.isAutoRepeat) homeRequested();
        } else if ((event.key === Qt.Key_Left || event.key === Qt.Key_Right) && currentRow() === mediaCard) {
            moveInCard(event.key === Qt.Key_Right ? 1 : -1);
        } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Right || event.key === Qt.Key_Menu
                   || event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
            // Focus stays inside the sheet scope.
        } else return;
        event.accepted = true;
    }

    // The Activity, or the shell screen, stays visible behind the sheet.
    Rectangle {
        objectName: "quickMenuScrim"
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, root.overActivity ? 0.35 : 0.55)
    }
    MouseArea { anchors.fill: parent; onClicked: root.backRequested() }

    component Glyph: Canvas {
        id: glyph
        property string kind: ""
        property color ink: Theme.textPrimary
        width: 38; height: 38
        onInkChanged: requestPaint()
        onKindChanged: requestPaint()
        Component.onCompleted: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            ctx.strokeStyle = ink;
            ctx.fillStyle = ink;
            ctx.lineWidth = 2.6;
            ctx.lineCap = "round";
            ctx.lineJoin = "round";
            const cx = width / 2, cy = height / 2;
            ctx.beginPath();
            if (kind === "pause") {
                ctx.fillRect(cx - 9, cy - 11, 6, 22);
                ctx.fillRect(cx + 3, cy - 11, 6, 22);
            } else if (kind === "play") {
                ctx.moveTo(cx - 7, cy - 12); ctx.lineTo(cx + 11, cy); ctx.lineTo(cx - 7, cy + 12);
                ctx.closePath(); ctx.fill();
            } else if (kind === "previous" || kind === "next") {
                const s = kind === "next" ? 1 : -1;
                ctx.moveTo(cx - 8 * s, cy - 10); ctx.lineTo(cx + 5 * s, cy); ctx.lineTo(cx - 8 * s, cy + 10);
                ctx.closePath(); ctx.fill();
                ctx.fillRect(s > 0 ? cx + 6 : cx - 10, cy - 10, 4, 20);
            } else if (kind === "sound" || kind === "muted") {
                ctx.moveTo(cx - 14, cy - 5); ctx.lineTo(cx - 8, cy - 5); ctx.lineTo(cx, cy - 12);
                ctx.lineTo(cx, cy + 12); ctx.lineTo(cx - 8, cy + 5); ctx.lineTo(cx - 14, cy + 5);
                ctx.closePath(); ctx.fill();
                ctx.beginPath();
                if (kind === "muted") {
                    ctx.moveTo(cx + 5, cy - 5); ctx.lineTo(cx + 14, cy + 5);
                    ctx.moveTo(cx + 14, cy - 5); ctx.lineTo(cx + 5, cy + 5);
                } else {
                    ctx.arc(cx + 2, cy, 8, -0.9, 0.9);
                    ctx.moveTo(cx + 2 + 14 * Math.cos(-0.9), cy + 14 * Math.sin(-0.9));
                    ctx.arc(cx + 2, cy, 14, -0.9, 0.9);
                }
                ctx.stroke();
            } else if (kind === "network") {
                ctx.arc(cx, cy + 9, 2.6, 0, Math.PI * 2); ctx.fill();
                for (let part = 0; part < 3; ++part) {
                    ctx.beginPath();
                    ctx.arc(cx, cy + 9, 7 + part * 6, Math.PI * 1.22, Math.PI * 1.78);
                    ctx.stroke();
                }
            } else if (kind === "sleep") {
                ctx.arc(cx - 1, cy, 12, 0.7, 5.7);
                ctx.arc(cx + 7, cy - 3, 10, 4.6, 1.5, true);
                ctx.fill();
            } else if (kind === "power") {
                ctx.arc(cx, cy + 2, 11, 0.85, Math.PI - 0.85, true);
                ctx.stroke();
                ctx.beginPath();
                ctx.moveTo(cx, cy - 13); ctx.lineTo(cx, cy + 2);
                ctx.stroke();
            } else if (kind === "home") {
                ctx.moveTo(cx - 12, cy - 1); ctx.lineTo(cx, cy - 12); ctx.lineTo(cx + 12, cy - 1);
                ctx.moveTo(cx - 8, cy - 4); ctx.lineTo(cx - 8, cy + 11); ctx.lineTo(cx + 8, cy + 11);
                ctx.lineTo(cx + 8, cy - 4);
                ctx.stroke();
            } else if (kind === "close") {
                ctx.moveTo(cx - 9, cy - 9); ctx.lineTo(cx + 9, cy + 9);
                ctx.moveTo(cx + 9, cy - 9); ctx.lineTo(cx - 9, cy + 9);
                ctx.stroke();
            }
        }
    }

    // A round button 64 on the media card.
    component CardButton: IconButton {
        id: button
        property string glyph: ""
        width: 64; height: 64
        iconName: glyph === "previous" ? "skip_previous" : glyph === "next" ? "skip_next" : glyph
        iconSize: 38
        focusScale: 1.06; focusLift: 0; liftFromBottom: false
        pointerMode: root.pointerMode
        Accessible.name: text
        onActiveFocusChanged: if (activeFocus) root.lastFocused = button
        onEnabledChanged: Qt.callLater(() => root.settle(button))
    }
    component SheetRow: Surface {
        id: row
        property string glyph: ""
        property string description: ""
        width: 624; height: 96
        // Rows are containers (ADR-0136): no plate at rest, an 8 % plate
        // and the rim in focus (#756 C21, C30).
        shapeRole: "container"
        cornerRadius: Theme.containerRadius
        focusScale: 1
        focusLift: 0
        liftFromBottom: false
        pointerMode: root.pointerMode
        onActiveFocusChanged: if (activeFocus) root.lastFocused = row
        onEnabledChanged: Qt.callLater(() => root.settle(row))
        contentItem: Item {
            Rectangle {
                id: plate
                x: 28; anchors.verticalCenter: parent.verticalCenter
                width: 64; height: 64; radius: 32
                color: Qt.rgba(1, 1, 1, 0.08)
                Glyph {
                    anchors.centerIn: parent
                    kind: row.glyph
                    ink: row.activeFocus ? Theme.textPrimary : Theme.textSecondary
                }
            }
            Column {
                x: plate.x + plate.width + 24
                width: parent.width - x - 28
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                UiText {
                    objectName: row.objectName + "Label"
                    width: parent.width
                    text: row.text
                    role: "control"; font.pixelSize: 32
                    color: Theme.textPrimary
                    elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                }
                UiText {
                    objectName: row.objectName + "Description"
                    width: parent.width
                    visible: text.length > 0
                    text: row.description
                    role: "meta"
                    color: Theme.textSecondary
                    elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                }
            }
        }
    }

    Rectangle {
        id: sheet
        objectName: "quickMenuSheet"
        x: parent.width - width; y: 0
        width: 720; height: parent.height
        color: "#1d1e21"
        // The sheet takes clicks itself; only the area outside it is Back.
        MouseArea { anchors.fill: parent }
        UiText {
            objectName: "quickMenuClock"
            x: 48; y: 64
            text: root.clockText; role: "meta"; color: Theme.textSecondary; wrapMode: Text.NoWrap
        }
        UiText {
            objectName: "quickMenuTitle"
            x: 48; y: 104; width: 624
            text: qsTr("Quick menu"); role: "title"; font.pixelSize: 52
            elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
        }
        // The rows from y 200 to y 960 form one column that scrolls with
        // focus; a 96 px fade over its bottom shows that more rows follow,
        // and a 48 px fade under the title appears once it has scrolled.
        Flickable {
            id: column
            objectName: "quickMenuColumn"
            x: 24; y: 200
            width: 672; height: 760
            clip: true
            contentHeight: columnContent.height + 24
            interactive: root.pointerMode
            boundsBehavior: Flickable.StopAtBounds
            Behavior on contentY { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        Column {
            id: columnContent
            x: 24; y: 12
            // Rows that change height keep the focused one in view.
            onHeightChanged: Qt.callLater(root.revealCurrent)
            spacing: 6
            // The media card (mh-06): the session the precedence order puts
            // first, while it plays or is paused.
            Item {
                id: mediaCard
                objectName: "quickMenuMediaCard"
                width: 624; height: visible ? 284 : 0
                visible: !!root.nowPlaying
                readonly property var session: root.nowPlaying || ({})
                readonly property bool cardPlaying: session.state === "playing"
                Rectangle {
                    anchors.fill: parent
                    radius: 20
                    color: root.within(root.Window.window ? root.Window.window.activeFocusItem : null, mediaCard)
                           ? Theme.plateContainer : Qt.rgba(1, 1, 1, 0.04)
                    border.width: 1
                    border.color: Qt.rgba(1, 1, 1, 0.08)
                    clip: true
                    // A faint glow in the artwork's hue at the left.
                    Rectangle {
                        objectName: "quickMenuCardGlow"
                        x: -120; y: -40
                        width: 420; height: 360; radius: 180
                        visible: mediaCard.cardTint.a > 0
                        color: Qt.rgba(mediaCard.cardTint.r, mediaCard.cardTint.g, mediaCard.cardTint.b, 0.16)
                    }
                }
                property color cardTint: "transparent"
                Image {
                    id: tintSample
                    visible: false
                    source: mediaCard.session.artwork || ""
                    sourceSize.width: 24; sourceSize.height: 24
                    asynchronous: true
                    onStatusChanged: status === Image.Ready ? tintSampler.requestPaint() : mediaCard.cardTint = "transparent"
                }
                Canvas {
                    id: tintSampler
                    width: 24; height: 24; opacity: 0
                    renderTarget: Canvas.Image
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.reset();
                        if (tintSample.status !== Image.Ready) return;
                        ctx.drawImage(tintSample, 0, 0, width, height);
                        const data = ctx.getImageData(0, 0, width, height).data;
                        let r = 0, g = 0, b = 0, weight = 0;
                        for (let i = 0; i < data.length; i += 4) {
                            const w = (Math.max(data[i], data[i + 1], data[i + 2]) - Math.min(data[i], data[i + 1], data[i + 2])) / 255 + 0.001;
                            r += data[i] * w; g += data[i + 1] * w; b += data[i + 2] * w; weight += w;
                        }
                        mediaCard.cardTint = Qt.rgba(r / weight / 255, g / weight / 255, b / weight / 255, 1);
                    }
                }
                Music.MusicArtwork {
                    objectName: "quickMenuCardCover"
                    x: 28; y: 28
                    width: 128; height: 128
                    radius: 8
                    source: mediaCard.session.artwork || ""
                }
                Column {
                    x: 28 + 128 + 24; y: 32
                    width: 624 - x - 28
                    spacing: 6
                    Row {
                        spacing: 12
                        Row {
                            objectName: "quickMenuCardEqualiser"
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 3; height: 18
                            Repeater {
                                model: [{rest: 10, period: 1100}, {rest: 16, period: 900}, {rest: 8, period: 1300}]
                                delegate: Rectangle {
                                    required property var modelData
                                    width: 4; radius: 2
                                    anchors.bottom: parent.bottom
                                    height: modelData.rest
                                    color: mediaCard.cardPlaying ? Theme.ember : Theme.textHint
                                    SequentialAnimation on height {
                                        running: root.visible && mediaCard.visible && mediaCard.cardPlaying
                                        loops: Animation.Infinite
                                        NumberAnimation { to: 18; duration: modelData.period / 2; easing.type: Easing.InOutQuad }
                                        NumberAnimation { to: 5; duration: modelData.period / 2; easing.type: Easing.InOutQuad }
                                    }
                                }
                            }
                        }
                        UiText {
                            objectName: "quickMenuCardEyebrow"
                            width: 624 - 28 - 128 - 24 - 28 - 12 - 18
                            text: mediaCard.session.playPause === false
                                  ? (mediaCard.cardPlaying ? qsTr("Cannot be paused from Arinn") : qsTr("Cannot be resumed from Arinn"))
                                  : mediaCard.session.eyebrow || (mediaCard.cardPlaying ? qsTr("Playing") : qsTr("Paused"))
                            role: "meta"; color: Theme.textSecondary
                            elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                        }
                    }
                    UiText {
                        objectName: "quickMenuCardTitle"
                        width: parent.width
                        // A title of spaces is none (Codex review of #841).
                        text: String(mediaCard.session.title || "").trim() || qsTr("Untitled")
                        font.pixelSize: 32; font.weight: 600
                        color: Theme.textPrimary
                        elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                    }
                    UiText {
                        objectName: "quickMenuCardSubtitle"
                        width: parent.width
                        visible: text.length > 0
                        text: mediaCard.session.subtitle || ""
                        font.pixelSize: 24
                        color: Theme.textSecondary
                        elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                    }
                }
                // The Ember progress line.
                Rectangle {
                    objectName: "quickMenuCardProgress"
                    x: 28; y: 28 + 128 + 16
                    width: 624 - 56; height: 4; radius: 2
                    color: Qt.rgba(1, 1, 1, 0.2)
                    visible: (mediaCard.session.durationUs || 0) > 0
                    Rectangle {
                        width: parent.width * Math.max(0, Math.min(1, (mediaCard.session.positionUs || 0)
                                                               / Math.max(1, mediaCard.session.durationUs || 0)))
                        height: parent.height; radius: 2
                        color: Theme.ember
                    }
                }
                Row {
                    x: 28; y: 284 - 28 - 64
                    spacing: 16
                    CardButton {
                        id: cardPrevious
                        objectName: "quickMenuCardPrevious"
                        glyph: "previous"
                        text: qsTr("Previous")
                        enabled: mediaCard.visible && mediaCard.session.previous === true
                        onActivated: root.previousRequested(mediaCard.session.id)
                    }
                    CardButton {
                        id: cardPlayPause
                        objectName: "quickMenuCardPlayPause"
                        glyph: mediaCard.cardPlaying ? "pause" : "play"
                        text: mediaCard.cardPlaying ? qsTr("Pause") : qsTr("Play")
                        enabled: mediaCard.visible && mediaCard.session.playPause === true
                        onActivated: root.playPauseRequested(mediaCard.session.id)
                    }
                    CardButton {
                        id: cardNext
                        objectName: "quickMenuCardNext"
                        glyph: "next"
                        text: qsTr("Next")
                        enabled: mediaCard.visible && mediaCard.session.next === true
                        onActivated: root.nextRequested(mediaCard.session.id)
                    }
                    ActionButton {
                        id: cardOpen
                        objectName: "quickMenuCardOpen"
                        anchors.verticalCenter: parent.verticalCenter
                        height: 64
                        text: qsTr("Open player")
                        pointerMode: root.pointerMode
                        enabled: mediaCard.visible && mediaCard.session.open === true
                        onActiveFocusChanged: if (activeFocus) root.lastFocused = cardOpen
                        onEnabledChanged: Qt.callLater(() => root.settle(cardOpen))
                        onActivated: root.openPlayerRequested(mediaCard.session.id)
                    }
                }
            }
            // The other session, 624 × 88 under the card: its artwork, title,
            // state and its own play/pause; OK toggles it.
            Surface {
                id: otherRow
                objectName: "quickMenuOtherSession"
                readonly property var session: root.otherSession || ({})
                readonly property bool rowPlaying: session.state === "playing"
                width: 624; height: visible ? 88 : 0
                visible: !!root.otherSession
                enabled: visible && session.playPause === true
                shapeRole: "container"
                cornerRadius: Theme.containerRadius
                focusScale: 1; focusLift: 0; liftFromBottom: false
                pointerMode: root.pointerMode
                text: String(session.title || "").trim() || qsTr("Untitled")
                onActiveFocusChanged: if (activeFocus) root.lastFocused = otherRow
                onEnabledChanged: Qt.callLater(() => root.settle(otherRow))
                onActivated: root.playPauseRequested(session.id)
                contentItem: Item {
                    Music.MusicArtwork {
                        objectName: "quickMenuOtherCover"
                        x: 16; anchors.verticalCenter: parent.verticalCenter
                        width: 56; height: 56; radius: 8
                        source: otherRow.session.artwork || ""
                    }
                    Column {
                        x: 16 + 56 + 20
                        width: parent.width - x - 96
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        UiText {
                            objectName: "quickMenuOtherTitle"
                            width: parent.width
                            text: otherRow.text
                            role: "control"; font.pixelSize: 28
                            elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                        }
                        UiText {
                            objectName: "quickMenuOtherState"
                            width: parent.width
                            text: otherRow.rowPlaying ? qsTr("Playing") : qsTr("Paused")
                            role: "meta"; color: Theme.textSecondary
                            elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                        }
                    }
                    Glyph {
                        anchors.right: parent.right; anchors.rightMargin: 28
                        anchors.verticalCenter: parent.verticalCenter
                        kind: otherRow.rowPlaying ? "pause" : "play"
                        ink: otherRow.activeFocus ? Theme.textPrimary : Theme.textSecondary
                    }
                }
            }
            SheetRow {
                id: mediaRow
                objectName: "quickMenuMedia"
                glyph: root.playing ? "pause" : "play"
                // Without a session the single row stays (brief).
                visible: !root.mediaAvailable
                height: visible ? 96 : 0
                text: qsTr("Nothing is playing")
                description: root.connected ? qsTr("Now playing") : qsTr("Not available")
                enabled: false
            }
            SheetRow {
                id: volumeRow
                objectName: "quickMenuVolume"
                glyph: root.muted ? "muted" : "sound"
                text: qsTr("Sound")
                description: !root.connected ? qsTr("Not available")
                    : root.volumeControl ? (root.muted ? qsTr("%1 · Left/Right: volume, OK: sound on")
                                                       : qsTr("%1 · Left/Right: volume, OK: mute")).arg(root.audioText)
                    : qsTr("%1 · volume control is not available").arg(root.audioText)
                enabled: root.connected && root.volumeControl
                onActivated: root.muteToggleRequested()
                Keys.onLeftPressed: root.volumeStepRequested(-1)
                Keys.onRightPressed: root.volumeStepRequested(1)
            }
            // Information only: network status is not an action.
            Item {
                objectName: "quickMenuNetwork"
                width: 624; height: 96
                Rectangle {
                    id: networkPlate
                    x: 28; anchors.verticalCenter: parent.verticalCenter
                    width: 64; height: 64; radius: 32
                    color: Qt.rgba(1, 1, 1, 0.08)
                    Glyph { anchors.centerIn: parent; kind: "network"; ink: Theme.textSecondary }
                }
                Column {
                    x: networkPlate.x + networkPlate.width + 24
                    width: parent.width - x - 28
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    UiText {
                        width: parent.width; text: qsTr("Network")
                        role: "control"; font.pixelSize: 32; wrapMode: Text.NoWrap
                    }
                    UiText {
                        objectName: "quickMenuNetworkState"
                        width: parent.width; text: root.networkText
                        role: "meta"; color: Theme.textSecondary
                        elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
                    }
                }
            }
            SheetRow {
                id: sleepRow
                objectName: "quickMenuSleep"
                glyph: "sleep"
                text: qsTr("Sleep")
                description: root.connected ? qsTr("Request system suspend") : qsTr("Not available")
                enabled: root.connected
                onActivated: root.sleepRequested()
            }
            SheetRow {
                id: powerRow
                objectName: "quickMenuPower"
                glyph: "power"
                text: qsTr("Power")
                description: root.connected ? qsTr("Restart, shut down, ambient") : qsTr("Not available")
                enabled: root.connected
                onActivated: root.powerMenuRequested()
            }
            SheetRow {
                id: homeRow
                objectName: "quickMenuHome"
                glyph: "home"
                text: qsTr("Open Home")
                onActivated: root.homeRequested()
            }
            SheetRow {
                id: closeRow
                objectName: "quickMenuClose"
                glyph: "close"
                text: qsTr("Close Arinn")
                onActivated: root.closeShellRequested()
            }
        }
        }
        Rectangle {
            objectName: "quickMenuTopFade"
            x: 0; y: column.y
            width: parent.width; height: 48
            visible: column.contentY > 0
            gradient: Gradient {
                GradientStop { position: 0; color: sheet.color }
                GradientStop { position: 1; color: Qt.rgba(sheet.color.r, sheet.color.g, sheet.color.b, 0) }
            }
        }
        Rectangle {
            objectName: "quickMenuBottomFade"
            x: 0; y: column.y + column.height - height
            width: parent.width; height: 96
            visible: column.contentY + column.height < column.contentHeight - 1
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(sheet.color.r, sheet.color.g, sheet.color.b, 0) }
                GradientStop { position: 1; color: sheet.color }
            }
        }
        UiText {
            objectName: "quickMenuNotice"
            x: 48; y: parent.height - 146; width: 624
            visible: text.length > 0
            text: root.notice
            role: "caption"
            elide: Text.ElideRight; maximumLineCount: 1; wrapMode: Text.NoWrap
        }
        Row {
            x: 48; y: parent.height - 84
            spacing: 16
            Rectangle {
                width: okHint.implicitWidth + 24; height: 44; radius: 10; color: Theme.plate
                UiText { id: okHint; anchors.centerIn: parent; text: qsTr("OK"); role: "caption"; font.weight: 700; wrapMode: Text.NoWrap }
            }
            UiText { anchors.verticalCenter: parent.verticalCenter; text: qsTr("Select"); role: "caption"; color: Theme.textSecondary; wrapMode: Text.NoWrap }
            Item { width: 16; height: 1 }
            Rectangle {
                width: backHint.implicitWidth + 24; height: 44; radius: 10; color: Theme.plate
                UiText { id: backHint; anchors.centerIn: parent; text: qsTr("Back"); role: "caption"; font.weight: 700; wrapMode: Text.NoWrap }
            }
            UiText {
                anchors.verticalCenter: parent.verticalCenter
                text: root.overActivity ? qsTr("Return") : qsTr("Close")
                role: "caption"; color: Theme.textSecondary; wrapMode: Text.NoWrap
            }
        }
    }
}
