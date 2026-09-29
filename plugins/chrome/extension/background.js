// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// Arinn browser bridge (spec §7.1, ADR-0048, ADR-0050). Permissions: tabs
// and nativeMessaging only. It never reads page content. It sends the complete
// tab state to arinn-chrome-host and executes the host's commands; it never
// replays a command after a reconnect.
//
// Every browser activity Arinn opens is its own fullscreen popup window
// (ADR-0050). Managed windows are recognised by their type, so the rules
// survive a service worker restart without extra storage permissions.
"use strict";

const HOST = "org.arinn.chrome_host";
const MANAGED = "popup";
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
    const windows = await chrome.windows.getAll({ populate: true, windowTypes: ["normal", MANAGED] });
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

function origin(url) {
    try {
        const parsed = new URL(url);
        return parsed.protocol === "http:" || parsed.protocol === "https:" ? parsed.origin : "";
    } catch (_) {
        return "";
    }
}

// A tab opened from a managed window (a target=_blank link) becomes its own
// managed window. Chrome puts such tabs into a normal window.
async function adoptOpened(tab) {
    if (tab.incognito || typeof tab.openerTabId !== "number") return;
    try {
        const opener = await chrome.tabs.get(tab.openerTabId);
        if ((await chrome.windows.get(opener.windowId)).type !== MANAGED) return;
        const own = await chrome.windows.get(tab.windowId, { populate: true });
        if (own.type === MANAGED && (own.tabs || []).length === 1) return;
        const window = await chrome.windows.create({ tabId: tab.id, type: MANAGED, focused: true });
        await chrome.windows.update(window.id, { state: "fullscreen" });
    } catch (_) {
        // The tab or its opener is already gone.
    }
}

// A managed window that leaves fullscreen (Esc, F11) returns to it. A
// minimised window is left alone.
async function keepFullscreen(window) {
    if (window.type !== MANAGED || window.incognito) return;
    if (window.state === "fullscreen" || window.state === "minimized") return;
    try {
        await chrome.windows.update(window.id, { state: "fullscreen" });
    } catch (_) {
        // The window is already gone.
    }
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
        const target = /^https?:\/\//i.test(command.url) ? origin(command.url) : "";
        if (!target) throw new Error("InvalidArgument");
        // An open service is focused instead of being opened twice.
        const windows = await chrome.windows.getAll({ populate: true, windowTypes: [MANAGED] });
        const open = windows.find(w => !w.incognito && (w.tabs || []).some(
            t => t.active && origin(t.url || t.pendingUrl || "") === target));
        const id = open ? open.id : (await chrome.windows.create({ url: command.url, type: MANAGED, focused: true })).id;
        await chrome.windows.update(id, { focused: true, state: "fullscreen" });
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
chrome.tabs.onCreated.addListener(adoptOpened);
chrome.windows.onBoundsChanged.addListener(keepFullscreen);
chrome.runtime.onStartup.addListener(() => { if (!port && reconnectTimer === null) connect(); });
connect();
