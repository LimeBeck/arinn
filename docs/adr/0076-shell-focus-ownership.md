# ADR-0076: Shell focus ownership

- **Status:** Accepted
- **Date:** 2026-10-03
- **Deciders:** delegated technical choice (Claude), requested by the owner in #256
- **Amends / Supersedes:** —
- **Amended by / Superseded by:** —
- **Specification:** [§6.1 Focus](../specifications/arinn-specification.md#61-focus), AC-02, AC-14

## Context

Every shell screen is a `FocusScope`, but focus moved imperatively. The shell had 108 `forceActiveFocus()` calls, and screens also made them from data callbacks while they were hidden. When `forceActiveFocus()` targets an item that is hidden or disabled, directly or through an ancestor, Qt moves active focus to the window's root item. No key handler runs there, so the remote stopped working until a pointer hovered a card. Two cases of this were found on a real TV: #252 (opening the Library left focus on a hidden button) and #254 (the hidden Library took focus from Home on every catalog refresh).

TV frameworks handle this with the same few rules:
- Qt Quick: focus scopes with declarative `focus`, and pages that are not shown do not take part.
- Android TV: `saveFocus`/`restoreFocus` per container, and a hidden view cannot be focused.
- Spatial navigation libraries for the web, such as Norigin and LRUD: one focus manager, and the parent or last child is focused when a node goes away.

Their shared rules are: focus has one owner; a hidden node never holds or takes focus; a container remembers its last focused child; and recovery goes to the nearest container, never to nothing.

## Decision

1. **`Ui.Focus.take(item[, reason])`** (`apps/arinn-shell/ui/Focus.qml`) is the only caller of `forceActiveFocus()` in shell QML. It moves focus only when the item is effectively visible and enabled, and returns whether it did. A screen that is not on display therefore cannot take focus from the one that is.
2. **Main owns screen focus.** `Main.qml` computes `focusedScreen` from the route, in priority order: player, session, browser, Home's navigation menu or shortcut editor, library, settings, Home. Each screen binds `focus: root.focusedScreen === "<name>"`, so the screen on display is the focused scope, and Qt restores the child that screen last focused. If a new route's screen gets no focused item that way, `restoreFocus()` focuses the screen's preferred item (`focusInitial()`, `restore()`, `settleFocus()` or `HomeView.restoreFocus()`).
3. **Screens move focus only inside themselves**, through `Focus.take()`. Moving between screens is Main's job.
4. **Last resort.** When the active window's focus still ends on the root item, for example because the focused item was destroyed, Main calls `restoreFocus()` and logs `Focus fell to the window root`. An inactive window parks focus on the root item by itself, so the guard ignores it.
5. **Enforcement.** The `focus-ownership` ctest fails on any `forceActiveFocus(` in shell QML outside `ui/Focus.qml`, comments excepted.

## Consequences

- A data refresh in a hidden screen can no longer take focus from the screen on display. Data handlers may call their focus helpers without their own visibility guards; the guards added in #255 stay but are no longer what keeps focus.
- Code that focuses an item in the same turn that makes it visible must make it visible first. A binding that has not updated yet leaves the item hidden, so `take()` returns false; defer the call with `Qt.callLater` as the existing code does.
- New screens must be added to `focusedScreen`, `screenItem()` and `restoreFocus()` in `Main.qml`.
- The root-focus warning in the log points to a path that bypasses these rules.

## Alternatives considered

- **Visibility guards in each screen callback** (the #255 approach alone). Rejected: it fixes call sites one by one and does not stop the next one.
- **Loading only the screen on display (`Loader`/`StackView`).** Rejected for now: the library and settings screens keep their state (tab, focused poster, open detail) between visits, and tests address them as permanent objects. It remains possible later; this ADR does not depend on it.
- **Declarative `focus` only, without `Focus.take()`.** Rejected: focus inside a screen still moves imperatively along explicit neighbours, and an imperative call on a hidden item would still drop focus to the root item.

## Verification

- `tests/ui/tst_focus.qml`: `take()` refuses items of a hidden or disabled screen and keeps the current focus.
- `LibraryCatalogTest::hiddenLibraryLeavesHomeFocusAlone`: a hidden library refresh or `focusInitial()` moves no focus, and focus lost by destroying the focused item returns to Home.
- `focus-ownership` ctest.
- Existing navigation suites (`tst_home*.qml`, `tst_settings.qml`, `tst_library.qml`, catalog tests) pass unchanged.
- On a real TV: keyboard-only navigation across Home, Library, playback and back, without pointer hover (recorded on the pull request).
