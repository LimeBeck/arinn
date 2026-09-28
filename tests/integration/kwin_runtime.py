#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Operator-only: restart/crash/reconfigure the isolated live demo; restore its manifest.
Requires explicit permission and scripts/phase0-session.py start.
"""
import json, subprocess, time
from pathlib import Path
root=Path(__file__).resolve().parents[2]
cli=str(root/'target/debug/arinnctl')
def call(*args):
    return subprocess.check_output(args,text=True,timeout=15)
def providers():return json.loads(call(cli,'--json','providers'))
def provider():return next(p for p in providers() if p['id']=='demo')
def wait(predicate):
    until=time.monotonic()+18
    while time.monotonic()<until:
        p=provider()
        if predicate(p):return p
        time.sleep(.1)
    raise AssertionError({k:p[k] for k in ('runtime_state','last_error','revision')})
p=provider();path=p['path'];generation=p['generation']
unit='arinn-provider-'+path.rsplit('p_',1)[1]+'.service'
call(cli,'provider','restart','demo')
p=wait(lambda p:p['runtime_state']=='ready' and p['generation']!=generation)
assert p['path']==path
generation=p['generation']
call('systemctl','--user','kill','--signal=SIGKILL',unit)
p=wait(lambda p:p['runtime_state']=='ready' and p['generation']!=generation)
wait(lambda p:p['restarts_since_daemon_start']>=2)
manifest=root/'.phase0/data/arinn/plugins/demo.toml'
original=manifest.read_text()
try:
    generation=p['generation']
    manifest.write_text(original+'args = ["--fixture", "empty"]\n')
    call(cli,'providers','reload')
    p=wait(lambda p:p['runtime_state']=='ready' and p['generation']!=generation)
    assert not json.loads(call(cli,'--json','snapshot','demo'))[0]['applications']
finally:
    generation=provider()['generation']
    manifest.write_text(original)
    call(cli,'providers','reload')
    wait(lambda p:p['runtime_state']=='ready' and p['generation']!=generation)
assert json.loads(call(cli,'--json','snapshot','demo'))[0]['applications']
report={'real_systemd':True,'manual_restart':True,'crash_auto_restart':True,'restart_counters':True,'argv_reload':True,'restored_populated_demo':True,'stable_provider_path':True}
(root/'.phase0/phase1-runtime.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
