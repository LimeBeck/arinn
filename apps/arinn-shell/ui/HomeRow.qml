// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

FocusScope {
    id: root
    property var entries: []
    // The entries the cards show. Home builds its rows with map and concat, so
    // every upstream notification yields a new array, mostly an equal one;
    // only a different one is taken over (#441).
    property var shown: []
    // Entries by card key. A card is keyed by its entry's id, so it follows
    // that entry, with its focus, when the row's order changes.
    property var byKey: ({})
    property var busyEntryIds: []
    property var openShortcutIds: []
    property bool browserUnavailable: false
    property string title: ""
    property string variant: "content"
    // Application rows are objects with identity light. The Home shelf is a
    // container: no plate at rest, an 8% plate and a rim in focus (#725).
    property bool identityLight: false
    property string shapeRole: variant === "app" ? "object" : variant === "shelf" ? "container" : "content"
    // Pool hue for the cast under a focused image. NaN uses the hearth hue.
    property real castHue: Theme.hearthHue
    property bool pointerMode: false
    property bool hoverFocusEnabled: true
    // A row Home has faded out of the header band (#659) takes no pointer
    // input; D-pad focus still reaches it, which brings it back.
    property bool pointerBlocked: false
    property string selectedId: ""
    property int selectedIndex: 0
    readonly property int count: shown.length
    readonly property bool shelf: variant === "shelf"
    property real cellWidth: variant === "app" ? (identityLight ? 300 : 264) : variant === "poster" ? 180 : 400
    readonly property real artworkHeight: shelf ? 88 : variant === "app" ? (identityLight ? 168 : 132) : variant === "poster" ? 270 : 225
    readonly property real captionHeight: variant === "poster" ? Theme.posterCaptionHeight
            : (variant === "app" || shelf) ? 0 : Theme.contentCaptionHeight
    readonly property real cellHeight: artworkHeight + captionHeight
    readonly property real gap: shelf ? 28 : variant === "content" ? 32 : 24
    // Room for the focus rim, lift and scale around a card. The right edge
    // keeps this margin. The left edge reaches the screen fade instead.
    readonly property real glowMargin: 48
    // X in the 1920-wide design canvas. mapToItem(null) is the window scene,
    // which is 0 for these rows under the offscreen tests, and a scaled
    // stage would turn the 24–112 fade into device pixels.
    function designRoot() {
        let item = parent;
        while (item) {
            if (item.width >= Theme.canvasWidth)
                return item;
            item = item.parent;
        }
        return null;
    }
    readonly property real screenX: {
        // mapToItem does not notify when this row moves, so read x here.
        const localX = x;
        const origin = designRoot();
        return origin ? mapToItem(origin, 0, 0).x : localX;
    }
    // Distance from this row's left to where the screen fade begins.
    readonly property real fadeInset: Math.max(0, screenX - Theme.edgeFadeStart)
    readonly property real leftBleed: Math.max(glowMargin, fadeInset)
    signal selected(var entry)
    signal activated(string entryId)
    signal secondaryActivated(string entryId)
    signal actionsRequested(string entryId)
    signal edge(int key, bool autoRepeat)
    signal focusLost()
    signal scrolled()
    signal dpadRequested()
    // The shelf is 125 px so Home can keep 94 px of the New poster on screen
    // while Continue is focused (#725). Its 88 px names sit under the label.
    height: shelf ? 125 : cellHeight + 74

    function available(index) {
        return index >= 0 && index < count && shown[index].enabled !== false;
    }
    function focusIndex(index) {
        index = Math.min(Math.max(index, 0), count - 1);
        let candidate = index;
        while (candidate >= 0 && !available(candidate)) --candidate;
        if (candidate < 0) {
            candidate = index + 1;
            while (candidate < count && !available(candidate)) ++candidate;
        }
        if (!available(candidate)) return false;
        const item = cards.itemAt(candidate);
        if (!item || !item.item) return false;
        Focus.take(item.item);
        return true;
    }
    function restore() { return focusIndex(selectedIndex); }
    function reconcile() {
        const previous = selectedIndex;
        const found = shown.findIndex(entry => entry.id === selectedId);
        selectedIndex = found >= 0 ? found : Math.max(0, previous - 1);
        if (activeFocus) {
            const card=cards.itemAt(selectedIndex);
            if(found>=0 && card && card.item && (card.item.activeFocus || card.item.focusWithin===true))return;
            if (!restore()) focusLost();
        }
    }
    function sync() {
        if (JSON.stringify(entries) === JSON.stringify(shown)) return;
        const keys = [];
        const map = {};
        const seen = {};
        for (const entry of entries) {
            // Ids are unique; a repeated one still gets its own card.
            let key = String(entry.id);
            if (seen[key] === undefined) seen[key] = 0;
            else key += "\n" + (++seen[key]);
            keys.push(key);
            map[key] = entry;
        }
        // Remove stale cards before the lookup changes: a card bound to an
        // entry that is gone would read an empty entry first.
        for (let i = keyModel.count - 1; i >= 0; --i)
            if (map[keyModel.get(i).key] === undefined) keyModel.remove(i);
        byKey = map;
        for (let i = 0; i < keys.length; ++i) {
            let at = -1;
            for (let j = i; j < keyModel.count && at < 0; ++j)
                if (keyModel.get(j).key === keys[i]) at = j;
            if (at < 0) keyModel.insert(i, {key: keys[i]});
            else if (at !== i) keyModel.move(at, i, 1);
        }
        shown = entries;
        // The focused card may have moved; D-pad steps from its new place.
        const found = shown.findIndex(entry => entry.id === selectedId);
        if (found >= 0) selectedIndex = found;
        Qt.callLater(reconcile);
    }
    onEntriesChanged: sync()
    Component.onCompleted: sync()
    function reveal(index) {
        const slot = cards.itemAt(index);
        const body = slot ? slot.width : cellWidth;
        const left = slot ? slot.x : index * (cellWidth + gap);
        const margin = pointerMode ? 0 : body + gap;
        let next = viewport.contentX;
        if (left < next + margin) next = left - margin;
        else if (left + body > next + viewport.width - margin)
            next = left + body - viewport.width + margin;
        // The viewport is wider than the safe area by its glow margins. Keep
        // the card's body inside the safe right edge, e.g. a partly visible
        // poster that the pointer focuses (#564).
        if (left + body > next + width) next = left + body - width;
        viewport.contentX = Math.max(0, Math.min(next, Math.max(0, viewport.contentWidth - viewport.width)));
    }
    Keys.onPressed: event => {
        if (![Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) return;
        dpadRequested();
        event.accepted = true;
        const autoRepeat = event.isAutoRepeat;
        if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            const step = event.key === Qt.Key_Left ? -1 : 1;
            let next = selectedIndex + step;
            while (next >= 0 && next < count && !available(next)) next += step;
            if (available(next)) {
                if (focusIndex(next)) interfaceSounds.playNavigationTap(autoRepeat);
            } else {
                if (event.key === Qt.Key_Right)
                    interfaceSounds.playEdgeBump(autoRepeat);
                edge(event.key, autoRepeat);
            }
        } else edge(event.key, autoRepeat);
    }
    UiText { width: parent.width; text: root.title; role: "section"; maximumLineCount: 1; elide: Text.ElideRight }
    Flickable {
        id: viewport
        objectName: root.objectName + "Viewport"
        x: -root.leftBleed; y: root.shelf ? 29 : 10
        width: parent.width + root.leftBleed + root.glowMargin
        height: root.shelf ? root.height - y : root.cellHeight + 104
        clip: true
        contentWidth: cardsRow.width + root.leftBleed + root.glowMargin
        contentHeight: height
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalFlick
        onContentXChanged: root.scrolled()
        Row {
            id: cardsRow
            x: root.leftBleed; y: root.shelf ? 8 : 48
            spacing: root.gap
            Repeater {
                id: cards
                // Cards are keyed by entry and updated in place. An array
                // model would recreate every card, and reload its artwork,
                // whenever one entry changes, e.g. a playing session's
                // position (#441).
                model: ListModel { id: keyModel }
                delegate: Item {
                    id: slot
                    required property string key
                    required property int index
                    readonly property var item: entryLoader.item
                    width: root.shelf ? entryLoader.width : root.cellWidth
                    height: root.cellHeight
                    // The row's safe area (handoff 01: x120 to x1824) in
                    // this slot's coordinates.
                    readonly property real safeLeft: viewport.contentX - x
                    readonly property real safeRight: safeLeft + root.width
                    // Only a focused card whose body is inside the safe
                    // area may paint past it: its glow and focus scale use
                    // the viewport's margin. Every other card is cut at the safe edges, so
                    // no neighbour or flicked card shows past them (#564).
                    readonly property bool focusedInside: !!item && item.activeFocus
                        && safeLeft <= 0 && safeRight >= width
                    readonly property bool painted: focusedInside || (safeLeft < width && safeRight > 0)
                    // Left: the screen fade, not a hard cut. Right: the safe edge.
                    readonly property real clipX: slot.focusedInside ? -root.leftBleed
                            : Math.max(-root.leftBleed, slot.safeLeft - root.fadeInset)
                    Item {
                        id: cardClip
                        objectName: "cardClip"
                        clip: true
                        x: slot.clipX
                        y: -root.glowMargin
                        width: !slot.painted ? 0 : Math.max(0, (slot.focusedInside ? slot.width + root.glowMargin
                            : Math.min(slot.width + root.glowMargin, slot.safeRight)) - x)
                        height: slot.height + 2 * root.glowMargin
                        Loader {
                            id: entryLoader
                            readonly property var modelData: root.byKey[slot.key] || ({})
                            x: -cardClip.x; y: root.glowMargin
                            width: root.shelf ? (item ? item.implicitWidth : 160) : root.cellWidth
                            height: root.cellHeight
                            sourceComponent: (root.variant === "app" || root.shelf) ? tile
                                    : (root.variant === "poster" && !String((modelData && modelData.artwork) || "").length
                                       && !(modelData && modelData.stack && modelData.stack.length)) ? titleCard : card
                            function chosen() {
                                root.selectedIndex = slot.index;
                                root.selectedId = modelData.id;
                                root.selected(modelData);
                                root.reveal(slot.index);
                            }
                            Component {
                                id: tile
                                AppTile {
                                    objectName: "entry-" + entryLoader.modelData.id
                                    width: entryLoader.width; height: entryLoader.height
                                    shelf: root.shelf
                                    identityLight: root.identityLight && !root.shelf
                                    shapeRole: root.shapeRole
                                    rowBrightness: activeFocus ? 1
                                            : root.activeFocus ? Theme.rowLitBrightness : Theme.rowDimBrightness
                                    text: entryLoader.modelData.title
                                    visualIdentity: entryLoader.modelData.visualIdentity || entryLoader.modelData.id
                                    monogram: entryLoader.modelData.monogram || entryLoader.modelData.title.trim().slice(0, 1).toUpperCase()
                                    iconSource: entryLoader.modelData.iconSource || ""
                                    subtitle: (root.browserUnavailable && entryLoader.modelData.webShortcut) ? qsTr("Unavailable") : (entryLoader.modelData.subtitle || "")
                                    running: entryLoader.modelData.running || false
                                    serviceOpen: root.openShortcutIds.indexOf(entryLoader.modelData.id) >= 0
                                    enabled: entryLoader.modelData.enabled !== false
                                    busy: entryLoader.modelData.busy || root.busyEntryIds.includes(entryLoader.modelData.id)
                                    pointerMode: root.pointerMode
                                    // A hidden card must not take focus from the pointer.
                                    hoverFocusEnabled: root.hoverFocusEnabled && slot.painted
                                    // An unavailable saved favorite is focusable so it can be removed.
                                    unavailable: entryLoader.modelData.unavailable || (root.browserUnavailable && !!entryLoader.modelData.webShortcut)
                                    onActivated: root.activated(entryLoader.modelData.id)
                                    onPressAndHold: root.actionsRequested(entryLoader.modelData.id)
                                    Keys.onPressed: event => {
                                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                            if (!event.isAutoRepeat && !busy) { holdHandled = false; down = true; holdTimer.restart(); }
                                            event.accepted = true;
                                        }
                                    }
                                    property bool holdHandled: false
                                    Timer {
                                        id: holdTimer; interval: 650
                                        onTriggered: {
                                            parent.holdHandled = true; parent.down = false;
                                            root.actionsRequested(entryLoader.modelData.id);
                                        }
                                    }
                                    Keys.onReleased: event => {
                                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                            if (event.isAutoRepeat) { event.accepted = true; return; }
                                            holdTimer.stop();
                                            if (holdHandled) { down = false; event.accepted = true; }
                                            else { const invoke = down && enabled && !busy; down = false; if (invoke) clicked(); }
                                            event.accepted = true;
                                        }
                                    }
                                    onActiveFocusChanged: {
                                        if (activeFocus) entryLoader.chosen();
                                        else holdTimer.stop();
                                    }
                                    onDpadRequested: root.dpadRequested()
                                }
                            }
                            Component {
                                id: card
                                ContentCard {
                                    objectName: "entry-" + entryLoader.modelData.id
                                    width: entryLoader.width; height: entryLoader.height
                                    artworkHeight: root.artworkHeight
                                    castHue: root.castHue
                                    rowBrightness: activeFocus ? 1
                                            : root.activeFocus ? Theme.rowLitBrightness : Theme.rowDimBrightness
                                    text: entryLoader.modelData.title
                                    visualIdentity: entryLoader.modelData.visualIdentity || entryLoader.modelData.id
                                    subtitle: entryLoader.modelData.subtitle || ""
                                    statusText: entryLoader.modelData.statusText || ""
                                    secondaryIcon: entryLoader.modelData.secondaryIcon || ""
                                    secondaryText: entryLoader.modelData.secondaryText || ""
                                    secondaryEnabled: entryLoader.modelData.secondaryEnabled !== false
                                    onSecondaryActivated: {entryLoader.chosen();root.secondaryActivated(entryLoader.modelData.id);}
                                    artwork: entryLoader.modelData.artwork || ""
                                    stack: entryLoader.modelData.stack || []
                                    running: entryLoader.modelData.running || false
                                    showProgress: entryLoader.modelData.progress !== undefined
                                    progress: entryLoader.modelData.progress || 0
                                    enabled: entryLoader.modelData.enabled !== false
                                    busy: entryLoader.modelData.busy || root.busyEntryIds.includes(entryLoader.modelData.id)
                                    pointerMode: root.pointerMode
                                    // A hidden card must not take focus from the pointer.
                                    hoverFocusEnabled: root.hoverFocusEnabled && slot.painted
                                    onFocusWithinChanged: if (focusWithin) entryLoader.chosen()
                                    onActivated: root.activated(entryLoader.modelData.id)
                                    onDpadRequested: root.dpadRequested()
                                }
                            }
                            // A poster with no artwork is a title card: the name
                            // in the content face on Charcoal, not a letter (#725).
                            Component {
                                id: titleCard
                                Surface {
                                    id: posterCard
                                    objectName: "entry-" + entryLoader.modelData.id
                                    shapeRole: "content"
                                    width: entryLoader.width; height: entryLoader.height
                                    cornerRadius: Theme.posterRadius
                                    text: entryLoader.modelData.title || ""
                                    visualIdentity: entryLoader.modelData.visualIdentity || entryLoader.modelData.id
                                    enabled: entryLoader.modelData.enabled !== false
                                    busy: entryLoader.modelData.busy || root.busyEntryIds.includes(entryLoader.modelData.id)
                                    rowBrightness: activeFocus ? 1
                                            : root.activeFocus ? Theme.rowLitBrightness : Theme.rowDimBrightness
                                    pointerMode: root.pointerMode
                                    hoverFocusEnabled: root.hoverFocusEnabled && slot.painted
                                    onActiveFocusChanged: if (activeFocus) entryLoader.chosen()
                                    onActivated: root.activated(entryLoader.modelData.id)
                                    onDpadRequested: root.dpadRequested()
                                    contentItem: Item {
                                        Item {
                                            id: titleFrame
                                            width: parent.width
                                            height: root.artworkHeight
                                            Rectangle {
                                                anchors.fill: parent
                                                radius: posterCard.cornerRadius
                                                gradient: Gradient {
                                                    GradientStop { position: 0; color: Theme.background }
                                                    GradientStop { position: 1; color: Theme.chip }
                                                }
                                            }
                                            UiText {
                                                objectName: "titleCardTitle"
                                                x: 18; width: parent.width - 36
                                                anchors.bottom: parent.bottom
                                                anchors.bottomMargin: 18
                                                text: posterCard.busy ? qsTr("Loading…") : posterCard.text
                                                role: "meta"
                                                font.family: Theme.contentFontFamily
                                                font.pixelSize: 26
                                                font.weight: 600
                                                font.features: ({ "tnum": 1, "lnum": 1 })
                                                lineHeight: 1.15
                                                color: posterCard.activeFocus ? Theme.textPrimary : Theme.textSecondary
                                                wrapMode: Text.WordWrap
                                                maximumLineCount: 4
                                                elide: Text.ElideRight
                                            }
                                            Rectangle {
                                                objectName: "rowShade"
                                                anchors.fill: parent
                                                radius: posterCard.cornerRadius
                                                color: "black"
                                                opacity: Math.max(0, 1 - posterCard.rowBrightness)
                                                z: 8
                                            }
                                            ArtworkFocus {
                                                shown: posterCard.activeFocus
                                                artworkRadius: posterCard.cornerRadius
                                            }
                                        }
                                        CastLight {
                                            objectName: "castLight"
                                            shown: posterCard.activeFocus
                                            hue: root.castHue
                                            width: titleFrame.width * Theme.castWidthRatio
                                            anchors.horizontalCenter: titleFrame.horizontalCenter
                                            anchors.top: titleFrame.bottom
                                            anchors.topMargin: -Math.round(Theme.castHeight * 0.4)
                                        }
                                        Column {
                                            objectName: "cardCaption"
                                            y: root.artworkHeight + 14
                                            width: parent.width
                                            spacing: 4
                                            UiText {
                                                objectName: "cardTitle"
                                                width: parent.width
                                                text: posterCard.busy ? qsTr("Loading…") : posterCard.text
                                                role: "caption"
                                                font.pixelSize: 24
                                                font.weight: 500
                                                color: posterCard.activeFocus ? Theme.textPrimary : Theme.textSecondary
                                                wrapMode: Text.NoWrap
                                                maximumLineCount: 1
                                                elide: Text.ElideRight
                                            }
                                            UiText {
                                                width: parent.width
                                                visible: (entryLoader.modelData.subtitle || "").length > 0
                                                text: entryLoader.modelData.subtitle || ""
                                                role: "caption"
                                                color: Theme.textHint
                                                maximumLineCount: 1
                                                elide: Text.ElideRight
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // A hidden card takes no clicks; D-pad focus still
                    // reaches it and reveal() scrolls it into view.
                    MouseArea {
                        objectName: "hiddenCardBlocker"
                        anchors.fill: parent
                        z: 1000
                        enabled: !slot.painted
                        acceptedButtons: Qt.AllButtons
                    }
                }
            }
        }
    }
    // Previous cards stay visible as they cross the left of the screen and
    // fade out from x 24 to x 112 (ADR-0136). A charcoal ramp matches the
    // background on the software renderer, which has no row mask.
    Rectangle {
        objectName: "rowEdgeFade"
        z: 80
        x: Theme.edgeFadeStart - root.screenX
        y: viewport.y
        width: Theme.edgeFadeEnd - Theme.edgeFadeStart
        height: viewport.height
        visible: root.screenX - root.leftBleed < Theme.edgeFadeEnd
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Theme.background }
            GradientStop { position: 1; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0) }
        }
    }
    MouseArea {
        objectName: "rowPointerBlocker"
        // The cards' viewport reaches past the row's edges.
        x: -root.leftBleed; width: parent.width + root.leftBleed + root.glowMargin; height: parent.height
        z: 2000
        enabled: root.pointerBlocked
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onWheel: wheel => wheel.accepted = true
    }
}
