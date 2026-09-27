#!/usr/bin/env python3
"""Explicit operator-only live test helper: focus an exact application class."""
import json,subprocess,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
if len(sys.argv)<2:raise SystemExit('usage: kwin_focus.py RESOURCE_CLASS [--fullscreen] [--output OUTPUT_NAME]')
klass=sys.argv[1];full='--fullscreen' in sys.argv
output=sys.argv[sys.argv.index('--output')+1] if '--output' in sys.argv else None
script=ROOT/'.phase0/focus-fixture.js'
script.write_text('const w=workspace.windowList().find(w=>String(w.resourceClass)==='+json.dumps(klass)+');\nif(w){w.minimized=false;'+('w.fullScreen=true;' if full else '')+(('const o=workspace.screens.find(o=>o.name==='+json.dumps(output)+');if(o)workspace.sendClientToScreen(w,o);') if output else '')+'workspace.activeWindow=w;}')
def call(*args):return subprocess.check_output(['qdbus6',*args],text=True).strip()
name='arinn-phase0-test-focus'
try:
    i=call('org.kde.KWin','/Scripting','org.kde.kwin.Scripting.loadScript',str(script),name)
    call('org.kde.KWin',f'/Scripting/Script{i}','org.kde.kwin.Script.run')
    time.sleep(.2)
finally:call('org.kde.KWin','/Scripting','org.kde.kwin.Scripting.unloadScript',name)
