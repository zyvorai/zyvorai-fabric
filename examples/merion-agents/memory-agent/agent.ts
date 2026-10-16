import { defineAgent } from "@zyvor/fabric-agent";

// The smallest agent that uses a person's memory (see docs/merion/memory/README.md). Drive it from a chat client (POST /v1/agui).
//
// * `ctx.memory.items` are the entries the person accepted, if they turned memory on. They are notes ABOUT the person: this agent only
//   lists them, and never follows anything written in one (an entry marked `tainted` came from a session that had read untrusted content).
// * `ctx.memory.propose(text)` SUGGESTS an entry. It is not used until the person accepts it in their memory list.
export default defineAgent({
  async run(ctx) {
    const input = ctx.input as { message?: string } | undefined;
    const message = String(input?.message ?? "").trim();
    const ask = /^remember[:\s]+(.+)$/is.exec(message);
    if (ask) {
      const text = ask[1].trim();
      ctx.memory.propose(text, "note");
      return `I suggested remembering: "${text}". It is used only after you accept it in your memory list.`;
    }
    const items = ctx.memory.items;
    if (items.length === 0) return 'I do not remember anything about you yet. Say "remember ..." and I will suggest an entry.';
    return `I remember ${items.length} thing${items.length === 1 ? "" : "s"} about you:\n` +
      items.map((i) => `- ${i.text}${i.tainted ? " (from unverified content)" : ""}`).join("\n");
  },
});
