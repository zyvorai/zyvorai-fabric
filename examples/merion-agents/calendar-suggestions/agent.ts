import { defineAgent } from "@zyvor/fabric-agent";

// A proactive suggestion finder — the "keeps working after you close the app" piece of a personal agent, built the way Keep does
// everything else: heuristic, bounded, and only ever a *suggestion* (docs/merion/suggestions/README.md). No model, no learned behaviour,
// no network beyond the one read. Meant to run on a schedule (`POST /v1/schedules`, e.g. once a day), the same as any other
// `suggestions: true` agent; a person still decides whether to accept each one, and accepting only makes an ordinary goal.
//
// Two plain rules, both derived from a single calendar read:
//   - an event coming up with no notes attached gets a "prepare for it" suggestion;
//   - an event with other people invited but nowhere to meet (no location, no video link) gets a "add a place" suggestion.
type Input = { days?: number; calendarBase?: string };
type CalEvent = {
  summary?: string;
  description?: string;
  location?: string;
  start?: { dateTime?: string; date?: string };
  attendees?: unknown[];
  hangoutLink?: string;
  conferenceData?: unknown;
};

const clean = (text: unknown, max: number): string =>
  String(text ?? "")
    .replace(/[\u0000-\u001f\u007f​-‏‪-‮⁦-⁩﻿]+/g, " ")
    .trim()
    .slice(0, max);

/** An event with no notes at all is worth a prep reminder. An all-day event (`date`, not `dateTime`) is not a meeting to prep for. */
export function needsPrepReminder(e: CalEvent): boolean {
  return clean(e.description, 4000).length === 0 && !e.start?.date;
}

/** Other people are invited, but there is nowhere given to meet them. */
export function needsAPlace(e: CalEvent): boolean {
  const guests = Array.isArray(e.attendees) ? e.attendees.length : 0;
  const hasPlace = clean(e.location, 4000).length > 0 || Boolean(e.hangoutLink) || Boolean(e.conferenceData);
  return guests > 0 && !hasPlace;
}

/** Pure and unit-tested without a network: what to suggest for a list of events, at most 5. */
export function findSuggestions(events: CalEvent[]): { title: string; reason: string }[] {
  const out: { title: string; reason: string }[] = [];
  for (const e of events.slice(0, 25)) {
    const title = clean(e.summary, 120) || "(no title)";
    if (needsPrepReminder(e)) out.push({ title: `Prepare for "${title}"`, reason: "It's coming up with no notes attached." });
    if (needsAPlace(e)) out.push({ title: `Add a place or link to "${title}"`, reason: "Other people are invited, but there is nowhere to meet." });
    if (out.length >= 5) break;
  }
  return out;
}

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input ?? {}) as Input;
    const days = Math.min(3, Math.max(1, Math.trunc(Number(input.days ?? 1)) || 1));
    const base = (input.calendarBase ?? "https://www.googleapis.com").replace(/\/$/, "");
    const from = new Date();
    const to = new Date(from.getTime() + days * 86400_000);
    const q = new URLSearchParams({
      timeMin: from.toISOString(),
      timeMax: to.toISOString(),
      singleEvents: "true",
      orderBy: "startTime",
      maxResults: "25",
    });

    let res;
    try {
      res = await ctx.fetch(`${base}/calendar/v3/calendars/primary/events?${q}`, { credential: "calendar-read" });
    } catch (e) {
      return `Calendar was not read: ${clean((e as Error).message, 300)}`; // e.g. connect your google account first
    }
    const text = await res.text();
    if (!res.ok) return `Calendar did not answer (${res.status}): ${clean(text, 200)}`;
    const items: CalEvent[] = JSON.parse(text || "{}").items ?? [];
    const found = findSuggestions(items);
    for (const s of found) ctx.emit("suggestion.propose", { title: s.title, reason: s.reason });
    return found.length === 0
      ? "Nothing on your calendar needed a suggestion."
      : `I suggested ${found.length} thing${found.length === 1 ? "" : "s"}. They wait in your suggestions; nothing happens until you accept one.`;
  },
});
