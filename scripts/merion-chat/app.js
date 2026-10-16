// Keep chat: talks to /agui, /threads on this page's own server, which forwards to the Keep host with the token added there.
// Text from the network or the user is only ever inserted with textContent; there is no innerHTML anywhere in this file.
"use strict";
const $ = (id) => document.getElementById(id);
const app = $("app"), log = $("log"), statusEl = $("status"), form = $("form"), input = $("input"), sendBtn = $("send");
const threadsEl = $("threads"), threadsEmpty = $("threads-empty"), hello = $("hello");
const agentName = $("agent-name").textContent.trim();
const KEY = "keep-chat-thread";   // the conversation to reopen after a reload; only a convenience, so a blocked or empty storage is fine
const remembered = () => { try { return localStorage.getItem(KEY); } catch { return null; } };
const remember = (id) => { try { localStorage.setItem(KEY, id); } catch { /* storage may be blocked */ } };

let threadId = remembered() || crypto.randomUUID();
remember(threadId);   // a first conversation must survive a reload too
let busy = false;
let known = [];

// ---- avatars: a colour from the name, set through the CSS object model (the page's CSP forbids style attributes) ----
function hueOf(s) { let h = 0; for (const c of s) h = (h * 31 + c.charCodeAt(0)) % 360; return h; }
function paintAvatar(el, name) { el.textContent = (name || "?").trim().charAt(0).toUpperCase(); el.style.setProperty("--h", String(hueOf(name || "?"))); }
paintAvatar($("avatar"), agentName); paintAvatar($("hello-avatar"), agentName);

// ---- time ----
const pad = (n) => String(n).padStart(2, "0");
const clock = (d) => pad(d.getHours()) + ":" + pad(d.getMinutes());
function dayLabel(d) {
  const t = new Date(), y = new Date(Date.now() - 864e5);
  const same = (a, b) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
  if (same(d, t)) return "Today";
  if (same(d, y)) return "Yesterday";
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: d.getFullYear() === t.getFullYear() ? undefined : "numeric" });
}
function listTime(iso) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return dayLabel(d) === "Today" ? clock(d) : dayLabel(d);
}

// ---- the message list ----
let lastSide = null, lastDay = "", typingEl = null, lastMine = [];
function nearBottom() { return log.scrollHeight - log.scrollTop - log.clientHeight < 140; }
function toBottom() { log.scrollTop = log.scrollHeight; }
function el(tag, cls, text) { const e = document.createElement(tag); if (cls) e.className = cls; if (text !== undefined) e.textContent = text; return e; }
function place(node) { if (typingEl && typingEl.parentNode === log) log.insertBefore(node, typingEl); else log.appendChild(node); }
function refreshHello() { hello.hidden = log.children.length > 0; }

function dayBreak(when) {
  const label = dayLabel(when);
  if (label !== lastDay) { lastDay = label; place(el("li", "day", label)); lastSide = null; }
}
function bubble(side, text, when) {
  when = when || new Date();
  const stick = nearBottom();
  dayBreak(when);
  const li = el("li", "msg " + side + (lastSide === side ? " join" : (lastSide ? " gap" : "")));
  lastSide = side;
  li.appendChild(document.createTextNode(text));
  const meta = el("span", "meta");
  meta.appendChild(el("time", "", clock(when)));
  if (side === "me") { const tick = el("span", "tick", "✓"); meta.appendChild(tick); li.tick = tick; lastMine.push(li); }
  li.appendChild(meta);
  place(li); refreshHello();
  if (stick) toBottom();
  return li;
}
function note(kind, text) {
  const stick = nearBottom();
  place(el("li", "note " + kind, text)); lastSide = null; refreshHello();
  if (stick) toBottom();
}
// An approval the host is holding for the agent: what would be sent, as the host read it from the request. This page can show it and can
// never decide it; the person does that on their own device.
const cards = new Map();
function approvalCard(v) {
  const stick = nearBottom();
  const card = el("li", "note approval");
  card.appendChild(el("strong", "", "Waiting for your phone"));
  const preview = v.preview && Array.isArray(v.preview.fields) ? v.preview.fields : [];
  if (preview.length) {
    const dl = el("dl");
    for (const f of preview.slice(0, 24)) { dl.appendChild(el("dt", "", String(f.label ?? ""))); dl.appendChild(el("dd", "", String(f.value ?? ""))); }
    card.appendChild(dl);
  } else if (v.prompt) card.appendChild(el("p", "", String(v.prompt)));
  const state = el("p", "state", "Approve or deny it on your device. This chat cannot.");
  card.appendChild(state);
  cards.set(v.approval_id, { card, state });
  place(card); lastSide = null; refreshHello();
  if (stick) toBottom();
}
function approvalDecided(v) {
  const c = cards.get(v.approval_id);
  if (!c) return;
  c.state.textContent = { approved: "Approved on your device.", denied: "Denied on your device.", expired: "Not decided in time, so it was not done." }[v.decision] || "Decided.";
  c.card.classList.add(v.decision === "approved" ? "yes" : "no");
  cards.delete(v.approval_id);
}
function markMine(state) {   // ✓ sent, ✓✓ the agent took it, blue ✓✓ the agent answered
  for (const li of lastMine) { li.tick.textContent = state === "sent" ? "✓" : "✓✓"; li.tick.classList.toggle("read", state === "read"); }
  if (state === "read") lastMine = [];
}
function showTyping(on) {
  if (on && !typingEl) {
    typingEl = el("li", "msg agent typing" + (lastSide === "agent" ? " join" : ""));
    for (let i = 0; i < 3; i++) typingEl.appendChild(el("i"));
    typingEl.setAttribute("aria-label", "The agent is working");
    log.appendChild(typingEl); toBottom();
  } else if (!on && typingEl) { typingEl.remove(); typingEl = null; }
}
function clearLog() { log.textContent = ""; lastSide = null; lastDay = ""; typingEl = null; lastMine = []; refreshHello(); }

function setBusy(b, text, live) {
  busy = b; sendBtn.disabled = b;
  form.classList.toggle("working", b);   // the glow around the field
  statusEl.textContent = text || "Runs on your Keep host";
  statusEl.classList.toggle("live", !!live);
  for (const btn of threadsEl.querySelectorAll("button")) btn.disabled = b;
  $("new").disabled = b;
}

// ---- the chat list: what the Keep host keeps for this agent ----
async function refreshThreads() {
  try {
    const res = await fetch("/threads");
    if (!res.ok) return;
    known = (await res.json()).items || [];
  } catch { return; }
  threadsEl.textContent = "";
  threadsEmpty.hidden = known.length > 0;
  for (const t of known) {
    const li = el("li", "thread" + (t.client_thread_id === threadId ? " current" : ""));
    const open = el("button", "open"); open.type = "button"; open.disabled = busy;
    const av = el("div", "avatar"); paintAvatar(av, agentName);
    const text = el("div", "text");
    text.append(el("span", "title", t.title || "(untitled)"), el("span", "sub", (t.message_count || 0) + (t.message_count === 1 ? " message" : " messages")));
    open.append(av, text, el("span", "when", listTime(t.updated_at)));
    open.addEventListener("click", () => openThread(t));
    const forget = el("button", "forget", "Forget"); forget.type = "button"; forget.disabled = busy;
    forget.setAttribute("aria-label", "Forget this chat: " + (t.title || "untitled"));
    forget.addEventListener("click", () => forgetThread(t, forget));
    li.append(open, forget);
    threadsEl.appendChild(li);
  }
}
async function openThread(t) {
  if (busy) return;
  const res = await fetch("/threads/" + encodeURIComponent(t.id) + "/messages");
  if (!res.ok) { note("error", "Could not open that chat (" + res.status + ")."); return; }
  const items = (await res.json()).items || [];
  clearLog(); statusEl.textContent = "Runs on your Keep host";
  items.forEach((m, i) => {
    const side = m.role === "user" ? "me" : m.role === "assistant" ? "agent" : null;
    const when = m.at ? new Date(m.at) : new Date();
    if (side) {
      const li = bubble(side, String(m.text ?? ""), Number.isNaN(when.getTime()) ? new Date() : when);
      if (side === "me") { li.tick.textContent = "✓✓"; if (items.slice(i + 1).some((n) => n.role === "assistant")) li.tick.classList.add("read"); }
    } else note("meta", String(m.text ?? ""));
  });
  lastMine = [];
  threadId = t.client_thread_id; remember(threadId);
  toBottom(); refreshThreads(); leaveList(); input.focus();
}
async function forgetThread(t, button) {
  if (busy) return;
  if (!button.classList.contains("sure")) {   // two clicks, so a stray one deletes nothing
    button.classList.add("sure"); button.textContent = "Sure?";
    setTimeout(() => { button.classList.remove("sure"); button.textContent = "Forget"; }, 3000);
    return;
  }
  const res = await fetch("/threads/" + encodeURIComponent(t.id), { method: "DELETE" });
  if (!res.ok) { note("error", "Could not forget that chat (" + res.status + ")."); return; }
  if (t.client_thread_id === threadId) newChat();
  refreshThreads();
}
function newChat() { threadId = crypto.randomUUID(); remember(threadId); clearLog(); statusEl.textContent = "Runs on your Keep host"; refreshThreads(); leaveList(); input.focus(); }
function leaveList() { app.classList.remove("show-list"); }

// ---- a run over AG-UI ----
async function run(text, retried) {
  const res = await fetch("/agui", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ threadId, runId: crypto.randomUUID(), messages: [{ id: crypto.randomUUID(), role: "user", content: text }], state: {}, forwardedProps: {} }),
  });
  if (res.status === 409 && !retried) { threadId = crypto.randomUUID(); remember(threadId); return run(text, true); }   // the thread belongs to another agent (the server keeps a thread with one agent): start a new one
  if (!res.ok) { note("error", "The Keep host answered " + res.status + ": " + (await res.text()).slice(0, 300)); return; }
  markMine("sent");
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = "", bubbleEl = null, sawText = false;
  const handle = (e) => {
    switch (e.type) {
      case "RUN_STARTED": markMine("taken"); setBusy(true, "working…", true); showTyping(true); break;
      case "MESSAGES_SNAPSHOT": break;   // the page already shows this conversation
      case "TEXT_MESSAGE_START": showTyping(false); markMine("read"); bubbleEl = bubble("agent", ""); sawText = true; break;
      case "TEXT_MESSAGE_CONTENT": {
        if (!bubbleEl) { showTyping(false); bubbleEl = bubble("agent", ""); }
        const stick = nearBottom();
        bubbleEl.firstChild.textContent += e.delta;
        if (stick) toBottom();
        break;
      }
      case "TEXT_MESSAGE_END": bubbleEl = null; break;
      case "CUSTOM":
        if (e.name === "keep.approval_requested") {
          showTyping(false); setBusy(true, "waiting for your approval", true);
          if ((e.value || {}).approval_id) approvalCard(e.value);   // held by the host, with a preview
          else note("notice", "Waiting for your approval: " + String((e.value || {}).prompt ?? "") + "\nDecide it on your device; this chat cannot approve or deny.");
        }
        else if (e.name === "keep.approval_decided") { approvalDecided(e.value || {}); showTyping(true); setBusy(true, "working…", true); }
        else if (e.name === "keep.waiting") setBusy(true, "waiting…", true);
        else if (e.name === "keep.event") note("meta", String((e.value || {}).kind ?? "event"));
        break;
      case "RUN_FINISHED":
        showTyping(false); markMine("read");
        if (!sawText && e.result !== undefined && e.result !== null) bubble("agent", typeof e.result === "string" ? e.result : JSON.stringify(e.result, null, 2));
        setBusy(false, "");
        break;
      case "RUN_ERROR": showTyping(false); note("error", String(e.message || "The run failed") + (e.code ? " (" + e.code + ")" : "")); setBusy(false, ""); break;
    }
  };
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += dec.decode(value, { stream: true });
    let i;
    while ((i = buf.indexOf("\n\n")) >= 0) {
      const block = buf.slice(0, i); buf = buf.slice(i + 2);
      for (const line of block.split("\n")) {
        if (!line.startsWith("data:")) continue;
        try { handle(JSON.parse(line.slice(5).trim())); } catch { /* ignore a malformed event */ }
      }
    }
  }
}

form.addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const text = input.value.trim();
  if (!text || busy) return;
  input.value = ""; input.style.height = "";
  bubble("me", text);
  setBusy(true, "sending…", true);
  try { await run(text, false); }
  catch (err) { showTyping(false); note("error", "Could not reach the chat server: " + err.message); }
  finally { showTyping(false); setBusy(false, ""); input.focus(); refreshThreads(); }
});
input.addEventListener("keydown", (ev) => { if (ev.key === "Enter" && !ev.shiftKey && !ev.isComposing) { ev.preventDefault(); form.requestSubmit(); } });
input.addEventListener("input", () => { input.style.height = "auto"; input.style.height = Math.min(input.scrollHeight, 136) + "px"; });
$("new").addEventListener("click", () => { if (!busy) newChat(); });
$("back").addEventListener("click", () => { app.classList.add("show-list"); });
// ---- tabs: chats, goals, memory ----
const listEl = document.querySelector(".list");
const tabs = { chats: $("tab-chats"), goals: $("tab-goals"), memory: $("tab-memory"), done: $("tab-done") };
const panes = { chats: $("pane-chats"), goals: $("pane-goals"), memory: $("pane-memory"), done: $("pane-done") };
let tab = "chats", pollTimer = null;
function showTab(name) {
  tab = name;
  for (const k of Object.keys(tabs)) { const on = k === name; tabs[k].classList.toggle("active", on); tabs[k].setAttribute("aria-selected", String(on)); panes[k].hidden = !on; }
  listEl.classList.toggle("tab-goals", name === "goals"); listEl.classList.toggle("tab-memory", name === "memory"); listEl.classList.toggle("tab-done", name === "done");
  if (name === "goals") { refreshGoals(); refreshSuggestions(); } if (name === "memory") refreshMemory(); if (name === "done") refreshDone();
  clearInterval(pollTimer);
  pollTimer = setInterval(() => { if (document.hidden) return; if (tab === "goals") { refreshGoals(); refreshSuggestions(); } if (tab === "memory") refreshMemory(); if (tab === "done") refreshDone(); }, 5000);
}
for (const k of Object.keys(tabs)) tabs[k].addEventListener("click", () => showTab(k));
// coming back to a tab that was in the background: catch up at once instead of waiting for the next tick
document.addEventListener("visibilitychange", () => { if (document.hidden) return; if (tab === "goals") { refreshGoals(); refreshSuggestions(); } if (tab === "memory") refreshMemory(); if (tab === "done") refreshDone(); });

async function api(method, path, body) {
  const res = await fetch(path, { method, headers: body === undefined ? {} : { "content-type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) });
  let data = null;
  if (res.status !== 204) { try { data = await res.json(); } catch { data = null; } }
  return { ok: res.ok, status: res.status, data };
}
function why(r) { return (r.data && (r.data.error || r.data.message)) ? String(r.data.error || r.data.message) : "The Keep host answered " + r.status + "."; }
function twoClick(button, label, action) {   // a stray click changes nothing: the first click asks, the second does it
  button.addEventListener("click", () => {
    if (!button.classList.contains("sure")) { button.classList.add("sure"); button.textContent = "Sure?"; setTimeout(() => { button.classList.remove("sure"); button.textContent = label; }, 3000); return; }
    action();
  });
}

// ---- goals ----
const goalsEl = $("goals"), goalsEmpty = $("goals-empty"), goalMsg = $("goal-msg");
const STEP_ICON = { done: "✓", running: "…", pending: "○", blocked: "!", skipped: "–" };
let lastGoals = "";
async function refreshGoals() {
  const r = await api("GET", "/goals");
  if (!r.ok) return;
  const snap = JSON.stringify(r.data.items || []);
  if (snap === lastGoals) return;   // nothing changed: leave the list (and any half-done two-click) alone
  lastGoals = snap;
  renderGoals(r.data.items || []);
}
function renderGoals(items) {
  goalsEl.textContent = ""; goalsEmpty.hidden = items.length > 0;
  for (const g of items) {
    const li = el("li", "goal");
    const top = el("div", "goal-top");
    top.append(el("span", "goal-title", g.title || "(untitled)"), el("span", "pill " + (g.status || ""), g.status || ""));
    const plan = g.plan || [];
    const finished = plan.filter((s) => s.status === "done" || s.status === "skipped").length;
    const bar = el("div", "bar"); const fill = el("i"); bar.appendChild(fill);
    bar.style.setProperty("--p", String(plan.length ? Math.round((finished / plan.length) * 100) : 0));
    bar.setAttribute("role", "progressbar"); bar.setAttribute("aria-valuemin", "0"); bar.setAttribute("aria-valuemax", String(plan.length)); bar.setAttribute("aria-valuenow", String(finished));
    const steps = el("ul", "steps");
    for (const s of plan) {
      const row = el("li");
      row.appendChild(el("span", "st " + (s.status || ""), STEP_ICON[s.status] || "○"));
      const label = el("span", "", s.title || "");
      if ((s.status === "blocked") && s.detail) label.appendChild(el("span", "why", s.detail));
      row.appendChild(label); steps.appendChild(row);
    }
    li.append(top, bar, el("div", "fine-print", finished + " of " + plan.length + " steps" + (g.autorun ? " · runs automatically" : " · paused")), steps);
    if (plan.length === 0 && (g.status === "open" || g.status === "blocked")) li.appendChild(planBox(g));
    if (g.status === "open" || g.status === "blocked") {
      const actions = el("div", "goal-actions");
      const toggle = el("button", "ghost-btn", g.autorun ? "Pause" : "Run automatically"); toggle.type = "button";
      toggle.addEventListener("click", async () => { const r = await api("PATCH", "/goals/" + encodeURIComponent(g.id), { autorun: !g.autorun }); if (!r.ok) goalMsg.textContent = why(r); goalMsg.classList.toggle("bad", !r.ok); lastGoals = ""; refreshGoals(); });
      const cancel = el("button", "ghost-btn", "Cancel goal"); cancel.type = "button";
      twoClick(cancel, "Cancel goal", async () => { const r = await api("PATCH", "/goals/" + encodeURIComponent(g.id), { status: "cancelled" }); if (!r.ok) goalMsg.textContent = why(r); goalMsg.classList.toggle("bad", !r.ok); lastGoals = ""; refreshGoals(); });
      actions.append(toggle, cancel); li.appendChild(actions);
    }
    goalsEl.appendChild(li);
  }
}
// A goal with no plan: ask the agent to propose one, read it, then accept or discard it. Nothing runs until it is accepted.
function planBox(g) {
  const box = el("div", "plan-box");
  const done = () => { lastGoals = ""; refreshGoals(); };
  const say = (r) => { goalMsg.classList.toggle("bad", !r.ok); goalMsg.textContent = r.ok ? "" : why(r); };
  if (g.proposed) {
    box.appendChild(el("strong", "", "Proposed plan"));
    if (g.proposed.tainted) box.appendChild(el("p", "why", "The agent had read untrusted content while planning. Read every step before accepting."));
    const ol = el("ol", "prop"); for (const s of g.proposed.steps) ol.appendChild(el("li", "", (s.title || "") + (s.requires_approval ? "  (asks you first)" : "")));
    box.appendChild(ol);
    const actions = el("div", "goal-actions");
    const accept = el("button", "ghost-btn", "Accept"); accept.type = "button";
    const run = el("button", "ghost-btn", "Accept and run"); run.type = "button";
    const drop = el("button", "ghost-btn", "Discard"); drop.type = "button";
    const decide = (autorun) => async () => { const r = await api("POST", "/goals/" + encodeURIComponent(g.id) + "/plan/accept", { confirm_tainted: g.proposed.tainted, autorun }); say(r); done(); };
    if (g.proposed.tainted) { twoClick(accept, "Accept", decide(false)); twoClick(run, "Accept and run", decide(true)); }
    else { accept.addEventListener("click", decide(false)); run.addEventListener("click", decide(true)); }
    drop.addEventListener("click", async () => { const r = await api("POST", "/goals/" + encodeURIComponent(g.id) + "/plan/reject"); say(r); done(); });
    actions.append(accept, run, drop); box.appendChild(actions);
  } else if (g.planning) {
    box.appendChild(el("p", "fine-print", "Your agent is planning this…"));
  } else {
    const ask = el("button", "ghost-btn", "Ask the agent for a plan"); ask.type = "button";
    ask.addEventListener("click", async () => { ask.disabled = true; const r = await api("POST", "/goals/" + encodeURIComponent(g.id) + "/plan"); say(r); done(); });
    box.appendChild(ask);
  }
  return box;
}
$("goal-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const title = $("goal-title").value.trim();
  const steps = $("goal-steps").value.split("\n").map((x) => x.trim()).filter(Boolean);
  const r = await api("POST", "/goals", { title, steps, autorun: $("goal-auto").checked });
  goalMsg.classList.toggle("bad", !r.ok); goalMsg.textContent = r.ok ? "Added." : why(r);
  if (r.ok) { $("goal-title").value = ""; $("goal-steps").value = ""; lastGoals = ""; refreshGoals(); }
});

// ---- suggestions: things an agent thinks you might want done. Accepting one makes a goal; nothing else happens. ----
const sugList = $("sug-list"), sugMsg = $("sug-msg");
let sugEnabled = false, lastSug = "";
async function refreshSuggestions() {
  const r = await api("GET", "/suggestions");
  if (!r.ok) return;
  const snap = JSON.stringify(r.data);
  if (snap === lastSug) return;
  lastSug = snap; sugEnabled = !!r.data.enabled;
  $("sug-state").textContent = sugEnabled ? "Suggestions are on" : "Suggestions are off";
  $("sug-toggle").textContent = sugEnabled ? "Turn off" : "Turn on"; $("sug-toggle").setAttribute("aria-pressed", String(sugEnabled));
  const items = r.data.pending || [];
  $("sug-h").hidden = items.length === 0; sugList.textContent = "";
  for (const s of items) {
    const li = el("li", "mem-item");
    const text = el("div", "mem-text");
    text.appendChild(el("strong", "", s.title || ""));
    if (s.reason) text.appendChild(el("div", "fine-print", s.reason));
    if (s.tainted) text.appendChild(el("div", "why", "Made by an agent that had read untrusted content."));
    const actions = el("div", "mem-actions");
    const yes = el("button", "ghost-btn", "Make it a goal"); yes.type = "button";
    const no = el("button", "ghost-btn", "Dismiss"); no.type = "button";
    const act = (path, body) => async () => { const r2 = await api("POST", "/suggestions/" + encodeURIComponent(s.id) + "/" + path, body); sugMsg.classList.toggle("bad", !r2.ok); sugMsg.textContent = r2.ok ? "" : why(r2); lastSug = ""; lastGoals = ""; refreshSuggestions(); refreshGoals(); };
    if (s.tainted) twoClick(yes, "Make it a goal", act("accept", { confirm_tainted: true })); else yes.addEventListener("click", act("accept"));
    no.addEventListener("click", act("dismiss"));
    actions.append(yes, no); li.append(text, actions); sugList.appendChild(li);
  }
}
$("sug-toggle").addEventListener("click", async () => { const r = await api("PUT", "/suggestions/settings", { enabled: !sugEnabled }); sugMsg.classList.toggle("bad", !r.ok); sugMsg.textContent = r.ok ? "" : why(r); lastSug = ""; refreshSuggestions(); });

// ---- done: receipts of what happened after an approval (read-only) ----
const doneEl = $("done"), doneEmpty = $("done-empty");
let lastDone = "";
async function refreshDone() {
  const r = await api("GET", "/receipts");
  if (!r.ok) return;
  const snap = JSON.stringify(r.data.items || []);
  if (snap === lastDone) return;
  lastDone = snap;
  const items = r.data.items || [];
  doneEl.textContent = ""; doneEmpty.hidden = items.length > 0;
  for (const x of items) {
    const li = el("li", "done-item");
    const ok = typeof x.status === "number" && x.status < 400;
    const top = el("div", "goal-top");
    top.append(el("span", "goal-title", (x.method || "") + " " + (x.host || "") + (x.path || "")), el("span", "pill " + (ok ? "done" : "blocked"), String(x.status ?? "")));
    const when = x.at ? new Date(x.at) : null;
    li.append(top, el("div", "fine-print", [x.agent, x.approved ? "approved by you" : "", when && !Number.isNaN(when.getTime()) ? dayLabel(when) + " " + clock(when) : ""].filter(Boolean).join(" · ")));
    doneEl.appendChild(li);
  }
}

// ---- memory ----
const memItems = $("mem-items"), memProps = $("mem-props"), memMsg = $("mem-msg");
let memEnabled = false, lastMem = "";
async function refreshMemory() {
  const r = await api("GET", "/memory");
  if (!r.ok) return;
  const snap = JSON.stringify(r.data);
  if (snap === lastMem) return;
  lastMem = snap; memEnabled = !!r.data.enabled;
  $("mem-state").textContent = memEnabled ? "Memory is on" : "Memory is off";
  $("mem-toggle").textContent = memEnabled ? "Turn off" : "Turn on"; $("mem-toggle").setAttribute("aria-pressed", String(memEnabled));
  $("mem-body").hidden = !memEnabled;
  renderMemory(r.data.items || [], r.data.proposals || []);
}
function memRow(m, actions) {
  const li = el("li", "mem-item");
  const text = el("span", "mem-text", m.text || "");
  text.appendChild(el("span", "badge", m.kind || "note"));
  if (m.origin === "agent") text.appendChild(el("span", "badge", "suggested by your agent"));
  if (m.tainted) text.appendChild(el("span", "badge warn", "came from unverified content"));
  const box = el("span", "mem-actions"); box.append(...actions);
  li.append(text, box); return li;
}
function renderMemory(items, proposals) {
  memItems.textContent = ""; memProps.textContent = "";
  $("mem-props-h").hidden = proposals.length === 0;
  for (const p of proposals) {
    const yes = el("button", "ghost-btn", "Accept"); yes.type = "button";
    yes.addEventListener("click", async () => { const r = await api("POST", "/memory/" + encodeURIComponent(p.id) + "/accept"); if (!r.ok) memMsg.textContent = why(r); lastMem = ""; refreshMemory(); });
    const no = el("button", "ghost-btn", "Reject"); no.type = "button";
    no.addEventListener("click", async () => { await api("POST", "/memory/" + encodeURIComponent(p.id) + "/reject"); lastMem = ""; refreshMemory(); });
    memProps.appendChild(memRow(p, [yes, no]));
  }
  $("mem-empty").hidden = items.length > 0 || proposals.length > 0;
  for (const m of items) {
    const del = el("button", "ghost-btn", "Forget"); del.type = "button";
    twoClick(del, "Forget", async () => { await api("DELETE", "/memory/" + encodeURIComponent(m.id)); lastMem = ""; refreshMemory(); });
    memItems.appendChild(memRow(m, [del]));
  }
}
$("mem-toggle").addEventListener("click", async () => {
  const r = await api("PUT", "/memory/settings", { enabled: !memEnabled });
  memMsg.classList.toggle("bad", !r.ok); memMsg.textContent = r.ok ? "" : why(r);
  lastMem = ""; refreshMemory();
});
$("mem-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const r = await api("POST", "/memory", { text: $("mem-text").value, kind: $("mem-kind").value });
  memMsg.classList.toggle("bad", !r.ok); memMsg.textContent = r.ok ? "Saved." : why(r);
  if (r.ok) { $("mem-text").value = ""; lastMem = ""; refreshMemory(); }
});

input.focus();
refreshHello();
// on load: reopen the remembered conversation if the host still has it
refreshThreads().then(() => { const t = known.find((x) => x.client_thread_id === threadId); if (t) openThread(t); });
