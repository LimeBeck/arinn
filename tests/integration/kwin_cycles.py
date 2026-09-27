#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Live test: current foreground external window -> global Home -> Return, 20 times.
Requires running phase0-session. Does not choose or launch the external application.
"""
import json,subprocess,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def run(*args):return subprocess.run(args,check=True,capture_output=True,text=True,timeout=5).stdout
def status():return json.loads(run(str(ROOT/'target/debug/arinnctl'),'status'))
def wait(test):
    end=time.monotonic()+4
    while time.monotonic()<end:
        state=status()
        if test(state):return state
        time.sleep(.04)
    raise AssertionError(state)
initial=status();target=initial['active_window'];klass=initial['active_class']
assert target and klass not in ('org.arinn.Shell','arinn-shell'),initial
latencies=[]
for i in range(20):
    t=time.monotonic()
    run('qdbus6','org.kde.kglobalaccel','/component/kwin','org.kde.kglobalaccel.Component.invokeShortcut','ArinnPhase0Home')
    state=wait(lambda s:s['home_visible'] and s['active_class'] in ('org.arinn.Shell','arinn-shell') and s['active_output']==s['home_output']==initial['active_output'])
    assert state['return_window']==target,state
    assert state['active_output']==initial['active_output']==state['home_output'],state
    run(str(ROOT/'target/debug/arinnctl'),'home') # repeat must preserve target
    assert wait(lambda s:s['home_visible'])['return_window']==target
    run(str(ROOT/'target/debug/arinnctl'),'back')
    state=wait(lambda s:not s['home_visible'] and s['active_window']==target)
    assert state['active_output']==initial['active_output'],state
    latencies.append(round((time.monotonic()-t)*1000,1))
report={'application_class':klass,'target_window':target,'initial_fullscreen':initial['active_fullscreen'],'output':initial['active_output'],'cycles':20,'same_window_restored':True,'cycle_ms':latencies,'input':'KGlobalAccel invokeShortcut, not physical remote'}
print(json.dumps(report,indent=2))
