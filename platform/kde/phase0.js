// Temporary Phase 0 KWin adapter. No startup installation or arbitrary script API.
const service = "org.arinn.Spike1";
const path = "/org/arinn/Spike1";
let target = "";
let detachedTarget = "";
let homeVisible = false;
let homeOutput = "";
let stopped = false;
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
function poll() {
    if (stopped) return;
    rpc("TakeCommand", command => {
        // D-Bus error returns undefined: stop, reload adapter after daemon restart.
        if (typeof command !== "string") { stopped = true; return; }
        if (command === "home") showHome();
        else if (command === "return") returnToActivity();
        poll();
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
    if (id(w) === target) { detachedTarget = target; target = ""; if (homeVisible) rpc("ReportHome", true, "", homeOutput); }
});
report(workspace.activeWindow);
poll();
