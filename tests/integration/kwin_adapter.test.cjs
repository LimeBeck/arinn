// Deterministic regression for SDL remap vs. genuinely closed Activity.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

function signal() {
    const handlers = [];
    return { connect: fn => handlers.push(fn), emit: w => handlers.forEach(fn => fn(w)) };
}
function fixture() {
    const output = { name: 'second-screen' };
    function window(uuid, klass) {
        return { internalId: uuid, resourceClass: klass, normalWindow: true,
            output, fullScreen: true, outputChanged: signal(), fullScreenChanged: signal() };
    }
    const activity = window('original-id', 'game');
    const home = window('shell-id', 'org.arinn.Shell');
    let windows = [activity, home];
    let shortcut, takeCommand, report, tick;
    let owner = ":1.10"; const polls = [];
    const workspace = { activeWindow: activity, activeScreen: output, screens: [output],
        windowList: () => windows, windowAdded: signal(), windowRemoved: signal(),
        windowActivated: signal(), sendClientToScreen: (w, o) => { w.output = o; } };
    const context = vm.createContext({ workspace,
        QTimer: class { constructor() { this.timeout = { connect: fn => { tick = fn; } }; } start() {} },
        registerShortcut: (_id, _label, _keys, fn) => { shortcut = fn; },
        callDBus: (_service, _path, _interface, method, ...args) => {
            if (method === 'GetActive') args[0](false);
            if (method === 'NameHasOwner') args[1](!!owner);
            if (method === 'GetNameOwner') args[1](owner);
            if (method === 'TakeCommand') { takeCommand = args[0]; polls.push(takeCommand); }
            if (method === 'ReportHome') {
                report = args.slice(0, 3);
                if (typeof args[3] === 'function') args[3]();
            }
        } });
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../../platform/kde/phase0.js'), 'utf8'), context);
    return { activity, home, workspace, window, polls,
        reconnect(value) { owner = value; tick(); },
        show: () => shortcut(), back: () => takeCommand('return'), report: () => report,
        remove(w) { windows = windows.filter(x => x !== w); workspace.windowRemoved.emit(w); },
        add(w) { windows.push(w); workspace.windowAdded.emit(w); } };
}

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
