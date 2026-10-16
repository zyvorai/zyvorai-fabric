import { defineAgent } from "@zyvor/fabric-agent";

// The smallest agent that makes suggestions (docs/merion/suggestions/README.md). It emits one `suggestion.propose` event per item in its input.
// A real one would work the ideas out first (from a calendar, a model, a feed) and then propose them the same way, typically from a
// scheduled run for one person. A proposal is only a suggestion: the person decides, and accepting it makes a goal, nothing more.
type Input = { suggestions?: { title?: string; reason?: string; agent?: string }[] };

export default defineAgent({
  async run(ctx) {
    const items = ((ctx.input ?? {}) as Input).suggestions ?? [];
    let sent = 0;
    for (const s of items.slice(0, 5)) {
      if (typeof s?.title !== "string" || !s.title.trim()) continue;
      ctx.emit("suggestion.propose", { title: s.title, reason: String(s.reason ?? ""), ...(s.agent ? { agent: String(s.agent) } : {}) });
      sent++;
    }
    return sent === 0
      ? "I had nothing to suggest."
      : `I suggested ${sent} thing${sent === 1 ? "" : "s"}. They wait in your suggestions; nothing happens until you accept one.`;
  },
});
