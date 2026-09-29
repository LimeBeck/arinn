// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Templates as T
import "../../" as Ui

// Fixture-driven settings. Categories, rows and capabilities are supplied by
// the caller. A change emits valueChangeRequested and leaves the fixture as it
// was: this view does not write daemon configuration.
FocusScope {
    id: root
    width: 1920
    height: 1080
    property var categories: []
    property var rows: []
    property var capabilities: []
    property bool pointerMode: false
    property int categoryIndex: 0
    property int rowFocusIndex: 0
    property bool detailFocused: false
    property string editingId: ""
    property string editingDraft: ""
    signal valueChangeRequested(string settingId, var value)
    signal dismissRequested()

    readonly property var activeRows: collectRows()
    readonly property string paneKind: paneFor(currentCategory())

    function currentCategory() {
        if (categoryIndex < 0 || categoryIndex >= categories.length) return null;
        return categories[categoryIndex];
    }
    function registeredCapability(name) {
        if (!name) return null;
        for (let i = 0; i < capabilities.length; ++i) {
            const entry = capabilities[i];
            if (entry && entry.id === name) return entry;
        }
        return null;
    }
    // An empty name means the row does not depend on a provider capability.
    // Anything else must be registered and available, or the UI is unavailable.
    function capabilityReady(name) {
        if (!name) return true;
        const found = registeredCapability(name);
        return !!found && found.available !== false;
    }
    function collectRows() {
        const category = categories[categoryIndex];
        if (!category || !capabilityReady(category.capability)) return [];
        const result = [];
        for (let i = 0; i < rows.length; ++i) {
            const row = rows[i];
            if (row && row.categoryId === category.id) result.push(row);
        }
        return result;
    }
    function paneFor(category) {
        if (!category) return "empty";
        if (!capabilityReady(category.capability)) return "unavailable";
        return collectRows().length === 0 ? "empty" : "ready";
    }
    function rowBlocked(row) {
        return !row || !capabilityReady(row.capability);
    }
    function constraintValues(row) {
        return row && row.constraints && row.constraints.values ? row.constraints.values : [];
    }
    function valueUnsupported(row) {
        if (!row) return true;
        if (row.type === "boolean") return row.currentValue !== true && row.currentValue !== false;
        if (row.type === "enum") return constraintValues(row).indexOf(row.currentValue) < 0;
        if (row.type === "integer") return !isFinite(Number(row.currentValue));
        if (row.type === "string" || row.type === "path") return false;
        return true;
    }
    function displayValue(row) {
        if (!row) return "";
        // A boolean is only the switch. Words would read as a second value.
        if (row.type === "boolean")
            return (row.currentValue === true || row.currentValue === false) ? "" : qsTr("Unsupported");
        if (row.type === "enum") {
            if (valueUnsupported(row)) return qsTr("Unsupported");
            return String(row.currentValue);
        }
        if (row.type === "integer") {
            if (valueUnsupported(row)) return qsTr("Unsupported");
            return String(row.currentValue);
        }
        if (row.type === "string" || row.type === "path") {
            if (row.currentValue === undefined || row.currentValue === null || row.currentValue === "")
                return qsTr("Not set");
            return String(row.currentValue);
        }
        return qsTr("Unsupported");
    }
    function paintIcon(ctx, name) {
        ctx.clearRect(0, 0, 38, 38);
        ctx.lineWidth = 2.2;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        const stroke = () => { ctx.stroke(); };
        if (name === "video_library") {
            ctx.beginPath();
            ctx.moveTo(8, 12); ctx.lineTo(30, 12); ctx.lineTo(30, 26); ctx.lineTo(8, 26); ctx.closePath();
            stroke();
            ctx.beginPath();
            ctx.moveTo(16, 16); ctx.lineTo(16, 22); ctx.lineTo(23, 19); ctx.closePath();
            ctx.fill();
        } else if (name === "play_circle") {
            ctx.beginPath();
            ctx.arc(19, 19, 10, 0, Math.PI * 2);
            stroke();
            ctx.beginPath();
            ctx.moveTo(16, 14); ctx.lineTo(16, 24); ctx.lineTo(25, 19); ctx.closePath();
            ctx.fill();
        } else if (name === "volume_up") {
            ctx.beginPath();
            ctx.moveTo(10, 16); ctx.lineTo(15, 16); ctx.lineTo(21, 11); ctx.lineTo(21, 27); ctx.lineTo(15, 22); ctx.lineTo(10, 22); ctx.closePath();
            ctx.fill();
            ctx.beginPath();
            ctx.arc(21, 19, 6, -0.9, 0.9);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(21, 19, 10, -0.8, 0.8);
            ctx.stroke();
        } else if (name === "wifi") {
            ctx.beginPath(); ctx.arc(19, 26, 2.2, 0, Math.PI * 2); ctx.fill();
            ctx.beginPath(); ctx.arc(19, 26, 7, Math.PI * 1.2, Math.PI * 1.8); ctx.stroke();
            ctx.beginPath(); ctx.arc(19, 26, 12, Math.PI * 1.25, Math.PI * 1.75); ctx.stroke();
        } else if (name === "apps") {
            for (let col = 0; col < 2; ++col) {
                for (let row = 0; row < 2; ++row) {
                    ctx.beginPath();
                    ctx.rect(9 + col * 11, 9 + row * 11, 8, 8);
                    ctx.fill();
                }
            }
        } else if (name === "palette") {
            ctx.beginPath();
            ctx.arc(19, 19, 11, 0, Math.PI * 2);
            stroke();
            ctx.beginPath(); ctx.arc(14, 16, 2, 0, Math.PI * 2); ctx.fill();
            ctx.beginPath(); ctx.arc(22, 14, 2, 0, Math.PI * 2); ctx.fill();
            ctx.beginPath(); ctx.arc(25, 21, 2, 0, Math.PI * 2); ctx.fill();
        } else if (name === "memory") {
            ctx.beginPath();
            ctx.rect(12, 12, 14, 14);
            stroke();
            for (let i = 0; i < 3; ++i) {
                ctx.beginPath();
                ctx.moveTo(15 + i * 4, 12); ctx.lineTo(15 + i * 4, 8);
                ctx.moveTo(15 + i * 4, 26); ctx.lineTo(15 + i * 4, 30);
                ctx.moveTo(12, 15 + i * 4); ctx.lineTo(8, 15 + i * 4);
                ctx.moveTo(26, 15 + i * 4); ctx.lineTo(30, 15 + i * 4);
                ctx.stroke();
            }
        } else {
            ctx.beginPath();
            ctx.arc(19, 19, 8, 0, Math.PI * 2);
            stroke();
        }
    }
    function proposedValue(row) {
        const constraints = row.constraints || {};
        if (row.type === "boolean") {
            if (row.currentValue !== true && row.currentValue !== false) return undefined;
            return !row.currentValue;
        }
        if (row.type === "enum") {
            const values = constraintValues(row);
            if (!values.length) return undefined;
            const index = values.indexOf(row.currentValue);
            return values[index < 0 ? 0 : (index + 1) % values.length];
        }
        if (row.type === "integer") {
            const current = Number(row.currentValue);
            if (!isFinite(current)) return undefined;
            const step = Number(constraints.step);
            const delta = isFinite(step) && step !== 0 ? step : 1;
            const next = current + delta;
            if (constraints.minimum !== undefined && next < Number(constraints.minimum)) return undefined;
            if (constraints.maximum !== undefined && next > Number(constraints.maximum)) return undefined;
            return next;
        }
        return undefined;
    }
    function commitRow(row) {
        if (!row || row.enabled === false || rowBlocked(row)) return;
        if (row.type === "string" || row.type === "path") {
            editingDraft = row.currentValue === undefined || row.currentValue === null ? "" : String(row.currentValue);
            editingId = row.id;
            return;
        }
        const next = proposedValue(row);
        if (next === undefined) return;
        valueChangeRequested(row.id, next);
    }
    function finishEdit(commit) {
        if (!editingId) return;
        const id = editingId;
        const draft = editingDraft;
        const row = rowById(id);
        if (commit && row && row.type === "string") {
            const max = row.constraints ? Number(row.constraints.maximumLength) : NaN;
            if (isFinite(max) && max >= 0 && draft.length > max) return;
        }
        editingId = "";
        if (commit) valueChangeRequested(id, draft);
        const index = activeRowIndex(id);
        if (index >= 0) focusRow(index);
    }
    function rowById(id) {
        for (let i = 0; i < rows.length; ++i) if (rows[i] && rows[i].id === id) return rows[i];
        return null;
    }
    function activeRowIndex(id) {
        for (let i = 0; i < activeRows.length; ++i) if (activeRows[i].id === id) return i;
        return -1;
    }
    function unavailableTitle(category) {
        const found = category ? registeredCapability(category.capability) : null;
        return found && found.title ? found.title : qsTr("Unavailable");
    }
    function unavailableDescription(category) {
        const found = category ? registeredCapability(category.capability) : null;
        return found && found.description ? found.description : qsTr("This capability is not registered.");
    }
    function focusCategory(index) {
        if (!categories.length) return;
        const next = Math.max(0, Math.min(index, categories.length - 1));
        editingId = "";
        detailFocused = false;
        categoryIndex = next;
        const item = categoryRepeater.itemAt(next);
        if (item) item.forceActiveFocus();
    }
    function rowScopeAt(index) { return rowRepeater.itemAt(index); }
    function focusRow(index) {
        const scope = rowScopeAt(index);
        if (!scope || !scope.canFocus) return false;
        detailFocused = true;
        rowFocusIndex = index;
        scope.focusTarget.forceActiveFocus();
        revealRow(scope);
        return true;
    }
    function firstFocusableRow() {
        for (let i = 0; i < rowRepeater.count; ++i) {
            const scope = rowScopeAt(i);
            if (scope && scope.canFocus) return i;
        }
        return -1;
    }
    function revealRow(scope) {
        const top = scope.y;
        const bottom = top + scope.height;
        if (top < rowFlick.contentY) rowFlick.contentY = top;
        else if (bottom > rowFlick.contentY + rowFlick.height)
            rowFlick.contentY = Math.max(0, bottom - rowFlick.height);
    }
    function moveVertical(step) {
        pointerMode = false;
        if (detailFocused) {
            let index = rowFocusIndex + step;
            while (index >= 0 && index < rowRepeater.count) {
                const scope = rowScopeAt(index);
                if (scope && scope.canFocus) { focusRow(index); return; }
                index += step;
            }
            return;
        }
        const next = categoryIndex + step;
        if (next < 0 || next >= categories.length) return;
        focusCategory(next);
    }
    function moveToRows() {
        pointerMode = false;
        if (detailFocused) return;
        const index = firstFocusableRow();
        if (index >= 0) focusRow(index);
    }
    function moveToCategories() {
        pointerMode = false;
        if (!detailFocused) return;
        focusCategory(categoryIndex);
    }

    Keys.onPressed: event => {
        if (editingId.length > 0) return;
        const arrows = [Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down];
        if (arrows.includes(event.key)) pointerMode = false;
        if (event.key === Qt.Key_Up) { moveVertical(-1); event.accepted = true; }
        else if (event.key === Qt.Key_Down) { moveVertical(1); event.accepted = true; }
        else if (event.key === Qt.Key_Left) { moveToCategories(); event.accepted = true; }
        else if (event.key === Qt.Key_Right) { moveToRows(); event.accepted = true; }
        else if (event.key === Qt.Key_Tab) { moveVertical(1); event.accepted = true; }
        else if (event.key === Qt.Key_Backtab) { moveVertical(-1); event.accepted = true; }
        else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace) {
            if (detailFocused) moveToCategories();
            else dismissRequested();
            event.accepted = true;
        }
    }
    Component.onCompleted: Qt.callLater(() => { if (categories.length) focusCategory(0); })
    onCategoryIndexChanged: rowFlick.contentY = 0

    function selectKey(button, event, release) {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter) return false;
        if (!release) {
            if (!event.isAutoRepeat && button.enabled) button.down = true;
        } else if (!event.isAutoRepeat) {
            const invoke = button.down && button.enabled;
            button.down = false;
            if (invoke) button.clicked();
        }
        event.accepted = true;
        return true;
    }

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.UiText {
        x: 120
        y: 96
        text: qsTr("Settings")
        role: "title"
    }
    Column {
        id: categoryColumn
        x: 120
        y: 220
        width: 540
        spacing: 8
        Repeater {
            id: categoryRepeater
            model: root.categories
            delegate: T.AbstractButton {
                id: categoryButton
                required property var modelData
                required property int index
                readonly property bool chosen: index === root.categoryIndex
                objectName: "category-" + modelData.id
                width: categoryColumn.width
                height: 88
                padding: 0
                focusPolicy: Qt.StrongFocus
                activeFocusOnTab: false
                text: modelData.title || ""
                focus: index === root.categoryIndex && !root.detailFocused
                onClicked: root.moveToRows()
                Keys.onPressed: event => root.selectKey(categoryButton, event, false)
                Keys.onReleased: event => root.selectKey(categoryButton, event, true)
                background: Rectangle {
                    radius: 26
                    color: categoryButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20)
                           : (categoryButton.chosen ? Qt.rgba(1, 1, 1, 0.08) : "transparent")
                }
                contentItem: Item {
                    Canvas {
                        id: categoryIcon
                        x: 26
                        y: (parent.height - height) / 2
                        width: 38
                        height: 38
                        property string glyph: modelData.icon || ""
                        property color ink: (categoryButton.activeFocus || categoryButton.chosen)
                                             ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                        onGlyphChanged: requestPaint()
                        onInkChanged: requestPaint()
                        onPaint: {
                            const ctx = getContext("2d");
                            ctx.reset();
                            ctx.strokeStyle = ink;
                            ctx.fillStyle = ink;
                            root.paintIcon(ctx, glyph);
                        }
                        Component.onCompleted: requestPaint()
                    }
                    Ui.UiText {
                        x: 86
                        width: parent.width - 86 - 52
                        anchors.verticalCenter: parent.verticalCenter
                        text: categoryButton.text
                        role: "section"
                        elide: Text.ElideRight
                        wrapMode: Text.NoWrap
                        color: (categoryButton.activeFocus || categoryButton.chosen)
                               ? Ui.Theme.textPrimary : Ui.Theme.textSecondary
                    }
                    Rectangle {
                        objectName: "category-dot-" + modelData.id
                        width: 10
                        height: 10
                        radius: 5
                        anchors.right: parent.right
                        anchors.rightMargin: 26
                        anchors.verticalCenter: parent.verticalCenter
                        color: Ui.Theme.ember
                        opacity: categoryButton.chosen && !categoryButton.activeFocus ? 1 : 0
                    }
                }
            }
        }
    }

    Item {
        id: detail
        x: 740
        y: 220
        width: 1920 - 740 - 96
        height: 1080 - 220 - 48
        Ui.UiText {
            id: eyebrow
            width: parent.width
            height: 40
            verticalAlignment: Text.AlignVCenter
            role: "eyebrow"
            font.capitalization: Font.AllUppercase
            color: Ui.Theme.textSecondary
            text: {
                const category = root.currentCategory();
                return category && category.title ? category.title : "";
            }
        }
        Ui.StatusPane {
            id: statusPane
            objectName: root.paneKind === "unavailable" ? "unavailablePane" : "emptyPane"
            anchors.top: eyebrow.bottom
            anchors.topMargin: 10
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(820, parent.width)
            visible: root.paneKind !== "ready"
            kind: root.paneKind === "unavailable" ? "unavailable" : "empty"
            title: root.paneKind === "unavailable" ? root.unavailableTitle(root.currentCategory()) : qsTr("Nothing to show")
            description: root.paneKind === "unavailable"
                         ? root.unavailableDescription(root.currentCategory())
                         : qsTr("No settings are registered for this category.")
            actionText: ""
            pointerMode: root.pointerMode
            hoverFocusEnabled: true
        }
        Flickable {
            id: rowFlick
            anchors.top: eyebrow.bottom
            anchors.topMargin: 10
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            visible: root.paneKind === "ready"
            contentWidth: width
            contentHeight: rowColumn.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: rowColumn
                width: rowFlick.width
                spacing: 10
                Repeater {
                    id: rowRepeater
                    model: root.activeRows
                    delegate: FocusScope {
                        id: rowScope
                        required property var modelData
                        required property int index
                        readonly property bool blocked: root.rowBlocked(modelData)
                        readonly property Item focusTarget: blocked ? blockedButton : settingButton
                        readonly property bool canFocus: focusTarget.visible && focusTarget.enabled
                        readonly property bool switchRow: !blocked && modelData.type === "boolean" && !root.valueUnsupported(modelData)
                        width: rowColumn.width
                        height: blocked ? blockedButton.height : settingButton.height

                        T.AbstractButton {
                            id: blockedButton
                            visible: rowScope.blocked
                            enabled: rowScope.blocked
                            width: rowScope.width
                            height: Math.max(112, blockedText.implicitHeight + 40)
                            padding: 0
                            focusPolicy: Qt.StrongFocus
                            activeFocusOnTab: false
                            text: modelData.title || ""
                            objectName: rowScope.blocked ? "unavailable-" + modelData.id : ""
                            focus: root.detailFocused && rowScope.index === root.rowFocusIndex && rowScope.blocked
                            background: Rectangle {
                                radius: 28
                                color: blockedButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.06)
                            }
                            contentItem: Item {
                                Column {
                                    id: blockedText
                                    x: 32
                                    y: 20
                                    width: parent.width - 32 - 220 - 32
                                    spacing: 6
                                    Ui.UiText {
                                        width: parent.width
                                        text: modelData.title || ""
                                        role: "control"
                                    }
                                    Ui.UiText {
                                        width: parent.width
                                        text: modelData.description || ""
                                        role: "meta"
                                        color: Ui.Theme.textSecondary
                                        visible: text.length > 0
                                    }
                                }
                                Ui.UiText {
                                    objectName: rowScope.blocked ? "state-" + modelData.id : ""
                                    width: 220
                                    anchors.right: parent.right
                                    anchors.rightMargin: 32
                                    anchors.verticalCenter: parent.verticalCenter
                                    horizontalAlignment: Text.AlignRight
                                    role: "meta"
                                    color: Ui.Theme.error
                                    text: qsTr("Unavailable")
                                }
                            }
                        }

                        T.AbstractButton {
                            id: settingButton
                            visible: !rowScope.blocked
                            enabled: !rowScope.blocked && modelData.enabled !== false
                            width: rowScope.width
                            height: Math.max(112, settingText.implicitHeight + 40)
                            padding: 0
                            focusPolicy: Qt.StrongFocus
                            activeFocusOnTab: false
                            text: modelData.title || ""
                            objectName: rowScope.blocked ? "" : "setting-" + modelData.id
                            focus: root.detailFocused && rowScope.index === root.rowFocusIndex && !rowScope.blocked
                            onClicked: root.commitRow(modelData)
                            Keys.onPressed: event => root.selectKey(settingButton, event, false)
                            Keys.onReleased: event => root.selectKey(settingButton, event, true)
                            background: Rectangle {
                                radius: 28
                                color: settingButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.06)
                            }
                            contentItem: Item {
                                Column {
                                    id: settingText
                                    x: 32
                                    y: 20
                                    width: parent.width - 64 - (rowScope.switchRow ? 88 : 240)
                                    spacing: 6
                                    Ui.UiText {
                                        width: parent.width
                                        text: modelData.title || ""
                                        role: "control"
                                    }
                                    Ui.UiText {
                                        width: parent.width
                                        text: modelData.description || ""
                                        role: "meta"
                                        color: Ui.Theme.textSecondary
                                        visible: text.length > 0
                                    }
                                }
                                Item {
                                    id: toggle
                                    objectName: rowScope.switchRow ? "toggle-" + modelData.id : ""
                                    visible: rowScope.switchRow
                                    width: 88
                                    height: 50
                                    anchors.right: parent.right
                                    anchors.rightMargin: 32
                                    anchors.verticalCenter: parent.verticalCenter
                                    Rectangle {
                                        objectName: rowScope.switchRow ? "track-" + modelData.id : ""
                                        anchors.fill: parent
                                        radius: 25
                                        color: modelData.currentValue === true ? Ui.Theme.ember : Qt.rgba(1, 1, 1, 0.20)
                                    }
                                    Rectangle {
                                        objectName: rowScope.switchRow ? "knob-" + modelData.id : ""
                                        y: 5
                                        x: modelData.currentValue === true ? 43 : 5
                                        width: 40
                                        height: 40
                                        radius: 20
                                        color: modelData.currentValue === true ? Ui.Theme.ink : Ui.Theme.textPrimary
                                    }
                                }
                                Row {
                                    visible: !rowScope.switchRow
                                    anchors.right: parent.right
                                    anchors.rightMargin: 32
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 8
                                    Ui.UiText {
                                        objectName: rowScope.blocked || rowScope.switchRow || root.editingId === modelData.id ? "" : "value-" + modelData.id
                                        visible: root.editingId !== modelData.id
                                        width: Math.min(220, implicitWidth)
                                        horizontalAlignment: Text.AlignRight
                                        role: "meta"
                                        color: root.valueUnsupported(modelData) ? Ui.Theme.error : Ui.Theme.textSecondary
                                        text: root.displayValue(modelData)
                                    }
                                    TextInput {
                                        id: editor
                                        objectName: "editor-" + modelData.id
                                        visible: root.editingId === modelData.id
                                        width: 180
                                        horizontalAlignment: Text.AlignRight
                                        color: Ui.Theme.textPrimary
                                        font.family: Ui.Theme.fontFamily
                                        font.pixelSize: 26
                                        clip: true
                                        onVisibleChanged: if (visible) {
                                            text = root.editingDraft;
                                            forceActiveFocus();
                                        }
                                        onTextChanged: if (visible && activeFocus) root.editingDraft = text
                                        Keys.onPressed: event => {
                                            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                                if (!event.isAutoRepeat) root.finishEdit(true);
                                                event.accepted = true;
                                            } else if (event.key === Qt.Key_Escape) {
                                                root.finishEdit(false);
                                                event.accepted = true;
                                            } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
                                                const step = event.key === Qt.Key_Up ? -1 : 1;
                                                root.finishEdit(false);
                                                root.moveVertical(step);
                                                event.accepted = true;
                                            }
                                        }
                                    }
                                    Canvas {
                                        objectName: rowScope.switchRow || root.editingId === modelData.id ? "" : "chevron-" + modelData.id
                                        visible: root.editingId !== modelData.id
                                        width: 28
                                        height: 36
                                        onPaint: {
                                            const ctx = getContext("2d");
                                            ctx.reset();
                                            ctx.strokeStyle = Ui.Theme.textSecondary;
                                            ctx.lineWidth = 2.4;
                                            ctx.lineCap = "round";
                                            ctx.lineJoin = "round";
                                            ctx.beginPath();
                                            ctx.moveTo(8, 8);
                                            ctx.lineTo(16, 18);
                                            ctx.lineTo(8, 28);
                                            ctx.stroke();
                                        }
                                        Component.onCompleted: requestPaint()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
