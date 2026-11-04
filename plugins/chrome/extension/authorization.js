// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";
// Only opaque leases/window IDs survive in storage.session. OAuth URLs, codes,
// application secrets and user tokens are never persisted or exported here.
globalThis.ArinnAuthorization = (() => {
    const KEY = "arinn.browserAuth.windows";
    const MAX_HANDLES = 128 * 17 + 128;
    const placeholder = chrome.runtime.getURL("authorization.html");
    const contexts = new WeakMap();
    const leases = new Map();
    let nextContext = 0;
    let write = Promise.resolve();
    const now = () => performance.now();
    const missing = error => /no window with id|no tab with id|invalid window id|invalid tab id|not found/i.test(String(error && error.message));
    function context(port) {
        let item = contexts.get(port);
        if (!item) { item = { id: ++nextContext, closed: false }; contexts.set(port, item); }
        return item;
    }
    function proof(window, record) {
        return window.type === "popup" && window.id === record.windowId
            && (window.tabs || []).some(tab => tab.id === record.tabId);
    }
    const revokedTabs = new Map();
    const orphanTabs = new Set();
    const BLOCKED = KEY + ".blocked";
    let blocked = false;
    let initialized = false, uncertain = true, recovering = null;
    function handles(record) {
        const result = Number.isInteger(record.windowId) ? [{windowId: record.windowId, tabId: record.tabId}] : [];
        for (const tabId of record.childTabs || []) result.push({tabId, tabOnly: true});
        return result;
    }
    async function removeTab(id) {
        try { await chrome.tabs.remove(id); } catch (error) { if (!missing(error)) throw error; }
    }
    async function remove(record) {
        if (Number.isInteger(record.windowId)) {
            try {
                const window = await chrome.windows.get(record.windowId, { populate: true });
                if (!proof(window, record)) throw new Error("Conflict");
                await chrome.windows.remove(record.windowId);
            } catch (error) { if (!missing(error)) throw error; }
            record.windowId = undefined; record.tabId = undefined;
        }
        for (const id of record.childTabs || []) { await removeTab(id); record.childTabs.delete(id); }
    }
    function persist() {
        const commit = async () => {
            const saved = Array.from(leases.values()).flatMap(handles).concat(Array.from(orphanTabs, tabId => ({tabId, tabOnly: true})));
            await chrome.storage.session.set({ [KEY]: saved });
        };
        write = write.then(commit, commit);
        return write;
    }
    function marker(window) {
        const tab = (window.tabs || []).find(t => (t.url || t.pendingUrl || "").startsWith(placeholder + "#"));
        return tab ? (tab.url || tab.pendingUrl).slice(placeholder.length + 1) : "";
    }
    async function reconcile() {
        let stored = [];
        if (!initialized) {
            blocked = (await chrome.storage.session.get(BLOCKED))[BLOCKED] === true;
            stored = (await chrome.storage.session.get(KEY))[KEY] || [];
            if (!Array.isArray(stored) || stored.length > MAX_HANDLES || stored.some(r => !Number.isInteger(r.tabId))) {
                blocked = true; await chrome.storage.session.set({[BLOCKED]: true});
                throw new Error("Unavailable");
            }
            for (const record of stored) if (Number.isInteger(record.tabId)) revokedTabs.set(record.tabId, now());
        }
        const windows = await chrome.windows.getAll({ populate: true, windowTypes: ["normal", "popup"] });
        if (!initialized) {
            // Discover descendants before deleting any opener. A worker may have
            // died between onCreated and persistence of a normal-window child.
            const known = new Set(stored.map(r => r.tabId));
            const tabs = windows.flatMap(w => w.tabs || []);
            let changed;
            do {
                changed = false;
                for (const tab of tabs) if (!known.has(tab.id) && known.has(tab.openerTabId)) {
                    if (stored.length >= MAX_HANDLES) {
                        blocked = true; await chrome.storage.session.set({[BLOCKED]: true});
                        throw new Error("Unavailable");
                    }
                    known.add(tab.id); revokedTabs.set(tab.id, now());
                    stored.push({tabId: tab.id, tabOnly: true}); changed = true;
                }
            } while (changed);
            await chrome.storage.session.set({[KEY]: stored});
        }
        for (const record of stored.filter(r => r.tabOnly).reverse()) await removeTab(record.tabId);
        for (const window of windows) {
            const lease = marker(window);
            const live = lease && Array.from(leases.values()).some(r => r.id === lease && !r.cancelled && !r.context.closed);
            const owned = stored.some(record => !record.tabOnly && proof(window, record)) || (lease && !live);
            if (owned) { try { await chrome.windows.remove(window.id); } catch (error) { if (!missing(error)) throw error; } }
        }
        for (const id of orphanTabs) { await removeTab(id); orphanTabs.delete(id); }
        if (!initialized) await chrome.storage.session.remove(KEY);
        initialized = true; uncertain = false;
        for (const [id, at] of revokedTabs) if (now() - at > 180000) revokedTabs.delete(id);
        return true;
    }
    function ready() {
        if (!recovering) recovering = reconcile().catch(() => { uncertain = !initialized; return false; }).finally(() => {recovering = null;});
        return recovering;
    }
    function sweep() {
        for (const [key, record] of leases) {
            if (!Number.isInteger(record.windowId) && !record.childTabs?.size && !record.children?.size
                && record.settled && now() - record.at > 180000) leases.delete(key);
        }
        if (leases.size >= 128) throw new Error("Unavailable");
    }
    function cancellation(ctx, id) {
        const record = { context: ctx, id, cancelled: true, settled: true, at: now(), creation: Promise.resolve() };
        leases.set(ctx.id + ":" + id, record);
        return record;
    }
    async function close(record) {
        record.cancelled = true;
        // Creation remains owned even after the native command result times out.
        try { await record.creation; } catch (_) { /* creation failure still owns cleanup below */ }
        while (record.children && record.children.size) await Promise.allSettled(Array.from(record.children));
        await remove(record);
        await persist();
        record.settled = true; record.at = now();
        return { windowId: -1, created: false };
    }
    async function execute(command, port) {
        if (!await ready()) throw new Error("Unavailable");
        const id = command.requestId;
        if (typeof id !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)) throw new Error("InvalidArgument");
        const ctx = context(port), key = ctx.id + ":" + id;
        let record = leases.get(key);
        if (command.action === "closeTemporary") {
            if (!record) { sweep(); record = cancellation(ctx, id); }
            return close(record);
        }
        if (command.action !== "openTemporary") throw new Error("InvalidArgument");
        // A replacement native port cannot outrun cleanup of a disconnected port.
        for (const old of leases.values()) if (old.context.closed && (Number.isInteger(old.windowId) || old.childTabs?.size || old.children?.size || !old.settled)) await close(old);
        record = leases.get(key);
        let url;
        try { url = new URL(command.url); } catch (_) { throw new Error("InvalidArgument"); }
        if (url.protocol !== "https:" || url.username || url.password || command.url.length > 8192 || /[\s\\]/.test(command.url)) throw new Error("InvalidArgument");
        if (blocked) throw new Error("Unavailable");
        if (ctx.closed || (record && record.cancelled)) throw new Error("Conflict");
        if (record) {
            if (record.url !== command.url) throw new Error("Conflict");
            return record.creation;
        }
        sweep();
        record = { context: ctx, id, url: command.url, cancelled: false, settled: false, at: now(), tabs: new Set(), childTabs: new Set(), children: new Set() };
        leases.set(key, record);
        record.creation = (async () => {
            try {
                const window = await chrome.windows.create({ url: placeholder + "#" + id, type: "popup", focused: false });
                record.windowId = window.id;
                record.tabId = (window.tabs || [])[0]?.id;
                if (Number.isInteger(record.tabId)) record.tabs.add(record.tabId);
                if (!Number.isInteger(record.tabId)) {
                    await chrome.windows.remove(window.id); record.windowId = undefined;
                    throw new Error("Unavailable");
                }
                await persist();
                if (ctx.closed || record.cancelled) throw new Error("Conflict");
                await chrome.tabs.update(record.tabId, { url: command.url });
                if (ctx.closed || record.cancelled) throw new Error("Conflict");
                await chrome.windows.update(window.id, { focused: true, state: "fullscreen" });
                if (ctx.closed || record.cancelled) throw new Error("Conflict");
                record.settled = true;
                return { windowId: window.id, created: true };
            } catch (error) {
                record.cancelled = true;
                await remove(record); await persist();
                record.settled = true; record.at = now();
                throw error;
            }
        })();
        return record.creation;
    }
    function disconnect(port) {
        const ctx = context(port); ctx.closed = true;
        for (const record of leases.values()) if (record.context === ctx) {
            record.cancelled = true;
            void close(record).catch(() => {}); // retained proof permits retry/recovery
        }
    }
    function sensitive(value) {
        let url;
        try { url = new URL(value); } catch (_) { return /access_token=|code_verifier=/.test(value || "") || (/code=/.test(value || "") && /state=/.test(value || "")); }
        const keys = Array.from(url.searchParams.keys()).concat(Array.from(new URLSearchParams(url.hash.slice(1)).keys())).map(k => k.toLowerCase());
        return (["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) && url.pathname === "/oauth/callback")
            || keys.some(k => ["access_token", "id_token", "refresh_token", "code_verifier", "oauth_token"].includes(k))
            || (keys.includes("code") && keys.includes("state"));
    }
    function redact(tab) {
        let value = tab.url || tab.pendingUrl || "";
        const privateResponse = sensitive(tab.url) || sensitive(tab.pendingUrl);
        if (!privateResponse) return { url: value, title: tab.title || "" };
        try { const url = new URL(value); url.search = ""; url.hash = ""; value = url.href; } catch (_) { value = ""; }
        return { url: value, title: "Sign-in" };
    }
    function ownsTab(tab) {
        return blocked || uncertain || revokedTabs.has(tab.id) || orphanTabs.has(tab.id)
            || Array.from(leases.values()).some(r => r.tabs && r.tabs.has(tab.id));
    }
    function owns(window) {
        return blocked || uncertain || Array.from(leases.values()).some(r => r.windowId === window.id)
            || !!marker(window) || ((window.tabs || []).length > 0 && (window.tabs || []).every(ownsTab));
    }
    async function adoptOpened(tab) {
        const parent = Array.from(leases.values()).find(r => r.tabs && r.tabs.has(tab.openerTabId));
        if (!parent || parent.cancelled || parent.context.closed) {
            if (!parent && !revokedTabs.has(tab.openerTabId)) return false;
            // Retain bounded retry proof. Budget exhaustion persists a fail-closed
            // latch: snapshots and new launches remain disabled for this browser
            // session, even if the service worker restarts. Close can still retry.
            if (orphanTabs.size >= 128 || revokedTabs.size >= 256) {
                blocked = true; await chrome.storage.session.set({[BLOCKED]: true});
                try { await removeTab(tab.id); } catch (_) { /* metadata stays suppressed */ }
                return true;
            }
            revokedTabs.set(tab.id, now()); orphanTabs.add(tab.id);
            await persist();
            try { await removeTab(tab.id); orphanTabs.delete(tab.id); await persist(); } catch (_) { /* ready retries */ }
            return true;
        }
        if (!Number.isInteger(tab.id)) return true;
        if (parent.tabs.size >= 16) {
            // One emergency proof slot; revoke before the first fallible await.
            parent.cancelled = true; parent.tabs.add(tab.id); parent.childTabs.add(tab.id);
            for (const id of parent.tabs) if (revokedTabs.size < 256) revokedTabs.set(id, now());
            await persist();
            try { await close(parent); } catch (_) { /* exact proof retained for Close */ }
            return true;
        }
        parent.tabs.add(tab.id); parent.childTabs.add(tab.id);
        const work = (async () => {
            await persist();
            if (parent.cancelled || parent.context.closed) { await removeTab(tab.id); parent.childTabs.delete(tab.id); await persist(); return; }
            let window = await chrome.windows.get(tab.windowId, {populate: true});
            if (window.type !== "popup" || window.tabs.length !== 1) {
                window = await chrome.windows.create({tabId: tab.id, type: "popup", focused: false});
            }
            if (parent.cancelled || parent.context.closed) { await removeTab(tab.id); parent.childTabs.delete(tab.id); await persist(); return; }
            await chrome.windows.update(window.id, {focused: true, state: "fullscreen"});
            if (parent.cancelled || parent.context.closed) { await removeTab(tab.id); parent.childTabs.delete(tab.id); await persist(); }
        })();
        parent.children.add(work);
        work.then(() => parent.children.delete(work), () => parent.children.delete(work));
        try { await work; } catch (_) { try { await removeTab(tab.id); parent.childTabs.delete(tab.id); await persist(); } catch (_) { /* proof retained for Close */ } }
        return true;
    }
    return Object.freeze({ ready, execute, disconnect, redact, owns, ownsTab, adoptOpened });
})();
