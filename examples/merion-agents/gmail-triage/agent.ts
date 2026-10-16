import { defineAgent } from "@zyvor/fabric-agent";

// Lists your unread inbox mail: who wrote, the subject, when. Read-only: the `gmail-read` credential can only GET (see
// docs/merion/connectors/README.md). No model. What it shows is text from other people, so it prints it as plain data and does nothing with it.
type Input = { max?: number; gmailBase?: string };

const clean = (text: unknown, max: number): string =>
  String(text ?? "").replace(/[\u0000-\u001f\u007f​-‏‪-‮⁦-⁩﻿]+/g, " ").trim().slice(0, max);

/** A refusal or error from the host or Gmail. It is said in the chat ("connect your google account first"), not thrown. */
class Refused extends Error {}

async function getJson(ctx: any, url: string): Promise<any> {
  // The host refuses some requests itself (no Google account connected yet, a policy) and that comes back as an error, not a status.
  const res = await ctx.fetch(url, { credential: "gmail-read" }).catch((e: Error) => {
    throw new Refused(clean(e.message, 300));
  });
  const text = await res.text();
  if (!res.ok) throw new Refused(`Gmail did not answer (${res.status}): ${clean(text, 200)}`);
  return text ? JSON.parse(text) : {};
}

async function triage(ctx: any): Promise<string> {
  const input = (ctx.input ?? {}) as Input;
  const base = (input.gmailBase ?? "https://gmail.googleapis.com").replace(/\/$/, "");
  const max = Math.min(25, Math.max(1, Math.trunc(Number(input.max ?? 10)) || 10));
  const list = await getJson(ctx, `${base}/gmail/v1/users/me/messages?q=${encodeURIComponent("is:unread in:inbox")}&maxResults=${max}`);
  const ids: string[] = (list.messages ?? []).map((m: { id: string }) => m.id).filter((id: unknown) => typeof id === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(id));
  if (ids.length === 0) return "No unread mail in your inbox.";
  const rows: string[] = [];
  for (const id of ids) {
    const m = await getJson(
      ctx,
      `${base}/gmail/v1/users/me/messages/${id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date`,
    );
    const header = (name: string) =>
      clean((m.payload?.headers ?? []).find((h: { name: string }) => String(h.name).toLowerCase() === name)?.value, 120);
    rows.push(`- ${header("from") || "(unknown sender)"}: ${header("subject") || "(no subject)"}${header("date") ? ` (${header("date")})` : ""}`);
  }
  ctx.emit("gmail.triage.done", { unread_listed: rows.length });
  return `${rows.length} unread message${rows.length === 1 ? "" : "s"} (newest first):\n${rows.join("\n")}`;
}

export default defineAgent({
  async run(ctx) {
    try {
      return await triage(ctx);
    } catch (e) {
      if (e instanceof Refused) return e.message;
      throw e;
    }
  },
});
