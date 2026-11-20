// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { defineAgent } from "@zyvor/fabric-agent";

export default defineAgent({
  async run(ctx) {
    ctx.emit("research.started", { prompt: ctx.input.prompt });

    const response = await ctx.fetch("https://api.anthropic.com/v1/messages", {
      method: "POST",
      credential: "anthropic",
      headers: {
        "content-type": "application/json",
        "anthropic-version": "2023-06-01",
      },
      body: JSON.stringify({
        model: ctx.input.model || "claude-sonnet-4-5",
        max_tokens: 1024,
        messages: [{ role: "user", content: ctx.input.prompt }],
      }),
    });

    const body = await response.json();
    const steer = await ctx.nextSteer({ timeoutMs: 1000 });
    if (steer) ctx.emit("research.steered", steer);
    return body;
  },
});
