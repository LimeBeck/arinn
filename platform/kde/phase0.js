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
let returnAttempt = 0;
let returnAttemptKey = "";
let returnAttemptAt = 0;
let detachedTarget = "";
let homeVisible = false;
let homeOutput = "";
let daemonOwner = "";
let mediaSupported = false;
let playbackGuardAt = 0, playbackGuardSerial = 0;
// mediaSerial changes with playback; mediaPollSerial only marks which media poll
// owns the slot, so a stale-poll reset never cancels a command (#369).
let mediaSerial = 0, mediaPollSerial = 0;
// Every media poll gets the next sequence number. presentedFrom is the
// sequence of the poll that delivered the latest Present (counted when it is
// received), or a number above every started poll at a re-presentation
// without a command. A Release from an older poll
// was delayed past that newer intent and must not tear it down.
let pollSequence = 0, presentedFrom = 0;
let mediaControllingAt = 0;
// Received media commands not yet answered, and the poll serial of the latest
// receipt: the slot is free again only when every received command has been
// answered and no newer poll has started since.
let mediaCommands = 0, mediaSlot = 0;
let mediaReportingAt = 0;
let lastMediaReport = "";
let playback = null;
// The managed player window closed: the shell keeps its playback layer until
// a fresh unlocked guard result decides whether Home is shown (#372).
let closing = null;
let pollSerial = 0;
let pollStarted = 0;
let ownerProbe = 0;
let probeStarted = 0;
const watchdog = new QTimer();
watchdog.interval = 1000;
watchdog.timeout.connect(checkDaemon);
watchdog.start();
function rpc(method, ...args) { callDBus(service, path, service, method, ...args); }
// Diagnostics (#369): fixed event and reason names only, never titles, window
// classes, tokens or launch IDs. A repeat of the same operation, reason and
// outcome prints at most once per ten seconds so a stuck state cannot flood
// the journal; a changed outcome (a media channel re-attaching) always prints. Records carry the
// specification §14.1 fields, null when unknown.
const diagnosedAt = new Map();
// KWin hides print() output unless the kwin_scripting debug category is
// enabled, so the records would be missing exactly when they are needed. A
// console warning reaches the journal by default; print() is the fallback.
const record = typeof console !== "undefined" && console && typeof console.warn === "function"
    ? line => console.warn(line) : line => print(line);
function diagnose(level, operation, fields) {
    const key = operation + ":" + (fields.reason || ""), now = Date.now(), last = diagnosedAt.get(key);
    if (last && last.outcome === fields.outcome && now - last.at < 10000) return;
    diagnosedAt.set(key, { at: now, outcome: fields.outcome });
    record(JSON.stringify(Object.assign({ level, component: "arinn-kwin", provider_id: null,
        request_id: null, operation, duration_ms: null, error: null }, fields)));
}
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
        mediaSupported = typeof xml === "string" && /<interface\s+name=["']org\.arinn\.KWinMedia1["']/.test(xml);
        diagnose("info", "media.channel", { outcome: mediaSupported ? "attached" : "absent" });
        publishMedia();
        pollMediaControl();
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
    // KWin drops error callbacks. Retry only after a bounded interval and
    // after recomputing the exact current object/inventory/Home evidence above.
    if (key === returnAttemptKey && Date.now() - returnAttemptAt < 5000) return;
    const attempt = ++returnAttempt;
    returnAttemptKey = key;
    returnAttemptAt = Date.now();
    callDBus(owner, path, "org.arinn.KWinReturn1", "Report", generation, revision, target, token, () => {
        if (attempt !== returnAttempt || owner !== daemonOwner || serial !== inventorySerial || home !== homeSerial) return;
        if (revision === inventoryRevision) lastReturnReport = key;
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
        // An explicit Home owns the shell layer a pending close kept.
        if (closing) { closing = null; ++playbackGuardSerial; }
        const active = workspace.activeWindow;
        if (!homeVisible) {
            detachedTarget = "";
            returnObject = playback ? playback.window : (active && !isShell(active) && active.normalWindow ? active : null);
            target = id(returnObject);
        }
        if (!homeVisible) homeOutput = returnObject && returnObject.output ? returnObject.output.name : workspace.activeScreen.name;
        homeVisible = true;
        // Pin a transient request to its daemon owner; reconnect only restores
        // state through ReportHome and must never replay presentation intent.
        reportHome(true, raiseShell);
    });
}
// Give the Activity the screen before the shell stops covering it. Clearing
// keepAbove first leaves a frame where neither surface covers the panel (#79).
function handOff(w) {
    if (playback || closing) releasePlayback(false);
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
    if (playback && playback.window === w) {
        const saved = playback, owner = daemonOwner, serial = mediaSerial, home = homeSerial, at = Date.now();
        callDBus("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive", locked => {
            if (locked !== false || owner !== daemonOwner || serial !== mediaSerial || playback !== saved || home !== homeSerial || !homeVisible || Date.now() - at >= 1000) return;
            if (!workspace.windowList().includes(w) || launchId(w) !== saved.launch) return;
            presentPlayback(w);
            // Newer than every poll already started: a Release one of them
            // still delivers predates this Return.
            presentedFrom = ++pollSequence;
            reportHome(false);
        });
    } else { handOff(w); reportHome(false); }
}
function dismissHome() {
    if (playback || closing) releasePlayback(false);
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
            mediaSupported = false;
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
    w.outputChanged.connect(() => {
        if (workspace.activeWindow === w) report(w);
        if (playback && playback.window === w && !homeVisible && workspace.activeWindow === shell()) {
            const home = shell(); if (home && w.output && home.output !== w.output) workspace.sendClientToScreen(home, w.output);
        }
    });
    if (w.windowClassChanged) w.windowClassChanged.connect(() => {
        if (playback && playback.window === w && launchId(w) !== playback.launch) releasePlayback(false);
        publishMedia();
    });
    w.fullScreenChanged.connect(() => { if (workspace.activeWindow === w) report(w); });
    if (isShell(w)) raiseShell();
}
workspace.windowList().forEach(watchWindow);
workspace.windowAdded.connect(watchWindow);
workspace.windowActivated.connect(w => {
    report(w);
    if (playback && w && !isShell(w)) releasePlayback(false);
});
workspace.windowRemoved.connect(w => {
    // The managed player's own window closing (leaving playback, Stop, a
    // crash) returns to Arinn, which owns the screen during managed playback;
    // the shell itself disappearing does not (#372).
    if (playback && playback.window === w) closePlayback();
    else if (playback && playback.shell === w) releasePlayback(false);
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
    releasePlayback(false);
    ++mediaSerial;
    mediaControllingAt = mediaReportingAt = 0;
    mediaCommands = 0;
    lastMediaReport = "";
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
    if (fingerprint === inventoryFingerprint) { publishMedia(); publishReturn(); return; }
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
                    lastMediaReport = "";
                    publishMedia();
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
            pollMediaControl();
        });
    } else publishInventory();
    pollMediaControl();
    checkPlaybackLock();
}

// Optional managed playback lane (ADR-0074). Identities are exact native
// Wayland app IDs or X11 instance names. No title or executable association.
function launchId(w) {
    // Ordinary scripts expose Window, not EffectWindow.waylandClient.
    // Qt's QObject wrapper exposes the compositor's actual meta-object type.
    // Fail closed for unknown classes and future incompatible wrappers.
    if (!w) return "";
    let native = "";
    try {
        native = String(w);
        if (!/^KWin::(?:XdgToplevelWindow|X11Window)\(0x[0-9a-f]*[1-9a-f][0-9a-f]*(?:, "[\s\S]*")?\)$/.test(native)) return "";
    } catch (_) { return ""; }
    const x11 = native.startsWith("KWin::X11Window(");
    const value = String((x11 ? w.resourceName : w.resourceClass) || "");
    const prefix = x11 ? "org.arinn.Playback.".toLowerCase() : "org.arinn.Playback.";
    if (!value.startsWith(prefix)) return "";
    const launch = value.slice(prefix.length);
    return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(launch) && launch !== "00000000-0000-0000-0000-000000000000" ? launch : "";
}
function mediaRows() {
    return workspace.windowList().filter(applicationWindow).map(w => [windowTokens.get(w) || "", launchId(w)]);
}
function mediaBridge(method, ...args) { callDBus(inventoryOwner, path, "org.arinn.KWinMedia1", method, ...args); }
function publishMedia() {
    if (!mediaSupported || !platformGeneration || !inventoryRevision || publishingAt || mediaReportingAt || inventoryOwner !== daemonOwner) return;
    if (JSON.stringify(inventory()) !== inventoryFingerprint) return;
    const rows = mediaRows(), serial = inventorySerial, generation = platformGeneration, revision = inventoryRevision;
    const key = JSON.stringify([generation, revision, rows]);
    if (key === lastMediaReport) return;
    mediaReportingAt = Date.now();
    function put(index) {
        if (serial !== inventorySerial || revision !== inventoryRevision) return;
        if (JSON.stringify([generation, revision, mediaRows()]) !== key) { mediaReportingAt = 0; publishMedia(); return; }
        if (index === rows.length) { mediaReportingAt = 0; lastMediaReport = key; return; }
        mediaBridge("Report", generation, revision, ...rows[index], () => put(index + 1));
    }
    put(0);
}
function savedFlags(w, names) {
    const values = {}; for (const name of names) if (w[name] !== undefined) values[name] = w[name]; return values;
}
function restoreFlags(w, values) { for (const name of Object.keys(values)) w[name] = values[name]; }
// `traced` carries the daemon command's diagnostic context when a command
// asked for this presentation; Return re-presents without one.
function presentPlayback(w, traced) {
    const home = shell();
    const reason = !home ? "no_shell_window" : !applicationWindow(w) ? "not_application_window" : !launchId(w) ? "no_launch_id" : "";
    if (reason) {
        diagnose("debug", "media.present", traced ? traced("unavailable", reason, "Unavailable")
            : { outcome: "unavailable", reason, error: "Unavailable" });
        throw new Error("unavailable");
    }
    if (closing) releasePlayback(false);
    if (playback && playback.window !== w) releasePlayback(false);
    if (!playback) playback = { window: w, shell: home, launch: launchId(w),
        windowFlags: savedFlags(w, ["fullScreen", "minimized", "keepAbove"]),
        shellFlags: savedFlags(home, ["keepAbove", "minimized", "onAllDesktops"]) };
    w.minimized = false;
    w.fullScreen = true;
    // The video stays in the normal fullscreen layer; only Arinn owns input.
    w.keepAbove = false;
    if (w.output && home.output !== w.output) workspace.sendClientToScreen(home, w.output);
    homeOutput = w.output ? w.output.name : workspace.activeScreen.name;
    home.keepAbove = true;
    home.minimized = false;
    home.onAllDesktops = true;
    homeVisible = false;
    target = ""; returnObject = null; detachedTarget = "";
    workspace.activeWindow = home;
}
function releasePlayback(show) {
    const saved = playback, pending = closing;
    ++mediaSerial;
    playback = null;
    closing = null;
    playbackGuardAt = 0; ++playbackGuardSerial;
    if (pending) {
        // Settle a pending close: an authorized Release may still show Home,
        // anything else only restores the shell.
        if (workspace.windowList().includes(pending.shell)) restoreFlags(pending.shell, pending.shellFlags);
        homeVisible = show;
        if (daemonOwner) reportHome(false, show ? raiseShell : undefined);
    }
    if (!saved) return;
    const windows = workspace.windowList();
    if (windows.includes(saved.window)) restoreFlags(saved.window, saved.windowFlags);
    if (windows.includes(saved.shell)) restoreFlags(saved.shell, saved.shellFlags);
    homeVisible = show;
    target = ""; returnObject = null; detachedTarget = "";
    if (daemonOwner) reportHome(false, show ? raiseShell : undefined);
}
// Leaving playback stops the managed player, so its window usually closes
// before the shell's Release (#372). Show Home only on a fresh unlocked
// result from the same daemon owner, bounded like every playback guard;
// a locked, unknown or late answer only restores the shell's flags.
function closePlayback() {
    const saved = playback, owner = daemonOwner, at = Date.now();
    ++mediaSerial;
    playback = null;
    playbackGuardAt = 0;
    const serial = ++playbackGuardSerial;
    closing = { shell: saved.shell, shellFlags: saved.shellFlags, at };
    const pending = closing;
    target = ""; returnObject = null; detachedTarget = "";
    callDBus("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive", locked => {
        if (closing !== pending || serial !== playbackGuardSerial) return;
        if (locked !== false || owner !== daemonOwner || Date.now() - at >= 1000 ||
            !workspace.windowList().includes(pending.shell)) { releasePlayback(false); return; }
        // Keep the shell's layer until the visible report is acknowledged.
        homeVisible = true;
        reportHome(false, () => {
            if (closing !== pending || serial !== playbackGuardSerial) return;
            if (owner !== daemonOwner || Date.now() - at >= 1000) { releasePlayback(false); return; }
            closing = null;
            raiseShell();
        });
    });
}
function checkPlaybackLock() {
    if (closing && Date.now() - closing.at >= 1000) releasePlayback(false);
    if (!playback) return;
    const now = Date.now();
    if (playbackGuardAt) {
        if (now - playbackGuardAt >= 1000) releasePlayback(false);
        return;
    }
    const saved = playback, owner = daemonOwner, serial = ++playbackGuardSerial;
    playbackGuardAt = now;
    callDBus("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive", locked => {
        if (saved !== playback || owner !== daemonOwner || serial !== playbackGuardSerial) return;
        const timely = Date.now() - playbackGuardAt < 1000;
        playbackGuardAt = 0;
        if (!timely || locked !== false) releasePlayback(false);
    });
}
function pollMediaControl() {
    if (!mediaSupported || !platformGeneration) return;
    const now = Date.now();
    if (mediaReportingAt && now - mediaReportingAt > 3000) { mediaReportingAt = 0; lastMediaReport = ""; }
    if (mediaControllingAt && now - mediaControllingAt > 1500) { ++mediaPollSerial; mediaControllingAt = 0; mediaCommands = 0; }
    if (mediaControllingAt) return;
    const serial = mediaSerial, poll = ++mediaPollSerial, sequence = ++pollSequence, inventory = inventorySerial, generation = platformGeneration, owner = daemonOwner;
    mediaControllingAt = now;
    mediaBridge("NextControl", generation, reply => {
        if (inventory !== inventorySerial || owner !== daemonOwner) return;
        const command = stringList(reply);
        if (serial !== mediaSerial) {
            // Playback changed while this poll waited. A command it still
            // received is answered, never dropped: the daemon would otherwise
            // wait for its deadline (#369).
            if (command && command.length === 3 && command.every(part => typeof part === "string") &&
                ["present", "release"].includes(command[1])) {
                diagnose("debug", "media." + command[1], { outcome: "cancelled", reason: "playback_changed",
                    error: "Cancelled", request_id: command[0], duration_ms: 0 });
                mediaBridge("CompleteControl", generation, command[0], "Cancelled", () => {});
            }
            return;
        }
        // An empty or invalid reply frees the slot only while this poll owns it.
        if (!command || command.some(part => typeof part !== "string") || (command.length !== 0 && (command.length !== 3 || !["present", "release"].includes(command[1])))) { if (poll === mediaPollSerial) mediaControllingAt = 0; return; }
        if (!command.length) { if (poll === mediaPollSerial) mediaControllingAt = 0; return; }
        // The daemon long-polls for up to a second before it hands over a
        // command. Freshness counts from receipt, and the stale-poll reset must
        // not drop a command this script now owns: the daemon would wait for
        // its completion until the operation times out (#369).
        // The daemon hands a Present over, no earlier than this poll started,
        // only with at least three seconds of its deadline left. Within three
        // seconds of the poll start it is therefore still valid; later, KWin
        // delayed this reply and it may have expired. Answer without presenting.
        const received = Date.now();
        const expired = () => command[1] === "present" && Date.now() - now >= 3000;
        if (expired()) {
            diagnose("debug", "media." + command[1], { outcome: "timeout", reason: "reply_late",
                error: "Timeout", request_id: command[0], duration_ms: 0 });
            mediaBridge("CompleteControl", generation, command[0], "Timeout", () => {});
            return;
        }
        // A command from a poll the stale-poll reset already replaced is still
        // presented; it takes the slot so the newer poll cannot free it. The
        // slot is held by a count of unanswered commands, not by a receipt
        // time, which two commands can share.
        mediaSlot = ++mediaPollSerial;
        ++mediaCommands;
        // A Present's intent counts from receipt, not from its lock check, so
        // an older delayed Release whose check finishes first is still fenced.
        if (command[1] === "present") presentedFrom = Math.max(presentedFrom, sequence);
        mediaControllingAt = received;
        // The daemon's request ID (a UUID it generated) and time since receipt.
        const traced = (outcome, reason, error) => ({ outcome, reason, error,
            request_id: command[0], duration_ms: Date.now() - received });
        function complete(error) {
            if (inventory !== inventorySerial || owner !== daemonOwner) return;
            const completionSerial = mediaSerial;
            mediaCommands = Math.max(0, mediaCommands - 1);
            mediaBridge("CompleteControl", generation, command[0], error, () => {
                if (completionSerial !== mediaSerial || inventory !== inventorySerial || owner !== daemonOwner) return;
                // Another received command may still be checking the lock, or a
                // stale-poll reset may have started a newer poll meanwhile.
                if (mediaCommands === 0 && mediaPollSerial === mediaSlot) mediaControllingAt = 0;
                publishInventory();
            });
        }
        callDBus("org.freedesktop.ScreenSaver", "/ScreenSaver", "org.freedesktop.ScreenSaver", "GetActive", locked => {
            if (inventory !== inventorySerial || owner !== daemonOwner) return;
            // Playback changed while the lock check ran: answer, never drop.
            if (serial !== mediaSerial) { diagnose("debug", "media." + command[1], traced("cancelled", "playback_changed", "Cancelled")); complete("Cancelled"); return; }
            if (Date.now() - received >= 1000 || expired()) { diagnose("debug", "media." + command[1], traced("timeout", expired() ? "reply_late" : "lock_check_late", "Timeout")); complete("Timeout"); return; }
            if (locked !== false) { diagnose("debug", "media." + command[1], traced("denied", "session_locked", "PermissionDenied")); releasePlayback(false); complete("PermissionDenied"); return; }
            try {
                if (command[1] === "release") {
                    // A Present delivered by a later poll, or a re-presentation,
                    // supersedes this delayed Release: answer, do not tear down.
                    if (presentedFrom > sequence) {
                        diagnose("debug", "media.release", traced("cancelled", "superseded_by_present", "Cancelled"));
                        complete("Cancelled");
                        return;
                    }
                    const token = playback ? windowTokens.get(playback.window) || "" : "";
                    if (command[2] && token && command[2] !== token) { complete("Conflict"); return; }
                    releasePlayback(true);
                } else {
                    const w = workspace.windowList().find(w => applicationWindow(w) && windowTokens.get(w) === command[2]);
                    if (!w || !launchId(w)) {
                        diagnose("debug", "media.present", traced("not_found", w ? "no_launch_id" : "no_matching_window", "NotFound"));
                        complete("NotFound");
                        return;
                    }
                    presentPlayback(w, traced);
                    reportHome(false);
                }
                complete("");
            } catch (_) { releasePlayback(false); complete("Unavailable"); }
        });
    });
}
checkDaemon();
