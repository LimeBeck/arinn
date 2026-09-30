// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Templates as T
import "../../" as Ui

// Settings presentation. Categories and rows come from the caller (the shell
// binds SettingsCatalog). This view does not call a daemon or a provider.
FocusScope {
    id: root
    width: 1920
    height: 1080
    property string screenState: "ready"
    property var categories: []
    property var rows: []
    property bool pointerMode: false
    property int categoryIndex: 0
    property int rowFocusIndex: 0
    property string partName: "main"
    property string focusedRowKey: ""
    property bool detailFocused: false
    property string editingKey: ""
    property string editingPart: ""
    property string editingDraft: ""
    property string choosingKey: ""
    property int optionIndex: 0
    signal valueChangeRequested(string providerId, string settingId, var value)
    signal dismissRequested()
    signal retryRequested()

    readonly property bool screenBlocked: screenState === "loading" || screenState === "error" || screenState === "unavailable"
    readonly property var activeRows: collectRows()
    onActiveRowsChanged: queueRowFocusRestore()
    readonly property string paneKind: paneFor(currentCategory())
    readonly property var choosingRow: rowByKey(choosingKey)
    readonly property var choosingOptions: choosingRow ? optionsOf(choosingRow) : []

    function currentCategory() {
        if (categoryIndex < 0 || categoryIndex >= categories.length) return null;
        return categories[categoryIndex];
    }
    function collectRows() {
        const category = categories[categoryIndex];
        if (!category) return [];
        const result = [];
        for (let i = 0; i < rows.length; ++i) {
            const row = rows[i];
            if (row && row.categoryId === category.id) result.push(row);
        }
        return result;
    }
    function paneFor(category) {
        if (!category) return "empty";
        return collectRows().length === 0 ? "empty" : "ready";
    }
    function rowKey(row) {
        if (!row) return "";
        const provider = row.providerId || "";
        return provider.length ? provider + "-" + row.id : String(row.id || "");
    }
    function rowByKey(key) {
        for (let i = 0; i < rows.length; ++i)
            if (rows[i] && rowKey(rows[i]) === key) return rows[i];
        return null;
    }
    function canEdit(row) {
        return !!row && row.enabled !== false && !row.pending;
    }
    function optionsOf(row) {
        const options = row && row.constraints && row.constraints.options;
        return options && options.length !== undefined ? options : [];
    }
    function isMultiple(row) {
        return !!row && row.type === "path" && !!(row.constraints && row.constraints.multiple);
    }
    function pathList(row) {
        if (!row || row.currentValue === undefined || row.currentValue === null) return [];
        return row.currentValue.length !== undefined && row.type === "path" && isMultiple(row)
               ? row.currentValue : [];
    }
    function hasBound(row, name) {
        return !!(row && row.constraints && row.constraints[name] !== undefined && row.constraints[name] !== null
                  && isFinite(Number(row.constraints[name])));
    }
    function integerValue(row) {
        if (!row || row.currentValue === undefined || row.currentValue === null || row.currentValue === "")
            return NaN;
        const n = Number(row.currentValue);
        if (!isFinite(n) || Math.trunc(n) !== n) return NaN;
        return n;
    }
    function valueUnsupported(row) {
        if (!row) return true;
        if (row.type === "boolean") return row.currentValue !== true && row.currentValue !== false;
        if (row.type === "enum") {
            const options = optionsOf(row);
            if (!options.length) return true;
            for (let i = 0; i < options.length; ++i)
                if (options[i] && options[i].value === row.currentValue) return false;
            return true;
        }
        if (row.type === "integer") return !isFinite(integerValue(row));
        if (row.type === "string" || row.type === "path") return false;
        return true;
    }
    function displayValue(row) {
        if (!row) return "";
        if (row.pending) return qsTr("Saving…");
        if (row.type === "boolean")
            return valueUnsupported(row) ? qsTr("Unsupported") : "";
        if (row.type === "enum") {
            if (valueUnsupported(row)) return qsTr("Unsupported");
            const options = optionsOf(row);
            for (let i = 0; i < options.length; ++i)
                if (options[i] && options[i].value === row.currentValue)
                    return options[i].title || String(row.currentValue);
            return String(row.currentValue);
        }
        if (row.type === "integer") {
            if (valueUnsupported(row)) return qsTr("Unsupported");
            return String(integerValue(row));
        }
        if (row.type === "string" || (row.type === "path" && !isMultiple(row))) {
            if (row.currentValue === undefined || row.currentValue === null || row.currentValue === "")
                return qsTr("Not set");
            return String(row.currentValue);
        }
        return qsTr("Unsupported");
    }
    function errorText(code) {
        if (!code) return "";
        if (code === "InvalidArgument") return qsTr("That value is not allowed.");
        if (code === "Conflict") return qsTr("Settings changed. Try again.");
        if (code === "Unavailable") return qsTr("Settings are unavailable.");
        if (code === "Timeout") return qsTr("Settings did not answer in time.");
        if (code === "Unsupported") return qsTr("That change is not available.");
        if (code === "NotFound") return qsTr("That setting is not available.");
        if (code === "PermissionDenied") return qsTr("Settings cannot be changed.");
        return qsTr("The change did not save.");
    }
    function partsFor(row) {
        if (!row) return [];
        if (row.type === "integer" && !valueUnsupported(row)) return ["minus", "value", "plus"];
        if (isMultiple(row)) {
            const list = pathList(row);
            const parts = [];
            for (let i = 0; i < list.length; ++i) {
                parts.push("path:" + i);
                parts.push("remove:" + i);
            }
            parts.push("add");
            return parts;
        }
        return ["main"];
    }
    function emitChange(row, value) {
        if (!canEdit(row)) return;
        valueChangeRequested(row.providerId || "", row.id, value);
    }
    function stepInteger(row, delta) {
        const current = integerValue(row);
        if (!isFinite(current)) return;
        const next = current + delta;
        if (hasBound(row, "min") && next < Number(row.constraints.min)) return;
        if (hasBound(row, "max") && next > Number(row.constraints.max)) return;
        emitChange(row, next);
    }
    function withinInteger(row, n) {
        if (!isFinite(n) || Math.trunc(n) !== n) return false;
        if (hasBound(row, "min") && n < Number(row.constraints.min)) return false;
        if (hasBound(row, "max") && n > Number(row.constraints.max)) return false;
        return true;
    }
    function openEditor(row, part, draft) {
        if (!canEdit(row)) return;
        choosingKey = "";
        editingPart = part;
        editingDraft = draft === undefined || draft === null ? "" : String(draft);
        editingKey = rowKey(row);
    }
    function finishEdit(commit) {
        if (!editingKey) return;
        const row = rowByKey(editingKey);
        const part = editingPart;
        const draft = editingDraft;
        const key = editingKey;
        if (!commit || !row || !canEdit(row)) {
            editingKey = "";
            editingPart = "";
            refocusKey(key);
            return;
        }
        if (row.type === "string") {
            const max = hasBound(row, "maxLength") ? Number(row.constraints.maxLength) : 1024;
            if (draft.length > max) return;
            if (/[\u0000-\u001F\u007F]/.test(draft)) return;
            editingKey = "";
            editingPart = "";
            emitChange(row, draft);
            refocusKey(key);
            return;
        }
        if (row.type === "integer") {
            if (!/^-?\d+$/.test(draft) || !withinInteger(row, Number(draft))) return;
            editingKey = "";
            editingPart = "";
            emitChange(row, Number(draft));
            refocusKey(key);
            return;
        }
        if (row.type === "path" && !isMultiple(row)) {
            editingKey = "";
            editingPart = "";
            emitChange(row, draft);
            refocusKey(key);
            return;
        }
        if (isMultiple(row)) {
            const list = pathList(row).slice();
            if (!draft.length) return;
            if (part === "add") {
                const maxItems = hasBound(row, "maxItems") ? Number(row.constraints.maxItems) : 64;
                if (list.length >= maxItems) return;
                list.push(draft);
            } else if (part.indexOf("path:") === 0) {
                list[Number(part.slice(5))] = draft;
            } else return;
            editingKey = "";
            editingPart = "";
            emitChange(row, list);
            refocusKey(key);
        }
    }
    function removePath(row, index) {
        if (!canEdit(row)) return;
        const list = pathList(row).slice();
        if (index < 0 || index >= list.length) return;
        list.splice(index, 1);
        emitChange(row, list);
    }
    function openOptions(row) {
        if (!canEdit(row) || row.type !== "enum" || valueUnsupported(row) && !optionsOf(row).length) return;
        if (!optionsOf(row).length) return;
        editingKey = "";
        choosingKey = rowKey(row);
        const options = optionsOf(row);
        let index = 0;
        for (let i = 0; i < options.length; ++i)
            if (options[i] && options[i].value === row.currentValue) index = i;
        optionIndex = index;
        Qt.callLater(() => focusOption(optionIndex));
    }
    function focusOption(index) {
        const item = optionRepeater.itemAt(index);
        if (!item) return;
        optionIndex = index;
        item.forceActiveFocus();
    }
    function closeOptions() {
        const key = choosingKey;
        choosingKey = "";
        refocusKey(key);
    }
    function chooseOption() {
        const row = rowByKey(choosingKey);
        const options = optionsOf(row);
        const option = options[optionIndex];
        const key = choosingKey;
        choosingKey = "";
        if (row && option && canEdit(row) && option.value !== row.currentValue)
            emitChange(row, option.value);
        refocusKey(key);
    }
    function activeRowIndex(key) {
        for (let i = 0; i < activeRows.length; ++i)
            if (rowKey(activeRows[i]) === key) return i;
        return -1;
    }
    function refocusKey(key) {
        const index = activeRowIndex(key);
        if (index >= 0) focusPart(index, partsFor(activeRows[index])[0] || "main");
        else if (categories.length) focusCategory(categoryIndex);
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
    function focusCategory(index) {
        if (!categories.length || screenBlocked) return;
        const next = Math.max(0, Math.min(index, categories.length - 1));
        editingKey = "";
        choosingKey = "";
        detailFocused = false;
        focusedRowKey = "";
        categoryIndex = next;
        const item = categoryRepeater.itemAt(next);
        if (item) item.forceActiveFocus();
    }
    function rowScopeAt(index) { return rowRepeater.itemAt(index); }
    function focusPart(rowIndex, name) {
        const scope = rowScopeAt(rowIndex);
        if (!scope) return false;
        const item = scope.itemFor(name);
        if (!item || !item.visible) return false;
        detailFocused = true;
        rowFocusIndex = rowIndex;
        partName = name;
        const row = activeRows[rowIndex];
        focusedRowKey = row ? rowKey(row) : focusedRowKey;
        item.forceActiveFocus();
        revealRow(scope);
        return true;
    }
    // The row repeater builds new buttons whenever rows are replaced. Without
    // this, focus falls back to the view and Select does nothing (AC-02).
    function queueRowFocusRestore() {
        if (!detailFocused || editingKey.length || choosingKey.length || !focusedRowKey.length)
            return;
        Qt.callLater(() => restoreRowFocus(0));
    }
    function restoreRowFocus(attempt) {
        if (!detailFocused || editingKey.length || choosingKey.length || !focusedRowKey.length)
            return;
        const index = activeRowIndex(focusedRowKey);
        const parts = index >= 0 ? partsFor(activeRows[index]) : [];
        const part = parts.indexOf(partName) >= 0 ? partName : (parts[0] || "");
        if (index >= 0 && part && focusPart(index, part))
            return;
        if (attempt < 2)
            Qt.callLater(() => restoreRowFocus(attempt + 1));
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
                const parts = partsFor(activeRows[index]);
                if (parts.length && focusPart(index, parts[0])) return;
                index += step;
            }
            return;
        }
        const next = categoryIndex + step;
        if (next < 0 || next >= categories.length) return;
        focusCategory(next);
    }
    function moveHorizontal(step) {
        pointerMode = false;
        if (!detailFocused) {
            if (step > 0) {
                const parts = partsFor(activeRows[0]);
                if (parts.length) focusPart(0, parts[0]);
            }
            return;
        }
        const parts = partsFor(activeRows[rowFocusIndex]);
        let index = parts.indexOf(partName);
        if (index < 0) index = 0;
        const next = index + step;
        if (next >= 0 && next < parts.length) {
            focusPart(rowFocusIndex, parts[next]);
            return;
        }
        if (step < 0) focusCategory(categoryIndex);
    }
    function focusInitial() {
        if (!visible) return;
        if (screenBlocked) {
            screenStatus.forceActiveFocus();
            return;
        }
        if (!categories.length) return;
        if (detailFocused) focusPart(rowFocusIndex, partName);
        else focusCategory(categoryIndex);
    }
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

    Keys.onPressed: event => {
        if (editingKey.length > 0) return;
        if (choosingKey.length > 0) {
            if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back)
                closeOptions();
            event.accepted = true;
            return;
        }
        if (screenBlocked) {
            if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back)
                dismissRequested();
            event.accepted = true;
            return;
        }
        const arrows = [Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down];
        if (arrows.includes(event.key)) pointerMode = false;
        if (event.key === Qt.Key_Up) { moveVertical(-1); event.accepted = true; }
        else if (event.key === Qt.Key_Down) { moveVertical(1); event.accepted = true; }
        else if (event.key === Qt.Key_Left) { moveHorizontal(-1); event.accepted = true; }
        else if (event.key === Qt.Key_Right) { moveHorizontal(1); event.accepted = true; }
        else if (event.key === Qt.Key_Tab) { moveVertical(1); event.accepted = true; }
        else if (event.key === Qt.Key_Backtab) { moveVertical(-1); event.accepted = true; }
        else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back) {
            if (detailFocused) focusCategory(categoryIndex);
            else dismissRequested();
            event.accepted = true;
        }
    }
    Component.onCompleted: Qt.callLater(focusInitial)
    onVisibleChanged: if (visible) Qt.callLater(focusInitial)
    onCategoryIndexChanged: rowFlick.contentY = 0

    Rectangle { anchors.fill: parent; color: Ui.Theme.background }
    Ui.UiText {
        x: 120
        y: 96
        text: qsTr("Settings")
        role: "title"
    }
    Ui.StatusPane {
        id: screenStatus
        objectName: "screenStatus"
        x: 550
        y: 280
        width: 820
        visible: root.screenBlocked
        kind: root.screenState === "loading" ? "loading" : root.screenState === "error" ? "error" : "unavailable"
        title: root.screenState === "loading" ? qsTr("Loading settings…")
               : root.screenState === "error" ? qsTr("Settings could not be loaded")
               : qsTr("Settings are unavailable")
        description: root.screenState === "loading" ? qsTr("Settings reconnect automatically.")
                     : qsTr("Nothing here can be changed until settings are available.")
        actionText: root.screenState === "loading" ? "" : qsTr("Retry")
        onActionRequested: root.retryRequested()
    }
    Column {
        id: categoryColumn
        x: 120
        y: 220
        width: 540
        spacing: 8
        visible: !root.screenBlocked
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
                focus: index === root.categoryIndex && !root.detailFocused && !root.screenBlocked
                onClicked: {
                    root.focusCategory(index);
                    root.moveHorizontal(1);
                }
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
        visible: !root.screenBlocked
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
            objectName: "emptyPane"
            anchors.top: eyebrow.bottom
            anchors.topMargin: 10
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(820, parent.width)
            visible: root.paneKind !== "ready"
            kind: "empty"
            title: qsTr("Nothing to show")
            description: qsTr("No settings are registered for this category.")
            actionText: ""
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
                    onItemAdded: root.queueRowFocusRestore()
                    delegate: FocusScope {
                        id: rowScope
                        required property var modelData
                        required property int index
                        readonly property var row: modelData
                        readonly property string key: root.rowKey(row)
                        // Keeps the focused row when the repeater recreates it. Stay
                        // focused while this row's editor is open so the field
                        // can hold active focus inside the scope.
                        focus: root.detailFocused && root.focusedRowKey === key && !root.choosingKey
                               && (!root.editingKey || root.editingKey === key)
                        function wants(name) {
                            return focus && !root.editingKey && root.partName === name;
                        }
                        readonly property bool multiple: root.isMultiple(row)
                        readonly property bool integerRow: row.type === "integer" && !root.valueUnsupported(row)
                        readonly property bool simpleRow: !multiple && !integerRow
                        readonly property bool switchRow: simpleRow && row.type === "boolean" && !root.valueUnsupported(row)
                        width: rowColumn.width
                        height: rowBody.implicitHeight
                        function itemFor(name) {
                            if (name === "main") return settingButton;
                            if (name === "minus") return minusButton;
                            if (name === "value") return valueButton;
                            if (name === "plus") return plusButton;
                            if (name === "add") return addButton;
                            if (name.indexOf("path:") === 0) {
                                const item = pathRepeater.itemAt(Number(name.slice(5)));
                                return item ? item.pathButton : null;
                            }
                            if (name.indexOf("remove:") === 0) {
                                const item = pathRepeater.itemAt(Number(name.slice(5)));
                                return item ? item.removeButton : null;
                            }
                            return null;
                        }
                        Column {
                            id: rowBody
                            width: parent.width
                            spacing: 8
                            T.AbstractButton {
                                id: settingButton
                                visible: rowScope.simpleRow
                                width: rowScope.width
                                height: Math.max(112, simpleText.implicitHeight + 40)
                                padding: 0
                                focusPolicy: Qt.StrongFocus
                                focus: rowScope.wants("main")
                                activeFocusOnTab: false
                                opacity: row.enabled === false || row.pending ? 0.45 : 1
                                text: row.title || ""
                                objectName: rowScope.simpleRow ? "setting-" + rowScope.key : ""
                                onClicked: {
                                    if (!root.canEdit(row)) return;
                                    if (row.type === "boolean" && !root.valueUnsupported(row))
                                        root.emitChange(row, !row.currentValue);
                                    else if (row.type === "enum")
                                        root.openOptions(row);
                                    else if (row.type === "string" || row.type === "path")
                                        root.openEditor(row, "main", row.currentValue || "");
                                }
                                Keys.onPressed: event => root.selectKey(settingButton, event, false)
                                Keys.onReleased: event => root.selectKey(settingButton, event, true)
                                background: Rectangle {
                                    radius: 28
                                    color: settingButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.06)
                                }
                                contentItem: Item {
                                    Column {
                                        id: simpleText
                                        x: 32
                                        y: 20
                                        width: parent.width - 64 - (rowScope.switchRow ? 88 : 280)
                                        spacing: 6
                                        Ui.UiText { width: parent.width; text: row.title || ""; role: "control" }
                                        Ui.UiText {
                                            width: parent.width
                                            text: row.description || ""
                                            role: "meta"
                                            color: Ui.Theme.textSecondary
                                            visible: text.length > 0
                                        }
                                        Ui.UiText {
                                            objectName: rowScope.simpleRow && row.providerName ? "provider-" + rowScope.key : ""
                                            width: parent.width
                                            text: row.providerName || ""
                                            role: "meta"
                                            color: Ui.Theme.textHint
                                            visible: text.length > 0
                                        }
                                        Ui.UiText {
                                            objectName: rowScope.simpleRow && row.error ? "error-" + rowScope.key : ""
                                            width: parent.width
                                            text: root.errorText(row.error || "")
                                            role: "meta"
                                            color: Ui.Theme.error
                                            visible: text.length > 0
                                        }
                                        Ui.UiText {
                                            objectName: rowScope.simpleRow && row.pending ? "pending-" + rowScope.key : ""
                                            width: parent.width
                                            text: row.pending ? qsTr("Saving…") : ""
                                            role: "meta"
                                            color: Ui.Theme.textSecondary
                                            visible: text.length > 0
                                        }
                                    }
                                    Item {
                                        id: toggle
                                        objectName: rowScope.switchRow ? "toggle-" + rowScope.key : ""
                                        visible: rowScope.switchRow && root.editingKey !== rowScope.key
                                        width: 88
                                        height: 50
                                        anchors.right: parent.right
                                        anchors.rightMargin: 32
                                        anchors.verticalCenter: parent.verticalCenter
                                        Rectangle {
                                            objectName: rowScope.switchRow ? "track-" + rowScope.key : ""
                                            anchors.fill: parent
                                            radius: 25
                                            color: row.currentValue === true ? Ui.Theme.ember : Qt.rgba(1, 1, 1, 0.20)
                                        }
                                        Rectangle {
                                            objectName: rowScope.switchRow ? "knob-" + rowScope.key : ""
                                            y: 5
                                            x: row.currentValue === true ? 43 : 5
                                            width: 40
                                            height: 40
                                            radius: 20
                                            color: row.currentValue === true ? Ui.Theme.ink : Ui.Theme.textPrimary
                                        }
                                    }
                                    Row {
                                        visible: !rowScope.switchRow
                                        anchors.right: parent.right
                                        anchors.rightMargin: 32
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 8
                                        Ui.UiText {
                                            objectName: rowScope.simpleRow && !rowScope.switchRow && root.editingKey !== rowScope.key ? "value-" + rowScope.key : ""
                                            visible: root.editingKey !== rowScope.key
                                            width: Math.min(240, implicitWidth)
                                            horizontalAlignment: Text.AlignRight
                                            role: "meta"
                                            color: root.valueUnsupported(row) ? Ui.Theme.error : Ui.Theme.textSecondary
                                            text: root.displayValue(row)
                                        }
                                        TextInput {
                                            id: simpleEditor
                                            objectName: rowScope.simpleRow ? "editor-" + rowScope.key : ""
                                            visible: rowScope.simpleRow && root.editingKey === rowScope.key && root.editingPart === "main"
                                            width: 220
                                            horizontalAlignment: Text.AlignRight
                                            color: Ui.Theme.textPrimary
                                            font.family: Ui.Theme.fontFamily
                                            font.pixelSize: 26
                                            clip: true
                                            onVisibleChanged: if (visible) {
                                                text = root.editingDraft;
                                                cursorPosition = text.length;
                                                forceActiveFocus();
                                            }
                                            onTextChanged: if (visible && activeFocus) root.editingDraft = text
                                            Keys.onPressed: event => root.editorKeys(event)
                                        }
                                        Canvas {
                                            objectName: rowScope.simpleRow && !rowScope.switchRow && root.editingKey !== rowScope.key ? "chevron-" + rowScope.key : ""
                                            visible: rowScope.simpleRow && root.editingKey !== rowScope.key
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
                            Item {
                                id: integerPlate
                                visible: rowScope.integerRow
                                width: rowScope.width
                                height: Math.max(112, integerText.implicitHeight + 40)
                                Rectangle {
                                    anchors.fill: parent
                                    radius: 28
                                    color: rowScope.activeFocus && rowScope.integerRow ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(1, 1, 1, 0.06)
                                }
                                Column {
                                    id: integerText
                                    x: 32
                                    y: 20
                                    width: parent.width - 64 - 320
                                    spacing: 6
                                    Ui.UiText { width: parent.width; text: row.title || ""; role: "control"; opacity: row.enabled === false || row.pending ? 0.45 : 1 }
                                    Ui.UiText {
                                        width: parent.width
                                        text: row.description || ""
                                        role: "meta"
                                        color: Ui.Theme.textSecondary
                                        visible: text.length > 0
                                    }
                                    Ui.UiText {
                                        objectName: row.providerName && rowScope.integerRow ? "provider-" + rowScope.key : ""
                                        width: parent.width
                                        text: row.providerName || ""
                                        role: "meta"
                                        color: Ui.Theme.textHint
                                        visible: text.length > 0
                                    }
                                    Ui.UiText {
                                        objectName: row.error && rowScope.integerRow ? "error-" + rowScope.key : ""
                                        width: parent.width
                                        text: root.errorText(row.error || "")
                                        role: "meta"
                                        color: Ui.Theme.error
                                        visible: text.length > 0
                                    }
                                    Ui.UiText {
                                        objectName: row.pending && rowScope.integerRow ? "pending-" + rowScope.key : ""
                                        width: parent.width
                                        text: row.pending ? qsTr("Saving…") : ""
                                        role: "meta"
                                        color: Ui.Theme.textSecondary
                                        visible: text.length > 0
                                    }
                                }
                                Row {
                                    anchors.right: parent.right
                                    anchors.rightMargin: 32
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 12
                                    T.AbstractButton {
                                        id: minusButton
                                        objectName: rowScope.integerRow ? "minus-" + rowScope.key : ""
                                        visible: rowScope.integerRow
                                        width: 64
                                        height: 64
                                        padding: 0
                                        focusPolicy: Qt.StrongFocus
                                        focus: rowScope.wants("minus")
                                        activeFocusOnTab: false
                                        opacity: !root.canEdit(row) || (root.hasBound(row, "min") && root.integerValue(row) <= Number(row.constraints.min)) ? 0.45 : 1
                                        text: "−"
                                        onClicked: root.stepInteger(row, -1)
                                        Keys.onPressed: event => root.selectKey(minusButton, event, false)
                                        Keys.onReleased: event => root.selectKey(minusButton, event, true)
                                        background: Rectangle { radius: 18; color: minusButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.08) }
                                        contentItem: Ui.UiText { text: minusButton.text; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                                    }
                                    T.AbstractButton {
                                        id: valueButton
                                        objectName: rowScope.integerRow ? "setting-" + rowScope.key : ""
                                        visible: rowScope.integerRow
                                        width: 140
                                        height: 64
                                        padding: 0
                                        focusPolicy: Qt.StrongFocus
                                        focus: rowScope.wants("value")
                                        activeFocusOnTab: false
                                        opacity: row.enabled === false || row.pending ? 0.45 : 1
                                        onClicked: root.openEditor(row, "value", String(root.integerValue(row)))
                                        Keys.onPressed: event => root.selectKey(valueButton, event, false)
                                        Keys.onReleased: event => root.selectKey(valueButton, event, true)
                                        background: Rectangle { radius: 18; color: valueButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : "transparent" }
                                        contentItem: Item {
                                            Ui.UiText {
                                                anchors.fill: parent
                                                visible: root.editingKey !== rowScope.key
                                                objectName: rowScope.integerRow && root.editingKey !== rowScope.key ? "value-" + rowScope.key : ""
                                                text: root.displayValue(row)
                                                horizontalAlignment: Text.AlignHCenter
                                                verticalAlignment: Text.AlignVCenter
                                                role: "control"
                                            }
                                            TextInput {
                                                id: numberEditor
                                                objectName: rowScope.integerRow ? "editor-" + rowScope.key : ""
                                                anchors.fill: parent
                                                visible: root.editingKey === rowScope.key && root.editingPart === "value"
                                                horizontalAlignment: Text.AlignHCenter
                                                verticalAlignment: Text.AlignVCenter
                                                color: Ui.Theme.textPrimary
                                                font.family: Ui.Theme.fontFamily
                                                font.pixelSize: 26
                                                clip: true
                                                inputMethodHints: Qt.ImhFormattedNumbersOnly
                                                onVisibleChanged: if (visible) {
                                                    text = root.editingDraft;
                                                    cursorPosition = text.length;
                                                    forceActiveFocus();
                                                }
                                                onTextChanged: if (visible && activeFocus) root.editingDraft = text
                                                Keys.onPressed: event => root.editorKeys(event)
                                            }
                                        }
                                    }
                                    T.AbstractButton {
                                        id: plusButton
                                        objectName: rowScope.integerRow ? "plus-" + rowScope.key : ""
                                        visible: rowScope.integerRow
                                        width: 64
                                        height: 64
                                        padding: 0
                                        focusPolicy: Qt.StrongFocus
                                        focus: rowScope.wants("plus")
                                        activeFocusOnTab: false
                                        opacity: !root.canEdit(row) || (root.hasBound(row, "max") && root.integerValue(row) >= Number(row.constraints.max)) ? 0.45 : 1
                                        text: "+"
                                        onClicked: root.stepInteger(row, 1)
                                        Keys.onPressed: event => root.selectKey(plusButton, event, false)
                                        Keys.onReleased: event => root.selectKey(plusButton, event, true)
                                        background: Rectangle { radius: 18; color: plusButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.08) }
                                        contentItem: Ui.UiText { text: plusButton.text; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                                    }
                                }
                            }
                            Column {
                                id: pathColumn
                                visible: rowScope.multiple
                                width: parent.width
                                spacing: 8
                                Rectangle {
                                    width: parent.width
                                    height: pathIntro.implicitHeight + 28
                                    radius: 28
                                    color: Qt.rgba(1, 1, 1, 0.06)
                                    Column {
                                        id: pathIntro
                                        x: 32
                                        y: 14
                                        width: parent.width - 64
                                        spacing: 6
                                        Ui.UiText { width: parent.width; text: row.title || ""; role: "control"; opacity: row.enabled === false || row.pending ? 0.45 : 1 }
                                        Ui.UiText {
                                            width: parent.width
                                            text: row.description || ""
                                            role: "meta"
                                            color: Ui.Theme.textSecondary
                                            visible: text.length > 0
                                        }
                                        Ui.UiText {
                                            objectName: row.providerName && rowScope.multiple ? "provider-" + rowScope.key : ""
                                            width: parent.width
                                            text: row.providerName || ""
                                            role: "meta"
                                            color: Ui.Theme.textHint
                                            visible: text.length > 0
                                        }
                                        Ui.UiText {
                                            objectName: row.error && rowScope.multiple ? "error-" + rowScope.key : ""
                                            width: parent.width
                                            text: root.errorText(row.error || "")
                                            role: "meta"
                                            color: Ui.Theme.error
                                            visible: text.length > 0
                                        }
                                        Ui.UiText {
                                            objectName: row.pending && rowScope.multiple ? "pending-" + rowScope.key : ""
                                            width: parent.width
                                            text: row.pending ? qsTr("Saving…") : ""
                                            role: "meta"
                                            color: Ui.Theme.textSecondary
                                            visible: text.length > 0
                                        }
                                    }
                                }
                                Repeater {
                                    id: pathRepeater
                                    model: rowScope.multiple ? root.pathList(row).length : 0
                                    delegate: Row {
                                        id: pathRow
                                        required property int index
                                        property alias pathButton: pathButton
                                        property alias removeButton: removeButton
                                        width: pathColumn.width
                                        spacing: 8
                                        T.AbstractButton {
                                            id: pathButton
                                            objectName: "path-" + rowScope.key + "-" + index
                                            width: parent.width - 160
                                            height: 72
                                            padding: 0
                                            focusPolicy: Qt.StrongFocus
                                            focus: rowScope.wants("path:" + index)
                                            activeFocusOnTab: false
                                            opacity: row.enabled === false || row.pending ? 0.45 : 1
                                            onClicked: root.openEditor(row, "path:" + index, root.pathList(row)[index] || "")
                                            Keys.onPressed: event => root.selectKey(pathButton, event, false)
                                            Keys.onReleased: event => root.selectKey(pathButton, event, true)
                                            background: Rectangle { radius: 18; color: pathButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.06) }
                                            contentItem: Item {
                                                Ui.UiText {
                                                    anchors.fill: parent
                                                    anchors.leftMargin: 20
                                                    anchors.rightMargin: 20
                                                    visible: !(root.editingKey === rowScope.key && root.editingPart === "path:" + index)
                                                    text: String(root.pathList(row)[index] || "")
                                                    verticalAlignment: Text.AlignVCenter
                                                    elide: Text.ElideRight
                                                    role: "meta"
                                                }
                                                TextInput {
                                                    anchors.fill: parent
                                                    anchors.leftMargin: 20
                                                    anchors.rightMargin: 20
                                                    objectName: "editor-" + rowScope.key + "-" + index
                                                    visible: root.editingKey === rowScope.key && root.editingPart === "path:" + index
                                                    color: Ui.Theme.textPrimary
                                                    font.family: Ui.Theme.fontFamily
                                                    font.pixelSize: 26
                                                    clip: true
                                                    verticalAlignment: Text.AlignVCenter
                                                    onVisibleChanged: if (visible) {
                                                        text = root.editingDraft;
                                                        cursorPosition = text.length;
                                                        forceActiveFocus();
                                                    }
                                                    onTextChanged: if (visible && activeFocus) root.editingDraft = text
                                                    Keys.onPressed: event => root.editorKeys(event)
                                                }
                                            }
                                        }
                                        T.AbstractButton {
                                            id: removeButton
                                            objectName: "remove-" + rowScope.key + "-" + index
                                            width: 152
                                            height: 72
                                            padding: 0
                                            focusPolicy: Qt.StrongFocus
                                            focus: rowScope.wants("remove:" + index)
                                            activeFocusOnTab: false
                                            opacity: root.canEdit(row) ? 1 : 0.45
                                            text: qsTr("Remove")
                                            onClicked: root.removePath(row, index)
                                            Keys.onPressed: event => root.selectKey(removeButton, event, false)
                                            Keys.onReleased: event => root.selectKey(removeButton, event, true)
                                            background: Rectangle { radius: 18; color: removeButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.06) }
                                            contentItem: Ui.UiText {
                                                text: removeButton.text
                                                horizontalAlignment: Text.AlignHCenter
                                                verticalAlignment: Text.AlignVCenter
                                                role: "meta"
                                            }
                                        }
                                    }
                                }
                                T.AbstractButton {
                                    id: addButton
                                    objectName: rowScope.multiple ? "add-" + rowScope.key : ""
                                    visible: rowScope.multiple
                                    width: parent.width
                                    height: 72
                                    padding: 0
                                    focusPolicy: Qt.StrongFocus
                                    focus: rowScope.wants("add")
                                    activeFocusOnTab: false
                                    readonly property bool full: root.hasBound(row, "maxItems") && root.pathList(row).length >= Number(row.constraints.maxItems)
                                    opacity: !root.canEdit(row) || full ? 0.45 : 1
                                    text: qsTr("Add path")
                                    onClicked: if (!full) root.openEditor(row, "add", "")
                                    Keys.onPressed: event => root.selectKey(addButton, event, false)
                                    Keys.onReleased: event => root.selectKey(addButton, event, true)
                                    background: Rectangle { radius: 18; color: addButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : Qt.rgba(1, 1, 1, 0.06) }
                                    contentItem: Item {
                                        Ui.UiText {
                                            anchors.fill: parent
                                            visible: !(root.editingKey === rowScope.key && root.editingPart === "add")
                                            text: addButton.text
                                            horizontalAlignment: Text.AlignHCenter
                                            verticalAlignment: Text.AlignVCenter
                                            role: "meta"
                                        }
                                        TextInput {
                                            anchors.centerIn: parent
                                            width: parent.width - 40
                                            objectName: rowScope.multiple ? "editor-" + rowScope.key + "-add" : ""
                                            visible: root.editingKey === rowScope.key && root.editingPart === "add"
                                            horizontalAlignment: Text.AlignHCenter
                                            color: Ui.Theme.textPrimary
                                            font.family: Ui.Theme.fontFamily
                                            font.pixelSize: 26
                                            clip: true
                                            onVisibleChanged: if (visible) {
                                                text = root.editingDraft;
                                                cursorPosition = text.length;
                                                forceActiveFocus();
                                            }
                                            onTextChanged: if (visible && activeFocus) root.editingDraft = text
                                            Keys.onPressed: event => root.editorKeys(event)
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

    Item {
        id: optionSheet
        anchors.fill: parent
        visible: root.choosingKey.length > 0
        z: 10
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.45) }
        Column {
            x: 740
            y: 220
            width: Math.min(720, parent.width - 836)
            spacing: 8
            Repeater {
                id: optionRepeater
                model: root.choosingOptions
                delegate: T.AbstractButton {
                    id: optionButton
                    required property var modelData
                    required property int index
                    objectName: "option-" + index
                    width: parent.width
                    height: 88
                    padding: 0
                    focusPolicy: Qt.StrongFocus
                    activeFocusOnTab: false
                    text: modelData.title || modelData.value || ""
                    onClicked: {
                        root.optionIndex = index;
                        root.chooseOption();
                    }
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Up) {
                            if (index > 0) root.focusOption(index - 1);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Down) {
                            if (index + 1 < root.choosingOptions.length) root.focusOption(index + 1);
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (!event.isAutoRepeat) {
                                root.optionIndex = index;
                                root.chooseOption();
                            }
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back || event.key === Qt.Key_Left) {
                            root.closeOptions();
                            event.accepted = true;
                        } else if (event.key === Qt.Key_Right) {
                            event.accepted = true;
                        }
                    }
                    background: Rectangle {
                        radius: 26
                        color: optionButton.activeFocus ? Qt.rgba(1, 1, 1, 0.20) : "#292b30"
                    }
                    contentItem: Ui.UiText {
                        text: optionButton.text
                        role: "control"
                        leftPadding: 28
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
        }
    }

    function editorKeys(event) {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (!event.isAutoRepeat) finishEdit(true);
            event.accepted = true;
        } else if (event.key === Qt.Key_Escape) {
            finishEdit(false);
            event.accepted = true;
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            const step = event.key === Qt.Key_Up ? -1 : 1;
            finishEdit(false);
            moveVertical(step);
            event.accepted = true;
        }
    }
}
