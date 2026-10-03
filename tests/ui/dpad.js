// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// D-pad reachability walker for the UI tests (AC-02, #147).
//
// An action is a visible, enabled item that takes focus and reacts to Select
// (`activated` or `clicked`) without containing another action, or a text
// field. The walker starts from a freshly built screen and its initial focus
// and explores breadth first: to try an arrow from an action it builds the
// screen again and replays the shortest arrow sequence that reached that
// action, so every step sees the state a remote would have produced. It
// never presses Select, Back, Home or Menu. Every action that is visible in
// any state the walk visits must itself be reached.
.pragma library

const arrows = [Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down];

function isAction(item) {
    if (!item || !item.visible || !item.enabled || item.opacity === 0)
        return false;
    // Pointer-only areas and controls that never take focus (their row
    // handles Select) are not remote actions.
    if (String(item).indexOf("MouseArea") >= 0 || item.focusPolicy === Qt.NoFocus)
        return false;
    // Text fields are focus targets too.
    if (typeof item.cursorPosition === "number" && typeof item.selectAll === "function")
        return true;
    return typeof item.activated === "function" || typeof item.clicked === "function";
}

function inside(item, ancestor) {
    for (let current = item.parent; current; current = current.parent) {
        if (current === ancestor)
            return true;
    }
    return false;
}

function collect(root) {
    const found = [];
    const seen = new Set();
    function walk(item) {
        if (!item || seen.has(item) || !item.visible)
            return;
        seen.add(item);
        if (isAction(item))
            found.push(item);
        const children = item.children || [];
        for (let i = 0; i < children.length; ++i)
            walk(children[i]);
        // Flickable content and control content items are not always listed
        // among the children.
        if (item.contentItem && item.contentItem !== item)
            walk(item.contentItem);
    }
    walk(root);
    // A row or list that forwards `activated` from its cards is a container,
    // not an action.
    return found.filter(item => !found.some(other => other !== item && inside(other, item)));
}

// The action that owns the focused item, if any.
function owner(item) {
    for (let current = item; current; current = current.parent) {
        if (isAction(current) && collect(current).every(other => other === current))
            return current;
    }
    // A focusable row that handles Select in its key handler instead of a
    // signal is a node of the walk too.
    for (let current = item; current; current = current.parent) {
        if (current.objectName && current.activeFocus)
            return current;
    }
    return null;
}

function label(item) {
    if (!item)
        return "";
    return item.objectName || item.text || String(item).replace(/\(0x[0-9a-f]+.*$/, "");
}

// Returns the labels of actions that arrows do not reach.
//
// `build()` creates the screen in its starting state and returns
// {view, scope}: `scope` limits which actions count, `view` is destroyed
// after each replay. `focusedItem()` returns the window's active focus item.
function unreachable(test, build, focusedItem) {
    const visible = new Set();
    const paths = new Map();
    function replay(keys) {
        const state = build();
        test.wait(0);
        for (const key of keys) {
            test.keyClick(key);
            test.wait(0);
        }
        test.wait(10);
        for (const action of collect(state.scope))
            visible.add(label(action));
        return state;
    }
    const first = replay([]);
    const start = label(owner(focusedItem()));
    first.view.destroy();
    test.verify(start !== "", "no action has focus at the start of the walk");
    paths.set(start, []);
    const queue = [start];
    while (queue.length) {
        const from = queue.shift();
        for (const key of arrows) {
            const keys = paths.get(from).concat([key]);
            const state = replay(keys);
            const to = label(owner(focusedItem()));
            state.view.destroy();
            test.wait(0);
            if (to && !paths.has(to)) {
                paths.set(to, keys);
                queue.push(to);
            }
        }
    }
    return Array.from(visible).filter(name => !paths.has(name));
}
