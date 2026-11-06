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
    // Applications rows run to the screen's right edge and fade out there,
    // mirroring the left fade, instead of being cut at the safe edge
    // (#756 C27). Home rows keep the handoff's safe right edge.
    property bool fadeRight: false
    readonly property real rightRoom: fadeRight
        ? Math.max(0, Theme.canvasWidth - Theme.edgeFadeStart - (screenX + width)) : 0
    readonly property real rightBleed: Math.max(glowMargin, rightRoom)
    signal selected(var entry)
    signal activated(string entryId)
    signal secondaryActivated(string entryId)
    signal actionsRequested(string entryId)
    // A short Menu on a tile: the tile keeps the key to tell a long press
    // (the tile sheet) from a tap, which the row reports here.
    signal menuTapped()
    // How long Menu, or OK, is held for the tile sheet (spec §6.2).
    readonly property int holdMs: 650
    // Busy close buttons beside a Continue card (CL-9).
    property var closingIds: []
    signal edge(int key, bool autoRepeat)
    signal focusLost()
    signal scrolled()
    signal dpadRequested()
    // Items sit far enough under the label that a focused one, lifted and
    // ringed, stays clear of it (#837): cards 58 px down, shelf names 20.
    height: shelf ? 137 : cellHeight + 84

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
    UiText { objectName: root.objectName + "Label"; width: parent.width; text: root.title; role: "section"; maximumLineCount: 1; elide: Text.ElideRight }
    Flickable {
        id: viewport
        objectName: root.objectName + "Viewport"
        x: -root.leftBleed; y: root.shelf ? 29 : 10
        width: parent.width + root.leftBleed + root.rightBleed
        height: root.shelf ? root.height - y : root.cellHeight + 114
        clip: true
        contentWidth: cardsRow.width + root.leftBleed + root.rightBleed
        contentHeight: height
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalFlick
        onContentXChanged: root.scrolled()
        Row {
            id: cardsRow
            x: root.leftBleed; y: root.shelf ? 20 : 58
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
                    // A Close button (CL-3a) is 128 wide and sits 16 from the
                    // card before it, not a row gap away.
                    readonly property bool closer: (root.byKey[slot.key] || {}).kind === "close"
                    readonly property real closeInset: closer ? Math.max(0, root.gap - 16) : 0
                    width: root.shelf ? entryLoader.width : closer ? 128 - closeInset : root.cellWidth
                    height: root.cellHeight
                    // The row's safe area (handoff 01: x120 to x1824) in
                    // this slot's coordinates.
                    readonly property real safeLeft: viewport.contentX - x
                    readonly property real safeEdge: safeLeft + root.width
                    // Where painting stops on the right: the safe edge, or
                    // the screen fade's end on an Applications row.
                    readonly property real safeRight: safeEdge + root.rightRoom
                    // Only a focused card whose body is inside the safe
                    // area may paint past it: its glow and focus scale use
                    // the viewport's margin. Every other card is cut at the safe edges, so
                    // no neighbour or flicked card shows past them (#564).
                    readonly property bool focusedInside: !!item && item.activeFocus
                        && safeLeft <= 0 && safeEdge >= width
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
                            width: root.shelf ? (item ? item.implicitWidth : 160) : slot.closer ? 128 : root.cellWidth
                            height: slot.closer ? root.artworkHeight : root.cellHeight
                            sourceComponent: slot.closer ? closeCard
                                    : (root.variant === "app" || root.shelf) ? tile
                                    : (root.variant === "poster" && !String((modelData && modelData.artwork) || "").length
                                       && !(modelData && modelData.stack && modelData.stack.length)) ? titleCard : card
                            function chosen() {
                                root.selectedIndex = slot.index;
                                root.selectedId = modelData.id;
                                root.selected(modelData);
                                root.reveal(slot.index);
                            }
                            // CL-3a: Close beside an application's Continue card,
                            // a plate at 6 % with text-2 at rest.
                            Component {
                                id: closeCard
                                Surface {
                                    id: closeSurface
                                    objectName: "entry-" + entryLoader.modelData.id
                                    x: -slot.closeInset
                                    width: 128; height: root.artworkHeight
                                    text: qsTr("Close %1").arg(entryLoader.modelData.title || "")
                                    shapeRole: "action"
                                    cornerRadius: 26
                                    focusScale: 1.06
                                    focusLift: 0
                                    liftFromBottom: false
                                    restingColor: Qt.rgba(1, 1, 1, 0.06)
                                    busy: root.closingIds.indexOf(entryLoader.modelData.windowId) >= 0
                                    rowBrightness: activeFocus ? 1
                                            : root.activeFocus ? Theme.rowLitBrightness : Theme.rowDimBrightness
                                    pointerMode: root.pointerMode
                                    hoverFocusEnabled: root.hoverFocusEnabled && slot.painted
                                    onActiveFocusChanged: if (activeFocus) entryLoader.chosen()
                                    onActivated: root.activated(entryLoader.modelData.id)
                                    onDpadRequested: root.dpadRequested()
                                    contentItem: Item {
                                        LineIcon {
                                            objectName: "closeCardIcon"
                                            anchors.centerIn: parent
                                            width: 44; height: 44
                                            visible: !closeSurface.busy
                                            name: "close"
                                            color: closeSurface.activeFocus ? Theme.ink : Theme.textSecondary
                                        }
                                        UiText {
                                            objectName: "closeCardBusy"
                                            anchors.centerIn: parent
                                            visible: closeSurface.busy
                                            text: "…"; font.pixelSize: 44
                                            color: closeSurface.activeFocus ? Theme.ink : Theme.textSecondary
                                        }
                                    }
                                }
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
                                    closing: root.closingIds.indexOf(entryLoader.modelData.runningId || entryLoader.modelData.id) >= 0
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
                                    // The tile sheet opens on a long Menu or a long OK
                                    // (handoff *Closing applications*, spec §6.2). A
                                    // short Menu stays the row's. Letter keys are not
                                    // product keys (spec §6.2; review of #777).
                                    Keys.onPressed: event => {
                                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                            if (!event.isAutoRepeat && !busy) { holdHandled = false; down = true; holdTimer.restart(); }
                                            event.accepted = true;
                                        } else if (event.key === Qt.Key_Menu && event.modifiers === Qt.NoModifier) {
                                            if (!event.isAutoRepeat) { menuHandled = false; menuHeld = true; menuTimer.restart(); }
                                            event.accepted = true;
                                        }
                                    }
                                    property bool holdHandled: false
                                    property bool menuHandled: false
                                    property bool menuHeld: false
                                    Timer {
                                        id: holdTimer; interval: root.holdMs
                                        onTriggered: {
                                            parent.holdHandled = true; parent.down = false;
                                            root.actionsRequested(entryLoader.modelData.id);
                                        }
                                    }
                                    Timer {
                                        id: menuTimer; interval: root.holdMs
                                        onTriggered: {
                                            parent.menuHandled = true;
                                            // A web shortcut keeps Menu for the shortcut
                                            // editor, held or not (review of #777).
                                            if (entryLoader.modelData.webShortcut) root.menuTapped();
                                            else root.actionsRequested(entryLoader.modelData.id);
                                        }
                                    }
                                    Keys.onReleased: event => {
                                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                                            if (event.isAutoRepeat) { event.accepted = true; return; }
                                            holdTimer.stop();
                                            if (holdHandled) { down = false; event.accepted = true; }
                                            else { const invoke = down && enabled && !busy; down = false; if (invoke) clicked(); }
                                            event.accepted = true;
                                        } else if (event.key === Qt.Key_Menu) {
                                            event.accepted = true;
                                            if (event.isAutoRepeat || !menuHeld) return;
                                            menuHeld = false;
                                            menuTimer.stop();
                                            if (!menuHandled) root.menuTapped();
                                        }
                                    }
                                    onActiveFocusChanged: {
                                        if (activeFocus) entryLoader.chosen();
                                        else { holdTimer.stop(); menuTimer.stop(); menuHeld = false; }
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
                                    // At the row's start the image grows from its left
                                    // edge, so its rim stays inside the safe area (#837).
                                    alignStart: Math.abs(slot.safeLeft) < 1
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
                                    // New lists what is new; a poster with a few seconds
                                    // watched showed a sliver of Ember line (#837).
                                    showProgress: entryLoader.modelData.progress !== undefined && root.variant !== "poster"
                                    progress: entryLoader.modelData.progress || 0
                                    musicState: entryLoader.modelData.musicState || ""
                                    musicSourceIcon: entryLoader.modelData.sourceIcon || "music_note"
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
                                    // Only the plate lifts and the caption stays, as on
                                    // ContentCard; at the row's start it grows from its
                                    // left edge (#837).
                                    focusScale: 1
                                    focusLift: 0
                                    property real imageFocus: activeFocus ? 1 : 0
                                    Behavior on imageFocus {
                                        NumberAnimation { duration: Theme.focusDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easing }
                                    }
                                    readonly property bool alignStart: Math.abs(slot.safeLeft) < 1
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
                                            objectName: "cardFrame"
                                            width: parent.width
                                            height: root.artworkHeight
                                            transform: [
                                                Scale {
                                                    origin.x: posterCard.alignStart ? 0 : titleFrame.width / 2
                                                    origin.y: titleFrame.height
                                                    xScale: 1 + (posterCard.pointerMode ? (Theme.contentFocusScale - 1) / 2 : Theme.contentFocusScale - 1) * posterCard.imageFocus
                                                    yScale: xScale
                                                },
                                                Translate {
                                                    x: posterCard.alignStart ? (Theme.matThickness + Theme.rimThickness) * (1 + (Theme.contentFocusScale - 1) * posterCard.imageFocus) * posterCard.imageFocus : 0
                                                    y: (posterCard.pointerMode ? -2 : Theme.contentFocusLift) * posterCard.imageFocus
                                                }
                                            ]
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
                                            // A title card is a flat plate: no bounce, and no
                                            // cast between it and its caption (#837).
                                            ArtworkFocus {
                                                shown: posterCard.activeFocus
                                                bounce: false
                                                artworkRadius: posterCard.cornerRadius
                                            }
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
    // The right fade of an Applications row, from x 1808 to x 1896 of the
    // screen, the mirror of the left one.
    Rectangle {
        objectName: "rowRightFade"
        z: 80
        x: Theme.canvasWidth - Theme.edgeFadeEnd - root.screenX
        y: viewport.y
        width: Theme.edgeFadeEnd - Theme.edgeFadeStart
        height: viewport.height
        visible: root.fadeRight
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Qt.rgba(Theme.background.r, Theme.background.g, Theme.background.b, 0) }
            GradientStop { position: 1; color: Theme.background }
        }
    }
    MouseArea {
        objectName: "rowPointerBlocker"
        // The cards' viewport reaches past the row's edges.
        x: -root.leftBleed; width: parent.width + root.leftBleed + root.rightBleed; height: parent.height
        z: 2000
        enabled: root.pointerBlocked
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onWheel: wheel => wheel.accepted = true
    }
}
