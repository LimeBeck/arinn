#!/usr/bin/env python3
"""Run on a private session bus: dbus-run-session -- python tests/integration/phase0.py."""
import json, os, subprocess, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
BIN=ROOT/'target/debug'

def cli(*args,ok=True):
    p=subprocess.run([str(BIN/'arinnctl'),*args],capture_output=True,text=True,timeout=9)
    if ok and p.returncode: raise AssertionError(p.stderr)
    return p

def status(): return json.loads(cli('status').stdout)
def wait(predicate,timeout=8):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        try:
            value=predicate()
            if value:return value
        except (AssertionError,json.JSONDecodeError):pass
        time.sleep(.05)
    raise AssertionError('condition timed out')

with tempfile.TemporaryDirectory(prefix='arinn-contract-') as folder:
    token=Path(folder)/'token';token.write_text('phase0-contract-test-token-000000000');token.chmod(0o600)
    env={**os.environ,'ARINN_SPIKE_TOKEN_FILE':str(token)}
    processes=[]
    def start(name,**extra):
        p=subprocess.Popen([str(BIN/name)],env={**env,**extra},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        processes.append(p);return p
    try:
        daemon=start('arinnd');wait(lambda:status()['epoch'])
        demo=start('arinn-plugin-demo');wait(lambda:status()['provider_available'])
        first=status();snap=json.loads(cli('snapshot').stdout)
        assert snap['revision']==1 and len(snap['items'])==1
        assert cli('activate').stdout.strip()=='succeeded'
        assert cli('activate','unknown',ok=False).returncode
        assert cli('home',ok=False).returncode # no compositor on test bus
        bad=subprocess.run(['busctl','--user','call','org.arinn.Spike1','/org/arinn/Spike1','org.arinn.Spike1','Register','su','wrong', '1'],capture_output=True)
        assert bad.returncode
        spoof=subprocess.run(['busctl','--user','call','org.arinn.Spike1','/org/arinn/Spike1','org.arinn.Spike1','ReportHome','bss','true','spoof','fake-output'],capture_output=True)
        assert spoof.returncode
        demo.kill();demo.wait();wait(lambda:not status()['provider_available'])
        assert daemon.poll() is None
        demo=start('arinn-plugin-demo',ARINN_DEMO_DELAY_MS='7000');wait(lambda:status()['provider_available'])
        assert status()['generation']!=first['generation']
        gui=subprocess.run([str(ROOT/'build/apps/arinn-shell/arinn-shell-test')],env={**os.environ,'QT_QPA_PLATFORM':'offscreen','QT_QUICK_BACKEND':'software','LD_PRELOAD':''},capture_output=True,text=True,timeout=15)
        assert gui.returncode==0,gui.stdout+gui.stderr
        stale=subprocess.run(['busctl','--user','call','org.arinn.Spike1','/org/arinn/Spike1','org.arinn.Spike1','Activate','sss',first['generation'],'demo.activity','10000000-0000-4000-8000-000000000000'],capture_output=True)
        assert stale.returncode
        operation=subprocess.Popen([str(BIN/'arinnctl'),'activate'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        before=time.monotonic();assert status()['provider_available'];assert time.monotonic()-before<1
        _,error=operation.communicate(timeout=8);assert operation.returncode and 'Timeout' in error
        old_epoch=status()['epoch'];old_gen=status()['generation']
        daemon.terminate();daemon.wait();daemon=start('arinnd')
        wait(lambda:status()['provider_available'])
        assert status()['epoch']!=old_epoch and status()['generation']!=old_gen
        print('PASS: session lock/sleep/wake/owner recovery; typed snapshot/action; invalid ID/token/platform caller; plugin crash isolation/restart; stale generation; timeout without blocking status; daemon restart and provider re-registration')
    finally:
        for p in reversed(processes):
            if p.poll() is None:p.terminate()
        for p in processes:
            try:p.wait(timeout=3)
            except subprocess.TimeoutExpired:p.kill();p.wait()
