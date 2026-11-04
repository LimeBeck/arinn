// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");
const crypto = require("node:crypto");
const source = fs.readFileSync(path.join(__dirname,"../../plugins/chrome/extension/authorization.js"),"utf8");
const BASE="chrome-extension://fixture/authorization.html";
const KEY="arinn.browserAuth.windows";
function deferred() {let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
function fixture() {
    const windows=new Map([[1,{id:1,type:"normal",tabs:[{id:10,url:"https://ordinary.invalid/"}]}]]);
    const saved={};const calls=[];let next=2;let createGate=null;let closeFailures=0;let clock=0;let readFailures=0;let tabFailures=0;let writeGate=null;let activeContext;const posted=[];
    const setWindow=windows.set.bind(windows);windows.set=(id,value)=>{next=Math.max(next,id+1);return setWindow(id,value);};
    const clone=x=>JSON.parse(JSON.stringify(x));
    const chrome={runtime:{getURL:()=>BASE},storage:{session:{
        async get(key){if(readFailures){readFailures--;throw new Error("Unavailable");}return clone({[key]:saved[key]||[]});},
        async set(value){if(writeGate)await writeGate.promise;Object.assign(saved,clone(value));},async remove(key){delete saved[key];},
    }},windows:{
        async getAll(options){return clone(Array.from(windows.values()).filter(w=>(options?.windowTypes||["popup"]).includes(w.type)));},
        async get(id){if(!windows.has(id))throw new Error("No window with id");return clone(windows.get(id));},
        async create(options){
            calls.push(["create",clone(options)]);if(createGate)await createGate.promise;
            while(windows.has(next))next++;const id=next++;
            let tab={id:id*10,url:options.url};
            if(options.tabId!==undefined){
                let original;
                for(const w of windows.values()){const i=w.tabs.findIndex(t=>t.id===options.tabId);if(i>=0){original=w.tabs.splice(i,1)[0];break;}}
                if(!original)throw new Error("No tab with id");tab=original;
            }
            const window={id,type:options.type,focused:options.focused,tabs:[tab]};windows.set(id,window);return clone(window);
        },
        async update(id,options){calls.push(["focus",id,clone(options)]);if(!windows.has(id))throw new Error("No window with id");Object.assign(windows.get(id),options);},
        async remove(id){calls.push(["remove",id]);if(closeFailures){closeFailures--;throw new Error("Unavailable");}if(!windows.delete(id))throw new Error("No window with id");},
    },tabs:{async remove(id){calls.push(["removeTab",id]);if(tabFailures){tabFailures--;throw new Error("Unavailable");}for(const w of windows.values()){const index=w.tabs.findIndex(t=>t.id===id);if(index>=0){w.tabs.splice(index,1);if(!w.tabs.length)windows.delete(w.id);return;}}throw new Error("No tab with id");},async update(id,options){calls.push(["navigate",id,clone(options)]);for(const w of windows.values()){const t=w.tabs.find(t=>t.id===id);if(t){Object.assign(t,options);return;}}throw new Error("Not found");}}};
    function load(){const context=vm.createContext({chrome,URL,URLSearchParams,performance:{now:()=>clock}});vm.runInContext(source,context);activeContext=context;return context.ArinnAuthorization;}
    function background(){
        const event=()=>({listeners:[],addListener(f){this.listeners.push(f);},fire(...args){for(const f of this.listeners)f(...args);}});
        for(const name of ["onCreated","onRemoved","onUpdated","onActivated","onMoved","onAttached","onDetached","onReplaced"])chrome.tabs[name]=event();
        for(const name of ["onCreated","onRemoved","onFocusChanged","onBoundsChanged"])chrome.windows[name]=event();
        chrome.runtime.onStartup=event();chrome.runtime.connectNative=()=>({onMessage:event(),onDisconnect:event(),postMessage:m=>posted.push(m)});
        chrome.tabs.get=async id=>{for(const w of windows.values()){const tab=w.tabs.find(t=>t.id===id);if(tab)return {...tab,windowId:w.id};}throw new Error("No tab with id");};
        activeContext.setTimeout=()=>1;activeContext.clearTimeout=()=>{};
        vm.runInContext(fs.readFileSync(path.join(__dirname,"../../plugins/chrome/extension/background.js"),"utf8"),activeContext);
        return activeContext;
    }
    return {windows,saved,calls,posted,load,background,failTabClose:()=>{tabFailures++;},failReads:n=>{readFailures=n;},parkWrites:g=>{writeGate=g;},setGate:g=>{createGate=g;},failClose:()=>{closeFailures++;},advance:ms=>{clock+=ms;}};
}
const open=(id,url="https://oauth.fixture.invalid/device")=>({action:"openTemporary",requestId:id,url});
const close=id=>({action:"closeTemporary",requestId:id,url:""});
async function until(predicate){for(let i=0;i<100;i++){if(predicate())return;await new Promise(r=>setImmediate(r));}throw new Error("fixture did not progress");}

test("each lease creates its own window and closure cannot remove an ordinary window",async()=>{
    const f=fixture(),a=f.load(),port={};await a.ready();
    const one=crypto.randomUUID(),two=crypto.randomUUID();
    const x=await a.execute(open(one),port),y=await a.execute(open(two),port);
    assert.notEqual(x.windowId,y.windowId);assert.equal(f.calls.filter(c=>c[0]==="create").length,2);
    assert.ok(f.calls.filter(c=>c[0]==="create").every(c=>c[1].focused===false && c[1].url.startsWith(BASE+"#")));
    assert.ok(!JSON.stringify(f.saved).includes("oauth.fixture"));
    await a.execute(close(one),port);await a.execute(close(one),port);
    assert.ok(f.windows.has(1));assert.ok(f.windows.has(y.windowId));assert.ok(!f.windows.has(x.windowId));
});
test("close before dispatch leaves a tombstone and cannot turn into a late open",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();
    await a.execute(close(id),port);await assert.rejects(a.execute(open(id),port),/Conflict/);
    assert.equal(f.calls.filter(c=>c[0]==="create").length,0);
});
test("lost or timed-out create acknowledgement stays owned until late creation is closed",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID(),gate=deferred();await a.ready();f.setGate(gate);
    const creation=a.execute(open(id),port);const rejected=assert.rejects(creation,/Conflict/);
    await until(()=>f.calls.some(c=>c[0]==="create"));
    let ended=false;const cleanup=a.execute(close(id),port).then(()=>{ended=true;});
    await new Promise(r=>setImmediate(r));assert.equal(ended,false);assert.equal(f.calls.filter(c=>c[0]==="focus").length,0);
    gate.resolve();await rejected;await cleanup;
    assert.equal(f.windows.size,1);assert.ok(f.windows.has(1));assert.equal(f.calls.filter(c=>c[0]==="focus").length,0);
});
test("duplicate lease replay is idempotent and a different payload conflicts",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();
    const one=await a.execute(open(id),port);const two=await a.execute(open(id),port);assert.equal(one.windowId,two.windowId);
    await assert.rejects(a.execute(open(id,"https://different.invalid/device"),port),/Conflict/);
    assert.equal(f.calls.filter(c=>c[0]==="create").length,1);
});
test("a failed Close retains ownership and retry closes the same window",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();const window=await a.execute(open(id),port);
    f.failClose();await assert.rejects(a.execute(close(id),port),/Unavailable/);assert.ok(f.windows.has(window.windowId));
    assert.equal(f.saved[KEY][0].windowId,window.windowId);
    await a.execute(close(id),port);assert.equal(f.windows.size,1);assert.equal(f.saved[KEY].length,0);
});
test("port replacement revokes a pending create before it can focus and does not target the new port",async()=>{
    const f=fixture(),a=f.load(),old={},fresh={},id=crypto.randomUUID(),gate=deferred();await a.ready();f.setGate(gate);
    const creating=a.execute(open(id),old);const rejected=assert.rejects(creating,/Conflict/);
    await until(()=>f.calls.some(c=>c[0]==="create"));a.disconnect(old);gate.resolve();await rejected;
    await until(()=>f.windows.size===1);f.setGate(null);
    const current=await a.execute(open(id),fresh);await a.execute(close(id),old);
    assert.ok(f.windows.has(current.windowId));assert.ok(f.windows.has(1));
});
test("worker restart reaps recorded handles and leaves unrelated windows",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();const created=await a.execute(open(id),port);
    const restarted=f.load();await restarted.ready();assert.ok(!f.windows.has(created.windowId));assert.ok(f.windows.has(1));
    assert.equal(f.saved[KEY],undefined);
});
test("worker restart also reaps an unrecorded unfocused placeholder",async()=>{
    const f=fixture();f.windows.set(2,{id:2,type:"popup",focused:false,tabs:[{id:20,url:BASE+"#"+crypto.randomUUID()}]});
    const a=f.load();await a.ready();assert.equal(f.windows.size,1);assert.ok(f.windows.has(1));
});
test("persisted handle proof never closes a different tab/window after replacement",async()=>{
    const f=fixture();f.saved[KEY]=[{windowId:2,tabId:20}];f.windows.set(2,{id:2,type:"popup",tabs:[{id:21,url:"https://ordinary.invalid/"}]});
    const a=f.load();await a.ready();assert.ok(f.windows.has(2));
});
test("callback credentials in either current or pending URL also redact a stale title",async()=>{
    const f=fixture(),a=f.load();await a.ready();
    for(const tab of [
        {url:"http://127.0.0.1:17845/oauth/callback?code=synthetic&state=fixture",title:"synthetic"},
        {url:"https://ordinary.invalid/watch?v=1",pendingUrl:"https://login.fixture.invalid/callback#access_token=synthetic",title:"synthetic"},
        {url:"https://login.fixture.invalid/callback?%63ode=synthetic&state=fixture",title:"synthetic"},
    ]) {const safe=a.redact(tab);assert.equal(safe.title,"Sign-in");assert.ok(!safe.url.includes("synthetic") && !safe.url.includes("?") && !safe.url.includes("#"));}
    const safe=a.redact({url:"https://ordinary.invalid/watch?v=1",title:"Video"});assert.equal(safe.url,"https://ordinary.invalid/watch?v=1");assert.equal(safe.title,"Video");
});
test("only HTTPS authorization pages and valid leases can create a window",async()=>{
    const f=fixture(),a=f.load(),port={};await a.ready();
    for(const url of ["http://fixture.invalid/","file:///fixture","https://user:password@fixture.invalid/","https://fixture.invalid/space here"])
        await assert.rejects(a.execute(open(crypto.randomUUID(),url),port),/InvalidArgument/);
    await assert.rejects(a.execute(open("invalid"),port),/InvalidArgument/);assert.equal(f.calls.length,0);
});
test("retained cancellation identities are bounded and expire after the dispatch lifetime",async()=>{
    const f=fixture(),a=f.load(),port={};await a.ready();
    for(let i=0;i<128;i++)await a.execute(close(crypto.randomUUID()),port);
    await assert.rejects(a.execute(close(crypto.randomUUID()),port),/Unavailable/);
    f.advance(180001);await a.execute(open(crypto.randomUUID()),port);assert.equal(f.windows.size,2);
});

test("startup cleanup retries after a transient close failure without reloading the worker",async()=>{
    const f=fixture();f.saved[KEY]=[{windowId:2,tabId:20}];f.windows.set(2,{id:2,type:"popup",tabs:[{id:20,url:"https://oauth.fixture.invalid/device"}]});f.failClose();
    const a=f.load();assert.equal(await a.ready(),false);assert.ok(f.saved[KEY]);assert.equal(a.owns(f.windows.get(2)),true);assert.equal(a.owns(f.windows.get(1)),true);
    assert.equal(await a.ready(),true);assert.ok(!f.windows.has(2));await a.execute(open(crypto.randomUUID()),{});
});
test("unknown startup ownership suppresses snapshots until auth storage recovers",async()=>{
    const f=fixture();f.failReads(10);const a=f.load();const bg=f.background();await until(()=>f.posted.some(m=>m.type==="snapshot"));
    const snapshot=f.posted.find(m=>m.type==="snapshot");assert.equal(snapshot.tabs.length,0);
    f.failReads(0);assert.equal(await a.ready(),true);await bg.sendSnapshot();
    assert.equal(f.posted.filter(m=>m.type==="snapshot").at(-1).tabs[0].url,"https://ordinary.invalid/");
});

test("a placeholder appearing after startup enumeration is reaped by the next window event",async()=>{
    const f=fixture(),a=f.load();await a.ready();const active=await a.execute(open(crypto.randomUUID()),{});
    f.windows.set(5,{id:5,type:"popup",tabs:[{id:50,url:BASE+"#"+crypto.randomUUID()}]});
    const bg=f.background();await bg.sendSnapshot();assert.ok(!f.windows.has(5));assert.ok(f.windows.has(active.windowId));
});
test("generic fullscreen handling cannot focus a placeholder before its ownership is persisted",async()=>{
    const f=fixture(),a=f.load();await a.ready();const bg=f.background(),gate=deferred();f.parkWrites(gate);
    const pending=a.execute(open(crypto.randomUUID()),{});await until(()=>f.windows.size===2);
    const temporary=Array.from(f.windows.values()).find(w=>w.id!==1);await bg.keepFullscreen({...temporary,state:"normal"});
    assert.equal(f.calls.filter(c=>c[0]==="focus").length,0);f.parkWrites(null);gate.resolve();await pending;
});
test("auth child tabs remain private, are adopted without ordinary-window reuse, and close with the lease",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();const root=await a.execute(open(id),port);const bg=f.background();
    f.windows.set(3,{id:3,type:"normal",tabs:[{id:30,url:"https://auth.fixture.invalid/child",title:"Private"},{id:31,url:"https://ordinary.invalid/other",title:"Ordinary"}]});
    await bg.adoptOpened({id:30,windowId:3,openerTabId:root.windowId*10});
    assert.equal(a.ownsTab({id:30}),true);await bg.sendSnapshot();
    const snapshot=f.posted.filter(m=>m.type==="snapshot").at(-1);assert.ok(snapshot.tabs.some(t=>t.id===31));assert.ok(!snapshot.tabs.some(t=>t.id===30));
    assert.ok(f.calls.filter(c=>c[0]==="create").every(c=>c[1].focused===false));
    await a.execute(close(id),port);assert.ok(f.windows.has(1));assert.ok(f.windows.has(3));assert.deepEqual(f.windows.get(3).tabs.map(t=>t.id),[31]);
});
test("cancel during child adoption waits for persistence and never focuses a late child",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();const root=await a.execute(open(id),port);const bg=f.background();const priorFocus=f.calls.filter(c=>c[0]==="focus").length;
    f.windows.set(3,{id:3,type:"popup",tabs:[{id:30,url:"https://auth.fixture.invalid/child"}]});
    const gate=deferred();f.parkWrites(gate);const child=bg.adoptOpened({id:30,windowId:3,openerTabId:root.windowId*10});
    await until(()=>a.ownsTab({id:30}));let ended=false;const pending=a.execute(close(id),port).then(()=>{ended=true;});
    await new Promise(r=>setImmediate(r));assert.equal(ended,false);await bg.keepFullscreen({...f.windows.get(3),state:"normal"});assert.equal(f.calls.filter(c=>c[0]==="focus").length,priorFocus);
    f.parkWrites(null);gate.resolve();await child;await pending;assert.equal(f.windows.size,1);assert.equal(f.calls.filter(c=>c[0]==="focus").length,priorFocus);
});

test("a new port retries failed child cleanup even after the parent window is gone",async()=>{
    const f=fixture(),a=f.load(),old={},fresh={},id=crypto.randomUUID();await a.ready();const root=await a.execute(open(id),old);const bg=f.background();
    f.windows.set(3,{id:3,type:"popup",tabs:[{id:30,url:"https://auth.fixture.invalid/child"}]});
    await bg.adoptOpened({id:30,windowId:3,openerTabId:root.windowId*10});f.failTabClose();a.disconnect(old);
    await until(()=>!f.windows.has(root.windowId));assert.ok(f.windows.has(3));
    const current=await a.execute(open(crypto.randomUUID()),fresh);assert.ok(!f.windows.has(3));assert.ok(f.windows.has(current.windowId));assert.ok(f.windows.has(1));
});

test("aged leases retain failed child cleanup proof when another lease is admitted",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();const root=await a.execute(open(id),port);
    f.windows.set(3,{id:3,type:"popup",tabs:[{id:30,url:"https://auth.fixture.invalid/child"}]});
    await a.adoptOpened({id:30,windowId:3,openerTabId:root.windowId*10});f.advance(180001);f.failTabClose();
    await assert.rejects(a.execute(close(id),port),/Unavailable/);
    await a.execute(open(crypto.randomUUID()),port);assert.equal(a.ownsTab({id:30}),true);
    assert.ok(f.saved[KEY].some(r=>r.tabOnly && r.tabId===30));await a.execute(close(id),port);assert.ok(!f.windows.has(3));
});

test("overflow revokes the lease and retains private retry proof when removal fails",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();const root=await a.execute(open(id),port);
    for(let i=0;i<15;i++){
        const windowId=100+i,tabId=1000+i;f.windows.set(windowId,{id:windowId,type:"popup",tabs:[{id:tabId,url:"https://auth.fixture.invalid/child"}]});
        await a.adoptOpened({id:tabId,windowId,openerTabId:root.windowId*10});
    }
    const focus=f.calls.filter(c=>c[0]==="focus").length;
    f.windows.set(200,{id:200,type:"popup",tabs:[{id:2000,url:"https://auth.fixture.invalid/overflow"}]});f.failTabClose();
    await a.adoptOpened({id:2000,windowId:200,openerTabId:root.windowId*10});
    assert.equal(a.ownsTab({id:2000}),true);assert.ok(f.saved[KEY].some(r=>r.tabId===2000));
    assert.equal(f.calls.filter(c=>c[0]==="focus").length,focus);
    f.windows.set(201,{id:201,type:"popup",tabs:[{id:2010,url:"https://auth.fixture.invalid/late"}]});f.failTabClose();
    await a.adoptOpened({id:2010,windowId:201,openerTabId:2000});assert.equal(a.ownsTab({id:2010}),true);
    assert.ok(f.saved[KEY].some(r=>r.tabId===2010));await a.ready();await a.execute(close(id),port);
    const next=await a.execute(open(crypto.randomUUID()),port);assert.ok(f.windows.has(next.windowId));assert.ok(f.windows.has(1));
});

test("exhausted emergency cleanup budget suppresses metadata and survives worker restart",async()=>{
    const f=fixture(),a=f.load(),port={},id=crypto.randomUUID();await a.ready();const root=await a.execute(open(id),port);await a.execute(close(id),port);
    for(let i=0;i<129;i++){
        const windowId=300+i,tabId=3000+i;f.windows.set(windowId,{id:windowId,type:"popup",tabs:[{id:tabId,url:"https://auth.fixture.invalid/late"}]});f.failTabClose();
        await a.adoptOpened({id:tabId,windowId,openerTabId:root.windowId*10});
    }
    assert.equal(a.owns(f.windows.get(1)),true);assert.ok(f.saved[KEY].length<=128);assert.equal(f.saved[KEY+".blocked"],true);
    await assert.rejects(a.execute(open(crypto.randomUUID()),port),/Unavailable/);
    await a.execute(close(id),port);const restarted=f.load();await restarted.ready();assert.equal(restarted.owns(f.windows.get(1)),true);
    await assert.rejects(restarted.execute(open(crypto.randomUUID()),{}),/Unavailable/);
});

test("worker restart recovers the maximum admitted proof budget",async()=>{
    const f=fixture();f.saved[KEY]=Array.from({length:128*17+128},(_,i)=>({tabId:10000+i,tabOnly:true}));
    const a=f.load();assert.equal(await a.ready(),true);assert.equal(f.saved[KEY],undefined);assert.equal(a.owns(f.windows.get(1)),false);
});
test("worker restart discovers unpersisted normal-window children before deleting their opener",async()=>{
    const f=fixture();f.saved[KEY]=[{windowId:2,tabId:20}];
    f.windows.set(2,{id:2,type:"popup",tabs:[{id:20,url:"https://auth.fixture.invalid/device"}]});
    f.windows.get(1).tabs.push({id:11,openerTabId:20,url:"https://auth.fixture.invalid/child"},{id:12,openerTabId:11,url:"https://auth.fixture.invalid/grandchild"});
    const a=f.load();assert.equal(await a.ready(),true);assert.deepEqual(f.windows.get(1).tabs.map(t=>t.id),[10]);assert.ok(!f.windows.has(2));
});

test("oversize recovery proof fails closed for normal windows as well",async()=>{
    const f=fixture();f.saved[KEY]=Array.from({length:128*17+129},(_,i)=>({tabId:10000+i,tabOnly:true}));
    const a=f.load();assert.equal(await a.ready(),false);assert.equal(a.owns(f.windows.get(1)),true);
    assert.equal(f.saved[KEY+".blocked"],true);
});
