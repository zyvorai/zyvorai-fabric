#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0
//
// Keep browser a11y driver — Playwright is private. Model tools only see
// snapshot refs (@eN) and act ops. No page.evaluate / page.content / DOM CDP.
//
// Listen: 127.0.0.1:9230  (host reaches via FluxVM guest HTTP bridge)
// Chromium CDP: 127.0.0.1:9222
//
// POST /v1/tool  { "op": "open"|"snapshot"|"act"|"tabs"|"close"|"health", ... }
//
import http from "node:http";
import { chromium } from "playwright-core";

const CDP = process.env.ZYVOR_BROWSER_CDP || "http://127.0.0.1:9222";
const PORT = Number(process.env.ZYVOR_BROWSER_DRIVER_PORT || "9230");
const EXEC = process.env.ZYVOR_CHROMIUM_PATH || "/usr/bin/chromium";
const MAX_TABS = Number(process.env.ZYVOR_BROWSER_MAX_TABS || "8");

/** @type {import('playwright-core').Browser | null} */
let browser = null;
/** @type {Map<string, { page: import('playwright-core').Page, refs: Map<string, any>, gen: number }>} */
const tabs = new Map();
let activeId = null;
let tabSeq = 0;

async function ensureBrowser() {
  if (browser && browser.isConnected()) return browser;
  // Prefer attaching to the CDP Chromium unit; fall back to launch for local smoke.
  try {
    browser = await chromium.connectOverCDP(CDP);
  } catch {
    const proxyUrl = process.env.ZYVOR_EGRESS_PROXY
      ? new URL(process.env.ZYVOR_EGRESS_PROXY)
      : null;
    browser = await chromium.launch({
      executablePath: EXEC,
      headless: true,
      args: [
        "--no-sandbox",
        "--disable-dev-shm-usage",
        "--remote-debugging-port=9222",
        "--remote-debugging-address=127.0.0.1",
      ],
      proxy: proxyUrl
        ? {
            server: `http://${proxyUrl.hostname}:${proxyUrl.port}`,
            username: decodeURIComponent(proxyUrl.username),
            password: decodeURIComponent(proxyUrl.password),
          }
        : undefined,
    });
  }
  return browser;
}

function activeTab() {
  if (!activeId || !tabs.has(activeId)) return null;
  return tabs.get(activeId);
}

function clearRefs(tab) {
  tab.refs = new Map();
  tab.gen += 1;
}

async function openUrl(url) {
  if (typeof url !== "string" || !url) {
    return { error: "url required" };
  }
  if (url.startsWith("file:")) {
    return { error: "file:// URLs are denied" };
  }
  if (tabs.size >= MAX_TABS && !activeId) {
    return { error: `max_tabs ${MAX_TABS}` };
  }
  await ensureBrowser();
  let tab = activeTab();
  if (!tab) {
    const ctx = browser.contexts()[0] || (await browser.newContext());
    const page = await ctx.newPage();
    const id = `t${++tabSeq}`;
    tab = { page, refs: new Map(), gen: 0 };
    tabs.set(id, tab);
    activeId = id;
    page.on("framenavigated", (frame) => {
      if (frame === page.mainFrame()) clearRefs(tab);
    });
  }
  await tab.page.goto(url, { waitUntil: "domcontentloaded", timeout: 60000 });
  clearRefs(tab);
  return {
    ok: true,
    tab: activeId,
    title: await tab.page.title(),
    url: tab.page.url(),
  };
}

function flattenA11y(node, out, interactiveOnly, counter) {
  if (!node) return;
  const role = node.role || "";
  const name = (node.name || "").slice(0, 120);
  const interactive =
    /^(button|link|textbox|searchbox|checkbox|radio|combobox|menuitem|tab|switch|option|slider)$/i.test(
      role,
    );
  if (!interactiveOnly || interactive) {
    const ref = `@e${counter.n++}`;
    out.push({ ref, role, name });
    // Store locator hints — never HTML.
    node.__ref = ref;
  }
  for (const child of node.children || []) {
    flattenA11y(child, out, interactiveOnly, counter);
  }
}

async function snapshot({ interactive = true } = {}) {
  const tab = activeTab();
  if (!tab) return { error: "no open tab — call open first" };
  const tree = await tab.page.accessibility.snapshot({ interestingOnly: false });
  const items = [];
  const counter = { n: 1 };
  flattenA11y(tree, items, interactive, counter);
  tab.refs = new Map();
  // Build locators from role+name for act resolution.
  for (const item of items) {
    tab.refs.set(item.ref, item);
  }
  return {
    ok: true,
    tab: activeId,
    url: tab.page.url(),
    title: await tab.page.title(),
    gen: tab.gen,
    nodes: items,
  };
}

async function resolveRef(tab, ref) {
  const meta = tab.refs.get(ref);
  if (!meta) return null;
  const role = meta.role;
  const name = meta.name || "";
  try {
    if (name) {
      return tab.page.getByRole(role, { name, exact: false }).first();
    }
    return tab.page.getByRole(role).first();
  } catch {
    return null;
  }
}

async function act(body) {
  const tab = activeTab();
  if (!tab) return { error: "no open tab" };
  const op = body.op;
  const ref = body.ref;
  if (!op) return { error: "op required" };
  if (["click", "fill", "type"].includes(op) && !ref) {
    return { error: "ref required" };
  }
  if (ref) {
    const meta = tab.refs.get(ref);
    if (!meta) return { error: "ref expired — snapshot again", code: "ref_expired" };
    if (op === "fill" && /password|textbox/i.test(meta.role) && /pass/i.test(meta.name || "")) {
      return {
        ok: false,
        needs_host_fill: true,
        ref,
        honesty: "Password fields must be filled via host POST …/browser/fill-secret",
      };
    }
    if (meta.role === "textbox" && body.password === true) {
      return {
        ok: false,
        needs_host_fill: true,
        ref,
        honesty: "Password fields must be filled via host POST …/browser/fill-secret",
      };
    }
  }
  if (op === "scroll") {
    await tab.page.mouse.wheel(0, Number(body.dy || 600));
    return { ok: true, op, tab: activeId };
  }
  if (op === "press") {
    await tab.page.keyboard.press(String(body.key || "Enter"));
    return { ok: true, op, tab: activeId };
  }
  const locator = await resolveRef(tab, ref);
  if (!locator) return { error: "could not resolve ref", code: "ref_expired" };
  if (op === "click") {
    await locator.click({ timeout: 15000 });
    return { ok: true, op, ref, tab: activeId };
  }
  if (op === "fill" || op === "type") {
    const text = body.text;
    if (typeof text !== "string") return { error: "text required" };
    // Refuse if the control looks like a password input.
    const role = tab.refs.get(ref)?.role || "";
    if (role === "textbox" && body.secret) {
      return { ok: false, needs_host_fill: true, ref };
    }
    if (op === "fill") await locator.fill(text, { timeout: 15000 });
    else await locator.type(text, { timeout: 15000 });
    return { ok: true, op, ref, tab: activeId, filled: true };
  }
  return { error: `unknown op ${op}` };
}

async function listTabs() {
  const items = [];
  for (const [id, tab] of tabs) {
    items.push({
      id,
      active: id === activeId,
      title: await tab.page.title().catch(() => ""),
      url: tab.page.url(),
    });
  }
  return { ok: true, tabs: items };
}

async function closeTab(which) {
  const id = which || activeId;
  if (!id || !tabs.has(id)) return { error: "tab not found" };
  const tab = tabs.get(id);
  await tab.page.close().catch(() => {});
  tabs.delete(id);
  if (activeId === id) {
    activeId = tabs.keys().next().value || null;
  }
  return { ok: true, closed: id, active: activeId };
}

async function handleTool(body) {
  const op = body.tool || body.op;
  switch (op) {
    case "health":
      return { ok: true, cdp: CDP, tabs: tabs.size, active: activeId };
    case "open":
      return openUrl(body.url);
    case "snapshot":
      return snapshot({ interactive: body.interactive !== false });
    case "act":
      return act(body);
    case "tabs":
      return listTabs();
    case "close":
      return closeTab(body.tab);
    default:
      return { error: `unknown tool ${op}` };
  }
}

const server = http.createServer(async (req, res) => {
  if (req.method === "GET" && (req.url === "/healthz" || req.url === "/v1/health")) {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ ok: true }));
    return;
  }
  if (req.method !== "POST" || !(req.url === "/v1/tool" || req.url === "/")) {
    res.writeHead(404, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "not found" }));
    return;
  }
  const chunks = [];
  for await (const c of req) chunks.push(c);
  let body = {};
  try {
    body = JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
  } catch {
    res.writeHead(400, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "invalid json" }));
    return;
  }
  try {
    const result = await handleTool(body);
    const status = result.error && !result.needs_host_fill ? 400 : 200;
    res.writeHead(status, { "content-type": "application/json" });
    res.end(JSON.stringify(result));
  } catch (e) {
    console.error("merion-browser-driver tool error:", e);
    res.writeHead(500, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "internal error" }));
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`merion-browser-driver listening on 127.0.0.1:${PORT} cdp=${CDP}`);
});
