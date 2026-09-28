#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Live test using the currently focused disposable Activity and phase0 units."""
import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

def run(*args):
    return subprocess.check_output(args, text=True, timeout=8, stderr=subprocess.DEVNULL).strip()

def status():
    return json.loads(run(str(ROOT / 'target/debug/arinnctl'), '--json', 'status'))

def wait(test, timeout=12):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        try:
            state = status()
            if test(state):
                return state
        except (subprocess.CalledProcessError, json.JSONDecodeError):
            pass
        time.sleep(.1)
    raise AssertionError('recovery timed out')

initial = status()
target = initial['active_window']
assert target and initial['active_class'] not in ('org.arinn.Shell', 'arinn-shell')
assert initial['platform_connected']
for visible in (False, True):
    if visible:
        run(str(ROOT / 'target/debug/arinnctl'), 'home')
        wait(lambda s: s['home_visible'] and s['return_window'] == target)
    before = status()
    run('systemctl', '--user', 'kill', '--signal=SIGKILL', 'arinn-phase0-daemon')
    restored = wait(lambda s: s['epoch'] != before['epoch'] and s['platform_connected'] and s['provider_available'] and s['home_visible'] == visible)
    if visible:
        assert restored['return_window'] == target
        wait(lambda s: s['active_class'] == 'org.arinn.Shell')
    else:
        run(str(ROOT / 'target/debug/arinnctl'), 'home')
        wait(lambda s: s['home_visible'] and s['return_window'] == target)
    run(str(ROOT / 'target/debug/arinnctl'), 'back')
    wait(lambda s: not s['home_visible'] and s['active_window'] == target)
print(json.dumps({'daemon_crash_home_hidden': True, 'daemon_crash_home_visible': True,
                  'adapter_reloaded': False, 'activity_preserved': True,
                  'home_and_return_after_recovery': True}, indent=2))
