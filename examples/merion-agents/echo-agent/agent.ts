import { defineAgent } from "@zyvor/fabric-agent";

// The smallest agent you can drive from a chat client (POST /v1/agui, see docs/merion/AGUI.md): it needs no model, no credentials and no network.
// The chat client's latest message arrives as `ctx.input.message`; a string result is shown as the assistant's reply.
export default defineAgent({
  async run(ctx) {
    const input = ctx.input as { message?: string } | undefined;
    const message = String(input?.message ?? "").trim();
    ctx.emit("echo.received", { chars: message.length });
    return message ? `You said: ${message}` : "Say something and I will repeat it.";
  },
});
