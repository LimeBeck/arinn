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
function fixture() {
    const output = { name: 'second-screen' };
    const trace = [];
    function window(uuid, klass) {
        const fields = { keepAbove: false, minimized: false };
        const w = { internalId: uuid, resourceClass: klass, normalWindow: true,
            output, fullScreen: true, desktopFileName: klass === 'org.arinn.Shell' ? klass : 'org.test',
            wantsInput: true, closeable: true, closed: 0, closeWindow() { this.closed++; },
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
    let shortcut, takeCommand, report, tick, control;
    const shortcuts = [];
    let locked = false, generation = randomUUID(), stage = [];
    let deferLock = false;
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
    const context = vm.createContext({ workspace,
        QTimer: class { constructor() { this.timeout = { connect: fn => { tick = fn; } }; } start() {} },
        registerShortcut: (id, _label, keys, fn) => { shortcuts.push({ id, keys }); shortcut = fn; },
        print: () => {},
        callDBus: (_service, _path, _interface, method, ...args) => {
            if (_interface === 'org.arinn.KWinBridge1') {
                assert.ok(args.length <= 9);
                const callback = args.at(-1);
                if (method === 'Connect') { generation = randomUUID(); callback(generation); }
                if (method === 'BeginInventory') { stage = []; callback(); }
                if (method === 'PutWindow') { stage[args[2]] = args.slice(3, 7); callback(); }
                if (method === 'CommitInventory') { snapshots.push({generation: args[0], revision: args[1], rows: stage}); callback(); }
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
                if (typeof args[3] === 'function') args[3]();
            }
            if (method === 'ReportHomeRequest') {
                homeRequests.push({ owner: _service, args: args.slice(0, 2) });
                report = [true, ...args.slice(0, 2)];
                args[2]();
            }
        } });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../../platform/kde/phase0.js'), 'utf8'), context);
    return { activity, home, workspace, window, polls, snapshots, acknowledgements, controls, trace,
        refresh() { tick(); }, lock(value) { locked = value; },
        command(action, token) { control([randomUUID(), action, token]); },
        reconnect(value) { owner = value; tick(); },
        show: () => shortcut(), back: () => takeCommand('return'),
        shortcuts, homeRequests, lockReplies,
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
