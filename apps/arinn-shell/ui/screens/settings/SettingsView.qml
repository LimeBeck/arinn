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
    property string awaitingKey: ""
    property var awaitingValue: null
    property string awaitingLabel: ""
    property bool saveSlow: false
    property string suppressedError: ""
    property string toastTitle: ""
    property string toastDetail: ""
    property bool toastIsError: false
    property string dismissedError: ""
    property string hiddenStringError: ""
    property bool keyboardAvailable: true
    property bool keyboardSeen: false
    readonly property bool keyboardReady: keyboardAvailable || keyboardSeen
    property int integerDraft: 0
    property int integerWas: 0
    property string integerNote: ""
    property double holdMark: 0
    property bool integerFresh: true
    property string stringPart: "field"
    // A string setting typed with the on-screen keyboard (KB-4).
    property bool stringTyping: false
    onStringSheetChanged: if (!stringSheet && stringTyping) {
        stringKeyboard.finish();
        stringTyping = false;
    }
    property string enumClosingKey: ""
    signal valueChangeRequested(string providerId, string settingId, var value)
    signal errorDismissRequested(string providerId, string settingId)
    signal dismissRequested()
    // Home from anywhere in Settings, editors and sheets included (§6.3).
    signal homeRequested()
    signal retryRequested()

    readonly property bool screenBlocked: screenState === "loading" || screenState === "error" || screenState === "unavailable"
    readonly property var activeRows: collectRows()
    onActiveRowsChanged: queueRowFocusRestore()
    readonly property string paneKind: paneFor(currentCategory())
    readonly property var choosingRow: rowByKey(choosingKey)
    readonly property var choosingOptions: choosingRow ? optionsOf(choosingRow) : []
    readonly property bool stringSheet: sheetRowIs("string")
    readonly property bool integerSheet: sheetRowIs("integer")
    readonly property bool sheetOpen: choosingKey.length > 0 || stringSheet || integerSheet
    readonly property var integerRowData: integerSheet ? rowByKey(editingKey) : null
    readonly property var stringRowData: stringSheet ? rowByKey(editingKey) : null

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
    function sheetRowIs(kind) {
        if (!editingKey.length) return false;
        const row = rowByKey(editingKey);
        return !!row && row.type === kind && (kind !== "string" || editingPart === "main");
    }
    function unitOf(row) {
        const unit = row && row.constraints ? row.constraints.unit : "";
        return unit ? String(unit) : "";
    }
    function integerStepOf(row) {
        const step = row && row.constraints ? Number(row.constraints.step) : 1;
        return isFinite(step) && step >= 1 ? step : 1;
    }
    function daemonErrorText(code) {
        if (!code) return "";
        if (code === "Timeout") return qsTr("Arinn did not reply. Try again.");
        return String(code);
    }
    function detailLine(row) {
        if (!row) return "";
        if (row.enabled === false) return qsTr("Cannot be changed now");
        if (row.deferred) return qsTr("Applies after Arinn restarts");
        if (row.type === "boolean" && row.error && dismissedError !== rowKey(row) + "\n" + row.error)
            return daemonErrorText(row.error);
        return row.description || "";
    }
    function showRowError(row) {
        if (!row || !row.error || row.type === "boolean") return false;
        return suppressedError !== rowKey(row) + "\n" + row.error;
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
            const number = String(integerValue(row));
            const unit = unitOf(row);
            return unit.length ? number + " " + unit : number;
        }
        if (row.type === "string" || (row.type === "path" && !isMultiple(row))) {
            if (row.currentValue === undefined || row.currentValue === null || row.currentValue === "")
                return qsTr("Not set");
            return String(row.currentValue);
        }
        return qsTr("Unsupported");
    }
    function partsFor(row) {
        if (!row) return [];
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
    function showToast(title, detail, isError) {
        toastTitle = title;
        toastDetail = detail;
        toastIsError = !!isError;
        toastTimer.restart();
    }
    function beginSave(row, value, label) {
        if (!canEdit(row)) return;
        awaitingKey = rowKey(row);
        awaitingValue = value;
        awaitingLabel = label === undefined || label === null ? String(value) : String(label);
        saveSlow = false;
        suppressedError = row.error ? rowKey(row) + "\n" + row.error : "";
        slowTimer.restart();
        emitChange(row, value);
    }
    function watchRows() {
        if (awaitingKey) {
            const row = rowByKey(awaitingKey);
            if (row && row.pending) suppressedError = "";
        }
        settleAwaiting();
    }
    function settleAwaiting() {
        if (!awaitingKey.length) return;
        const row = rowByKey(awaitingKey);
        if (!row || row.pending) return;
        slowTimer.stop();
        saveSlow = false;
        const key = awaitingKey;
        const label = awaitingLabel;
        const kind = row.type;
        const failed = !!(row.error && String(row.error).length);
        const deferred = !!row.deferred;
        const open = choosingKey === key || editingKey === key;
        awaitingKey = "";
        awaitingValue = null;
        awaitingLabel = "";
        if (failed) {
            if (!open)
                showToast(qsTr("Could not save"), daemonErrorText(row.error), true);
            return;
        }
        if (open) closeSheet(key);
        if (kind === "boolean") return;
        if (deferred)
            showToast(qsTr("Saved"), qsTr("Applies after Arinn restarts"), false);
        else
            showToast(qsTr("Saved"), (row.title || "") + ": " + label, false);
    }
    function closeSheet(key) {
        if (choosingKey === key) {
            enumClosingKey = key;
            enumCloseTimer.restart();
            return;
        }
        if (editingKey === key) {
            editingKey = "";
            editingPart = "";
            stringPart = "field";
            integerNote = "";
            refocusKey(key);
        }
    }
    function finishEnumClose() {
        const key = enumClosingKey || choosingKey;
        enumCloseTimer.stop();
        choosingKey = "";
        enumClosingKey = "";
        refocusKey(key);
    }
    function rememberBooleanError(row) {
        if (!row || row.type !== "boolean" || !row.error) return;
        dismissedError = rowKey(row) + "\n" + row.error;
        errorDismissRequested(row.providerId || "", row.id);
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
            if ([...draft].length > max) return;
            if (/[\u0000-\u001F\u007F]/.test(draft)) return;
            beginSave(row, draft, draft);
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
        editingPart = "";
        enumCloseTimer.stop();
        enumClosingKey = "";
        choosingKey = rowKey(row);
        const options = optionsOf(row);
        let index = 0;
        for (let i = 0; i < options.length; ++i)
            if (options[i] && options[i].value === row.currentValue) index = i;
        optionIndex = index;
        Qt.callLater(() => focusOption(optionIndex));
    }
    function openInteger(row) {
        if (!canEdit(row) || row.type !== "integer" || valueUnsupported(row)) return;
        choosingKey = "";
        const current = integerValue(row);
        integerWas = current;
        integerDraft = current;
        integerNote = "";
        integerFresh = true;
        holdMark = 0;
        editingPart = "integer";
        editingDraft = String(current);
        editingKey = rowKey(row);
        Qt.callLater(() => { if (integerStepper) Ui.Focus.take(integerStepper); });
    }
    function openString(row) {
        if (!canEdit(row) || row.type !== "string") return;
        choosingKey = "";
        // Without a hardware keyboard the remote types through the panel.
        stringPart = keyboardReady ? "field" : "keyboard";
        hiddenStringError = "";
        editingPart = "main";
        editingDraft = row.currentValue === undefined || row.currentValue === null ? "" : String(row.currentValue);
        editingKey = rowKey(row);
        Qt.callLater(() => focusString());
    }
    function focusString() {
        if (!stringSheet) return;
        if (stringTyping) {
            stringKeyboard.resume();
            return;
        }
        if (stringPart === "keyboard" || (stringPart === "field" && !keyboardReady)) {
            if (stringKeyboardButton) Ui.Focus.take(stringKeyboardButton);
            return;
        }
        if (stringPart === "save" && stringSave) Ui.Focus.take(stringSave);
        else if (stringPart === "cancel" && stringCancel) Ui.Focus.take(stringCancel);
        else if (stringField) Ui.Focus.take(stringField);
    }
    function nudgeInteger(direction, autoRepeat) {
        const row = integerRowData;
        if (!row) return;
        const step = integerStepOf(row);
        if (!autoRepeat) holdMark = Date.now();
        const factor = autoRepeat && holdMark > 0 && (Date.now() - holdMark) >= 1000 ? 10 : 1;
        const min = hasBound(row, "min") ? Number(row.constraints.min) : -Infinity;
        const max = hasBound(row, "max") ? Number(row.constraints.max) : Infinity;
        if ((direction < 0 && integerDraft <= min) || (direction > 0 && integerDraft >= max)) {
            integerNote = direction > 0 ? qsTr("This is the maximum") : qsTr("This is the minimum");
            return;
        }
        let next = integerDraft + direction * step * factor;
        if (next < min) next = min;
        if (next > max) next = max;
        integerDraft = next;
        integerFresh = true;
        editingDraft = String(next);
        integerNote = "";
        if (next <= min) integerNote = qsTr("This is the minimum");
        if (next >= max) integerNote = qsTr("This is the maximum");
    }
    function typeInteger(text) {
        const row = integerRowData;
        if (!row || text !== "-" && !/^\d$/.test(text)) return;
        let digits = integerFresh || editingDraft === String(integerWas) && integerFresh ? "" : String(integerDraft);
        if (integerFresh) digits = "";
        integerFresh = false;
        if (text === "-") {
            if (digits.indexOf("-") === 0) return;
            digits = "-" + digits;
        } else digits += text;
        if (digits === "-" || digits === "") return;
        const number = Number(digits);
        if (!withinInteger(row, number)) {
            integerNote = text === "-" ? "" : (number > integerDraft ? qsTr("This is the maximum") : qsTr("This is the minimum"));
            return;
        }
        integerDraft = number;
        editingDraft = String(number);
        integerNote = "";
        const min = hasBound(row, "min") ? Number(row.constraints.min) : -Infinity;
        const max = hasBound(row, "max") ? Number(row.constraints.max) : Infinity;
        if (number <= min) integerNote = qsTr("This is the minimum");
        if (number >= max) integerNote = qsTr("This is the maximum");
    }
    function commitInteger() {
        const row = integerRowData;
        if (!row || !withinInteger(row, integerDraft)) return;
        const unit = unitOf(row);
        const label = unit.length ? String(integerDraft) + " " + unit : String(integerDraft);
        beginSave(row, integerDraft, label);
    }
    function commitString() {
        finishEdit(true);
    }
    function startStringTyping() {
        const row = stringRowData;
        if (!row || !canEdit(row)) return;
        stringTyping = true;
        stringTypingInput.maximumLength = hasBound(row, "maxLength") ? Number(row.constraints.maxLength) : 1024;
        stringTypingInput.text = editingDraft || "";
        stringTypingInput.cursorPosition = stringTypingInput.text.length;
        stringKeyboard.start();
    }
    // Save commits the setting (SV); Back returns to the sheet's button.
    function finishStringTyping(save) {
        if (!stringTyping) return;
        stringKeyboard.finish();
        stringTyping = false;
        if (save) {
            commitString();
            return;
        }
        stringPart = "keyboard";
        Qt.callLater(() => focusString());
    }
    // Home closes transient UI without committing a draft, then asks the
    // shell for Home (#329). Editors and sheets consume keys, so each calls
    // this before its own handling.
    function homeKey(event) {
        if (event.key !== Qt.Key_Home || event.modifiers !== Qt.NoModifier) return false;
        if (!event.isAutoRepeat) {
            if (editingKey.length) cancelEditor();
            if (choosingKey.length) closeOptions();
            homeRequested();
        }
        event.accepted = true;
        return true;
    }
    function cancelEditor() {
        const key = editingKey;
        if (!key) return;
        editingKey = "";
        editingPart = "";
        stringPart = "field";
        integerNote = "";
        refocusKey(key);
    }
    function noteStringEdit() {
        const row = stringRowData;
        if (!row || !row.error) return;
        hiddenStringError = rowKey(row) + "\n" + row.error;
        errorDismissRequested(row.providerId || "", row.id);
    }
    function wakeKeyboard(text) {
        keyboardSeen = true;
        stringPart = "field";
        if (text && text.length === 1 && text >= " ") {
            editingDraft = (editingDraft || "") + text;
            noteStringEdit();
        }
        Qt.callLater(() => {
            if (stringField) {
                stringField.text = editingDraft;
                stringField.cursorPosition = stringField.text.length;
            }
            focusString();
        });
    }
    function pageOptions(delta) {
        if (!choosingOptions.length) return;
        const next = Math.max(0, Math.min(choosingOptions.length - 1, optionIndex + delta * 6));
        focusOption(next);
    }
    function optionPositionText() {
        if (choosingOptions.length <= 6) return "";
        return qsTr("%1 of %2").arg(optionIndex + 1).arg(choosingOptions.length);
    }
    function focusOption(index) {
        const item = optionRepeater.itemAt(index);
        if (!item) return;
        optionIndex = index;
        Ui.Focus.take(item);
        if (choosingOptions.length > 6 && optionFlick) {
            const y = Math.max(0, item.y + item.height / 2 - optionFlick.height / 2);
            optionFlick.contentY = Math.min(y, Math.max(0, optionFlick.contentHeight - optionFlick.height));
        }
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
        if (!row || !option || !canEdit(row)) return;
        if (option.value === row.currentValue) {
            closeSheet(choosingKey);
            return;
        }
        beginSave(row, option.value, option.title || String(option.value));
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
        const leaving = rowByKey(focusedRowKey);
        rememberBooleanError(leaving);
        const next = Math.max(0, Math.min(index, categories.length - 1));
        editingKey = "";
        choosingKey = "";
        enumCloseTimer.stop();
        enumClosingKey = "";
        detailFocused = false;
        focusedRowKey = "";
        categoryIndex = next;
        const item = categoryRepeater.itemAt(next);
        if (item) Ui.Focus.take(item);
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
        Ui.Focus.take(item);
        revealRow(scope);
        return true;
    }
    // The row repeater builds new buttons whenever rows are replaced. Without
    // this, focus falls back to the view and Select does nothing (AC-02).
    function queueRowFocusRestore() {
        if (stringSheet || integerSheet || choosingKey.length) {
            Qt.callLater(() => {
                if (stringSheet) focusString();
                else if (integerSheet && integerStepper) Ui.Focus.take(integerStepper);
                else if (choosingKey.length) focusOption(optionIndex);
            });
            return;
        }
        if (!detailFocused || editingKey.length || enumClosingKey.length || !focusedRowKey.length)
            return;
        Qt.callLater(() => restoreRowFocus(0));
    }
    function restoreRowFocus(attempt) {
        if (!detailFocused || editingKey.length || choosingKey.length || enumClosingKey.length || !focusedRowKey.length)
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
            rememberBooleanError(activeRows[rowFocusIndex]);
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
        if (step < 0) {
            rememberBooleanError(activeRows[rowFocusIndex]);
            focusCategory(categoryIndex);
        }
    }
    function focusInitial() {
        if (!visible) return;
        if (screenBlocked) {
            Ui.Focus.take(screenStatus);
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

    onRowsChanged: watchRows()
    Keys.onPressed: event => {
        if (homeKey(event)) return;
        if (editingKey.length > 0) return;
        if (choosingKey.length > 0) {
            if (event.key === Qt.Key_Escape || event.key === Qt.Key_Backspace || event.key === Qt.Key_Back)
                closeOptions();
            event.accepted = true;
            return;
        }
        if (screenBlocked) {
            // Menu stays global (#329): the shell handles it.
            if (event.key === Qt.Key_Menu) return;
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
        opacity: root.sheetOpen ? 0.55 : 1
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
        opacity: root.sheetOpen ? 0.55 : 1
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
                        readonly property bool simpleRow: !multiple
                        readonly property bool switchRow: simpleRow && row.type === "boolean" && !root.valueUnsupported(row)
                        readonly property bool locked: row.enabled === false
                        width: rowColumn.width
                        height: rowBody.implicitHeight
                            function itemFor(name) {
                            if (name === "main") return settingButton;
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
                                opacity: row.pending && !rowScope.locked ? 0.72 : 1
                                text: row.title || ""
                                objectName: rowScope.simpleRow ? "setting-" + rowScope.key : ""
                                onClicked: {
                                    if (!root.canEdit(row)) return;
                                    if (row.type === "boolean" && !root.valueUnsupported(row))
                                        root.beginSave(row, !row.currentValue, row.currentValue ? qsTr("Off") : qsTr("On"));
                                    else if (row.type === "enum")
                                        root.openOptions(row);
                                    else if (row.type === "integer")
                                        root.openInteger(row);
                                    else if (row.type === "string")
                                        root.openString(row);
                                    else if (row.type === "path")
                                        root.openEditor(row, "main", row.currentValue || "");
                                }
                                Keys.onPressed: event => root.selectKey(settingButton, event, false)
                                Keys.onReleased: event => root.selectKey(settingButton, event, true)
                                background: Rectangle {
                                    radius: 28
                                    color: settingButton.activeFocus
                                           ? (rowScope.locked ? Qt.rgba(1, 1, 1, 0.12) : Qt.rgba(1, 1, 1, 0.20))
                                           : Qt.rgba(1, 1, 1, 0.06)
                                }
                                contentItem: Item {
                                    Column {
                                        id: simpleText
                                        x: 32
                                        y: 20
                                        width: parent.width - 64 - (rowScope.switchRow ? 88 : 280)
                                        spacing: 6
                                        Ui.UiText {
                                            width: parent.width
                                            text: row.title || ""
                                            role: "control"
                                            color: rowScope.locked ? Ui.Theme.textHint : Ui.Theme.textPrimary
                                        }
                                        Ui.UiText {
                                            objectName: rowScope.simpleRow ? "detail-" + rowScope.key : ""
                                            width: parent.width
                                            text: root.detailLine(row)
                                            role: "meta"
                                            color: row.deferred ? Ui.Theme.textPrimary
                                                   : (row.type === "boolean" && row.error && root.dismissedError !== root.rowKey(row) + "\n" + row.error
                                                      ? Ui.Theme.error : (rowScope.locked ? Ui.Theme.textHint : Ui.Theme.textSecondary))
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
                                            objectName: rowScope.simpleRow && root.showRowError(row) ? "error-" + rowScope.key : ""
                                            width: parent.width
                                            text: root.showRowError(row) ? root.daemonErrorText(row.error || "") : ""
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
                                            visible: root.editingKey !== rowScope.key || row.type !== "path"
                                            width: Math.min(280, implicitWidth)
                                            horizontalAlignment: Text.AlignRight
                                            role: "meta"
                                            color: rowScope.locked ? Ui.Theme.textHint
                                                   : (root.valueUnsupported(row) ? Ui.Theme.error : Ui.Theme.textSecondary)
                                            text: root.displayValue(row)
                                        }
                                        TextInput {
                                            id: simpleEditor
                                            objectName: rowScope.simpleRow && row.type === "path" ? "editor-" + rowScope.key : ""
                                            visible: row.type === "path" && root.editingKey === rowScope.key && root.editingPart === "main"
                                            width: 220
                                            horizontalAlignment: Text.AlignRight
                                            color: Ui.Theme.textPrimary
                                            font.family: Ui.Theme.fontFamily
                                            font.pixelSize: 26
                                            clip: true
                                            onVisibleChanged: if (visible) {
                                                text = root.editingDraft;
                                                cursorPosition = text.length;
                                                Ui.Focus.take(simpleEditor);
                                            }
                                            onTextChanged: if (visible && activeFocus) root.editingDraft = text
                                            Keys.onPressed: event => root.editorKeys(event)
                                        }
                                        Canvas {
                                            objectName: rowScope.locked ? "lock-" + rowScope.key : ""
                                            visible: rowScope.locked && root.editingKey !== rowScope.key
                                            width: 28
                                            height: 36
                                            onPaint: {
                                                const ctx = getContext("2d");
                                                ctx.reset();
                                                ctx.strokeStyle = Ui.Theme.textHint;
                                                ctx.fillStyle = Ui.Theme.textHint;
                                                ctx.lineWidth = 2.2;
                                                ctx.lineJoin = "round";
                                                ctx.beginPath();
                                                ctx.arc(14, 14, 6, Math.PI, 0);
                                                ctx.stroke();
                                                ctx.beginPath();
                                                ctx.rect(7, 14, 14, 12);
                                                ctx.fill();
                                            }
                                            Component.onCompleted: requestPaint()
                                        }
                                        Canvas {
                                            objectName: rowScope.simpleRow && !rowScope.switchRow && !rowScope.locked && root.editingKey !== rowScope.key ? "chevron-" + rowScope.key : ""
                                            visible: rowScope.simpleRow && !rowScope.locked && root.editingKey !== rowScope.key
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
                                            text: root.daemonErrorText(row.error || "")
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
                                                    id: pathEditor
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
                                                        Ui.Focus.take(pathEditor);
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
                                            id: addEditor
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
                                                Ui.Focus.take(addEditor);
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
        visible: root.sheetOpen
        z: 10
        Rectangle { anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.45) }
        Rectangle {
            x: 1920 - 720 - 48
            y: 72
            width: 720
            height: 1080 - 144
            radius: 44
            color: Qt.rgba(0.09, 0.09, 0.1, 0.96)
            Column {
                x: 56
                y: 64
                width: parent.width - 112
                spacing: 16
                Row {
                    width: parent.width
                    spacing: 16
                    Ui.UiText {
                        width: parent.width - optionPosition.implicitWidth - 16
                        role: "eyebrow"
                        font.capitalization: Font.AllUppercase
                        color: Ui.Theme.textSecondary
                        text: {
                            const row = root.choosingRow || root.integerRowData || root.stringRowData;
                            if (!row) return "";
                            for (let i = 0; i < root.categories.length; ++i)
                                if (root.categories[i] && root.categories[i].id === row.categoryId)
                                    return root.categories[i].title || "";
                            return "";
                        }
                    }
                    Ui.UiText {
                        id: optionPosition
                        objectName: "optionPosition"
                        visible: root.choosingKey.length > 0 && text.length > 0
                        role: "eyebrow"
                        color: Ui.Theme.textSecondary
                        text: root.optionPositionText()
                    }
                }
                Ui.UiText {
                    width: parent.width
                    role: "title"
                    text: {
                        const row = root.choosingRow || root.integerRowData || root.stringRowData;
                        return row && row.title ? row.title : "";
                    }
                }
                Ui.UiText {
                    width: parent.width
                    role: "meta"
                    color: Ui.Theme.textSecondary
                    visible: {
                        const row = root.choosingRow || root.integerRowData || root.stringRowData;
                        return !!(row && row.description);
                    }
                    text: {
                        const row = root.choosingRow || root.integerRowData || root.stringRowData;
                        return row && row.description ? row.description : "";
                    }
                }
                Rectangle {
                    objectName: "enumError"
                    visible: root.choosingRow && root.showRowError(root.choosingRow) && !root.choosingRow.pending
                    width: parent.width
                    height: enumErrorText.implicitHeight + 28
                    radius: 22
                    color: Qt.rgba(0.91, 0.28, 0.31, 0.18)
                    Ui.UiText {
                        id: enumErrorText
                        x: 20
                        y: 14
                        width: parent.width - 40
                        role: "meta"
                        color: Ui.Theme.error
                        text: root.choosingRow ? root.daemonErrorText(root.choosingRow.error || "") : ""
                    }
                }
                Flickable {
                    id: optionFlick
                    visible: root.choosingKey.length > 0
                    width: parent.width
                    height: root.choosingOptions.length > 6 ? 672 : optionColumn.height
                    contentWidth: width
                    contentHeight: optionColumn.height
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    Column {
                        id: optionColumn
                        width: optionFlick.width
                        spacing: 8
                        Repeater {
                            id: optionRepeater
                            model: root.choosingOptions
                            delegate: T.AbstractButton {
                                id: optionButton
                                required property var modelData
                                required property int index
                                readonly property bool current: root.choosingRow && modelData.value === root.choosingRow.currentValue
                                readonly property bool busy: root.saveSlow && root.awaitingKey === root.choosingKey
                                                          && modelData.value === root.awaitingValue
                                objectName: "option-" + index
                                width: parent.width
                                height: Math.max(104, optionLabel.implicitHeight + 36)
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
                                    } else if (event.key === Qt.Key_PageUp || event.key === Qt.Key_ChannelUp) {
                                        root.pageOptions(-1);
                                        event.accepted = true;
                                    } else if (event.key === Qt.Key_PageDown || event.key === Qt.Key_ChannelDown) {
                                        root.pageOptions(1);
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
                                    radius: 28
                                    color: optionButton.activeFocus ? Ui.Theme.textPrimary : Qt.rgba(1, 1, 1, 0.06)
                                }
                                contentItem: Item {
                                    Ui.UiText {
                                        id: optionLabel
                                        x: 28
                                        width: parent.width - 100
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: optionButton.text
                                        role: "control"
                                        wrapMode: Text.WordWrap
                                        color: optionButton.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                                    }
                                    Canvas {
                                        objectName: optionButton.current && !optionButton.busy ? "optionCheck-" + index : ""
                                        visible: optionButton.current && !optionButton.busy
                                        anchors.right: parent.right
                                        anchors.rightMargin: 28
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 36
                                        height: 36
                                        onPaint: {
                                            const ctx = getContext("2d");
                                            ctx.reset();
                                            ctx.strokeStyle = optionButton.activeFocus ? Ui.Theme.ink : Ui.Theme.ember;
                                            ctx.lineWidth = 3;
                                            ctx.lineCap = "round";
                                            ctx.beginPath();
                                            ctx.moveTo(8, 18);
                                            ctx.lineTo(15, 26);
                                            ctx.lineTo(28, 10);
                                            ctx.stroke();
                                        }
                                        Component.onCompleted: requestPaint()
                                    }
                                    Canvas {
                                        objectName: optionButton.busy ? "optionSpinner" : ""
                                        visible: optionButton.busy
                                        anchors.right: parent.right
                                        anchors.rightMargin: 28
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 36
                                        height: 36
                                        onPaint: {
                                            const ctx = getContext("2d");
                                            ctx.reset();
                                            ctx.strokeStyle = optionButton.activeFocus ? Ui.Theme.ink : Ui.Theme.ember;
                                            ctx.lineWidth = 3;
                                            ctx.beginPath();
                                            ctx.arc(18, 18, 12, 0, Math.PI * 1.4);
                                            ctx.stroke();
                                        }
                                        RotationAnimation on rotation {
                                            running: optionButton.busy
                                            from: 0
                                            to: 360
                                            duration: 800
                                            loops: Animation.Infinite
                                        }
                                        Component.onCompleted: requestPaint()
                                    }
                                }
                            }
                        }
                    }
                }
                Rectangle {
                    objectName: "integerError"
                    visible: root.integerSheet && root.integerRowData && root.showRowError(root.integerRowData) && !root.integerRowData.pending
                    width: parent.width
                    height: integerErrorText.implicitHeight + 28
                    radius: 22
                    color: Qt.rgba(0.91, 0.28, 0.31, 0.18)
                    Ui.UiText {
                        id: integerErrorText
                        x: 20
                        y: 14
                        width: parent.width - 40
                        role: "meta"
                        color: Ui.Theme.error
                        text: root.integerRowData ? root.daemonErrorText(root.integerRowData.error || "") : ""
                    }
                }
                FocusScope {
                    id: integerStepper
                    objectName: "integerStepper"
                    visible: root.integerSheet
                    width: parent.width
                    height: 280
                    focus: root.integerSheet
                    Column {
                        width: parent.width
                        spacing: 18
                        Rectangle {
                            width: parent.width
                            height: 200
                            radius: 36
                            color: Qt.rgba(1, 1, 1, 0.08)
                            Row {
                                anchors.centerIn: parent
                                spacing: 28
                                Rectangle {
                                    objectName: "integerMinus"
                                    width: 96
                                    height: 96
                                    radius: 48
                                    color: Qt.rgba(1, 1, 1, 0.14)
                                    opacity: {
                                        const row = root.integerRowData;
                                        return row && root.hasBound(row, "min") && root.integerDraft <= Number(row.constraints.min) ? 0.3 : 1;
                                    }
                                    Ui.UiText { anchors.centerIn: parent; text: "−"; role: "title" }
                                }
                                Row {
                                    spacing: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    Ui.UiText {
                                        id: integerReadout
                                        objectName: "integerValue"
                                        text: String(root.integerDraft)
                                        role: "display"
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                    Ui.UiText {
                                        objectName: "integerUnit"
                                        visible: root.integerRowData && root.unitOf(root.integerRowData).length > 0
                                        text: root.integerRowData ? root.unitOf(root.integerRowData) : ""
                                        role: "title"
                                        color: Ui.Theme.textSecondary
                                        anchors.verticalCenter: parent.verticalCenter
                                    }
                                }
                                Rectangle {
                                    objectName: "integerPlus"
                                    width: 96
                                    height: 96
                                    radius: 48
                                    color: Qt.rgba(1, 1, 1, 0.14)
                                    opacity: {
                                        const row = root.integerRowData;
                                        return row && root.hasBound(row, "max") && root.integerDraft >= Number(row.constraints.max) ? 0.3 : 1;
                                    }
                                    Ui.UiText { anchors.centerIn: parent; text: "+"; role: "title" }
                                }
                            }
                        }
                        Ui.UiText {
                            objectName: "integerBounds"
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            role: "meta"
                            color: Ui.Theme.textSecondary
                            text: {
                                const row = root.integerRowData;
                                if (!row) return "";
                                const min = root.hasBound(row, "min") ? String(row.constraints.min) : "—";
                                const max = root.hasBound(row, "max") ? String(row.constraints.max) : "—";
                                return qsTr("min. %1 · was %2 · max. %3").arg(min).arg(root.integerWas).arg(max);
                            }
                        }
                        Ui.UiText {
                            objectName: "integerNote"
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            role: "meta"
                            color: Ui.Theme.textSecondary
                            visible: root.integerNote.length > 0
                            text: root.integerNote
                        }
                    }
                    Keys.onPressed: event => {
                        if (event.key === Qt.Key_Left || event.key === Qt.Key_Down)
                            root.nudgeInteger(-1, event.isAutoRepeat);
                        else if (event.key === Qt.Key_Right || event.key === Qt.Key_Up)
                            root.nudgeInteger(1, event.isAutoRepeat);
                        else if (event.key === Qt.Key_Backspace) {
                            const text = String(root.integerDraft);
                            if (text.length > 1 && text !== String(root.integerWas)) {
                                const next = Number(text.slice(0, -1));
                                if (root.withinInteger(root.integerRowData, next)) {
                                    root.integerDraft = next;
                                    root.integerFresh = false;
                                }
                            }
                        } else if (event.key === Qt.Key_Minus || event.text === "-")
                            root.typeInteger("-");
                        else if (event.text && event.text.length === 1 && event.text >= "0" && event.text <= "9")
                            root.typeInteger(event.text);
                        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (!event.isAutoRepeat) root.commitInteger();
                        } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)
                            root.cancelEditor();
                        else {
                            event.accepted = false;
                            return;
                        }
                        event.accepted = true;
                    }
                }
                Column {
                    visible: root.stringSheet
                    width: parent.width
                    spacing: 16
                    Rectangle {
                        width: parent.width
                        height: 112
                        radius: 30
                        color: Qt.rgba(1, 1, 1, 0.1)
                        // Keys the field and its keyboard button leave unhandled.
                        Keys.onPressed: event => {
                            if (root.homeKey(event)) return;
                            const onButton = stringKeyboardButton.activeFocus;
                            if (event.key === Qt.Key_Right && stringField.activeFocus) {
                                root.stringPart = "keyboard";
                                Ui.Focus.take(stringKeyboardButton);
                            } else if (event.key === Qt.Key_Left && onButton && root.keyboardReady) {
                                root.stringPart = "field";
                                Ui.Focus.take(stringField);
                            } else if (event.key === Qt.Key_Down && onButton) {
                                root.stringPart = "save";
                                Ui.Focus.take(stringSave);
                            } else if ((event.key === Qt.Key_Escape || event.key === Qt.Key_Back) && onButton) {
                                if (!event.isAutoRepeat) root.cancelEditor();
                            } else if (onButton && event.text && event.text.length === 1 && event.text >= " ") {
                                root.wakeKeyboard(event.text);
                            } else if (!onButton || event.key !== Qt.Key_Up) {
                                return;
                            }
                            event.accepted = true;
                        }
                        TextInput {
                            id: stringField
                            objectName: root.stringSheet ? "editor-" + root.editingKey : ""
                            anchors.fill: parent
                            anchors.leftMargin: 28
                            anchors.rightMargin: stringKeyboardButton.width + 40
                            opacity: root.keyboardReady ? 1 : 0.6
                            verticalAlignment: Text.AlignVCenter
                            color: Ui.Theme.textPrimary
                            font.family: Ui.Theme.fontFamily
                            font.pixelSize: 36
                            clip: true
                            enabled: root.keyboardReady
                            onVisibleChanged: if (visible) {
                                text = root.editingDraft;
                                cursorPosition = text.length;
                                if (root.keyboardReady) Ui.Focus.take(stringField);
                            }
                            onTextChanged: if (activeFocus) {
                                root.editingDraft = text;
                                root.noteStringEdit();
                            }
                            Keys.onPressed: event => {
                                if (root.homeKey(event)) return;
                                if (!root.keyboardReady) {
                                    event.accepted = true;
                                    return;
                                }
                                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                    if (!event.isAutoRepeat) root.commitString();
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_Escape) {
                                    root.cancelEditor();
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_Down) {
                                    root.stringPart = "save";
                                    Ui.Focus.take(stringSave);
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_Up) {
                                    event.accepted = true;
                                } else root.noteStringEdit();
                            }
                        }
                        Ui.KeyboardButton {
                            id: stringKeyboardButton
                            objectName: "stringKeyboard"
                            x: parent.width - width - 14
                            anchors.verticalCenter: parent.verticalCenter
                            onActivated: root.startStringTyping()
                        }
                    }
                    Ui.UiText {
                        objectName: "stringCounter"
                        visible: root.stringRowData !== null && root.hasBound(root.stringRowData, "maxLength")
                        width: parent.width
                        role: "meta"
                        color: Ui.Theme.textSecondary
                        text: {
                            const row = root.stringRowData;
                            if (!row || !root.hasBound(row, "maxLength")) return "";
                            return qsTr("%1 of %2 characters").arg([...(root.editingDraft || "")].length).arg(row.constraints.maxLength);
                        }
                    }
                    Ui.UiText {
                        objectName: "stringError"
                        visible: root.stringRowData !== null && !!root.stringRowData.error
                                 && root.hiddenStringError !== root.rowKey(root.stringRowData) + "\n" + root.stringRowData.error
                                 && !root.stringRowData.pending
                        width: parent.width
                        role: "meta"
                        color: Ui.Theme.error
                        text: visible ? root.daemonErrorText(root.stringRowData.error) : ""
                    }
                    Row {
                        spacing: 16
                        T.AbstractButton {
                            id: stringSave
                            objectName: "stringSave"
                            height: 88
                            width: Math.max(180, saveLabel.implicitWidth + 48)
                            padding: 0
                            focusPolicy: Qt.StrongFocus
                            activeFocusOnTab: false
                            onClicked: root.commitString()
                            Keys.onPressed: event => {
                                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                    if (!event.isAutoRepeat) root.commitString();
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_Right || event.key === Qt.Key_Down) {
                                    root.stringPart = "cancel";
                                    Ui.Focus.take(stringCancel);
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Left) {
                                    root.stringPart = root.keyboardReady ? "field" : "keyboard";
                                    root.focusString();
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                                    root.cancelEditor();
                                    event.accepted = true;
                                }
                            }
                            background: Rectangle {
                                radius: height / 2
                                color: stringSave.activeFocus ? Ui.Theme.textPrimary : Qt.rgba(1, 1, 1, 0.14)
                            }
                            contentItem: Ui.UiText {
                                id: saveLabel
                                text: qsTr("Save")
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                color: stringSave.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                            }
                        }
                        T.AbstractButton {
                            id: stringCancel
                            objectName: "stringCancel"
                            height: 88
                            width: Math.max(180, cancelLabel.implicitWidth + 48)
                            padding: 0
                            focusPolicy: Qt.StrongFocus
                            activeFocusOnTab: false
                            onClicked: root.cancelEditor()
                            Keys.onPressed: event => {
                                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                                    if (!event.isAutoRepeat) root.cancelEditor();
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_Left || event.key === Qt.Key_Up) {
                                    root.stringPart = "save";
                                    Ui.Focus.take(stringSave);
                                    event.accepted = true;
                                }
                            }
                            background: Rectangle {
                                radius: height / 2
                                color: stringCancel.activeFocus ? Ui.Theme.textPrimary : Qt.rgba(1, 1, 1, 0.08)
                            }
                            contentItem: Ui.UiText {
                                id: cancelLabel
                                text: qsTr("Cancel")
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                color: stringCancel.activeFocus ? Ui.Theme.ink : Ui.Theme.textPrimary
                            }
                        }
                    }
                }
            }
        }
    }

    // A string setting typed with the on-screen keyboard (KB-4): the field
    // on top and the panel below. The draft follows each key; Save commits
    // it as the sheet does, Back returns to the sheet.
    FocusScope {
        id: stringTypingLayer
        objectName: "stringTyping"
        anchors.fill: parent
        z: 30
        visible: root.stringTyping && root.stringSheet
        enabled: visible
        readonly property var row: root.stringRowData
        readonly property int limit: row && root.hasBound(row, "maxLength") ? Number(row.constraints.maxLength) : 0
        readonly property int count: [...stringTypingInput.text].length
        Keys.onPressed: event => {
            if (root.homeKey(event)) return;
            // Menu reaches the shell's navigation, as on every screen (§6.2).
            if (event.key === Qt.Key_Menu) return;
            event.accepted = true;
            if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back || event.key === Qt.Key_Backspace) {
                if (!event.isAutoRepeat) root.finishStringTyping(false);
            } else if (event.key === Qt.Key_Down) {
                stringKeyboard.enterFromAbove();
            } else if (event.text && event.text.length === 1 && event.text >= " "
                       && !(event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))) {
                stringKeyboard.insertText(event.text);
            }
        }
        Rectangle { anchors.fill: parent; color: Ui.Theme.background; MouseArea { anchors.fill: parent } }
        Row {
            x: 120
            y: 64
            spacing: 24
            Ui.UiText {
                text: qsTr("Settings")
                font.pixelSize: 56
                font.weight: 700
                wrapMode: Text.NoWrap
            }
            Ui.UiText {
                y: 26
                text: {
                    const category = root.currentCategory();
                    const parts = [category && category.title ? category.title : "",
                                   stringTypingLayer.row ? stringTypingLayer.row.title || "" : ""];
                    return parts.filter(part => part.length).join(" · ");
                }
                color: Ui.Theme.textHint
                wrapMode: Text.NoWrap
            }
        }
        Rectangle {
            x: 120
            y: 160
            width: 1680
            height: 132
            radius: 34
            color: Qt.rgba(1, 1, 1, 0.12)
            border.width: 3
            border.color: Ui.Theme.ring
            TextInput {
                id: stringTypingInput
                objectName: "stringTypingInput"
                x: 32
                width: stringTypingButton.x - x - 24
                height: 80
                anchors.verticalCenter: parent.verticalCenter
                font.family: Ui.Theme.fontFamily
                font.pixelSize: 48
                font.weight: 500
                color: Ui.Theme.textPrimary
                verticalAlignment: TextInput.AlignVCenter
                clip: true
                cursorVisible: stringTypingLayer.visible
                cursorDelegate: Rectangle { width: 4; color: Ui.Theme.ember; visible: stringTypingInput.cursorVisible }
                onTextChanged: if (stringTypingLayer.visible) {
                    root.editingDraft = text;
                    stringField.text = text;
                    root.noteStringEdit();
                }
            }
            Ui.UiText {
                // From 75% of a limit up to 64 (KB-4); error-coloured at the limit.
                objectName: "stringTypingCounter"
                anchors.right: stringTypingButton.left
                anchors.rightMargin: 24
                anchors.verticalCenter: parent.verticalCenter
                visible: stringTypingLayer.limit > 0 && stringTypingLayer.limit <= 64
                         && stringTypingLayer.count >= Math.ceil(stringTypingLayer.limit * 0.75)
                text: stringTypingLayer.count + " / " + stringTypingLayer.limit
                role: "meta"
                color: stringTypingLayer.count >= stringTypingLayer.limit ? Ui.Theme.error : Ui.Theme.textSecondary
            }
            Ui.KeyboardButton {
                id: stringTypingButton
                objectName: "stringTypingKeyboard"
                x: parent.width - width - 24
                anchors.verticalCenter: parent.verticalCenter
                panelOpen: true
                onActivated: root.finishStringTyping(false)
            }
        }
        Ui.UiText {
            objectName: "stringTypingError"
            x: 152
            y: 320
            width: 1600
            role: "body"
            color: Ui.Theme.error
            // The provider's answer to the last save (SV-4).
            visible: text.length > 0
            text: {
                const row = stringTypingLayer.row;
                if (!row || !row.error || row.pending
                    || root.hiddenStringError === root.rowKey(row) + "\n" + row.error)
                    return "";
                return root.daemonErrorText(row.error);
            }
        }
        Ui.OnScreenKeyboard {
            id: stringKeyboard
            objectName: "stringOnScreenKeyboard"
            y: parent.height - implicitHeight
            target: stringTypingInput
            memoryKey: "setting-" + root.editingKey
            submitLabel: qsTr("Save")
            emptySuggestions: stringTypingLayer.limit > 0 ? qsTr("Up to %1 characters").arg(stringTypingLayer.limit) : ""
            onSubmitted: root.finishStringTyping(true)
            onHideRequested: root.finishStringTyping(false)
            onLeaveUp: Ui.Focus.take(stringTypingButton)
        }
    }

    Rectangle {
        objectName: "settingsToast"
        visible: root.toastTitle.length > 0
        z: 20
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 48
        height: toastColumn.implicitHeight + 36
        width: Math.min(parent.width - 240, Math.max(420, toastColumn.implicitWidth + 64))
        radius: 28
        color: root.toastIsError ? Qt.rgba(0.91, 0.28, 0.31, 0.22) : Qt.rgba(1, 1, 1, 0.12)
        Column {
            id: toastColumn
            anchors.centerIn: parent
            width: parent.width - 64
            spacing: 4
            Ui.UiText {
                objectName: "settingsToastTitle"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                role: "control"
                text: root.toastTitle
            }
            Ui.UiText {
                objectName: "settingsToastDetail"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                role: "meta"
                color: Ui.Theme.textSecondary
                visible: root.toastDetail.length > 0
                text: root.toastDetail
            }
        }
    }
    Timer {
        id: slowTimer
        interval: 300
        onTriggered: {
            if (!root.awaitingKey) return;
            const row = root.rowByKey(root.awaitingKey);
            if (row && row.pending) root.saveSlow = true;
            else root.settleAwaiting();
        }
    }
    Timer {
        id: enumCloseTimer
        interval: 180
        onTriggered: root.finishEnumClose()
    }
    Timer {
        id: toastTimer
        interval: 3000
        onTriggered: {
            root.toastTitle = "";
            root.toastDetail = "";
        }
    }

    function editorKeys(event) {
        if (homeKey(event)) return;
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
