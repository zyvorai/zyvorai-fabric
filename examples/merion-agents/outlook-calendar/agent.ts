import { defineAgent } from "@zyvor/fabric-agent";

// Reads your next day (or few) of Outlook calendar, or adds one event. Adding waits for the person: the `outlook-calendar-write` credential needs
// an approval decided with the phone key, showing the title, time and guests as the HOST read them from the request. Microsoft emails every guest
// when an event is created, so guests are only added when you say `notify: yes`.
//
// Chat form (or the same fields as structured input):
//   agenda                        <- the default: your next 24 hours (input.days: up to 14)
//   add
//   title: Dinner
//   start: 2026-10-01T19:00:00+02:00      (or a date, 2026-10-01, for an all-day event)
//   end: 2026-10-01T21:00:00+02:00
//   guests: ana@example.com, ben@example.com
//   where: Home
//   notify: yes                    <- required when there are guests: they are emailed
type Input = { message?: string; action?: string; days?: number; title?: string; start?: string; end?: string; guests?: string; where?: string; notify?: string; graphBase?: string };

const ADDRESS = /^[^\s@<>,;"()\[\]\\]+@[^\s@<>,;"()\[\]\\]+\.[^\s@<>,;"()\[\]\\]+$/;
const DATETIME = /^\d{4}-\d\d-\d\dT\d\d:\d\d(:\d\d)?(Z|[+-]\d\d:\d\d)$/;
const DATE = /^\d{4}-\d\d-\d\d$/;
const clean = (text: unknown, max: number): string =>
  String(text ?? "").replace(/[\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff]+/g, " ").trim().slice(0, max);

/** An OData query string with spaces as %20 (not "+"), which is what Graph's $filter and $orderby are documented with. */
const qs = (params: Record<string, string>): string => Object.entries(params).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join("&");

export function parseChat(message: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of message.replace(/\r\n?/g, "\n").split("\n")) {
    const kv = /^\s*(title|start|end|guests|where|notify)\s*:\s*(.*)$/i.exec(line);
    if (kv) out[kv[1].toLowerCase()] = kv[2].trim();
    else if (/^\s*(agenda|add)\s*$/i.test(line)) out.action = line.trim().toLowerCase();
  }
  return out;
}

/** Graph wants a local dateTime plus a zone name, not an offset: send UTC. */
const utc = (t: string): string => new Date(Date.parse(t)).toISOString().slice(0, 19);

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input ?? {}) as Input;
    const chat = typeof input.message === "string" ? parseChat(input.message) : {};
    const pick = (k: keyof Input) => (input[k] ?? chat[k as string]) as string | undefined;
    const action = String(pick("action") ?? "agenda").toLowerCase();
    const base = (input.graphBase ?? "https://graph.microsoft.com").replace(/\/$/, "");

    if (action === "agenda") {
      const days = Math.min(14, Math.max(1, Math.trunc(Number(input.days ?? 1)) || 1));
      const from = new Date();
      const to = new Date(from.getTime() + days * 86400_000);
      const q = qs({ startDateTime: from.toISOString(), endDateTime: to.toISOString(), $top: "25", $orderby: "start/dateTime", $select: "subject,start,location" });
      let res;
      try {
        res = await ctx.fetch(`${base}/v1.0/me/calendarView?${q}`, { credential: "outlook-calendar-read", headers: { prefer: 'outlook.timezone="UTC"' } });
      } catch (e) {
        return `Calendar was not read: ${clean((e as Error).message, 300)}`; // e.g. connect your microsoft account first
      }
      const text = await res.text();
      if (!res.ok) return `Calendar did not answer (${res.status}): ${clean(text, 200)}`;
      const items: any[] = JSON.parse(text || "{}").value ?? [];
      if (items.length === 0) return `Nothing on your calendar in the next ${days === 1 ? "24 hours" : `${days} days`}.`;
      const rows = items.map((e) => `- ${clean(e.start?.dateTime, 40)} UTC: ${clean(e.subject, 120) || "(no title)"}${e.location?.displayName ? ` @ ${clean(e.location.displayName, 80)}` : ""}`);
      ctx.emit("outlook.agenda.done", { events_listed: rows.length });
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
    if (guests.length > 0 && !/^(yes|true|all)$/i.test(String(pick("notify") ?? ""))) {
      return "Microsoft emails every guest when an event is created. Add notify: yes to invite them, or leave the guests out.";
    }
    const allDay = DATE.test(start);
    const event: Record<string, unknown> = allDay
      ? { subject: title, isAllDay: true, start: { dateTime: `${start}T00:00:00`, timeZone: "UTC" }, end: { dateTime: `${end}T00:00:00`, timeZone: "UTC" } }
      : { subject: title, start: { dateTime: utc(start), timeZone: "UTC" }, end: { dateTime: utc(end), timeZone: "UTC" } };
    if (guests.length) event.attendees = guests.map((address) => ({ emailAddress: { address }, type: "required" }));
    const where = clean(pick("where"), 200);
    if (where) event.location = { displayName: where };

    let res;
    try {
      res = await ctx.fetch(`${base}/v1.0/me/events`, {
        method: "POST",
        credential: "outlook-calendar-write",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(event),
      });
    } catch (e) {
      return `Event not added: ${clean((e as Error).message, 300)}`; // denied, timed out, or could not be shown for approval
    }
    const text = await res.text();
    if (!res.ok) return `Event not added (${res.status}): ${text.replace(/\s+/g, " ").slice(0, 200)}`;
    ctx.emit("outlook.add.done", { guests: guests.length });
    return `Added "${title}".${guests.length ? " Your guests were emailed." : ""}`;
  },
});
