#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Render a selected disposable window through KWin thumbnail + near/far shaders.
Explicit operator test; writes only into ignored .phase0, never exports to network.
"""
import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]

def call(*args):
    return subprocess.check_output(['qdbus6', *args], text=True, timeout=8).strip()

state = json.loads(subprocess.check_output([str(ROOT/'target/debug/arinnctl'), '--json', 'status']))
assert state['active_window'] and not state['home_visible'], state
assert call('org.freedesktop.ScreenSaver', '/ScreenSaver', 'org.freedesktop.ScreenSaver.GetActive') == 'false'
output = ROOT / '.phase0/background-probe'
output.mkdir(mode=0o700, parents=True, exist_ok=True)
output.chmod(0o700)
for name in ('raw', 'near', 'far', 'raw-later'):
    (output / (name + '.png')).unlink(missing_ok=True)
source = (ROOT/'platform/kde/background-probe.qml.in').read_text()
source = source.replace('@OUTPUT_DIR@', json.dumps(str(output))).replace('@WINDOW_ID@', json.dumps(state['active_window']))
name = 'arinn-phase0-background-probe'
with tempfile.TemporaryDirectory(prefix='arinn-background-') as folder:
    file = Path(folder)/'main.qml'
    file.write_text(source)
    shutil.copy(ROOT/"apps/arinn-shell/ActivityBackground.qml", Path(folder)/"ActivityBackground.qml")
    try:
        i = call('org.kde.KWin', '/Scripting', 'org.kde.kwin.Scripting.loadDeclarativeScript', str(file), name)
        call('org.kde.KWin', f'/Scripting/Script{i}', 'org.kde.kwin.Script.run')
        started = time.monotonic()
        deadline = started + 10
        while time.monotonic() < deadline:
            if all((output/(n+'.png')).exists() for n in ('raw', 'near', 'far', 'raw-later')):
                break
            time.sleep(.1)
        else:
            raise AssertionError('KWin background frames were not produced')
        # A PNG file alone is not proof: inspect pixel content before accepting.
        print(json.dumps({'output': str(output), 'target_class': state['active_class'], 'output_name': state['active_output'], 'frames_written': 4, 'visual_verification_required': True}, indent=2))
    finally:
        call('org.kde.KWin', '/Scripting', 'org.kde.kwin.Scripting.unloadScript', name)
