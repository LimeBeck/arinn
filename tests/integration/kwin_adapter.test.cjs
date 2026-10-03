// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// Deterministic regression for SDL remap vs. genuinely closed Activity.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');
const {randomUUID} = require('node:crypto');

function signal() {
    const handlers = [];
    return { connect: fn => handlers.push(fn), emit: w => handlers.forEach(fn => fn(w)) };
}
function fixture(options = {}) {
    let now = Date.now();
    const output = { name: 'second-screen' };
    const trace = [];
    function window(uuid, klass) {
        const fields = { keepAbove: false, minimized: false };
        const w = { internalId: uuid, resourceClass: klass, normalWindow: true,
            output, fullScreen: true, desktopFileName: klass === 'org.arinn.Shell' ? klass : 'org.test',
            wantsInput: true, closeable: true, closed: 0, closeWindow() { this.closed++; },
            nativeType: "KWin::X11Window", toString() { return this.nativeType + "(0x1)"; },
            onAllDesktops: false, windowClassChanged: signal(),
            outputChanged: signal(), fullScreenChanged: signal() };
        for (const name of ['keepAbove', 'minimized']) {
            Object.defineProperty(w, name, {
                enumerable: true,
                get() { return fields[name]; },
                set(value) {
                    fields[name] = value;
                    trace.push({ kind: name, window: w, value });
                },
            });
        }
        return w;
    }
    const activity = window('original-id', 'game');
    const home = window('shell-id', 'org.arinn.Shell');
    let windows = [activity, home];
    let shortcut, takeCommand, report, tick, control, mediaControl;
    const mediaReports = [], mediaAcks = [];
    const shortcuts = [];
    let locked = false, generation = randomUUID(), stage = [];
    let deferLock = false, deferHome = false;
    const homeReplies = [], commitReplies = [], returnReports = [], discoveryReplies = [];
    const acknowledgeHome = callback => { if (deferHome) homeReplies.push(callback); else callback(); };
    const lockReplies = [], homeRequests = [];
    const snapshots = [], acknowledgements = [], controls = [];
    let owner = ":1.10"; const polls = [];
    let activeWindow = activity;
    const workspace = { activeScreen: output, screens: [output],
        get activeWindow() { return activeWindow; },
        set activeWindow(value) {
            activeWindow = value;
            trace.push({ kind: 'activeWindow', window: value });
        },
        windowList: () => windows, windowAdded: signal(), windowRemoved: signal(),
        windowActivated: signal(), sendClientToScreen: (w, o) => { w.output = o; } };
    const context = vm.createContext({ workspace, Date: { now: () => now },
        QTimer: class { constructor() { this.timeout = { connect: fn => { tick = fn; } }; } start() {} },
        registerShortcut: (id, _label, keys, fn) => { shortcuts.push({ id, keys }); shortcut = fn; },
        print: () => {},
        callDBus: (_service, _path, _interface, method, ...args) => {
            if (method === 'Introspect') {
                const reply = () => args[0](options.unsupported ? '<node/>' : '<node><interface name="org.arinn.KWinReturn1"/>' + (options.managed ? '<interface name="org.arinn.KWinMedia1"/>' : '') + '</node>');
                if (options.deferDiscovery) discoveryReplies.push(reply); else reply();
            }
            if (_interface === 'org.arinn.KWinMedia1') {
                const callback = args.at(-1);
                if (method === 'Report') { mediaReports.push(args.slice(0, 4)); callback(); }
                if (method === 'NextControl') mediaControl = callback;
                if (method === 'CompleteControl') { mediaAcks.push(args.slice(0, 3)); callback(); }
                return;
            }
            if (_interface === 'org.arinn.KWinReturn1') {
                assert.equal(method, 'Report');
                assert.equal(args.length, 5);
                returnReports.push({ owner: _service, generation: args[0], revision: args[1], target: args[2], token: args[3], reply: args[4] });
            }
            if (_interface === 'org.arinn.KWinBridge1') {
                assert.ok(args.length <= 9);
                const callback = args.at(-1);
                if (method === 'Connect') { generation = randomUUID(); callback(generation); }
                if (method === 'BeginInventory') { stage = []; callback(); }
                if (method === 'PutWindow') { stage[args[2]] = args.slice(3, 7); callback(); }
                if (method === 'CommitInventory') {
                    const committed = {generation: args[0], revision: args[1], rows: stage};
                    const reply = () => { snapshots.push(committed); callback(); };
                    if (options.deferInventory) commitReplies.push(reply); else reply();
                }
                if (method === 'NextControl') { control = callback; controls.push(callback); }
                if (method === 'CompleteControl') { acknowledgements.push(args.slice(0, 3)); callback(); }
            }
            if (method === 'GetActive') {
                if (deferLock) lockReplies.push(args[0]);
                else args[0](locked);
            }
            if (method === 'NameHasOwner') args[1](!!owner);
            if (method === 'GetNameOwner') args[1](owner);
            if (method === 'TakeCommand') { takeCommand = args[0]; polls.push(takeCommand); }
            if (method === 'ReportHome') {
                report = args.slice(0, 3);
                trace.push({ kind: 'ReportHome', visible: args[0] });
                if (typeof args[3] === 'function') acknowledgeHome(args[3]);
            }
            if (method === 'ReportHomeRequest') {
                homeRequests.push({ owner: _service, args: args.slice(0, 2) });
                report = [true, ...args.slice(0, 2)];
                acknowledgeHome(args[2]);
            }
        } });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../../platform/kde/phase0.js'), 'utf8'), context);
    return { activity, home, workspace, window, polls, snapshots, acknowledgements, controls, trace, mediaReports, mediaAcks,
        media(action, token) { mediaControl([randomUUID(), action, token]); },
        mediaIdle() { mediaControl([]); },
        advance(ms) { now += ms; },
        refresh() { tick(); }, lock(value) { locked = value; },
        command(action, token) { control([randomUUID(), action, token]); },
        reconnect(value) { owner = value; tick(); },
        show: () => shortcut(), back: () => takeCommand('return'),
        shortcuts, homeRequests, lockReplies, returnReports, homeReplies, commitReplies, discoveryReplies,
        deferHome() { deferHome = true; },
        deferLock() { deferLock = true; },
        daemonHome: () => takeCommand('home'),
        dismiss: () => takeCommand('dismiss'), report: () => report,
        remove(w) { windows = windows.filter(x => x !== w); workspace.windowRemoved.emit(w); },
        add(w) { windows.push(w); workspace.windowAdded.emit(w); } };
}

test('AC-18: each global or daemon Home emits one explicit presentation request', () => {
    const f = fixture();
    assert.equal(f.homeRequests.length, 0, 'initial state synchronization is not Home');
    f.show();
    f.show();
    f.daemonHome();
    assert.deepEqual(f.homeRequests, Array.from({ length: 3 }, () => ({
        owner: ':1.10', args: ['original-id', 'second-screen'],
    })));
    f.remove(f.activity);
    f.add(f.activity);
    f.refresh();
    f.reconnect('');
    f.reconnect(':1.20');
    assert.equal(f.homeRequests.length, 3, 'remap and reconnect only restore state');
    f.remove(f.activity);
    f.back();
    assert.equal(f.homeRequests.length, 3, 'missing Return target is not Home');
    f.show();
    assert.deepEqual(f.homeRequests.at(-1), { owner: ':1.20', args: ['', 'second-screen'] });
});

test('Home requests are not replayed after lock checks or queued commands outlive the owner', () => {
    const f = fixture();
    f.lock(true);
    f.show();
    assert.equal(f.homeRequests.length, 0);
    f.lock(false);
    f.deferLock();
    f.show();
    const oldPoll = f.polls.at(-1);
    f.reconnect('');
    f.reconnect(':1.20');
    f.lockReplies[0](false);
    oldPoll('home');
    assert.equal(f.homeRequests.length, 0);
    assert.equal(f.workspace.activeWindow, f.activity);
    f.reconnect('');
    f.show();
    assert.equal(f.lockReplies.length, 1, 'no pending Home intent while disconnected');
});

test('an external window keeps Back until Home opens the overlay on that window', () => {
    const f = fixture();
    assert.deepEqual(f.shortcuts, [{ id: 'ArinnPhase0Home', keys: 'Meta+F12' }]);
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.deepEqual(f.report(), [false, '', '']);
    f.back();
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.home.keepAbove, false);
    assert.deepEqual(f.report(), [false, '', '']);
    f.show();
    assert.deepEqual(f.report(), [true, 'original-id', 'second-screen']);
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.home.keepAbove, true);
});

test('same KWin identity remaps and remains the Return target on its output', () => {
    const f = fixture();
    f.show();
    assert.deepEqual(f.report(), [true, 'original-id', 'second-screen']);
    f.show(); // repeat Home must not select shell
    f.remove(f.activity);
    assert.equal(f.report()[1], '');
    f.add(f.activity);
    assert.equal(f.report()[1], 'original-id');
    f.back();
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.report()[0], false);
});

test('a launched application dismisses Home without returning to the Activity', () => {
    const f = fixture();
    f.show();
    assert.equal(f.home.keepAbove, true);
    assert.deepEqual(f.report(), [true, 'original-id', 'second-screen']);
    f.dismiss();
    assert.deepEqual(f.report(), [false, '', 'second-screen']);
    assert.equal(f.home.keepAbove, false);
    f.dismiss(); // idempotent when Home is already hidden
    assert.deepEqual(f.report(), [false, '', 'second-screen']);
});

test('another window of the same application never replaces a closed target', () => {
    const f = fixture();
    f.show();
    f.remove(f.activity);
    f.add(f.window('different-id', 'game'));
    f.back();
    assert.deepEqual(f.report(), [true, '', 'second-screen']);
    assert.equal(f.workspace.activeWindow, f.home);
});


test('daemon owner change restores Home context and ignores late old commands', () => {
    const f = fixture();
    f.show();
    const oldPoll = f.polls[0];
    f.reconnect(""); // vanished owner; real KWin sends no error callback
    f.reconnect(":1.20");
    assert.deepEqual(f.report(), [true, 'original-id', 'second-screen']);
    const count = f.polls.length;
    oldPoll('return');
    assert.equal(f.report()[0], true);
    assert.equal(f.polls.length, count);
    f.back();
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.report()[0], false);
});


test('inventory has exact desktop IDs, distinct remap tokens and change-only revisions', () => {
    const f = fixture();
    const first = f.snapshots.at(-1);
    assert.equal(first.rows.length, 1);
    assert.equal(first.rows[0][1], 'org.test.desktop');
    assert.equal(first.rows[0][3], 15);
    f.refresh();
    assert.equal(f.snapshots.length, 1);
    const extra = f.window('extra', 'same-class');
    extra.desktopFileName = '/unknown/location/org.test.desktop';
    f.add(extra); f.refresh();
    assert.equal(f.snapshots.at(-1).rows[1][1], '');
    f.remove(f.activity); f.refresh(); f.add(f.activity); f.refresh();
    const remapped = f.snapshots.at(-1).rows.find(row => row[1] === 'org.test.desktop');
    assert.notEqual(remapped[0], first.rows[0][0]);
    f.command('activate', first.rows[0][0]);
    assert.equal(f.acknowledgements.at(-1)[2], 'NotFound');
});

test('controls acknowledge graceful dispatch, unsupported and locked failures', () => {
    const f = fixture();
    const token = f.snapshots.at(-1).rows[0][0];
    f.command('close', token);
    assert.equal(f.activity.closed, 1);
    assert.equal(f.acknowledgements.at(-1)[2], '');
    f.activity.closeable = false;
    f.command('close', token);
    assert.equal(f.acknowledgements.at(-1)[2], 'Unsupported');
    f.lock(true); f.command('activate', token);
    assert.equal(f.acknowledgements.at(-1)[2], 'PermissionDenied');
    f.lock(false); f.show(); f.command('activate', token);
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.report()[0], false);
});

test('idle control polls with array-like or missing lists keep the inventory', () => {
    // Real KWin may deliver an empty "as" reply as a non-Array value.
    for (const idle of [undefined, null, { length: 0 }]) {
        const f = fixture();
        const before = f.snapshots.at(-1).generation;
        const polls = f.controls.length;
        f.controls.at(-1)(idle);
        assert.equal(f.controls.length, polls + 1, 'polls again');
        f.refresh();
        assert.equal(f.snapshots.at(-1).generation, before, 'no reconnect');
    }
    // An array-like command is executed, not treated as a protocol error.
    const f = fixture();
    const token = f.snapshots.at(-1).rows[0][0];
    f.show();
    f.controls.at(-1)({ length: 3, 0: 'request', 1: 'activate', 2: token });
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.acknowledgements.at(-1)[2], '');
    // Anything else is still a protocol error.
    const g = fixture();
    const generation = g.snapshots.at(-1).generation;
    g.controls.at(-1)(42);
    g.refresh();
    assert.notEqual(g.snapshots.at(-1).generation, generation);
});

test('generation replacement ignores old control callbacks', () => {
    const f = fixture();
    const before = f.snapshots.at(-1);
    const old = f.controls[0];
    f.reconnect(':1.99');
    assert.notEqual(f.snapshots.at(-1).generation, before.generation);
    old([randomUUID(), 'close', before.rows[0][0]]);
    assert.equal(f.activity.closed, 0);
    assert.equal(f.acknowledgements.length, 0);
});


test('Plasma lock surface and input method are never application windows', () => {
    const f = fixture();
    const locker = f.window('lock-surface', 'kscreenlocker_greet');
    const input = f.window('input-surface', 'input'); input.inputMethod = true;
    f.add(locker); f.add(input); f.refresh();
    assert.equal(f.snapshots.at(-1).rows.length, 1);
});

// Clearing keepAbove before the Activity is active leaves a frame where the
// Plasma panel is covered by neither surface (#79).
function assertCoversPanelFirst(trace, from, activity, home) {
    function at(pred) {
        for (let i = from; i < trace.length; i++) if (pred(trace[i])) return i;
        return -1;
    }
    const unminimized = at(e => e.kind === 'minimized' && e.window === activity && e.value === false);
    const activated = at(e => e.kind === 'activeWindow' && e.window === activity);
    const lowered = at(e => e.kind === 'keepAbove' && e.window === home && e.value === false);
    const reported = at(e => e.kind === 'ReportHome' && e.visible === false);
    assert.ok(unminimized !== -1, 'un-minimizes the Activity');
    assert.ok(activated !== -1, 'activates the Activity');
    assert.ok(lowered !== -1, 'clears keepAbove on the shell');
    assert.ok(reported !== -1, 'reports Home hidden');
    assert.ok(unminimized < activated, 'un-minimize before focus');
    assert.ok(activated < lowered, 'focus before the shell drops keepAbove');
    assert.ok(lowered < reported, 'ReportHome after the shell steps down');
}

test('Return and activate cover the panel before the shell steps down', () => {
    const f = fixture();
    f.show();
    f.activity.minimized = true;
    let from = f.trace.length;
    f.back();
    assertCoversPanelFirst(f.trace, from, f.activity, f.home);
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.report()[0], false);

    f.show();
    f.activity.minimized = true;
    from = f.trace.length;
    const token = f.snapshots.at(-1).rows[0][0];
    f.command('activate', token);
    assertCoversPanelFirst(f.trace, from, f.activity, f.home);
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.acknowledgements.at(-1)[2], '');
    assert.equal(f.report()[0], false);
});


test('ADR-0073: exact saved object is linked after committed inventory and Home, despite focus loss', () => {
    const f = fixture();
    const other = f.window('other-id', 'game');
    f.add(other);
    f.refresh();
    f.show();
    assert.equal(f.workspace.activeWindow, f.home);
    const link = f.returnReports.at(-1);
    const committed = f.snapshots.at(-1);
    assert.equal(link.owner, ':1.10');
    assert.equal(link.target, 'original-id');
    assert.equal(link.generation, committed.generation);
    assert.equal(link.revision, committed.revision);
    assert.equal(link.token, committed.rows[0][0]);
    assert.notEqual(link.token, link.target);
    const count = f.returnReports.length;
    f.refresh();
    assert.equal(f.returnReports.length, count, 'unchanged evidence is not replayed');
    f.show();
    assert.equal(f.returnReports.length, count + 1, 'same-target Home requires fresh evidence');
    f.remove(f.activity);
    assert.equal(f.returnReports.at(-1).token, '');
    f.add(f.activity);
    assert.notEqual(f.returnReports.at(-1).token, link.token, 'remap mints a new token');
    assert.equal(f.returnReports.at(-1).target, link.target);
    f.back();
    const dismissedCount = f.returnReports.length;
    link.reply();
    assert.equal(f.returnReports.length, dismissedCount, 'old callback cannot restore dismissed mapping');
});

test('ADR-0073: no mapping before inventory/Home acknowledgements or on an old daemon', () => {
    const f = fixture({ deferInventory: true });
    f.show();
    assert.equal(f.workspace.activeWindow, f.home, 'Home does not wait for metadata');
    assert.equal(f.returnReports.length, 0);
    f.commitReplies.shift()();
    f.refresh();
    f.commitReplies.shift()();
    assert.equal(f.returnReports.length, 1);
    f.deferHome();
    f.show();
    f.refresh();
    assert.equal(f.returnReports.length, 1);
    f.homeReplies.shift()();
    assert.equal(f.returnReports.length, 2);
    const old = fixture({ unsupported: true });
    old.show();
    assert.equal(old.workspace.activeWindow, old.home);
    assert.equal(old.returnReports.length, 0);
    old.back();
    assert.equal(old.workspace.activeWindow, old.activity);
});

test('ADR-0073: ambiguous identities and callbacks from old contexts cannot assert a match', () => {
    const f = fixture();
    f.add(f.window('original-id', 'game'));
    f.show();
    assert.equal(f.returnReports.at(-1).token, '', 'duplicate compositor identities are unresolved');
    f.deferHome();
    f.show();
    f.dismiss();
    const before = f.returnReports.length;
    f.homeReplies.shift()();
    assert.equal(f.returnReports.length, before, 'late visible-Home acknowledgement is obsolete');
    f.homeReplies.shift()();
    const g = fixture({ deferDiscovery: true });
    g.show();
    g.reconnect(':1.20');
    g.discoveryReplies.shift()();
    assert.equal(g.returnReports.length, 0, 'old owner discovery must not enable the new owner');
    g.discoveryReplies.shift()();
    assert.equal(g.returnReports.at(-1).owner, ':1.20');
    const oldReport = g.returnReports.at(-1);
    g.reconnect(':1.30');
    const count = g.returnReports.length;
    oldReport.reply();
    assert.equal(g.returnReports.length, count);
});


test('ADR-0073: unacknowledged reports recover at a bounded rate using fresh evidence', () => {
    const f = fixture();
    f.show();
    const first = f.returnReports.at(-1);
    const count = f.returnReports.length;
    for (let i = 0; i < 4; ++i) { f.advance(1000); f.controls.at(-1)([]); f.refresh(); }
    assert.equal(f.returnReports.length, count, 'no tight retry loop');
    f.advance(1000);
    // Keep the inventory heartbeat current in this synthetic transport.
    f.controls.at(-1)([]);
    f.refresh();
    const retried = f.returnReports.at(-1);
    assert.equal(f.returnReports.length, count + 1);
    assert.equal(retried.token, first.token);
    first.reply(); // obsolete attempt acknowledgement cannot stop recovery
    f.advance(5000);
    f.controls.at(-1)([]);
    f.refresh();
    assert.equal(f.returnReports.length, count + 2);
    f.returnReports.at(-1).reply();
    f.advance(5000);
    f.controls.at(-1)([]);
    f.refresh();
    assert.equal(f.returnReports.length, count + 2, 'accepted stable evidence is not replayed');
    const g = fixture();
    g.show();
    const stale = g.returnReports.at(-1);
    g.remove(g.activity);
    g.advance(5000);
    g.controls.at(-1)([]);
    g.refresh();
    stale.reply();
    assert.equal(g.returnReports.at(-1).token, '', 'timeout recomputes instead of replaying the removed token');
});

function managedFixture(x11 = false) {
    const f = fixture({ managed: true });
    const launch = randomUUID();
    f.activity.nativeType = x11 ? 'KWin::X11Window' : 'KWin::XdgToplevelWindow';
    f.activity.resourceClass = 'org.arinn.Playback.' + launch;
    f.activity.resourceName = x11 ? 'org.arinn.playback.' + launch : '';
    f.activity.fullScreen = false;
    f.activity.windowClassChanged.emit();
    f.refresh();
    return { f, launch, token: f.snapshots.at(-1).rows[0][0] };
}
test('managed media reports exact native identity and empty identities for ordinary windows', () => {
    const {f, launch, token} = managedFixture();
    assert.ok(f.mediaReports.some(row => row[2] === token && row[3] === launch));
    f.activity.nativeType = 'KWin::X11Window';
    f.activity.windowClassChanged.emit();
    f.refresh();
    assert.equal(f.mediaReports.at(-1)[3], '');
});
test('X11 media uses the exact instance name and preserves Home/Return', () => {
    const {f, launch, token} = managedFixture(true);
    assert.ok(f.mediaReports.some(row => row[2] === token && row[3] === launch));
    f.media('present', token);
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.activity.fullScreen, true);
    f.show();
    f.back();
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.report()[0], false);
    f.activity.resourceName = 'org.arinn.playback.00000000-0000-0000-0000-000000000000';
    f.activity.windowClassChanged.emit();
    f.refresh();
    assert.equal(f.mediaReports.at(-1)[3], '');
});
test('managed Present keeps the shell focused over fullscreen video; Home and Return preserve the same window', () => {
    const {f, token} = managedFixture();
    f.media('present', token);
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.activity.fullScreen, true);
    assert.equal(f.report()[0], false);
    f.show();
    assert.equal(f.report()[0], true);
    f.back();
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.report()[0], false);
    assert.equal(f.activity.closed, 0);
    f.refresh();
    f.media('release', token);
    assert.equal(f.home.keepAbove, true, 'ordinary Home owns the shell layer after Release');
    assert.equal(f.home.onAllDesktops, true);
    assert.equal(f.activity.fullScreen, false);
    assert.equal(f.mediaAcks.at(-1)[2], '');
});
test('unrelated focus releases managed flags without stealing focus', () => {
    const {f, token} = managedFixture();
    f.media('present', token);
    const other = f.window('other', 'editor'); f.add(other);
    f.workspace.activeWindow = other;
    f.workspace.windowActivated.emit(other);
    assert.equal(f.workspace.activeWindow, other);
    assert.equal(f.home.keepAbove, false);
    assert.equal(f.activity.fullScreen, false);
});
test('lock and late guard responses cannot present managed video', () => {
    const {f, token} = managedFixture();
    f.lock(true); f.media('present', token);
    assert.equal(f.home.keepAbove, false);
    assert.equal(f.mediaAcks.at(-1)[2], 'PermissionDenied');
    f.refresh(); f.lock(false); f.deferLock(); f.media('present', token);
    f.advance(1001); f.lockReplies.at(-1)(false);
    assert.equal(f.home.keepAbove, false);
    assert.equal(f.mediaAcks.at(-1)[2], 'Timeout');
});
test('window disappearance and daemon replacement restore managed flags', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.remove(f.activity);
    assert.equal(f.home.keepAbove, false);
    const next = managedFixture(); next.f.media('present', next.token); next.f.reconnect(':1.99');
    assert.equal(next.f.home.keepAbove, false);
    assert.equal(next.f.activity.fullScreen, false);
});

test('missing lock callback tears down managed flags after the bounded unknown-state interval', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.deferLock(); f.refresh();
    assert.equal(f.home.keepAbove, true);
    f.mediaIdle(); f.advance(1001); f.refresh();
    assert.equal(f.home.keepAbove, false);
    assert.equal(f.activity.fullScreen, false);
    f.lockReplies.at(-1)(false);
    assert.equal(f.home.keepAbove, false, 'late guard cannot revive the layer');
});

test('unrelated focus from playback Home and explicit video focus relinquish the layer', () => {
    for (const fromHome of [false, true]) {
        const {f, token} = managedFixture(); f.media('present', token);
        if (fromHome) f.show();
        const selected = fromHome ? f.window('other-home', 'editor') : f.activity;
        if (fromHome) f.add(selected);
        f.workspace.activeWindow = selected; f.workspace.windowActivated.emit(selected);
        assert.equal(f.workspace.activeWindow, selected);
        assert.equal(f.home.keepAbove, false);
    }
});
