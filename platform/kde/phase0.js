// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// KWin platform adapter. Installation remains explicit; no arbitrary script API.
const service = "org.arinn.Daemon1";
const path = "/org/arinn/Daemon1";
let target = "";
let detachedTarget = "";
let homeVisible = false;
let homeOutput = "";
let daemonOwner = "";
let pollSerial = 0;
let pollStarted = 0;
let ownerProbe = 0;
let probeStarted = 0;
const watchdog = new QTimer();
watchdog.interval = 1000;
watchdog.timeout.connect(checkDaemon);
watchdog.start();
function rpc(method, ...args) { callDBus(service, path, service, method, ...args); }
function id(w) { return w ? w.internalId.toString() : ""; }
function isShell(w) { return w && (w.desktopFileName === "org.arinn.Shell" || w.resourceClass === "org.arinn.Shell"); }
function shell() { return workspace.windowList().find(isShell); }
function report(w) { rpc("ReportWindow", id(w), w ? String(w.resourceClass) : "", w ? w.fullScreen : false, w && w.output ? w.output.name : ""); }
function raiseShell() {
    const w = shell();
    if (!w || !homeVisible) return;
    const output = workspace.screens.find(o => o.name === homeOutput);
    if (output && w.output !== output) workspace.sendClientToScreen(w, output);
    w.keepAbove = true;
    w.minimized = false;
    w.onAllDesktops = true;
    workspace.activeWindow = w;
}
function showHome() {
    // Do not surface over a locked session. ScreenSaver owns lock state.
    callDBus("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive", locked => {
        if (locked !== false) return;
        const active = workspace.activeWindow;
        if (!homeVisible) {
            detachedTarget = "";
            target = active && !isShell(active) && active.normalWindow ? id(active) : "";
        }
        if (!homeVisible) homeOutput = active && !isShell(active) && active.output ? active.output.name : workspace.activeScreen.name;
        homeVisible = true;
        rpc("ReportHome", true, target, homeOutput, raiseShell);
    });
}
function returnToActivity() {
    if (!homeVisible) return;
    const w = workspace.windowList().find(w => id(w) === (target || detachedTarget));
    if (!w) { target = ""; rpc("ReportHome", true, "", homeOutput); return; }
    const home = shell(); if (home) home.keepAbove = false;
    homeVisible = false;
    rpc("ReportHome", false, "", homeOutput, () => { w.minimized = false; workspace.activeWindow = w; });
    target = "";
    detachedTarget = "";
}
function dismissHome() {
    // A launched application replaces Home: hide it without activating the
    // previous Activity; the new window takes focus as it maps.
    if (!homeVisible) return;
    const home = shell(); if (home) home.keepAbove = false;
    homeVisible = false;
    target = "";
    detachedTarget = "";
    rpc("ReportHome", false, "", homeOutput);
}
function poll() {
    if (!daemonOwner) return;
    const serial = ++pollSerial;
    pollStarted = Date.now();
    rpc("TakeCommand", command => {
        if (serial !== pollSerial) return; // reply from an obsolete owner/request
        pollStarted = 0;
        if (command === "home") showHome();
        else if (command === "dismiss") dismissHome();
        else if (command === "return") returnToActivity();
        poll();
    });
}
function checkDaemon() {
    // KWin drops error callbacks entirely. A timer must recover both failed
    // calls and requests that never receive a callback; no tight retry loop.
    if (probeStarted && Date.now() - probeStarted < 5000) return;
    const probe = ++ownerProbe;
    probeStarted = Date.now();
    callDBus("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", service, exists => {
        if (probe !== ownerProbe) return;
        if (!exists) {
            probeStarted = 0;
            daemonOwner = "";
            resetInventory();
            ++pollSerial;
            pollStarted = 0;
            return;
        }
        callDBus("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner", service, owner => {
            if (probe !== ownerProbe) return;
            probeStarted = 0;
            if (owner !== daemonOwner) {
                daemonOwner = owner;
                ++pollSerial;
                // KWin retains the Activity context across daemon restarts.
                rpc("ReportHome", homeVisible, target, homeOutput);
                report(workspace.activeWindow);
                poll();
            } else if (!pollStarted || Date.now() - pollStarted > 26000) {
                poll();
            }
            inventoryTick();
        });
    });
}
registerShortcut("ArinnPhase0Home", "Arinn Phase 0: Home", "Meta+F12", showHome);
function watchWindow(w) {
    // SDL may unmap/remap its fullscreen window on focus loss. Recover only
    // the exact KWin identity, never another window with the same app class.
    if (homeVisible && detachedTarget && id(w) === detachedTarget) {
        target = detachedTarget;
        detachedTarget = "";
        rpc("ReportHome", true, target, homeOutput);
    }
    w.outputChanged.connect(() => { if (workspace.activeWindow === w) report(w); });
    w.fullScreenChanged.connect(() => { if (workspace.activeWindow === w) report(w); });
    if (isShell(w)) raiseShell();
}
workspace.windowList().forEach(watchWindow);
workspace.windowAdded.connect(watchWindow);
workspace.windowActivated.connect(report);
workspace.windowRemoved.connect(w => {
    windowTokens.delete(w);
    if (id(w) === target) { detachedTarget = target; target = ""; if (homeVisible) rpc("ReportHome", true, "", homeOutput); }
});

// Versioned internal transport: bounded scalar callDBus arguments and explicit
// command acknowledgements. None of these tokens is a process ID or command.
let inventoryOwner = "";
let inventorySerial = 0;
let inventoryInstance = "";
let platformGeneration = "";
let inventoryRevision = 0;
let windowCounter = 0;
let windowTokens = new Map();
let inventoryFingerprint = "";
let connectingAt = 0;
let publishingAt = 0;
let controllingAt = 0;
function uuid() {
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, c => {
        const r = Math.floor(Math.random() * 16);
        return (c === "x" ? r : (r & 3) | 8).toString(16);
    });
}
function bridge(method, ...args) { callDBus(service, path, "org.arinn.KWinBridge1", method, ...args); }
function resetInventory() {
    ++inventorySerial;
    inventoryOwner = "";
    inventoryInstance = uuid();
    platformGeneration = "";
    inventoryRevision = 0;
    windowCounter = 0;
    windowTokens = new Map();
    inventoryFingerprint = "";
    connectingAt = publishingAt = controllingAt = 0;
}
function boundedId(value) {
    try { return value && !value.includes("\0") && encodeURIComponent(value).replace(/%[0-9A-F]{2}/g, "x").length <= 256 ? value : ""; }
    catch (_) { return ""; }
}
function applicationWindow(w) {
    // isLockScreen() is not exported to KWin JavaScript. WaylandWindow derives
    // resourceClass from the executable basename; exclude Plasma's locker here,
    // like the shell, without using resourceClass for application association.
    return w && w.normalWindow && !isShell(w) && w.resourceClass !== "kscreenlocker_greet" && !w.inputMethod;
}
function desktopId(w) {
    // KWin documents a basename WITHOUT extension, or a full path. A full
    // path cannot establish an XDG desktop ID here and remains unassociated.
    const name = String(w.desktopFileName || "");
    return name && !name.includes("/") ? boundedId(name + ".desktop") : "";
}
function inventory() {
    const windows = workspace.windowList().filter(applicationWindow);
    for (const w of windowTokens.keys()) if (!windows.includes(w)) windowTokens.delete(w);
    return windows.map(w => {
        if (!windowTokens.has(w)) windowTokens.set(w, platformGeneration + ":" + (++windowCounter));
        const flags = (workspace.activeWindow === w ? 1 : 0) | (w.fullScreen ? 2 : 0) |
            (w.wantsInput === true ? 4 : 0) | (w.closeable === true && typeof w.closeWindow === "function" ? 8 : 0);
        return [windowTokens.get(w), desktopId(w), boundedId(w.output ? String(w.output.name) : ""), flags];
    });
}
function publishInventory() {
    if (!platformGeneration || publishingAt) return;
    const rows = inventory();
    const fingerprint = JSON.stringify(rows);
    if (fingerprint === inventoryFingerprint) return;
    if (rows.length > 100000 || inventoryRevision >= 2147483647) { resetInventory(); return; }
    const serial = inventorySerial, generation = platformGeneration, revision = inventoryRevision + 1;
    publishingAt = Date.now();
    bridge("BeginInventory", generation, revision, rows.length, () => {
        function put(index) {
            if (serial !== inventorySerial) return;
            if (index === rows.length) {
                bridge("CommitInventory", generation, revision, () => {
                    if (serial !== inventorySerial) return;
                    publishingAt = 0;
                    inventoryRevision = revision;
                    inventoryFingerprint = fingerprint;
                });
                return;
            }
            bridge("PutWindow", generation, revision, index, ...rows[index], () => put(index + 1));
        }
        put(0);
    });
}
function pollControl() {
    if (!platformGeneration) return;
    const serial = inventorySerial, generation = platformGeneration;
    controllingAt = Date.now();
    bridge("NextControl", generation, command => {
        if (serial !== inventorySerial) return;
        if (!Array.isArray(command)) { resetInventory(); return; }
        if (command.length === 0) { controllingAt = 0; pollControl(); return; }
        if (command.length !== 3 || !["activate", "close"].includes(command[1])) { resetInventory(); return; }
        function complete(error) {
            if (serial !== inventorySerial) return;
            bridge("CompleteControl", generation, command[0], error, () => {
                if (serial !== inventorySerial) return;
                controllingAt = 0;
                publishInventory();
                pollControl();
            });
        }
        callDBus("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive", locked => {
            if (serial !== inventorySerial) return;
            if (locked !== false) { complete("PermissionDenied"); return; }
            const w = workspace.windowList().find(w => applicationWindow(w) && windowTokens.get(w) === command[2]);
            if (!w) { complete("NotFound"); return; }
            try {
                if (command[1] === "close") {
                    if (w.closeable !== true || typeof w.closeWindow !== "function") { complete("Unsupported"); return; }
                    w.closeWindow();
                } else {
                    if (w.wantsInput !== true) { complete("Unsupported"); return; }
                    const home = shell(); if (home) home.keepAbove = false;
                    homeVisible = false; target = detachedTarget = "";
                    rpc("ReportHome", false, "", homeOutput);
                    w.minimized = false;
                    workspace.activeWindow = w;
                }
                complete("");
            } catch (_) { complete("Unavailable"); }
        });
    });
}
function inventoryTick() {
    if (!daemonOwner) return;
    if (inventoryOwner && inventoryOwner !== daemonOwner) resetInventory();
    const now = Date.now();
    if ((connectingAt && now - connectingAt > 5000) ||
        (publishingAt && now - publishingAt > 3500) ||
        (controllingAt && now - controllingAt > 2500)) resetInventory();
    if (!platformGeneration) {
        if (connectingAt) return;
        if (!inventoryInstance) inventoryInstance = uuid();
        inventoryOwner = daemonOwner;
        const serial = inventorySerial;
        connectingAt = now;
        bridge("Connect", inventoryInstance, generation => {
            if (serial !== inventorySerial) return;
            if (typeof generation !== "string" || !/^[0-9a-f-]{36}$/i.test(generation)) { resetInventory(); return; }
            connectingAt = 0;
            platformGeneration = generation;
            publishInventory();
            pollControl();
        });
    } else publishInventory();
}
checkDaemon();
