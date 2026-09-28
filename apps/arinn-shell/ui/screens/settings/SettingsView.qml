// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
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
        if (row.type === "boolean") {
            if (row.currentValue === true) return qsTr("On");
            if (row.currentValue === false) return qsTr("Off");
            return qsTr("Unsupported");
        }
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
    function pathKindLabel(row) {
        if (!row || row.type !== "path" || !row.constraints) return "";
        const kind = row.constraints.kind;
        if (!kind) return "";
        if (kind === "directory") return qsTr("Folder");
        if (kind === "file") return qsTr("File");
        return qsTr("Unsupported");
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

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.UiText {
        x: Ui.Theme.safeLeft
        y: 64
        text: qsTr("Settings")
        role: "title"
    }
    Column {
        id: categoryColumn
        x: Ui.Theme.safeLeft
        y: 168
        width: 540
        spacing: 12
        Repeater {
            id: categoryRepeater
            model: root.categories
            delegate: Ui.Surface {
                id: categoryButton
                required property var modelData
                required property int index
                objectName: "category-" + modelData.id
                width: categoryColumn.width
                height: 88
                cornerRadius: 26
                text: modelData.title || ""
                pointerMode: root.pointerMode
                hoverFocusEnabled: true
                focus: index === root.categoryIndex && !root.detailFocused
                restingColor: index === root.categoryIndex ? Qt.rgba(1, 1, 1, 0.08) : Ui.Theme.plate
                onDpadRequested: root.pointerMode = false
                onActivated: root.moveToRows()
                contentItem: Item {
                    Rectangle {
                        x: 28
                        anchors.verticalCenter: parent.verticalCenter
                        width: 10
                        height: 10
                        radius: 5
                        color: Ui.Theme.textPrimary
                        opacity: categoryButton.index === root.categoryIndex ? 1 : 0
                    }
                    Ui.UiText {
                        x: 54
                        width: parent.width - 78
                        anchors.verticalCenter: parent.verticalCenter
                        text: categoryButton.text
                        role: "section"
                        elide: Text.ElideRight
                        wrapMode: Text.NoWrap
                    }
                }
            }
        }
    }

    Item {
        id: detail
        x: 740
        y: 168
        width: 1920 - 740 - Ui.Theme.safeRight
        height: 1080 - 168 - 72
        Ui.UiText {
            id: eyebrow
            width: parent.width
            role: "eyebrow"
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
            anchors.topMargin: 36
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
            anchors.topMargin: 28
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
                spacing: 16
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
                        width: rowColumn.width
                        height: blocked ? blockedButton.implicitHeight : settingButton.implicitHeight

                        Ui.Surface {
                            id: blockedButton
                            visible: rowScope.blocked
                            enabled: rowScope.blocked
                            width: rowScope.width
                            implicitHeight: Math.max(112, blockedText.implicitHeight + 40)
                            cornerRadius: 28
                            text: modelData.title || ""
                            objectName: rowScope.blocked ? "unavailable-" + modelData.id : ""
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: true
                            focus: root.detailFocused && rowScope.index === root.rowFocusIndex && rowScope.blocked
                            onDpadRequested: root.pointerMode = false
                            contentItem: Item {
                                Column {
                                    id: blockedText
                                    x: 32
                                    y: 20
                                    width: parent.width - 32 - 220 - 24 - 32
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
                                    }
                                }
                                Ui.UiText {
                                    id: blockedState
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

                        Ui.Surface {
                            id: settingButton
                            visible: !rowScope.blocked
                            enabled: !rowScope.blocked && modelData.enabled !== false
                            width: rowScope.width
                            implicitHeight: Math.max(112, settingText.implicitHeight + 40)
                            cornerRadius: 28
                            text: modelData.title || ""
                            objectName: rowScope.blocked ? "" : "setting-" + modelData.id
                            pointerMode: root.pointerMode
                            hoverFocusEnabled: true
                            focus: root.detailFocused && rowScope.index === root.rowFocusIndex && !rowScope.blocked
                            onDpadRequested: root.pointerMode = false
                            onActivated: root.commitRow(modelData)
                            contentItem: Item {
                                Row {
                                    id: settingLayout
                                    anchors.fill: parent
                                    anchors.leftMargin: 32
                                    anchors.rightMargin: 32
                                    anchors.topMargin: 20
                                    anchors.bottomMargin: 20
                                    spacing: 24
                                    Column {
                                        id: settingText
                                        width: settingLayout.width - valueBox.width - settingLayout.spacing
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
                                        }
                                    }
                                    Item {
                                        id: valueBox
                                        width: 320
                                        height: settingText.implicitHeight
                                        Row {
                                            anchors.right: parent.right
                                            anchors.verticalCenter: parent.verticalCenter
                                            spacing: 16
                                            Rectangle {
                                                objectName: rowScope.blocked || modelData.type !== "boolean" ? "" : "toggle-" + modelData.id
                                                visible: modelData.type === "boolean" && !root.valueUnsupported(modelData)
                                                width: 88
                                                height: 50
                                                radius: 25
                                                anchors.verticalCenter: parent.verticalCenter
                                                color: modelData.currentValue === true ? Ui.Theme.textPrimary : Ui.Theme.plate
                                                Rectangle {
                                                    width: 36
                                                    height: 36
                                                    radius: 18
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    x: modelData.currentValue === true ? parent.width - width - 7 : 7
                                                    color: modelData.currentValue === true ? Ui.Theme.ink : Ui.Theme.textPrimary
                                                }
                                            }
                                            Column {
                                                spacing: 4
                                                anchors.verticalCenter: parent.verticalCenter
                                                Ui.UiText {
                                                    objectName: rowScope.blocked || root.editingId === modelData.id ? "" : "value-" + modelData.id
                                                    visible: root.editingId !== modelData.id
                                                    width: 180
                                                    horizontalAlignment: Text.AlignRight
                                                    role: "meta"
                                                    color: root.valueUnsupported(modelData) ? Ui.Theme.error : Ui.Theme.textPrimary
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
                                                Ui.UiText {
                                                    width: 180
                                                    horizontalAlignment: Text.AlignRight
                                                    role: "caption"
                                                    color: root.pathKindLabel(modelData) === qsTr("Unsupported") ? Ui.Theme.error : Ui.Theme.textHint
                                                    text: root.pathKindLabel(modelData)
                                                    visible: text.length > 0 && root.editingId !== modelData.id
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
        }
    }
}
