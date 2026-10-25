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
    const trace = [], printed = [], channels = [];
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
    let takeCommand, report, tick, control, mediaControl;
    const mediaReports = [], mediaAcks = [], mediaPolls = { count: 0 }, mediaControls = [];
    const shortcuts = [], handlers = {}, keyboardRequests = [];
    let locked = false, generation = randomUUID(), stage = [];
    let deferLock = false, deferHome = false;
    const homeReplies = [], commitReplies = [], returnReports = [], discoveryReplies = [];
    const acknowledgeHome = callback => { if (deferHome) homeReplies.push(callback); else callback(); };
    const lockReplies = [], homeRequests = [], quickMenuRequests = [];
    const snapshots = [], acknowledgements = [], controls = [];
    let owner = ":1.10"; const polls = [], pollTargets = [], lost = [];
    let daemon = 'serving';
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
        registerShortcut: (id, _label, keys, fn) => { shortcuts.push({ id, keys }); handlers[id] = fn; },
        print: line => { printed.push(line); channels.push('print'); },
        console: options.noConsole ? undefined : { warn: line => { printed.push(line); channels.push('warn'); } },
        callDBus: (_service, _path, _interface, method, ...args) => {
            // A restarting daemon owns its name before it serves its objects:
            // a call that reaches it then gets no reply, and KWin drops the
            // error callback. Next it serves Daemon1 alone, then the rest.
            if (daemon === 'silent' && !['org.freedesktop.DBus', 'org.freedesktop.ScreenSaver'].includes(_service)) {
                lost.push({ service: _service, method });
                return;
            }
            if (method === 'Introspect') {
                const iface = name => '<interface name="' + name + '"/>';
                const xml = daemon === 'partial' ? '<node>' + iface('org.arinn.Daemon1') + '</node>'
                    : '<node>' + iface('org.arinn.Daemon1') + (options.unsupported ? '' : iface('org.arinn.KWinReturn1')) +
                        (options.managed ? iface('org.arinn.KWinMedia1') : '') + iface('org.arinn.KWinBridge1') + '</node>';
                const reply = () => args[0](xml);
                if (options.deferDiscovery) discoveryReplies.push(reply); else reply();
            }
            if (_interface === 'org.arinn.KWinMedia1') {
                const callback = args.at(-1);
                if (method === 'Report') { mediaReports.push(args.slice(0, 4)); callback(); }
                if (method === 'NextControl') { mediaControl = callback; mediaControls.push(callback); mediaPolls.count++; }
                if (method === 'CompleteControl') { mediaAcks.push(args.slice(0, 3)); callback(); }
                return;
            }
            if (_interface === 'org.arinn.KeyboardBridge1') {
                assert.equal(method, 'Request');
                keyboardRequests.push(_service);
                args.at(-1)();
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
            if (method === 'TakeCommand') { takeCommand = args[0]; polls.push(takeCommand); pollTargets.push(_service); }
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
            if (method === 'ReportQuickMenuRequest') {
                quickMenuRequests.push({ owner: _service, args: args.slice(0, 2) });
                report = [true, ...args.slice(0, 2)];
                acknowledgeHome(args[2]);
            }
        } });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../../platform/kde/phase0.js'), 'utf8'), context);
    return { activity, home, workspace, window, polls, snapshots, acknowledgements, controls, trace, mediaReports, mediaAcks, printed, channels, mediaPolls, mediaControls,
        media(action, token) { mediaControl([randomUUID(), action, token]); },
        mediaIdle() { mediaControl([]); },
        advance(ms) { now += ms; },
        refresh() { tick(); }, lock(value) { locked = value; },
        command(action, token) { control([randomUUID(), action, token]); },
        reconnect(value) { owner = value; tick(); },
        // 'silent' (name owned, nothing served), 'partial' (Daemon1 only) or 'serving'.
        daemonState(value) { daemon = value; },
        pollTargets, lost,
        setManaged(value) { options.managed = value; },
        show: () => handlers.ArinnPhase0Home(), back: () => takeCommand('return'),
        quickMenu: () => handlers.ArinnQuickMenu(),
        keyboard: () => handlers.ArinnKeyboard(), keyboardRequests,
        shortcuts, homeRequests, quickMenuRequests, lockReplies, returnReports, homeReplies, commitReplies, discoveryReplies,
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

test('ADR-0106: the quick-menu key shows the shell like Home with its own request', () => {
    const f = fixture();
    f.quickMenu();
    f.quickMenu();
    assert.deepEqual(f.quickMenuRequests, Array.from({ length: 2 }, () => ({
        owner: ':1.10', args: ['original-id', 'second-screen'],
    })));
    assert.equal(f.homeRequests.length, 0, 'the quick menu is not a Home request');
    assert.deepEqual(f.report(), [true, 'original-id', 'second-screen']);
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.home.keepAbove, true);
    // Home over the quick menu keeps the same Return target.
    f.show();
    assert.deepEqual(f.homeRequests, [{ owner: ':1.10', args: ['original-id', 'second-screen'] }]);
    assert.equal(f.quickMenuRequests.length, 2);
    // Back returns to the same Activity.
    f.back();
    assert.equal(f.workspace.activeWindow, f.activity);
    assert.equal(f.home.keepAbove, false);
    assert.deepEqual(f.report().slice(0, 2), [false, '']);
    // Not over a locked session, and not without a daemon.
    f.lock(true);
    f.quickMenu();
    assert.equal(f.quickMenuRequests.length, 2);
    f.lock(false);
    f.reconnect('');
    f.quickMenu();
    assert.equal(f.quickMenuRequests.length, 2);
    // Reconnect restores state only; it never replays the quick menu.
    f.reconnect(':1.20');
    assert.equal(f.quickMenuRequests.length, 2);
    f.quickMenu();
    assert.deepEqual(f.quickMenuRequests.at(-1), { owner: ':1.20', args: ['original-id', 'second-screen'] });
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
    assert.deepEqual(f.shortcuts, [{ id: 'ArinnPhase0Home', keys: 'Meta+F12' },
        { id: 'ArinnQuickMenu', keys: 'Meta+F11' },
        { id: 'ArinnKeyboard', keys: 'Meta+F10' }]);
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

// Steps the watchdog one second at a time.
function seconds(f, count) {
    for (let i = 0; i < count; i++) { f.advance(1000); f.refresh(); }
}

test('#447: TakeCommand polling resumes on a restarted daemon that answered nothing at first', () => {
    const f = fixture();
    assert.deepEqual(f.pollTargets, [':1.10']);
    // systemctl --user restart arinnd: the old owner's poll never returns,
    // the new daemon owns the name before it serves its objects.
    f.daemonState('silent');
    f.reconnect(':1.20');
    seconds(f, 1);
    f.daemonState('partial');
    seconds(f, 2);
    assert.deepEqual(f.pollTargets, [':1.10'], 'no poll before the daemon serves the adapter interfaces');
    f.daemonState('serving');
    // The daemon reports the adapter connected only while TakeCommand polls
    // arrive (Status.platform_connected, 30 s): resume within seconds.
    seconds(f, 3);
    assert.deepEqual(f.pollTargets, [':1.10', ':1.20'], 'exactly one poll, pinned to the new owner');
    f.daemonHome();
    assert.deepEqual(f.homeRequests.at(-1), { owner: ':1.20', args: ['original-id', 'second-screen'] });
    assert.deepEqual(f.pollTargets, [':1.10', ':1.20', ':1.20'], 'the poll loop continues');
    // A further restart is noticed the same way.
    f.reconnect(':1.30');
    assert.equal(f.pollTargets.at(-1), ':1.30');
});

test('#447: discovery asks a starting daemon again until its KWin interfaces are exported', () => {
    const f = fixture({ managed: true });
    const polls = f.mediaPolls.count;
    f.daemonState('partial');
    f.reconnect(':1.20');
    seconds(f, 1);
    f.daemonState('serving');
    seconds(f, 3);
    const channel = f.printed.map(line => JSON.parse(line)).filter(entry => entry.operation === 'media.channel');
    assert.equal(channel.at(-1).outcome, 'attached', 'the media channel is not left absent');
    assert.ok(f.mediaPolls.count > polls, 'media control is polled on the new owner');
    assert.equal(f.pollTargets.at(-1), ':1.20');
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
test('a managed player window is never associated with an application (#376)', () => {
    for (const x11 of [false, true]) {
        const {f, token} = managedFixture(x11);
        const row = f.snapshots.at(-1).rows.find(r => r[0] === token);
        assert.equal(row[1], '', 'the player window has no desktop ID');
        const other = f.window('other', 'org.test');
        f.add(other); f.refresh();
        assert.ok(f.snapshots.at(-1).rows.some(r => r[0] !== token && r[1] === 'org.test.desktop'),
            'an ordinary window of the same desktop file keeps its ID');
        f.activity.resourceClass = 'mpv';
        f.activity.resourceName = 'mpv';
        f.activity.windowClassChanged.emit();
        f.refresh();
        assert.equal(f.snapshots.at(-1).rows.find(r => r[0] === token)[1], 'org.test.desktop',
            'without a launch identity the window is an ordinary application window');
    }
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
test('the closing player window returns to Arinn; daemon replacement restores managed flags', () => {
    // #372: Back stops the managed mpv, so its window closes before the
    // shell's Release arrives. Arinn must be shown, not left behind the desktop.
    const {f, token} = managedFixture();
    f.media('present', token); f.remove(f.activity);
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.home.minimized, false);
    assert.equal(f.report()[0], true, 'Home is reported visible');
    // The late Release from the shell finds nothing to undo and keeps Arinn.
    f.refresh(); f.media('release', token);
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.mediaAcks.at(-1)[2], '');
    // Another application taking focus still lets Arinn go.
    const other = managedFixture(); other.f.media('present', other.token);
    const editor = other.f.window('other', 'editor'); other.f.add(editor);
    other.f.workspace.activeWindow = editor; other.f.workspace.windowActivated.emit(editor);
    assert.equal(other.f.home.keepAbove, false);
    const next = managedFixture(); next.f.media('present', next.token); next.f.reconnect(':1.99');
    assert.equal(next.f.home.keepAbove, false);
    assert.equal(next.f.activity.fullScreen, false);
});

test('a closing player window shows Arinn only on a fresh unlocked guard', () => {
    // Review of #374: lock or an unknown session state must only restore flags.
    const locked = managedFixture();
    locked.f.media('present', locked.token); locked.f.lock(true); locked.f.remove(locked.f.activity);
    assert.equal(locked.f.report()[0], false);
    assert.equal(locked.f.home.keepAbove, false);
    // No answer: the shell keeps its playback layer, then the bounded guard
    // tears it down and a late answer cannot show Home.
    const silent = managedFixture();
    silent.f.media('present', silent.token); silent.f.deferLock(); silent.f.remove(silent.f.activity);
    assert.equal(silent.f.home.keepAbove, true, 'no desktop flash while the guard is pending');
    silent.f.mediaIdle(); silent.f.advance(1001); silent.f.refresh();
    assert.equal(silent.f.home.keepAbove, false);
    assert.equal(silent.f.report()[0], false);
    silent.f.lockReplies[0](false);
    assert.equal(silent.f.home.keepAbove, false, 'a late guard cannot show Home');
    assert.equal(silent.f.report()[0], false);
    // A late but delivered answer only restores flags.
    const late = managedFixture();
    late.f.media('present', late.token); late.f.deferLock(); late.f.remove(late.f.activity);
    late.f.advance(1001); late.f.lockReplies[0](false);
    assert.equal(late.f.home.keepAbove, false);
    assert.equal(late.f.report()[0], false);
    // The daemon owner changing while the guard is pending tears down.
    const replaced = managedFixture();
    replaced.f.media('present', replaced.token); replaced.f.deferLock(); replaced.f.remove(replaced.f.activity);
    replaced.f.reconnect(':1.99'); replaced.f.lockReplies[0](false);
    assert.equal(replaced.f.home.keepAbove, false);
});
test('a closing player window fences the Home acknowledgement', () => {
    // A late acknowledgement must not raise the shell.
    const late = managedFixture();
    late.f.media('present', late.token); late.f.deferHome(); late.f.remove(late.f.activity);
    assert.equal(late.f.report()[0], true);
    late.f.advance(1001); late.f.homeReplies.at(-1)();
    assert.equal(late.f.home.keepAbove, false);
    assert.equal(late.f.report()[0], false);
    // The daemon owner changing before the acknowledgement tears down.
    const replaced = managedFixture();
    replaced.f.media('present', replaced.token); replaced.f.deferHome(); replaced.f.remove(replaced.f.activity);
    const ack = replaced.f.homeReplies.at(-1);
    replaced.f.reconnect(':1.99'); ack();
    assert.equal(replaced.f.home.keepAbove, false);
    // KWin activating the desktop as the player window closes does not
    // cancel the return to Arinn (#372).
    const desktop = managedFixture();
    desktop.f.media('present', desktop.token); desktop.f.deferLock(); desktop.f.remove(desktop.f.activity);
    const plasma = desktop.f.window('desktop', 'plasmashell'); desktop.f.add(plasma);
    desktop.f.workspace.activeWindow = plasma; desktop.f.workspace.windowActivated.emit(plasma);
    desktop.f.lockReplies[0](false);
    assert.equal(desktop.f.workspace.activeWindow, desktop.f.home);
    assert.equal(desktop.f.home.keepAbove, true);
    assert.equal(desktop.f.report()[0], true);
    // An explicit Home during the pending close owns the shell afterwards.
    const explicit = managedFixture();
    explicit.f.media('present', explicit.token); explicit.f.deferLock(); explicit.f.remove(explicit.f.activity);
    explicit.f.show(); explicit.f.lockReplies.at(-1)(false);
    explicit.f.advance(1001); explicit.f.refresh();
    assert.equal(explicit.f.home.keepAbove, true, 'the bounded guard does not hide an explicit Home');
    assert.equal(explicit.f.report()[0], true);
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

// #369: the daemon long-polls NextControl for up to a second before handing a
// command over. Freshness counts from receipt, not from the start of the poll.
test('a Present handed over late in the long poll still presents', () => {
    const {f, token} = managedFixture();
    f.advance(900); f.deferLock(); f.media('present', token);
    f.advance(200); f.lockReplies[0](false);
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.activity.fullScreen, true);
    assert.equal(f.mediaAcks.at(-1)[2], '');
});
test('the stale-poll reset never drops a command the script received', () => {
    const {f, token} = managedFixture();
    f.advance(900); f.deferLock(); f.media('present', token);
    f.advance(700); f.refresh();
    f.lockReplies[0](false);
    assert.equal(f.mediaAcks.length, 1, 'the daemon gets an answer instead of waiting for its deadline');
    assert.equal(f.mediaAcks.at(-1)[2], '');
    assert.equal(f.home.keepAbove, true);
});
test('playback changing during the lock check answers Cancelled instead of dropping', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.refresh();
    f.deferLock(); f.media('present', token);
    const other = f.window('other', 'editor'); f.add(other);
    f.workspace.activeWindow = other; f.workspace.windowActivated.emit(other);
    f.lockReplies[0](false);
    assert.equal(f.mediaAcks.at(-1)[2], 'Cancelled');
    assert.equal(f.home.keepAbove, false);
});
test('after a daemon owner change the media channel re-attaches and Present succeeds', () => {
    const {f, token} = managedFixture();
    f.reconnect(':1.99');
    const fresh = f.snapshots.at(-1).rows[0][0];
    assert.notEqual(fresh, token, 'the new owner gets a new inventory generation');
    assert.ok(f.mediaReports.some(row => row[2] === fresh), 'media identities are reported to the new owner');
    f.media('present', fresh);
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.activity.fullScreen, true);
    assert.equal(f.mediaAcks.at(-1)[2], '');
    const attached = f.printed.map(line => JSON.parse(line)).filter(entry => entry.operation === 'media.channel');
    assert.ok(attached.length >= 1 && attached.every(entry => entry.outcome === 'attached'));
});
test('diagnostics use the always-enabled console warning channel, print only as fallback', () => {
    // KWin shows print() only with kwin_scripting debug output enabled.
    const f = fixture({ managed: true });
    assert.ok(f.channels.length > 0);
    assert.ok(f.channels.every(channel => channel === 'warn'), f.channels.join());
    const old = fixture({ managed: true, noConsole: true });
    assert.ok(old.channels.length > 0);
    assert.ok(old.channels.every(channel => channel === 'print'));
});
test('a media channel that re-attaches within the rate limit is still printed', () => {
    const f = fixture({ managed: false });
    f.setManaged(true); f.reconnect(':1.98');
    f.setManaged(false); f.reconnect(':1.99');
    f.reconnect(':1.100');
    const channel = f.printed.map(line => JSON.parse(line)).filter(entry => entry.operation === 'media.channel');
    // Each transition prints; the unchanged repeat within ten seconds does not.
    assert.deepEqual(channel.map(entry => entry.outcome), ['absent', 'attached', 'absent']);
});
test('presentation failures print rate-limited fixed reasons without identities', () => {
    const {f, launch, token} = managedFixture();
    f.media('present', 'missing-token');
    f.refresh(); f.media('present', 'missing-token');
    const failures = f.printed.map(line => JSON.parse(line)).filter(entry => entry.operation === 'media.present');
    assert.equal(failures.length, 1);
    const {request_id: requestId, ...failure} = failures[0];
    assert.match(requestId, /^[0-9a-f-]{36}$/, 'the daemon request ID');
    assert.deepEqual(failure, { level: 'debug', component: 'arinn-kwin', provider_id: null,
        operation: 'media.present', duration_ms: 0, error: 'NotFound', outcome: 'not_found', reason: 'no_matching_window' });
    // Specification §14.1 fields are present in every record, null when unknown.
    for (const entry of f.printed.map(line => JSON.parse(line)))
        for (const field of ['component', 'provider_id', 'request_id', 'operation', 'duration_ms', 'error'])
            assert.ok(field in entry, field);
    f.advance(10000); f.refresh(); f.media('present', 'missing-token');
    assert.equal(f.printed.filter(line => line.includes('no_matching_window')).length, 2, 'printed again after the interval');
    for (const line of f.printed) {
        for (const secret of [launch, token, 'org.arinn.Playback', 'game'])
            assert.ok(!line.includes(secret), line);
    }
});
test('a late answer to a superseded command keeps the newer poll in flight', () => {
    const {f, token} = managedFixture();
    f.advance(900); f.deferLock(); f.media('present', token);
    f.advance(1600); f.refresh();
    const polls = f.mediaPolls.count;
    f.lockReplies[0](false);
    assert.equal(f.mediaAcks.at(-1)[2], 'Timeout', 'answered late, not cancelled: playback did not change');
    // Keep the window-control lane answered so only the media lane is observed.
    f.controls.at(-1)([]);
    f.advance(100); f.refresh();
    assert.equal(f.mediaPolls.count, polls, 'no conflicting second poll while the newer one waits');
    f.mediaIdle(); f.controls.at(-1)([]); f.advance(1000); f.refresh();
    assert.equal(f.mediaPolls.count, polls + 1, 'polling continues after the newer poll answers');
});
test('a command delivered after the stale-poll reset is still presented', () => {
    const {f, token} = managedFixture();
    const stale = f.mediaControls.at(-1);
    // The NextControl reply is delayed past the 1.5-second reset.
    f.advance(1600); f.controls.at(-1)([]); f.refresh();
    assert.notEqual(f.mediaControls.at(-1), stale, 'the reset started a newer poll');
    const newer = f.mediaControls.at(-1);
    f.deferLock(); stale([randomUUID(), 'present', token]);
    // The newer poll answering empty must not free the slot the command holds.
    newer([]);
    const polls = f.mediaPolls.count;
    f.controls.at(-1)([]); f.advance(100); f.refresh();
    assert.equal(f.mediaPolls.count, polls, 'no poll while the received command is handled');
    f.lockReplies.at(-1)(false);
    assert.equal(f.mediaAcks.at(-1)[2], '', 'presented, not Cancelled');
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.activity.fullScreen, true);
});
test('a reply delayed past the daemon deadline is answered Timeout without presenting', () => {
    const {f, token} = managedFixture();
    const stale = f.mediaControls.at(-1);
    // Newer polls keep running while KWin holds the old callback.
    for (let i = 0; i < 4; ++i) {
        f.advance(1600); f.controls.at(-1)([]); f.refresh();
        f.mediaControls.at(-1)([]);
    }
    const request = randomUUID();
    stale([request, 'present', token]);
    assert.deepEqual(f.mediaAcks.at(-1).slice(1), [request, 'Timeout']);
    assert.equal(f.home.keepAbove, false, 'no late input layer');
    assert.equal(f.activity.fullScreen, false, 'no late fullscreen');
    const late = f.printed.map(line => JSON.parse(line)).filter(entry => entry.reason === 'reply_late');
    assert.equal(late.length, 1);
    assert.equal(late[0].request_id, request);
});
test('a reply within the handover budget is presented', () => {
    const {f, token} = managedFixture();
    const stale = f.mediaControls.at(-1);
    // 2.9 s after the poll started: the daemon guaranteed three seconds of
    // deadline at hand-over, which came no earlier than the poll start.
    f.advance(1600); f.controls.at(-1)([]); f.refresh();
    f.mediaControls.at(-1)([]);
    f.advance(1300);
    stale([randomUUID(), 'present', token]);
    assert.equal(f.mediaAcks.at(-1)[2], '');
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.activity.fullScreen, true);
});
test('a Present delayed past the handover budget is never presented', () => {
    // A reply at 3.1 s, and a reply at 2.5 s whose lock check ends at 3.1 s.
    for (const [reply, lock] of [[3100, 0], [2500, 600]]) {
        const {f, token} = managedFixture();
        const stale = f.mediaControls.at(-1);
        f.advance(1600); f.controls.at(-1)([]); f.refresh();
        f.mediaControls.at(-1)([]);
        f.advance(reply - 1600);
        f.deferLock(); stale([randomUUID(), 'present', token]);
        if (lock) { f.advance(lock); f.lockReplies.at(-1)(false); }
        assert.equal(f.mediaAcks.at(-1)[2], 'Timeout');
        assert.equal(f.home.keepAbove, false);
        assert.equal(f.activity.fullScreen, false);
        assert.ok(f.printed.some(line => JSON.parse(line).reason === 'reply_late'));
    }
});
test('a delayed Release never tears down a newer Present of the same window', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.refresh();
    const oldReleasePoll = f.mediaControls.at(-1);
    // KWin holds the Release callback while the daemon times it out and
    // hands a new Present of the same window to a later poll.
    f.advance(1600); f.controls.at(-1)([]); f.refresh();
    f.advance(4000); f.controls.at(-1)([]); f.refresh();
    f.media('present', token);
    assert.equal(f.report()[0], false);
    const request = randomUUID();
    oldReleasePoll([request, 'release', token]);
    assert.equal(f.report()[0], false, 'the newer playback stays presented');
    assert.equal(f.home.keepAbove, true);
    assert.deepEqual(f.mediaAcks.at(-1).slice(1), [request, 'Cancelled']);
    assert.ok(f.printed.some(line => JSON.parse(line).reason === 'superseded_by_present'));
});
test('a delayed Release is fenced while the newer Present still checks the lock', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.refresh();
    const oldReleasePoll = f.mediaControls.at(-1);
    f.advance(1600); f.controls.at(-1)([]); f.refresh();
    f.advance(4000); f.controls.at(-1)([]); f.refresh();
    // The newer Present is received first; both lock checks are pending and
    // the old Release's check answers first.
    f.deferLock();
    const present = randomUUID(), release = randomUUID();
    f.mediaControls.at(-1)([present, 'present', token]);
    oldReleasePoll([release, 'release', token]);
    f.lockReplies.at(-1)(false);
    f.lockReplies.at(-2)(false);
    const answers = Object.fromEntries(f.mediaAcks.map(ack => [ack[1], ack[2]]));
    assert.equal(answers[release], 'Cancelled', 'the older Release is fenced');
    assert.equal(answers[present], '', 'the newer Present is presented');
    assert.equal(f.home.keepAbove, true);
    assert.equal(f.report()[0], false);
});
test('a Release delayed from the poll active at Return does not undo the Return', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.refresh();
    const activePoll = f.mediaControls.at(-1);
    f.show();
    assert.equal(f.report()[0], true, 'Home over playback');
    f.back();
    assert.equal(f.report()[0], false, 'Return re-presented playback');
    const request = randomUUID();
    activePoll([request, 'release', token]);
    assert.deepEqual(f.mediaAcks.at(-1).slice(1), [request, 'Cancelled']);
    assert.equal(f.report()[0], false, 'the returned playback stays');
    assert.equal(f.home.keepAbove, true);
});
test('two commands received in the same millisecond own the poll slot separately', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.refresh();
    const oldReleasePoll = f.mediaControls.at(-1);
    f.advance(1600); f.controls.at(-1)([]); f.refresh();
    f.advance(4000); f.controls.at(-1)([]); f.refresh();
    f.deferLock();
    const present = randomUUID(), release = randomUUID();
    f.mediaControls.at(-1)([present, 'present', token]);
    oldReleasePoll([release, 'release', token]);
    const [presentLock, releaseLock] = f.lockReplies.slice(-2);
    // The Release's check answers first; the Present still owns the slot.
    releaseLock(false);
    const polls = f.mediaPolls.count;
    f.controls.at(-1)([]); f.advance(100); f.refresh();
    assert.equal(f.mediaPolls.count, polls, 'no poll while the Present checks the lock');
    presentLock(false);
    const answers = Object.fromEntries(f.mediaAcks.map(ack => [ack[1], ack[2]]));
    assert.equal(answers[present], '');
    assert.equal(answers[release], 'Cancelled');
    assert.equal(f.home.keepAbove, true);
});
test('a delayed Release is carried out when the newer Present fails', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.refresh();
    const oldReleasePoll = f.mediaControls.at(-1);
    f.advance(1600); f.controls.at(-1)([]); f.refresh();
    f.advance(4000); f.controls.at(-1)([]); f.refresh();
    f.deferLock();
    const present = randomUUID(), release = randomUUID();
    f.mediaControls.at(-1)([present, 'present', 'missing-token']);
    oldReleasePoll([release, 'release', token]);
    const [presentLock, releaseLock] = f.lockReplies.slice(-2);
    releaseLock(false);
    assert.ok(!f.mediaAcks.some(ack => ack[1] === release), 'the Release waits for the newer Present');
    presentLock(false);
    const answers = Object.fromEntries(f.mediaAcks.map(ack => [ack[1], ack[2]]));
    assert.equal(answers[present], 'NotFound');
    assert.equal(answers[release], '', 'the old playback is released');
    assert.equal(f.report()[0], true, 'Home is shown');
    assert.equal(f.activity.fullScreen, false, 'the player left fullscreen');
});
test('a Release from a poll started before the Present was applied still releases', () => {
    const {f, token} = managedFixture();
    // The Present arrives on a poll the stale-poll reset already replaced, so
    // the newer poll was started first; Back then arrives on that newer poll.
    const stale = f.mediaControls.at(-1);
    f.advance(1600); f.controls.at(-1)([]); f.refresh();
    const releasePoll = f.mediaControls.at(-1);
    stale([randomUUID(), 'present', token]);
    assert.equal(f.home.keepAbove, true, 'presented');
    releasePoll([randomUUID(), 'release', token]);
    assert.equal(f.mediaAcks.at(-1)[2], '');
    assert.equal(f.report()[0], true, 'Back reached Home');
});
test('a command delivered to a poll that playback outdated is answered Cancelled', () => {
    const {f, token} = managedFixture();
    f.media('present', token); f.refresh();
    const acks = f.mediaAcks.length;
    // Focus moves away while the next poll waits: playback is released.
    const other = f.window('other', 'editor'); f.add(other);
    f.workspace.activeWindow = other; f.workspace.windowActivated.emit(other);
    f.media('present', token);
    assert.equal(f.mediaAcks.length, acks + 1, 'the daemon gets an answer instead of waiting for its deadline');
    assert.equal(f.mediaAcks.at(-1)[2], 'Cancelled');
    assert.equal(f.home.keepAbove, false);
});
test('a Present without a shell window logs the daemon command context', () => {
    const {f, token} = managedFixture();
    f.remove(f.home);
    f.media('present', token);
    assert.equal(f.mediaAcks.at(-1)[2], 'Unavailable');
    const entry = f.printed.map(line => JSON.parse(line))
        .find(record => record.operation === 'media.present' && record.reason === 'no_shell_window');
    assert.ok(entry, f.printed.join('\n'));
    assert.match(entry.request_id, /^[0-9a-f-]{36}$/);
    assert.equal(entry.request_id, f.mediaAcks.at(-1)[1]);
    assert.equal(typeof entry.duration_ms, 'number');
    assert.equal(entry.error, 'Unavailable');
});

test('ADR-0105: the keyboard key asks the daemon only over an unlocked application', () => {
    const f = fixture();
    f.keyboard();
    assert.deepEqual(f.keyboardRequests, [':1.10']);
    f.lock(true);
    f.keyboard();
    assert.equal(f.keyboardRequests.length, 1, 'locked: no request');
    f.lock(false);
    f.deferLock();
    f.keyboard();
    f.workspace.activeWindow = f.home;
    f.lockReplies[0](false);
    assert.equal(f.keyboardRequests.length, 1, 'focus moved during the lock check');
    f.workspace.activeWindow = f.activity;
    f.show();
    f.keyboard();
    assert.equal(f.keyboardRequests.length, 1, 'Home visible: inside the shell');
});

test('ADR-0105 after ADR-0106: the keyboard key does nothing over the quick menu', () => {
    const f = fixture();
    f.quickMenu();
    assert.equal(f.workspace.activeWindow, f.home);
    f.keyboard();
    assert.equal(f.keyboardRequests.length, 0, 'the sheet is the shell on screen');
    // Back hands the screen to the same Activity, and the key works again.
    f.back();
    assert.equal(f.workspace.activeWindow, f.activity);
    f.keyboard();
    assert.deepEqual(f.keyboardRequests, [':1.10']);
});

test('ADR-0105: the panel window is neither the shell nor an application and sits at the bottom', () => {
    const f = fixture();
    const panel = f.window('panel-id', 'org.arinn.Shell');
    panel.caption = 'org.arinn.KeyboardPanel';
    panel.frameGeometry = { x: 0, y: 0, width: 1920, height: 400 };
    f.activity.output = f.workspace.activeScreen;
    f.workspace.activeScreen.geometry = { x: 0, y: 0, width: 1920, height: 1080 };
    f.add(panel);
    assert.deepEqual({ ...panel.frameGeometry }, { x: 0, y: 680, width: 1920, height: 400 });
    assert.equal(panel.keepAbove, true);
    f.refresh();
    assert.ok(f.snapshots.at(-1).rows.every(row => row[0] !== 'panel-id' && !String(row).includes('Shell')));
    assert.equal(f.snapshots.at(-1).rows.length, 1);
    // Home raises the real shell, never the panel.
    f.show();
    assert.equal(f.workspace.activeWindow, f.home);
    assert.equal(f.home.keepAbove, true);
    // The panel is never a keyboard target.
    f.workspace.activeWindow = panel;
    f.dismiss();
    f.keyboard();
    assert.equal(f.keyboardRequests.length, 0);
});
