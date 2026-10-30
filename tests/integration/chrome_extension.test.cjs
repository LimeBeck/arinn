// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// The Arinn MV3 extension against a mocked chrome API (spec §7.1, ADR-0048, ADR-0050).
"use strict";
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const root = path.join(__dirname, "..", "..", "plugins", "chrome");
const source = fs.readFileSync(path.join(root, "extension", "background.js"), "utf8");
const manifest = JSON.parse(fs.readFileSync(path.join(root, "extension", "manifest.json"), "utf8"));

function event() {
    const listeners = [];
    return { addListener: f => listeners.push(f), fire: (...args) => listeners.forEach(f => f(...args)), listeners };
}

function load({ windows, connectFails = false } = {}) {
    const timers = [];
    const posted = [];
    const calls = [];
    const ports = [];
    let state = windows || [
        { id: 1, focused: true, incognito: false, tabs: [
            { id: 7, title: "Video", url: "https://video.example/", active: true, audible: true },
            { id: 8, title: "", pendingUrl: "https://mail.example/", active: false },
        ] },
        { id: 2, focused: false, incognito: true, tabs: [{ id: 9, title: "Private", url: "https://p.example/", active: true }] },
    ];
    const tabsById = () => new Map(state.flatMap(w => (w.tabs || []).map(t => [t.id, { ...t, windowId: w.id }])));
    const chrome = {
        runtime: {
            lastError: undefined,
            onStartup: event(),
            connectNative(name) {
                calls.push(["connectNative", name]);
                if (connectFails) throw new Error("no host");
                const port = { onMessage: event(), onDisconnect: event(), postMessage: m => posted.push(m) };
                ports.push(port);
                return port;
            },
        },
        windows: {
            onCreated: event(), onRemoved: event(), onFocusChanged: event(), onBoundsChanged: event(),
            async getAll(options) {
                calls.push(["windows.getAll", options]);
                return state.filter(w => options.windowTypes.includes(w.type || "normal"))
                    .map(w => ({ type: "normal", ...w, tabs: options.populate ? w.tabs : undefined }));
            },
            async get(id, options = {}) {
                const w = state.find(w => w.id === id);
                if (!w) throw new Error(`No window with id: ${id}.`);
                return { type: "normal", ...w, tabs: options.populate ? w.tabs : undefined };
            },
            async update(id, props) { calls.push(["windows.update", id, props]); return { id }; },
            async create(props) { calls.push(["windows.create", props]); return { id: 99 }; },
        },
        tabs: {
            onCreated: event(), onRemoved: event(), onUpdated: event(), onActivated: event(),
            onMoved: event(), onAttached: event(), onDetached: event(), onReplaced: event(),
            async update(id, props) {
                calls.push(["tabs.update", id, props]);
                const tab = tabsById().get(id);
                if (!tab) throw new Error(`No tab with id: ${id}.`);
                return tab;
            },
            async remove(id) {
                calls.push(["tabs.remove", id]);
                if (!tabsById().has(id)) throw new Error(`No tab with id: ${id}.`);
            },
            async create(props) { calls.push(["tabs.create", props]); return { id: 100 }; },
            async get(id) {
                const tab = tabsById().get(id);
                if (!tab) throw new Error(`No tab with id: ${id}.`);
                return tab;
            },
        },
    };
    const context = {
        chrome, console, String, Math, Map, Error, URL,
        setTimeout: (f, ms) => { timers.push({ f, ms }); return timers.length; },
        clearTimeout: id => { if (timers[id - 1]) timers[id - 1].f = null; },
    };
    vm.createContext(context);
    vm.runInContext(source, context);
    const flush = () => new Promise(resolve => setImmediate(resolve));
    const runTimers = async () => {
        const due = timers.splice(0);
        for (const t of due) if (t.f) t.f();
        await flush();
        return due.filter(t => t.f).map(t => t.ms);
    };
    return { chrome, posted, calls, ports, timers, flush, runTimers, setState: s => { state = s; } };
}

test("manifest requests only tabs and nativeMessaging and has the packaged ID", () => {
    assert.equal(manifest.manifest_version, 3);
    assert.deepEqual([...manifest.permissions].sort(), ["nativeMessaging", "tabs"]);
    for (const key of ["host_permissions", "content_scripts", "optional_permissions", "web_accessible_resources"]) {
        assert.equal(manifest[key], undefined, key);
    }
    const digest = crypto.createHash("sha256").update(Buffer.from(manifest.key, "base64")).digest("hex");
    const id = [...digest.slice(0, 32)].map(c => String.fromCharCode(97 + parseInt(c, 16))).join("");
    const lib = fs.readFileSync(path.join(root, "src", "lib.rs"), "utf8");
    assert.match(lib, new RegExp(`EXTENSION_ID: &str = "${id}"`));
    // One host manifest is installed for both Google Chrome and Chromium.
    const host = JSON.parse(fs.readFileSync(
        path.join(__dirname, "..", "..", "packaging", "chrome", "org.arinn.chrome_host.json"), "utf8"));
    assert.deepEqual(host.allowed_origins, [`chrome-extension://${id}/`]);
    assert.equal(host.name, "org.arinn.chrome_host");
    assert.equal(host.type, "stdio");
});

test("connects and sends the tab snapshot without incognito windows", async () => {
    const x = load();
    await x.flush();
    assert.deepEqual(x.calls[0], ["connectNative", "org.arinn.chrome_host"]);
    assert.deepEqual(JSON.parse(JSON.stringify(x.posted[0])), {
        type: "snapshot",
        windows: [{ id: 1, focused: true }],
        tabs: [
            { id: 7, windowId: 1, title: "Video", url: "https://video.example/", active: true, audible: true },
            { id: 8, windowId: 1, title: "", url: "https://mail.example/", active: false, audible: false },
        ],
    });
});

test("tab events are debounced into one snapshot", async () => {
    const x = load();
    await x.flush();
    x.posted.length = 0;
    x.chrome.tabs.onUpdated.fire(7, {}, {});
    x.chrome.tabs.onActivated.fire({});
    x.chrome.windows.onFocusChanged.fire(1);
    assert.equal(x.timers.filter(t => t.f).length, 1);
    await x.runTimers();
    assert.equal(x.posted.filter(m => m.type === "snapshot").length, 1);
});

test("commands run and report results by serial", async () => {
    const x = load();
    await x.flush();
    const port = x.ports[0];
    const results = () => x.posted.filter(m => m.type === "result").map(m => [m.serial, m.error]).sort((a, b) => a[0] - b[0]);
    port.onMessage.fire({ type: "command", serial: 1, action: "activate", tabId: 7, url: "" });
    port.onMessage.fire({ type: "command", serial: 2, action: "close", tabId: 404, url: "" });
    port.onMessage.fire({ type: "command", serial: 3, action: "open", tabId: 0, url: "https://example.org/" });
    port.onMessage.fire({ type: "command", serial: 4, action: "open", tabId: 0, url: "javascript:alert(1)" });
    port.onMessage.fire({ type: "command", serial: 5, action: "eval", tabId: 0, url: "" });
    port.onMessage.fire({ type: "other" });
    await x.flush();
    await x.flush();
    assert.deepEqual(results(), [[1, ""], [2, "NotFound"], [3, ""], [4, "InvalidArgument"], [5, "InvalidArgument"]]);
    assert.ok(x.calls.some(c => c[0] === "tabs.update" && c[1] === 7));
    assert.ok(x.calls.some(c => c[0] === "windows.update" && c[1] === 1 && c[2].focused));
    assert.ok(!x.calls.some(c => c[0] === "tabs.create"), "open never adds a tab to a window");
});

async function open(x, url) {
    x.calls.length = 0;
    x.ports[0].onMessage.fire({ type: "command", serial: 1, action: "open", tabId: 0, url });
    for (let i = 0; i < 4; i++) await x.flush();
    return x.posted.filter(m => m.type === "result").at(-1).error;
}

const managed = [
    { id: 1, focused: false, incognito: false, tabs: [{ id: 7, title: "Browser", url: "https://video.example/", active: true }] },
    { id: 3, type: "popup", focused: true, incognito: false, tabs: [{ id: 10, title: "Video", url: "https://video.example/watch?v=1", active: true }] },
    { id: 4, type: "popup", focused: false, incognito: true, tabs: [{ id: 11, title: "Private", url: "https://mail.example/", active: true }] },
];

test("open creates a fullscreen popup window for a new service", async () => {
    const x = load({ windows: [] });
    await x.flush();
    assert.equal(await open(x, "https://example.org/"), "");
    assert.deepEqual(JSON.parse(JSON.stringify(x.calls.filter(c => c[0].startsWith("windows.") && c[0] !== "windows.getAll"))), [
        ["windows.create", { url: "https://example.org/", type: "popup", focused: true }],
        ["windows.update", 99, { focused: true, state: "fullscreen" }],
    ]);
});

test("open of an already open origin focuses its window instead of creating one", async () => {
    const x = load({ windows: managed });
    await x.flush();
    assert.equal(await open(x, "https://video.example/other"), "");
    assert.ok(!x.calls.some(c => c[0] === "windows.create" || c[0] === "tabs.create"));
    assert.deepEqual(JSON.parse(JSON.stringify(x.calls.find(c => c[0] === "windows.update"))),
        ["windows.update", 3, { focused: true, state: "fullscreen" }]);
});

test("open reports the selected window even while desktop focus stays on the last service", async () => {
    const x = load({ windows: [
        { id: 3, type: "popup", focused: false, tabs: [{ id: 10, url: "https://video.example/", active: true }] },
        { id: 5, type: "popup", focused: true, tabs: [{ id: 12, url: "https://mail.example/", active: true }] },
    ] });
    await x.flush();
    // The mock deliberately leaves focused unchanged, as when Home owns focus.
    for (const [url, expected] of [["https://video.example/", 3], ["https://mail.example/", 5],
        ["https://new.example/", 99], ["https://video.example/", 3]]) {
        assert.equal(await open(x, url), "");
        assert.equal(x.posted.filter(m => m.type === "result").at(-1).windowId, expected);
        assert.equal(x.posted.filter(m => m.type === "result").at(-1).created, expected === 99);
    }
});

test("open matches the origin, not the host or the path", async () => {
    const x = load({ windows: managed });
    await x.flush();
    // A different scheme or port is a different service; incognito and
    // ordinary windows are never reused.
    for (const url of ["http://video.example/", "https://video.example:8443/", "https://mail.example/"]) {
        assert.equal(await open(x, url), "");
        assert.ok(x.calls.some(c => c[0] === "windows.create" && c[1].url === url), url);
    }
});

test("a tab opened from a managed window moves into its own fullscreen window", async () => {
    const x = load({ windows: managed });
    await x.flush();
    // Chrome put the new tab into the ordinary window 1.
    x.setState([{ ...managed[0], tabs: [
        { id: 7, title: "Browser", url: "https://video.example/", active: false },
        { id: 12, title: "", url: "https://video.example/next", active: true, openerTabId: 10 },
    ] }, managed[1], managed[2]]);
    x.calls.length = 0;
    x.chrome.tabs.onCreated.fire({ id: 12, windowId: 1, openerTabId: 10, incognito: false });
    for (let i = 0; i < 6; i++) await x.flush();
    assert.deepEqual(JSON.parse(JSON.stringify(x.calls.filter(c => c[0] !== "windows.getAll"))), [
        ["windows.create", { tabId: 12, type: "popup", focused: true }],
        ["windows.update", 99, { state: "fullscreen" }],
    ]);
});

test("tabs not opened from a managed window stay where they are", async () => {
    const x = load({ windows: managed });
    await x.flush();
    x.calls.length = 0;
    // No opener, an opener in an ordinary window, an unknown opener, a lone
    // tab already in its own popup, and an incognito tab.
    x.chrome.tabs.onCreated.fire({ id: 20, windowId: 1, incognito: false });
    x.chrome.tabs.onCreated.fire({ id: 21, windowId: 1, openerTabId: 7, incognito: false });
    x.chrome.tabs.onCreated.fire({ id: 22, windowId: 1, openerTabId: 404, incognito: false });
    x.chrome.tabs.onCreated.fire({ id: 10, windowId: 3, openerTabId: 10, incognito: false });
    x.chrome.tabs.onCreated.fire({ id: 23, windowId: 4, openerTabId: 11, incognito: true });
    for (let i = 0; i < 6; i++) await x.flush();
    assert.ok(!x.calls.some(c => c[0] === "windows.create" || c[0] === "windows.update"));
});

test("a managed window that leaves fullscreen returns to it", async () => {
    const x = load({ windows: managed });
    await x.flush();
    x.calls.length = 0;
    x.chrome.windows.onBoundsChanged.fire({ id: 3, type: "popup", state: "normal", incognito: false });
    // Ordinary, minimised, fullscreen and incognito windows are left alone.
    x.chrome.windows.onBoundsChanged.fire({ id: 1, type: "normal", state: "normal", incognito: false });
    x.chrome.windows.onBoundsChanged.fire({ id: 3, type: "popup", state: "minimized", incognito: false });
    x.chrome.windows.onBoundsChanged.fire({ id: 3, type: "popup", state: "fullscreen", incognito: false });
    x.chrome.windows.onBoundsChanged.fire({ id: 4, type: "popup", state: "normal", incognito: true });
    await x.flush();
    assert.deepEqual(JSON.parse(JSON.stringify(x.calls)), [["windows.update", 3, { state: "fullscreen" }]]);
});

test("snapshots include managed popup windows", async () => {
    const x = load({ windows: managed });
    await x.flush();
    assert.deepEqual(JSON.parse(JSON.stringify(x.posted[0].windows)), [{ id: 1, focused: false }, { id: 3, focused: true }]);
    assert.deepEqual(JSON.parse(JSON.stringify(x.posted[0].tabs.map(t => t.id))), [7, 10]);
});

test("reconnects with capped exponential backoff and never replays commands", async () => {
    const x = load({ connectFails: true });
    await x.flush();
    const delays = [];
    for (let i = 0; i < 8; i++) delays.push(...(await x.runTimers()));
    const bases = [1, 2, 4, 8, 16, 30, 30, 30].map(s => s * 1000);
    assert.equal(delays.length, bases.length);
    delays.forEach((d, i) => assert.ok(d >= bases[i] && d <= Math.min(30000, bases[i] * 1.2), `${i}: ${d}`));

    const y = load();
    await y.flush();
    y.ports[0].onMessage.fire({ type: "command", serial: 1, action: "close", tabId: 7, url: "" });
    y.ports[0].onDisconnect.fire();
    await y.flush();
    const removals = () => y.calls.filter(c => c[0] === "tabs.remove").length;
    assert.equal(removals(), 1);
    await y.runTimers();
    assert.equal(y.ports.length, 2);
    await y.flush();
    assert.equal(removals(), 1, "no replay after reconnect");
    assert.equal(y.posted.at(-1).type, "snapshot");
});
