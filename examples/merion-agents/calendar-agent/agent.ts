import { defineAgent } from "@zyvor/fabric-agent";

// Reads your next day (or few) of calendar, or adds one event. Adding waits for the person: the `calendar-write` credential needs an approval
// decided with the phone key, showing the title, time, guests and whether the guests get an email, as the HOST read them from the request.
//
// Chat form (or the same fields as structured input):
//   agenda                        <- the default: your next 24 hours (input.days: up to 14)
//   add
//   title: Dinner
//   start: 2026-10-01T19:00:00+02:00      (or a date, 2026-10-01, for an all-day event)
//   end: 2026-10-01T21:00:00+02:00
//   guests: ana@example.com, ben@example.com
//   where: Home
//   notify: yes                    <- only then are the guests emailed
type Input = { message?: string; action?: string; days?: number; title?: string; start?: string; end?: string; guests?: string; where?: string; notify?: string; calendarBase?: string };

const ADDRESS = /^[^\s@<>,;"()\[\]\\]+@[^\s@<>,;"()\[\]\\]+\.[^\s@<>,;"()\[\]\\]+$/;
const DATETIME = /^\d{4}-\d\d-\d\dT\d\d:\d\d(:\d\d)?(Z|[+-]\d\d:\d\d)$/;
const DATE = /^\d{4}-\d\d-\d\d$/;
const clean = (text: unknown, max: number): string =>
  String(text ?? "").replace(/[\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff]+/g, " ").trim().slice(0, max);

export function parseChat(message: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of message.replace(/\r\n?/g, "\n").split("\n")) {
    const kv = /^\s*(title|start|end|guests|where|notify)\s*:\s*(.*)$/i.exec(line);
    if (kv) out[kv[1].toLowerCase()] = kv[2].trim();
    else if (/^\s*(agenda|add)\s*$/i.test(line)) out.action = line.trim().toLowerCase();
  }
  return out;
}

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input ?? {}) as Input;
    const chat = typeof input.message === "string" ? parseChat(input.message) : {};
    const pick = (k: keyof Input) => (input[k] ?? chat[k as string]) as string | undefined;
    const action = String(pick("action") ?? "agenda").toLowerCase();
    const base = (input.calendarBase ?? "https://www.googleapis.com").replace(/\/$/, "");
    const events = `${base}/calendar/v3/calendars/primary/events`;

    if (action === "agenda") {
      const days = Math.min(14, Math.max(1, Math.trunc(Number(input.days ?? 1)) || 1));
      const from = new Date();
      const to = new Date(from.getTime() + days * 86400_000);
      const q = new URLSearchParams({ timeMin: from.toISOString(), timeMax: to.toISOString(), singleEvents: "true", orderBy: "startTime", maxResults: "25" });
      let res;
      try {
        res = await ctx.fetch(`${events}?${q}`, { credential: "calendar-read" });
      } catch (e) {
        return `Calendar was not read: ${clean((e as Error).message, 300)}`; // e.g. connect your google account first
      }
      const text = await res.text();
      if (!res.ok) return `Calendar did not answer (${res.status}): ${clean(text, 200)}`;
      const items: any[] = JSON.parse(text || "{}").items ?? [];
      if (items.length === 0) return `Nothing on your calendar in the next ${days === 1 ? "24 hours" : `${days} days`}.`;
      const rows = items.map((e) => `- ${clean(e.start?.dateTime ?? e.start?.date, 40)}: ${clean(e.summary, 120) || "(no title)"}${e.location ? ` @ ${clean(e.location, 80)}` : ""}`);
      ctx.emit("calendar.agenda.done", { events_listed: rows.length });
      return `${rows.length} event${rows.length === 1 ? "" : "s"}:\n${rows.join("\n")}`;
    }

    if (action !== "add") return 'Say "agenda", or "add" with title:, start:, end: (and guests:, where:, notify: yes).';
    const title = clean(pick("title"), 200);
    const start = String(pick("start") ?? "").trim();
    const end = String(pick("end") ?? "").trim();
    if (!title) return "Give the event a title: title: Dinner";
    const okTime = (t: string) => DATETIME.test(t) || DATE.test(t);
    if (!okTime(start) || !okTime(end) || DATE.test(start) !== DATE.test(end)) {
      return "start: and end: must both be a date (2026-10-01) or both a date and time with a zone (2026-10-01T19:00:00+02:00).";
    }
    if (!(Date.parse(end) > Date.parse(start))) return "The event must end after it starts.";
    const guests = String(pick("guests") ?? "").split(",").map((g) => g.trim()).filter(Boolean);
    if (guests.length > 20) return "At most 20 guests.";
    const bad = guests.find((g) => !ADDRESS.test(g));
    if (bad) return `"${bad.slice(0, 80)}" does not look like an email address.`;
    const time = (t: string) => (DATE.test(t) ? { date: t } : { dateTime: t });
    const event: Record<string, unknown> = { summary: title, start: time(start), end: time(end) };
    if (guests.length) event.attendees = guests.map((email) => ({ email }));
    const where = clean(pick("where"), 200);
    if (where) event.location = where;
    const notify = /^(yes|true|all)$/i.test(String(pick("notify") ?? "")) && guests.length > 0;

    let res;
    try {
      res = await ctx.fetch(`${events}${notify ? "?sendUpdates=all" : ""}`, {
        method: "POST",
        credential: "calendar-write",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(event),
      });
    } catch (e) {
      return `Event not added: ${clean((e as Error).message, 300)}`; // denied, timed out, or could not be shown for approval
    }
    const text = await res.text();
    if (!res.ok) return `Event not added (${res.status}): ${text.replace(/\s+/g, " ").slice(0, 200)}`;
    ctx.emit("calendar.add.done", { guests: guests.length, notified: notify });
    return `Added "${title}".${guests.length ? (notify ? " Your guests were emailed." : " Your guests were not emailed.") : ""}`;
  },
});
