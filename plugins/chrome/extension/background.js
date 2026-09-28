// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// Arinn browser bridge (spec §7.1, ADR-0048). Permissions: tabs and
// nativeMessaging only. It never reads page content. It sends the complete
// tab state to arinn-chrome-host and executes the host's commands; it never
// replays a command after a reconnect.
"use strict";

const HOST = "org.arinn.chrome_host";
const BACKOFF = [1, 2, 4, 8, 16, 30];
const DEBOUNCE_MS = 150;

let port = null;
let attempt = 0;
let reconnectTimer = null;
let snapshotTimer = null;

function connect() {
    reconnectTimer = null;
    try {
        port = chrome.runtime.connectNative(HOST);
    } catch (_) {
        port = null;
        scheduleReconnect();
        return;
    }
    port.onMessage.addListener(onCommand);
    port.onDisconnect.addListener(() => {
        // Reading lastError marks it handled; its text is not needed.
        void chrome.runtime.lastError;
        port = null;
        clearTimeout(snapshotTimer);
        snapshotTimer = null;
        scheduleReconnect();
    });
    sendSnapshot();
}

function scheduleReconnect() {
    if (reconnectTimer !== null) return;
    const base = BACKOFF[Math.min(attempt, BACKOFF.length - 1)] * 1000;
    attempt += 1;
    // Up to 20% jitter, never above the 30 s cap.
    const delay = Math.min(30000, base + Math.floor(Math.random() * base * 0.2));
    reconnectTimer = setTimeout(connect, delay);
}

function post(message) {
    if (!port) return false;
    try {
        port.postMessage(message);
        return true;
    } catch (_) {
        return false;
    }
}

async function sendSnapshot() {
    snapshotTimer = null;
    if (!port) return;
    const windows = await chrome.windows.getAll({ populate: true, windowTypes: ["normal"] });
    const state = { type: "snapshot", windows: [], tabs: [] };
    for (const window of windows) {
        if (window.incognito) continue;
        state.windows.push({ id: window.id, focused: !!window.focused });
        for (const tab of window.tabs || []) {
            if (tab.incognito || typeof tab.id !== "number" || tab.id < 0) continue;
            state.tabs.push({
                id: tab.id,
                windowId: window.id,
                title: tab.title || "",
                url: tab.url || tab.pendingUrl || "",
                active: !!tab.active,
                audible: !!tab.audible,
            });
        }
    }
    // A successful exchange resets the reconnect backoff.
    if (post(state)) attempt = 0;
}

function scheduleSnapshot() {
    if (!port || snapshotTimer !== null) return;
    snapshotTimer = setTimeout(sendSnapshot, DEBOUNCE_MS);
}

function errorCode(error) {
    const text = String(error && error.message || error || "");
    return /no tab|no window|not found/i.test(text) ? "NotFound" : "Unavailable";
}

async function execute(command) {
    if (command.action === "activate") {
        const tab = await chrome.tabs.update(command.tabId, { active: true });
        await chrome.windows.update(tab.windowId, { focused: true });
    } else if (command.action === "close") {
        await chrome.tabs.remove(command.tabId);
    } else if (command.action === "open") {
        if (!/^https?:\/\//i.test(command.url)) throw new Error("InvalidArgument");
        const windows = await chrome.windows.getAll({ windowTypes: ["normal"] });
        const target = windows.find(w => w.focused && !w.incognito) || windows.find(w => !w.incognito);
        if (target) {
            await chrome.tabs.create({ windowId: target.id, url: command.url, active: true });
            await chrome.windows.update(target.id, { focused: true });
        } else {
            await chrome.windows.create({ url: command.url, focused: true });
        }
    } else {
        throw new Error("InvalidArgument");
    }
}

async function onCommand(message) {
    if (!message || message.type !== "command" || typeof message.serial !== "number") return;
    let error = "";
    try {
        await execute(message);
    } catch (e) {
        error = String(e && e.message) === "InvalidArgument" ? "InvalidArgument" : errorCode(e);
    }
    post({ type: "result", serial: message.serial, error });
    scheduleSnapshot();
}

for (const event of [
    chrome.tabs.onCreated, chrome.tabs.onRemoved, chrome.tabs.onUpdated,
    chrome.tabs.onActivated, chrome.tabs.onMoved, chrome.tabs.onAttached,
    chrome.tabs.onDetached, chrome.tabs.onReplaced,
    chrome.windows.onCreated, chrome.windows.onRemoved, chrome.windows.onFocusChanged,
]) {
    event.addListener(scheduleSnapshot);
}
chrome.runtime.onStartup.addListener(() => { if (!port && reconnectTimer === null) connect(); });
connect();
