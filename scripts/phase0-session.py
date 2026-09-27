#!/usr/bin/env python3
"""Explicitly start/stop the temporary live KWin experiment; no autostart installation."""
import os,secrets,subprocess,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
RUN=ROOT/'.phase0'
UNITS=['arinn-phase0-shell','arinn-phase0-demo','arinn-phase0-daemon']
NAME='arinn-phase0'
def run(*args,check=True):return subprocess.run(args,check=check,capture_output=True,text=True)
def dbus(*args,check=True):return run('qdbus6',*args,check=check)
def stop():
    # Restore previous activity if possible before removing the adapter.
    run(str(ROOT/'target/debug/arinnctl'),'back',check=False)
    time.sleep(.15)
    dbus('org.kde.KWin','/Scripting','org.kde.kwin.Scripting.unloadScript',NAME,check=False)
    run('systemctl','--user','stop',*UNITS,check=False)
def start():
    if run('systemctl','--user','is-active','--quiet','arinn-phase0-daemon',check=False).returncode==0:
        raise SystemExit('Phase 0 is already running. Use stop before start.')
    if dbus('org.kde.KWin','/Scripting','org.kde.kwin.Scripting.isScriptLoaded',NAME).stdout.strip()=='true':
        raise SystemExit('KWin adapter already loaded; use stop first.')
    RUN.mkdir(mode=0o700,exist_ok=True);RUN.chmod(0o700)
    token=RUN/'token';token.write_text(secrets.token_hex(32));token.chmod(0o600)
    def unit(name,binary,*env):
        run('systemd-run','--user','--collect',f'--unit={name}',
            '--property=Type=exec','--property=Restart=on-failure','--property=RestartSec=2s',
            '--property=StartLimitIntervalSec=60s','--property=StartLimitBurst=5',
            f'--setenv=ARINN_SPIKE_TOKEN_FILE={token}',*env,str(ROOT/binary))
    try:
        unit('arinn-phase0-daemon','target/debug/arinnd')
        for _ in range(40):
            if run(str(ROOT/'target/debug/arinnctl'),'status',check=False).returncode==0:break
            time.sleep(.1)
        else:raise RuntimeError('daemon startup timed out')
        unit('arinn-phase0-demo','target/debug/arinn-plugin-demo')
        unit('arinn-phase0-shell','build/apps/arinn-shell/arinn-shell','--setenv=QT_QPA_PLATFORM=wayland',f'--setenv=WAYLAND_DISPLAY={os.environ.get("WAYLAND_DISPLAY","wayland-0")}')
        script_id=dbus('org.kde.KWin','/Scripting','org.kde.kwin.Scripting.loadScript',str(ROOT/'platform/kde/phase0.js'),NAME).stdout.strip()
        if not script_id.isdigit():raise RuntimeError('KWin refused script: '+script_id)
        dbus('org.kde.KWin',f'/Scripting/Script{script_id}','org.kde.kwin.Script.run')
        print('Phase 0 running. Meta+F12: Home; Esc: return. Stop: python scripts/phase0-session.py stop')
    except BaseException:stop();raise
if __name__=='__main__':
    if sys.argv[1:]==['start']:start()
    elif sys.argv[1:]==['stop']:stop();print('Phase 0 stopped; temporary KWin adapter unloaded.')
    else:raise SystemExit('usage: phase0-session.py start|stop')
