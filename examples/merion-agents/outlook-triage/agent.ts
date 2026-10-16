import { defineAgent } from "@zyvor/fabric-agent";

// Lists your unread Outlook inbox mail: who wrote, the subject, when. Read-only: the `outlook-read` credential can only GET (see
// docs/merion/connectors/README.md). No model. What it shows is text from other people, so it prints it as plain data and does nothing with it.
type Input = { max?: number; graphBase?: string };

const clean = (text: unknown, max: number): string =>
  String(text ?? "").replace(/[\u0000-\u001f\u007f\u200b-\u200f\u202a-\u202e\u2066-\u2069\ufeff]+/g, " ").trim().slice(0, max);

/** An OData query string with spaces as %20 (not "+"), which is what Graph's $filter and $orderby are documented with. */
const qs = (params: Record<string, string>): string => Object.entries(params).map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join("&");

/** A refusal or error from the host or Microsoft. It is said in the chat ("connect your microsoft account first"), not thrown. */
class Refused extends Error {}

async function triage(ctx: any): Promise<string> {
  const input = (ctx.input ?? {}) as Input;
  const base = (input.graphBase ?? "https://graph.microsoft.com").replace(/\/$/, "");
  const max = Math.min(25, Math.max(1, Math.trunc(Number(input.max ?? 10)) || 10));
  const q = qs({ $filter: "isRead eq false", $top: String(max), $select: "from,subject,receivedDateTime" });
  // The host refuses some requests itself (no Microsoft account connected yet, a policy) and that comes back as an error, not a status.
  const res = await ctx.fetch(`${base}/v1.0/me/mailFolders/inbox/messages?${q}`, { credential: "outlook-read" }).catch((e: Error) => {
    throw new Refused(clean(e.message, 300));
  });
  const text = await res.text();
  if (!res.ok) throw new Refused(`Outlook did not answer (${res.status}): ${clean(text, 200)}`);
  const list: any[] = (text ? JSON.parse(text) : {}).value ?? [];
  if (list.length === 0) return "No unread mail in your inbox.";
  const rows = list.map((m) => {
    const who = clean(m.from?.emailAddress?.name || m.from?.emailAddress?.address, 120) || "(unknown sender)";
    const addr = clean(m.from?.emailAddress?.address, 120);
    const date = clean(m.receivedDateTime, 40);
    return `- ${who}${addr && addr !== who ? ` <${addr}>` : ""}: ${clean(m.subject, 120) || "(no subject)"}${date ? ` (${date})` : ""}`;
  });
  ctx.emit("outlook.triage.done", { unread_listed: rows.length });
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
