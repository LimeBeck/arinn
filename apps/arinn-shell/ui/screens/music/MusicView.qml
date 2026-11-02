// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Window
import "../.." as Ui
FocusScope {
    id: root
    required property var catalog
    property string mode: "catalogue"
    property string facet: "albums"
    property string parentId: ""
    property string parentTitle: ""
    property string pendingPlay: ""
    // An upcoming occurrence waiting for removal confirmation.
    property string pendingRemove: ""
    property string pendingRemoveTitle: ""
    property bool typing: false
    property var rememberedFocus: null
    readonly property var hostWindow: Window.window
    function containsFocusItem(item) {
        for(let current=item;current;current=current.parent)if(current===root)return true;
        return false;
    }
    function recoverFocus() {
        if(!root.visible || !root.enabled || !root.activeFocus)return;
        const current=hostWindow ? hostWindow.activeFocusItem : null;
        if(current && current!==root && containsFocusItem(current) && current.visible && current.enabled)return;
        if(rememberedFocus && rememberedFocus.visible && rememberedFocus.enabled)Ui.Focus.take(rememberedFocus);
        else focusInitial();
    }
    onSelectedChanged: Qt.callLater(recoverFocus)
    Connections {
        target: root.hostWindow
        function onActiveFocusItemChanged() {
            const item=root.hostWindow.activeFocusItem;
            if(item && item!==root && root.containsFocusItem(item) && item.visible && item.enabled)root.rememberedFocus=item;
        }
    }
    Connections {
        target: root.catalog
        ignoreUnknownSignals: true
        function onStatusChanged(){Qt.callLater(root.recoverFocus);}
        function onBusyChanged(){Qt.callLater(root.recoverFocus);}
    }
    function startTyping() {typing=true;Qt.callLater(searchKeyboard.start);}
    function finishTyping() {searchKeyboard.finish();typing=false;Ui.Focus.take(search);}
    onVisibleChanged: if(!visible && typing){searchKeyboard.finish();typing=false;}
    readonly property var selected: catalog.state || ({})
    readonly property var player: catalog.activeState || ({})
    readonly property var current: player.current ? player.current.track : ({})
    readonly property var actions: player.actions || []
    readonly property bool playing: player.playback === "playing"
    signal backRequested()
    signal settingsRequested()
    function catalogueAction() {
        if(connectButton.visible)return connectButton;
        if(cancelAuthButton.visible)return cancelAuthButton;
        if(disconnectButton.visible)return disconnectButton;
        if(rescanButton.visible)return rescanButton;
        return settingsButton;
    }
    // The chosen source chip, or the last chip when `last` (left of the facets).
    function sourceItem(last) {
        for (let i = 0; i < sourceButtons.count; ++i) if (!last && sourceButtons.itemAt(i).chosen) return sourceButtons.itemAt(i);
        return sourceButtons.count ? sourceButtons.itemAt(last ? sourceButtons.count - 1 : 0) : search;
    }
    function belowSearch() {Ui.Focus.take(tracks.count && tracks.visible ? tracks : catalogueAction());}
    function supported(action) { return actions.includes(action); }
    function focusInitial() { if (mode === "player") Ui.Focus.take(playButton); else if (mode === "queue") Ui.Focus.take(queueList); else if (tracks.count) Ui.Focus.take(tracks); else Ui.Focus.take(catalogueAction()); }
    function openPlayer() { mode = "player"; Qt.callLater(focusInitial); }
    function requestPlay(id) {
        if (player.current && ["playing", "paused", "loading"].includes(player.playback)) { pendingPlay = id; Qt.callLater(() => Ui.Focus.take(cancelReplace)); }
        else { catalog.command("play", id); openPlayer(); }
    }
    function closeRemove() { pendingRemove = ""; pendingRemoveTitle = ""; Qt.callLater(focusInitial); }
    function back() {
        if (typing) {finishTyping();}
        else if (pendingRemove) closeRemove();
        else if (pendingPlay) { pendingPlay = ""; focusInitial(); }
        else if (mode === "queue") { mode = "player"; Qt.callLater(focusInitial); }
        else if (mode === "player") { mode = "catalogue"; Qt.callLater(focusInitial); }
        else if (parentId) { parentId = ""; parentTitle = ""; catalog.browse(facet, "", search.text); }
        else backRequested();
    }
    Keys.onPressed: event => {
        if (root.pendingPlay) {
            if ([Qt.Key_Escape,Qt.Key_Back,Qt.Key_Backspace].includes(event.key)) root.back();
            else if ([Qt.Key_Left,Qt.Key_Right,Qt.Key_Tab,Qt.Key_Backtab].includes(event.key)) Ui.Focus.take(cancelReplace.activeFocus ? confirmReplace : cancelReplace);
            event.accepted=true;return;
        }
        if (root.pendingRemove) {
            if ([Qt.Key_Escape,Qt.Key_Back,Qt.Key_Backspace].includes(event.key)) root.back();
            else if ([Qt.Key_Left,Qt.Key_Right,Qt.Key_Tab,Qt.Key_Backtab].includes(event.key)) Ui.Focus.take(keepEntry.activeFocus ? removeEntry : keepEntry);
            event.accepted=true;return;
        }
        if ([Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace].includes(event.key) && !search.activeFocus) { back(); event.accepted = true; }
        else if (event.key === Qt.Key_MediaTogglePlayPause || event.key === Qt.Key_MediaPlay || event.key === Qt.Key_MediaPause) { catalog.transport(playing ? "pause" : "resume"); event.accepted = true; }
    }
    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.UiText { x: 120; y: 80; width: 1150; role: "title"; text: mode === "player" ? qsTr("Now playing") : mode === "queue" ? qsTr("Queue") : parentTitle || qsTr("Music"); textFormat: Text.PlainText; maximumLineCount: 1; elide: Text.ElideRight }
    Ui.UiText { x: 120; y: 975; width: 1680; text: catalog.errorText || ((root.selected.error || root.player.error) ? qsTr("Unavailable") : ""); role: "caption"; color: Ui.Theme.textSecondary; textFormat: Text.PlainText; visible: text.length > 0 }
    Row {
        x: 120; y: 180; spacing: 20; enabled: !root.pendingPlay; visible: root.mode === "catalogue"
        Repeater {
            id: sourceButtons; model: root.catalog.sources
            Ui.ActionButton {
                required property var modelData
                required property int index
                readonly property bool chosen: modelData.id === root.catalog.sourceId
                objectName: "musicSource-" + index
                width: 260; height: 76
                text: modelData.name
                restingColor: chosen ? Ui.Theme.plateFocus : Ui.Theme.plate
                Accessible.name: qsTr("Music source: %1").arg(modelData.name)
                Keys.onUpPressed: {}
                Keys.onDownPressed: Ui.Focus.take((root.selected.facets || []).length ? search : root.catalogueAction())
                Keys.onLeftPressed: if(index) Ui.Focus.take(sourceButtons.itemAt(index-1))
                Keys.onRightPressed: if(index+1<sourceButtons.count) Ui.Focus.take(sourceButtons.itemAt(index+1)); else if(facets.count) Ui.Focus.take(facets.itemAt(0))
                onActivated: if(!chosen) { root.parentId = ""; root.parentTitle = ""; root.catalog.choose(modelData.id); }
            }
        }
        Repeater {
            id: facets; model: root.selected.facets || []
            Ui.ActionButton {
                required property string modelData
                required property int index
                Keys.onDownPressed: Ui.Focus.take(search)
                Keys.onLeftPressed: Ui.Focus.take(index ? facets.itemAt(index-1) : root.sourceItem(true))
                Keys.onRightPressed: if(index+1<facets.count) Ui.Focus.take(facets.itemAt(index+1))
                width: 220; height: 76
                text: ({albums: qsTr("Albums"), artists: qsTr("Artists"), playlists: qsTr("Playlists"), tracks: qsTr("Tracks"), selections: qsTr("For you")})[modelData] || modelData
                onActivated: { root.facet = modelData; root.parentId = ""; root.parentTitle = ""; root.catalog.browse(modelData, "", search.text); }
            }
        }
    }
    Rectangle {
        enabled: !root.pendingPlay; x: 120; y: 278; width: 1160; height: 76; radius: 24; color: Ui.Theme.plate; visible: root.mode === "catalogue"
        Ui.UiText {
            anchors.fill: search; verticalAlignment: Text.AlignVCenter; role: "body"
            text: qsTr("Search this source"); color: Ui.Theme.textHint; visible: !search.text.length
        }
        TextInput {
            id: search; objectName:"musicSearch"; anchors.fill: parent; anchors.margins: 14; anchors.leftMargin: 28; anchors.rightMargin: 100
            verticalAlignment: TextInput.AlignVCenter; clip: true
            font.family: Ui.Theme.fontFamily; font.pixelSize: 28; color: Ui.Theme.textPrimary
            Accessible.role: Accessible.EditableText; Accessible.name: qsTr("Search this source")
            onTextChanged: searchDelay.restart()
            Keys.onDownPressed: root.belowSearch()
            Keys.onUpPressed: Ui.Focus.take(root.sourceItem(false))
            Keys.onEscapePressed: root.back()
            Keys.onRightPressed: event => {if(cursorPosition===length)Ui.Focus.take(searchKeyboardButton);else event.accepted=false;}
            Keys.onReturnPressed: root.startTyping()
            Keys.onEnterPressed: root.startTyping()
            Timer { id: searchDelay; interval: 350; onTriggered: root.catalog.browse(root.facet, root.parentId, search.text) }
        }
    }
    Ui.KeyboardButton { id: searchKeyboardButton; objectName:"musicKeyboard"; KeyNavigation.right: openPlayerButton.visible ? openPlayerButton : settingsButton; x: 1170; y: 278; visible: root.mode === "catalogue"; panelOpen: root.typing; onActivated: root.typing ? root.finishTyping() : root.startTyping(); Keys.onDownPressed: root.belowSearch(); Keys.onLeftPressed: Ui.Focus.take(search) }
    Ui.ActionButton { id:openPlayerButton; objectName:"musicOpenPlayer"; KeyNavigation.left:searchKeyboardButton;KeyNavigation.right:settingsButton;KeyNavigation.down:root.catalogueAction(); x: 1320; y: 278; width: 240; height: 76; visible: root.mode === "catalogue" && !!root.player.current; text: qsTr("Open player"); onActivated: root.openPlayer() }
    Ui.ActionButton { id:settingsButton;objectName:"musicSettings";KeyNavigation.left:openPlayerButton.visible ? openPlayerButton : searchKeyboardButton;KeyNavigation.down:root.catalogueAction()===settingsButton ? root.sourceItem(false) : root.catalogueAction(); x: 1580; y: 278; width: 220; height: 76; visible: root.mode === "catalogue"; text: qsTr("Settings"); onActivated: root.settingsRequested() }
    Ui.ActionButton { id:disconnectButton;objectName:"musicDisconnect";KeyNavigation.up:root.sourceItem(false);KeyNavigation.right:settingsButton;x: 1320; y: 880; width: 300; enabled: !root.pendingPlay && !root.catalog.busy; visible: root.mode === "catalogue" && (root.selected.actions || []).includes("disconnect"); text: qsTr("Disconnect"); onActivated: root.catalog.command("disconnect") }
    Ui.ActionButton { id:rescanButton;objectName:"musicRescan";KeyNavigation.up:tracks.count ? tracks : search;KeyNavigation.right:settingsButton;x: 1320; y: 880; width: 300; enabled: !root.pendingPlay && !root.catalog.busy; visible: root.mode === "catalogue" && (root.selected.actions || []).includes("scan"); text: qsTr("Rescan"); onActivated: root.catalog.command("scan") }
    ListView {
        id: tracks; objectName: "musicEntries"; x: 120; y: 385; width: 1680; height: 450; clip: true; spacing: 12
        visible: root.mode === "catalogue" && ["local", "connected"].includes(root.selected.account)
        enabled: !root.pendingPlay
        Keys.onUpPressed: event => {if(currentIndex<=0) Ui.Focus.take(search);else event.accepted=false;}
        Keys.onDownPressed: event => {if(currentIndex>=count-1){if(root.catalog.more){tracks.positionViewAtEnd();Qt.callLater(()=>Ui.Focus.take(tracks.footerItem));}else Ui.Focus.take(root.catalogueAction());}else event.accepted=false;}
        model: root.catalog.entries
        keyNavigationEnabled: true
        delegate: Ui.Surface {
            id: entry; focusedColor: Ui.Theme.textPrimary; required property var modelData; required property int index
            width: tracks.width - 8; height: 90; focus: tracks.currentIndex === index
            text: modelData.title || qsTr("Untitled")
            contentItem: Row {
                x: 28; spacing: 28
                MusicIcon { width: 38; height: 38; anchors.verticalCenter: parent.verticalCenter; name: "music"; color: entry.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary }
                Column {
                    width: 1400; anchors.verticalCenter: parent.verticalCenter
                    Ui.UiText { width: parent.width; text: entry.text; textFormat: Text.PlainText; role: "body"; maximumLineCount: 1; elide: Text.ElideRight; color: entry.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary }
                    Ui.UiText { width: parent.width; text: entry.modelData.available ? (entry.modelData.artist || entry.modelData.album || "") : qsTr("Unavailable"); textFormat: Text.PlainText; role: "caption"; maximumLineCount: 1; elide: Text.ElideRight; color: entry.activeFocus ? Ui.Theme.ink : Ui.Theme.textSecondary }
                }
            }
            onActivated: {
                tracks.currentIndex = index;
                if (modelData.kind === "track") root.requestPlay(modelData.id);
                else { root.parentId = modelData.id; root.parentTitle = text; root.catalog.browse(root.facet, modelData.id, search.text); }
            }
        }
        footer: Ui.ActionButton {id:loadMoreButton;objectName:"musicLoadMore";Keys.onUpPressed:Ui.Focus.take(tracks.currentItem || tracks);Keys.onDownPressed:Ui.Focus.take(root.catalogueAction()); visible: root.catalog.more; text: qsTr("Load more"); enabled: !root.catalog.busy; onActivated: root.catalog.loadMore() }
    }
    Column {
        x: 120; y: 420; width: 1300; spacing: 28
        enabled: !root.pendingPlay
        visible: root.mode === "catalogue" && (!root.selected.account || !["local", "connected"].includes(root.selected.account))
        Ui.UiText { width: parent.width; role: "section"; text: root.selected.account === "connecting" ? qsTr("Confirm sign-in") : root.selected.account === "unconfigured" ? qsTr("Connection is not available yet") : qsTr("Connect your account") }
        Ui.UiText { width: parent.width; role: "body"; textFormat: Text.PlainText; wrapMode: Text.WordWrap; text: root.selected.prompt ? root.selected.prompt.verification_url : qsTr("Sign in on the provider's page. Your password stays there.") }
        Ui.UiText { width: parent.width; role: "hero"; textFormat: Text.PlainText; text: root.selected.prompt ? root.selected.prompt.user_code : "" }
        Row {
            spacing: 24
            Ui.ActionButton {id:connectButton;objectName:"musicConnect";KeyNavigation.up:root.sourceItem(false);KeyNavigation.right:settingsButton;visible: (root.selected.actions || []).includes("connect"); text: qsTr("Connect"); onActivated: root.catalog.command("connect") }
            Ui.ActionButton {id:cancelAuthButton;objectName:"musicCancelAuth";KeyNavigation.up:root.sourceItem(false);KeyNavigation.right:settingsButton;visible: (root.selected.actions || []).includes("cancel"); text: qsTr("Cancel"); onActivated: root.catalog.command("cancel") }
        }
    }
    Ui.UiText { x: 120; y: 460; width: 1400; visible: root.mode === "catalogue" && tracks.visible && !tracks.count && !root.catalog.busy; role: "section"; text: root.selected.account === "connected" && !(root.selected.facets || []).length ? qsTr("Account connected. Playback is not available yet.") : search.text.length ? qsTr("Nothing found") : qsTr("Add music folders in Settings, then rescan.") }
    Item {
        enabled: !root.pendingPlay; x: 120; y: 235; width: 1680; height: 680; visible: root.mode === "player"
        Rectangle { x: 0; y: 0; width: 500; height: 500; radius: 48; color: Ui.Theme.plate; MusicIcon { anchors.centerIn: parent; width: 180; height: 180; name: "music"; color: Ui.Theme.textSecondary } }
        Column {
            x: 580; width: 1100; spacing: 24
            Ui.UiText { width: parent.width; role: "eyebrow"; text: root.catalog.activeSourceName; textFormat: Text.PlainText; color: Ui.Theme.textSecondary }
            Ui.UiText { width: parent.width; role: "hero"; text: root.current.title || qsTr("Nothing is playing"); textFormat: Text.PlainText; maximumLineCount: 1; elide: Text.ElideRight }
            Ui.UiText { width: parent.width; role: "section"; text: root.current.artist || ""; textFormat: Text.PlainText; maximumLineCount: 1; elide: Text.ElideRight; color: Ui.Theme.textSecondary }
            Ui.UiText { width: parent.width; role: "body"; text: root.current.album || ""; textFormat: Text.PlainText; maximumLineCount: 1; elide: Text.ElideRight; color: Ui.Theme.textSecondary }
            Item {
                id: seek; objectName: "musicSeek"; width: parent.width; height: 40
                enabled: root.supported("seek") && (root.current.duration_us || 0) > 0
                activeFocusOnTab: enabled
                KeyNavigation.down: playButton
                // One remote press moves 10 s; presses before the next
                // position report add up instead of restarting from it.
                readonly property real stepUs: 10000000
                property real target: -1
                readonly property real shown: seekSettle.running && target >= 0 ? target : (root.player.position_us || 0)
                function step(direction) {
                    const duration = root.current.duration_us || 0;
                    if (!enabled || duration <= 0) return;
                    target = Math.max(0, Math.min(duration, shown + direction * stepUs));
                    seekSettle.restart();
                    root.catalog.transport("seek", "", "", Math.round(target));
                }
                Keys.onLeftPressed: step(-1)
                Keys.onRightPressed: step(1)
                Timer { id: seekSettle; interval: 1500 }
                Accessible.role: Accessible.Slider
                Accessible.name: qsTr("Playback position")
                Rectangle {
                    anchors.fill: parent; anchors.margins: -8; radius: 20
                    color: "transparent"; border.width: 3; border.color: Ui.Theme.textPrimary; visible: seek.activeFocus
                }
                Ui.ProgressTrack {
                    anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: seek.activeFocus ? 10 : 6
                    value: (root.current.duration_us || 0) > 0 ? seek.shown / root.current.duration_us : 0
                }
            }
            Ui.UiText { role: "caption"; text: Math.floor(seek.shown / 60000000) + ":" + String(Math.floor(seek.shown / 1000000) % 60).padStart(2, "0"); color: Ui.Theme.textSecondary }
        }
        Item {
            objectName: "musicControlsB"; x: 580; y: 405; width: 1100; height: 112
            Row {
                x: 0; spacing: 16
                MusicButton { id: likeButton; KeyNavigation.right: dislikeButton; KeyNavigation.down: playButton; iconName: "like"; text: qsTr("Like"); visible: root.supported("like"); onActivated: root.catalog.transport("like") }
                MusicButton { id: dislikeButton; KeyNavigation.left: likeButton; KeyNavigation.right: previousButton; KeyNavigation.down: playButton; iconName: "dislike"; text: qsTr("Not for me"); visible: root.supported("dislike"); onActivated: root.catalog.transport("dislike") }
            }
            Row {
                x: 330; spacing: 24
                MusicButton { id: previousButton; KeyNavigation.left: dislikeButton; KeyNavigation.right: playButton; KeyNavigation.down: stopButton; KeyNavigation.up: seek; iconName: "previous"; text: qsTr("Previous"); enabled: root.supported("previous") && !root.catalog.busy; onActivated: root.catalog.transport("previous") }
                MusicButton { id: playButton; KeyNavigation.left: previousButton; KeyNavigation.right: nextButton; KeyNavigation.up: seek; KeyNavigation.down: stopButton; objectName: "musicPlayPause"; width: 96; height: 96; iconName: root.playing ? "pause" : "play"; text: root.playing ? qsTr("Pause") : qsTr("Play"); enabled: !root.catalog.busy && (root.supported("pause") || root.supported("resume") || !!root.current.id); onActivated: root.catalog.transport(root.playing ? "pause" : root.player.playback === "paused" ? "resume" : "play", root.current.id || "") }
                MusicButton { id: nextButton; objectName:"musicNext"; KeyNavigation.left: playButton; KeyNavigation.right: queueButton; KeyNavigation.up: seek; KeyNavigation.down: stopButton; iconName: "next"; text: qsTr("Next"); enabled: root.supported("next") && !root.catalog.busy; onActivated: root.catalog.transport("next") }
            }
            Row {
                anchors.right: parent.right; spacing: 16
                MusicButton { id: queueButton; objectName:"musicQueue"; KeyNavigation.left: nextButton; KeyNavigation.right: seekButton; KeyNavigation.down: stopButton; iconName: "queue"; text: qsTr("Queue"); onActivated: { root.mode = "queue"; Qt.callLater(root.focusInitial); } }
                MusicButton { id: seekButton; KeyNavigation.left: queueButton; KeyNavigation.right: stopButton; KeyNavigation.down: stopButton; iconName: "seek"; text: qsTr("Seek"); enabled: root.supported("seek"); onActivated: Ui.Focus.take(seek) }
                MusicButton { id: stopButton; objectName:"musicStop";KeyNavigation.left:seekButton;KeyNavigation.right:radioButton;KeyNavigation.up:playButton;iconName:"stop";text:qsTr("Stop");enabled:root.supported("stop") && !root.catalog.busy;onActivated:root.catalog.transport("stop") }
                MusicButton { id: radioButton; KeyNavigation.left: stopButton; KeyNavigation.down: stopButton; iconName: "radio"; text: qsTr("Radio from this track"); visible: root.supported("radio"); onActivated: root.catalog.transport("radio", root.current.id || "") }
            }
        }
    }
    ListView {
        id: queueList; objectName: "musicQueueList"; enabled: !root.pendingPlay && !root.pendingRemove; x: 120; y: 230; width: 1680; height: 680; clip: true; spacing: 12; visible: root.mode === "queue"; keyNavigationEnabled: true
        model: root.player.upcoming || []
        header: Ui.UiText { role: "section"; text: root.player.continuous ? qsTr("Upcoming · continues automatically") : qsTr("Upcoming"); height: 70 }
        delegate: Ui.Surface {
            id: queueEntry; required property var modelData; required property int index; width: queueList.width - 8; height: 90; focus: queueList.currentIndex === index
            text: modelData.track.title || qsTr("Untitled")
            contentItem: Ui.UiText { anchors.fill: parent; anchors.margins: 24; text: queueEntry.text; role: "body"; textFormat: Text.PlainText; maximumLineCount: 1; elide: Text.ElideRight }
            onActivated: if (root.supported("remove")) { root.pendingRemove = modelData.occurrence; root.pendingRemoveTitle = text; Qt.callLater(() => Ui.Focus.take(keepEntry)); }
        }
    }
    Rectangle {
        x:0;y:root.height-height;width:root.width;height:searchKeyboard.implicitHeight
        color:Ui.Theme.background;visible:root.typing;enabled:visible;z:15
        Ui.OnScreenKeyboard {id:searchKeyboard;anchors.fill:parent;target:search;memoryKey:"music-search";maxCharacters:256;onSubmitted:root.finishTyping();onHideRequested:root.finishTyping();onLeaveUp:Ui.Focus.take(searchKeyboardButton)}
    }
    Rectangle {
        anchors.fill: parent; color: "#df131416"; visible: root.pendingPlay.length > 0; z: 20
        MouseArea {anchors.fill: parent;acceptedButtons:Qt.AllButtons}
        Column {
            x: 480; y: 360; width: 960; spacing: 40
            Ui.UiText { width: parent.width; role: "title"; text: qsTr("Replace the current queue?") }
            Row { spacing: 30
                Ui.ActionButton { id: cancelReplace; objectName:"musicKeepQueue"; text: qsTr("Cancel"); onActivated: { root.pendingPlay = ""; root.focusInitial(); } }
                Ui.ActionButton { id: confirmReplace; objectName:"musicReplaceQueue"; text: qsTr("Replace and play"); onActivated: { const id = root.pendingPlay; root.pendingPlay = ""; root.catalog.command("play", id); root.openPlayer(); } }
            }
        }
    }
    Rectangle {
        anchors.fill: parent; color: "#df131416"; visible: root.pendingRemove.length > 0; z: 20
        MouseArea {anchors.fill: parent;acceptedButtons:Qt.AllButtons}
        Column {
            x: 480; y: 360; width: 960; spacing: 40
            Ui.UiText { width: parent.width; role: "title"; text: qsTr("Remove from the queue?") }
            Ui.UiText { width: parent.width; role: "body"; textFormat: Text.PlainText; text: root.pendingRemoveTitle; maximumLineCount: 1; elide: Text.ElideRight; color: Ui.Theme.textSecondary }
            Row { spacing: 30
                Ui.ActionButton { id: keepEntry; objectName:"musicKeepEntry"; text: qsTr("Cancel"); KeyNavigation.right: removeEntry; onActivated: root.closeRemove() }
                Ui.ActionButton { id: removeEntry; objectName:"musicRemoveEntry"; destructive: true; text: qsTr("Remove"); KeyNavigation.left: keepEntry; onActivated: { const occurrence = root.pendingRemove; root.closeRemove(); root.catalog.transport("remove", "", occurrence); } }
            }
        }
    }
}
