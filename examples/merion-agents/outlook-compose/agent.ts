import { defineAgent } from "@zyvor/fabric-agent";

// Writes a plain-text mail as an Outlook DRAFT, or sends it. Either way the request waits for the person: the credential needs an approval
// decided with the phone key, and the approval shows the recipients, subject and text as the HOST read them out of the request (not as this
// agent describes them). See docs/merion/connectors/README.md.
//
// Chat form (or the same fields as structured input: action, to, subject, body):
//   send                     <- "draft" (the default) or "send"
//   to: ana@example.com, ben@example.com
//   subject: Lunch?
//                            <- a blank line, then the text
//   Are you free at noon?
type Input = { message?: string; action?: string; to?: string; subject?: string; body?: string; graphBase?: string };

const ADDRESS = /^[^\s@<>,;"()\[\]\\]+@[^\s@<>,;"()\[\]\\]+\.[^\s@<>,;"()\[\]\\]+$/;

export function parseChat(message: string): { action?: string; to?: string; subject?: string; body?: string } {
  const [head, ...rest] = message.replace(/\r\n?/g, "\n").split(/\n\s*\n/);
  const out: { action?: string; to?: string; subject?: string; body?: string } = {};
  for (const line of head.split("\n")) {
    const kv = /^\s*(to|subject)\s*:\s*(.*)$/i.exec(line);
    if (kv) out[kv[1].toLowerCase() as "to" | "subject"] = kv[2].trim();
    else if (/^\s*(draft|send)\s*$/i.test(line)) out.action = line.trim().toLowerCase();
  }
  out.body = rest.join("\n\n").trim();
  return out;
}

export function buildMessage(to: string[], subject: string, body: string) {
  return {
    subject,
    body: { contentType: "Text", content: body.replace(/\r\n?/g, "\n") },
    toRecipients: to.map((address) => ({ emailAddress: { address } })),
  };
}

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input ?? {}) as Input;
    const chat = typeof input.message === "string" && input.to === undefined ? parseChat(input.message) : {};
    const action = String(input.action ?? (chat as { action?: string }).action ?? "draft").toLowerCase();
    if (action !== "draft" && action !== "send") return 'Say "draft" or "send" first, then to:, subject: and the text after a blank line.';
    const to = String(input.to ?? (chat as { to?: string }).to ?? "")
      .split(",")
      .map((a) => a.trim())
      .filter(Boolean);
    const subject = String(input.subject ?? (chat as { subject?: string }).subject ?? "");
    const body = String(input.body ?? (chat as { body?: string }).body ?? "");
    if (to.length === 0 || to.length > 10) return "Give one to ten recipients: to: a@example.com, b@example.com";
    const bad = to.find((a) => !ADDRESS.test(a));
    if (bad) return `"${bad.slice(0, 80)}" does not look like an email address.`;
    if (/[\r\n]/.test(subject) || subject.length > 200) return "The subject must be one line of at most 200 characters.";
    if (!body.trim() || body.length > 20000) return "Write the message text after a blank line (up to 20000 characters).";

    const base = (input.graphBase ?? "https://graph.microsoft.com").replace(/\/$/, "");
    const message = buildMessage(to, subject, body);
    const send = action === "send";
    const what = send ? "sent" : "saved as a draft";
    // The host answers with an error, not a status, when the person denied it, it timed out, or it could not be shown for approval.
    let res;
    try {
      res = await ctx.fetch(send ? `${base}/v1.0/me/sendMail` : `${base}/v1.0/me/messages`, {
        method: "POST",
        credential: send ? "outlook-send" : "outlook-draft",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(send ? { message, saveToSentItems: true } : message),
      });
    } catch (e) {
      return `Not ${what}: ${String((e as Error).message).replace(/\s+/g, " ").slice(0, 300)}`;
    }
    const text = await res.text();
    if (!res.ok) return `Not ${what} (${res.status}): ${text.replace(/\s+/g, " ").slice(0, 200)}`;
    let id = "";
    try {
      id = String(JSON.parse(text).id ?? "").slice(0, 12); // a sent mail (202) has no body
    } catch {
      /* the id is only for display */
    }
    ctx.emit("outlook.compose.done", { action, recipients: to.length });
    return send
      ? `Sent to ${to.length} recipient${to.length === 1 ? "" : "s"}.`
      : `Saved as a draft${id ? ` (${id}…)` : ""}. Nothing was sent.`;
  },
});
