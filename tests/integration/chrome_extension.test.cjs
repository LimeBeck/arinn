// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// The Arinn MV3 extension against a mocked chrome API (spec §7.1, ADR-0048).
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
            onCreated: event(), onRemoved: event(), onFocusChanged: event(),
            async getAll(options) {
                calls.push(["windows.getAll", options]);
                return state.map(w => ({ ...w, tabs: options.populate ? w.tabs : undefined }));
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
        },
    };
    const context = {
        chrome, console, String, Math, Map, Error,
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
    assert.ok(x.calls.some(c => c[0] === "tabs.create" && c[1].windowId === 1 && c[1].url === "https://example.org/"));
});

test("open without a normal window creates one", async () => {
    const x = load({ windows: [] });
    await x.flush();
    x.ports[0].onMessage.fire({ type: "command", serial: 1, action: "open", tabId: 0, url: "https://example.org/" });
    await x.flush();
    await x.flush();
    assert.ok(x.calls.some(c => c[0] === "windows.create" && c[1].url === "https://example.org/"));
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
