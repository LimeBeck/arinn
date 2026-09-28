#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Operator-approved live lock/unlock check. Requires manual user unlock."""
import json
import subprocess
import time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]

def call(*args):
    return subprocess.check_output(['qdbus6', *args], text=True, timeout=5).strip()

def blocked():
    return call('org.arinn.Shell1', '/org/arinn/Session1', 'org.freedesktop.DBus.Properties.Get', 'org.arinn.Session1', 'blocked') == 'true'

def state():
    return json.loads(subprocess.check_output([str(ROOT/'target/debug/arinnctl'), '--json', 'status']))

initial = state()
assert initial['home_visible'] and initial['return_window'], initial
assert not blocked()
call('org.freedesktop.ScreenSaver', '/ScreenSaver', 'org.freedesktop.ScreenSaver.Lock')
end = time.monotonic() + 10
while not blocked() and time.monotonic() < end:
    time.sleep(.05)
assert blocked(), 'shell did not block on screen lock'
print('LOCK VERIFIED. Unlock the screen manually to continue.', flush=True)
end = time.monotonic() + 180
while blocked() and time.monotonic() < end:
    time.sleep(.2)
assert not blocked(), 'manual unlock timeout'
assert state()['return_window'] == initial['return_window']
print(json.dumps({'real_lock_unlock': True, 'shell_blocked_while_locked': True,
                  'activity_context_preserved': True, 'manual_unlock': True}, indent=2))
