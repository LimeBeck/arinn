<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Closing applications and the Running screen

Presentation for [specification §5.5](../specifications/arinn-specification.md#55-applications)
and [§6.2](../specifications/arinn-specification.md#62-keys), after the handoff's
*Closing applications* (CL-1…11, CL-13…16;
[#205](https://github.com/LimeBeck/arinn/issues/205)). Implemented in
[#759](https://github.com/LimeBeck/arinn/issues/759) for the TV design review
[#756](https://github.com/LimeBeck/arinn/issues/756) A4. Arinn only asks an
application to close (`Application1.Terminate`); it never kills a process, never
retries and never forces. Web services keep closing on Open services.

## Entry points

All three lead to the same close flow.

- **Tile sheet** (CL-1, CL-2). A long Menu (650 ms), a long OK or a
  pointer press-and-hold on an application tile in Applications or Home
  favorites opens the sheet. Letter keys such as O are prototype bindings, not
  product keys (spec §6.2). A short Menu is reported by the tile
  (`HomeRow.menuTapped`) and keeps its meaning: navigation, or the shortcut
  editor on a web shortcut in Applications. On a web shortcut a held Menu keeps
  that meaning too. The sheet is the right glass sheet,
  720 wide: the group (or Favorites) as eyebrow, the chip and the name at 52,
  and the running state with an Ember dot. Its options are, in order:
  - Pin or Unpin, which is applied and closes the sheet after 180 ms;
  - Close, with "Arinn will ask {name} to close", only while the application
    has exactly one closable window and never on a web shortcut;
  - Refresh list, after a favourites error.

  Back, Menu and Home close the sheet and give focus back to the tile. While an
  application tile has focus, the hint reads "OK — open · Menu — actions".
- **Continue** (CL-3a). When the activity card stands for an application whose
  window Arinn can close (`Main.activityWindow`, the running window whose
  platform token is the Return target), a Close button 128 × 225, r26, plate 6 %,
  sits 16 px to its right. Right from the card reaches it, and the next Right
  reaches the first media card. With focus on it the hero meta reads
  "Close {name}". A web-service card has none.
- **Running screen** (CL-4…7, CL-13…15). The navigation item *Running*, directly
  below Open services, replaces the old *Return to activity* row. It stays when
  nothing runs: the count is then hidden, and the subline "On screen: {name}"
  with an Ember dot shows only while an application is on screen. It opens the
  `running` route, drawn by `RunningView` in `ui/screens/running/`.

## Running screen

The screen follows the Open services geometry. The title is followed by the count
(40/600, `text-3`). As on the other section screens, the shared status area
(clock, network, mute, downloads) sits top right, and the screen blurs under the
navigation where the renderer supports it (#756 C17, C26). One row per running window of an ordinary application, from
`Backend.items`; browser windows are not in that list. The window on screen
comes first.

Each row is min-h128 r26, a container: no plate at rest, an 8 % plate and the rim
in focus. It has a chip 76 (the application's icon or its letter on the neutral
chip), the name at 30/600 on one line, and a status:
- "On screen" with an Ember dot, or "In the background";
- "One of {M} windows" first when the application has several (CL-14);
- "Closing…" at 50 % opacity while a request is in flight (CL-9);
- "Did not close" after CL-11.

A Close button 128 × 128 sits 16 px to the right of each row:
- OK on the row switches to the window;
- Right moves to Close and Left returns;
- Left on a row opens navigation;
- Back returns Home.

A window without the terminate capability (CL-15) keeps its row with "Arinn
cannot close this" and a Close button at 40 % with a `block` icon. The button
stays focusable; OK shows a toast and opens no dialog.

The list scrolls so focus stays two rows from the bottom, with 72 px fade masks
and the position "5 of 9" (CL-13). With nothing running (CL-6) the screen shows
"Nothing is running", a short explanation and "Back to Home" in focus. That
needs the daemon's registry (`registryKnown`: connected, and `applicationsState`
ready or empty); otherwise the same place reads "Running applications are
unavailable" and "Try again in a moment.". The hint
reads "OK Go to · → Close · Back Exit".

## Close flow

`CloseFlow` (`ui/screens/running/CloseFlow.qml`) is one overlay in `Main.qml`,
above the screens and Home's sheet and below the quick menu and the power scope.
While a dialog is open, `Main.focusedScreen` is `closeFlow`.

- **Confirm** (CL-8). A dialog 900 wide, r48, over a 0.58 scrim: "Close {name}?",
  the warning that unsaved changes and game progress may be lost, and Cancel
  (default focus) and Close (destructive). Back is Cancel; Home closes the dialog
  without closing the application. Focus returns to the opener.
- **Closing** (CL-9). Close sends `Backend.terminate(id)`. If another operation
  on that window (such as an Activate) is still running, the terminate waits for
  it and then starts, unless the window has gone or the flow has given up on it
  (`CloseFlow.gaveUp` calls `Backend.cancelQueuedTerminate`). The window is in
  `closingIds` until it leaves `Backend.items`. Rows and the Continue Close
  button show it, and an application tile in Favorites, Applications or Open
  windows whose `runningId` is closing shows "…" in place of its Ember dot. On the Running screen focus moves on to the next row at once.
- **Closed** (CL-10, CL-16). When the window leaves the list, the toast
  "{name} closed" shows for 2.6 s; windows that go together get their toasts
  one after another. Home's "Activity is no longer available" notice is not
  shown for a window the flow is closing or closed on request. That holds only
  for the Return target the window belonged to: a new target, such as a
  browser activity, starts without it. An id the provider lists again is a new
  window and loses the mark. While the quick menu, the power scope or the ambient
  screen hides the flow (`suppressed`), a toast waits, and its 2.6 s start when
  the flow shows again.
- **Did not close** (CL-11). This happens when `Backend.operationFinished`
  reports the `Terminate` as anything but succeeded or cancelled, or when the
  window is still listed `closeWaitMs` (10 s) after the request. An
  informational dialog then says "{name} did not close" and explains that the
  application may be waiting for an answer on its own screen. Its buttons are
  "Go to {name}" (default; it activates the window) and "Stay". The row then
  reads "Did not close".

The 10 s wait answers the handoff's open question on how long Arinn waits. It is
a delegated technical choice and a named property, not a theme token.

`Backend.operationFinished(id, method, state)` is new and is emitted for every
application operation:
- `method` is the Application1 method: `Launch`, `Activate` or `Terminate`.
  The close flow reads only `Terminate`, so an Activate that ends while a close
  is pending is not taken as its result;
- `state` is `failed` when the call or the operation could not be read, and the
  Operation1 state otherwise.

## Verification

- `tests/ui/tst_running.qml` covers the screen and the close flow:
  - rows, order, statuses, CL-14 and CL-15;
  - keys and intents, the closing and failed states, nothing running, and many
    rows;
  - confirm, Back, Home, closed with the toast, did not close after the wait,
    and a failed request.
- `tests/ui/tst_home.qml` covers:
  - the right sheet and its hand-over to the close flow;
  - the long Menu, O and a short Menu;
  - the Continue Close button.
- `apps/arinn-shell/backend_test.cpp` checks `operationFinished` with its method
  on a failed Activate, and that a terminate asked for during it starts
  afterwards (run through `tests/integration/phase1.py`).
- A real application on the TV, and so the 10 s wait against a slow
  application, were not tested.
