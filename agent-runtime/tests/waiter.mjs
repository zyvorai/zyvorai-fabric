// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Stays running until the host steers, so CI can delegate from a live session.
export default async function run(ctx) {
  ctx.emit("waiter.ready", {});
  const message = await ctx.nextSteer({ timeoutMs: 0 });
  return { steered: message ?? null };
}
