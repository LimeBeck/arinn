// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// KWin platform adapter. Installation remains explicit; no arbitrary script API.
const service = "org.arinn.Daemon1";
const path = "/org/arinn/Daemon1";
let target = "";
let returnObject = null;
let homeSerial = 0;
let homeAcknowledged = false;
let returnSupported = false;
let lastReturnReport = "";
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
// Discovery and evidence are scoped to one unique daemon owner. Home never
// waits for either optional interface or a mapping acknowledgement.
function discoverReturn(owner) {
    returnSupported = false;
    lastReturnReport = "";
    callDBus(owner, path, "org.freedesktop.DBus.Introspectable", "Introspect", xml => {
        if (owner !== daemonOwner) return;
        returnSupported = typeof xml === "string" && /<interface\s+name=["']org\.arinn\.KWinReturn1["']/.test(xml);
        publishReturn();
    });
}
function reportHome(explicit, done) {
    const owner = daemonOwner, serial = ++homeSerial;
    homeAcknowledged = false;
    lastReturnReport = "";
    if (!owner) return;
    const args = explicit ? [target, homeOutput] : [homeVisible, target, homeOutput];
    callDBus(owner, path, service, explicit ? "ReportHomeRequest" : "ReportHome", ...args, () => {
        if (owner !== daemonOwner || serial !== homeSerial) return;
        homeAcknowledged = true;
        if (done) done();
        publishInventory();
        publishReturn();
    });
}
function publishReturn() {
    if (!returnSupported || !homeAcknowledged || !homeVisible || !platformGeneration ||
        !inventoryRevision || publishingAt || inventoryOwner !== daemonOwner) return;
    // Require the exact saved object and its committed token, never focus or
    // class/title matching. Ambiguous/remapped objects need fresh Home evidence.
    if (JSON.stringify(inventory()) !== inventoryFingerprint) return;
    const matches = workspace.windowList().filter(w => applicationWindow(w) && id(w) === target);
    const token = matches.length === 1 && matches[0] === returnObject ? (windowTokens.get(returnObject) || "") : "";
    const owner = daemonOwner, serial = inventorySerial, home = homeSerial;
    const generation = platformGeneration, revision = inventoryRevision;
    const key = JSON.stringify([owner, serial, home, generation, revision, target, token]);
    if (key === lastReturnReport) return;
    lastReturnReport = key;
    callDBus(owner, path, "org.arinn.KWinReturn1", "Report", generation, revision, target, token, () => {
        if (owner !== daemonOwner || serial !== inventorySerial || home !== homeSerial) return;
        // A late acknowledgement never installs cached evidence or replays a
        // mutation. Recompute only from the current committed inventory.
        if (revision !== inventoryRevision) publishReturn();
    });
}
function showHome() {
    const owner = daemonOwner;
    if (!owner) return;
    // Do not surface over a locked session. ScreenSaver owns lock state.
    callDBus("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive", locked => {
        if (locked !== false || owner !== daemonOwner) return;
        const active = workspace.activeWindow;
        if (!homeVisible) {
            detachedTarget = "";
            returnObject = active && !isShell(active) && active.normalWindow ? active : null;
            target = id(returnObject);
        }
        if (!homeVisible) homeOutput = active && !isShell(active) && active.output ? active.output.name : workspace.activeScreen.name;
        homeVisible = true;
        // Pin a transient request to its daemon owner; reconnect only restores
        // state through ReportHome and must never replay presentation intent.
        reportHome(true, raiseShell);
    });
}
// Give the Activity the screen before the shell stops covering it. Clearing
// keepAbove first leaves a frame where neither surface covers the panel (#79).
function handOff(w) {
    w.minimized = false;
    workspace.activeWindow = w;
    const home = shell();
    if (home) home.keepAbove = false;
    homeVisible = false;
    target = "";
    returnObject = null;
    detachedTarget = "";
}
function returnToActivity() {
    if (!homeVisible) return;
    const w = workspace.windowList().find(w => id(w) === (target || detachedTarget));
    if (!w) { target = ""; reportHome(false); return; }
    handOff(w);
    reportHome(false);
}
function dismissHome() {
    // A launched application replaces Home: hide it without activating the
    // previous Activity; the new window takes focus as it maps.
    if (!homeVisible) return;
    const home = shell(); if (home) home.keepAbove = false;
    homeVisible = false;
    target = "";
    returnObject = null;
    detachedTarget = "";
    reportHome(false);
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
            homeAcknowledged = false;
            returnSupported = false;
            ++homeSerial;
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
                reportHome(false);
                discoverReturn(owner);
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
        returnObject = w;
        target = detachedTarget;
        detachedTarget = "";
        reportHome(false);
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
    if (id(w) === target) { detachedTarget = target; target = ""; if (homeVisible) reportHome(false); }
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
function bridge(method, ...args) { callDBus(inventoryOwner, path, "org.arinn.KWinBridge1", method, ...args); }
function resetInventory() {
    ++inventorySerial;
    lastReturnReport = "";
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
    if (fingerprint === inventoryFingerprint) { publishReturn(); return; }
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
                    publishReturn();
                });
                return;
            }
            bridge("PutWindow", generation, revision, index, ...rows[index], () => put(index + 1));
        }
        put(0);
    });
}
// KWin's callDBus may hand a D-Bus string array ("as") to the script as an
// array-like object rather than a JS Array, especially when it is empty.
// Treating that as a protocol error reset the inventory after every idle
// NextControl poll, invalidating every window token (#73).
function stringList(value) {
    if (Array.isArray(value)) return value;
    if (value === undefined || value === null) return [];
    if (typeof value === "object" && typeof value.length === "number") return Array.from(value);
    return null;
}
function pollControl() {
    if (!platformGeneration) return;
    const serial = inventorySerial, generation = platformGeneration;
    controllingAt = Date.now();
    bridge("NextControl", generation, reply => {
        if (serial !== inventorySerial) return;
        const command = stringList(reply);
        if (command === null || command.some(part => typeof part !== "string")) {
            print("arinn: unexpected NextControl reply of type " + typeof reply);
            resetInventory();
            return;
        }
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
                    handOff(w);
                    reportHome(false);
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
