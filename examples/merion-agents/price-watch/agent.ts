import { defineAgent } from "@zyvor/fabric-agent";

// A proactive finder that connects to something other than mail or calendar (docs/merion/connectors/README.md's "a plain API-key
// connector"): checks each tracked item's current price against the target you set, and proposes a suggestion when it has dropped to
// or below it. No model, nothing learned, no state carried between runs — the target price lives in the input you give the schedule
// (`POST /v1/schedules`), the same as calendar-suggestions' `days`. api.pricewatch.example is a placeholder host, not a real vendor.
type Item = { name?: string; alertBelow?: number };
type Input = { items?: Item[]; priceBase?: string };
type PriceReply = { price?: number };

const clean = (text: unknown, max: number): string =>
  String(text ?? "")
    .replace(/[\u0000-\u001f\u007f​-‏‪-‮⁦-⁩﻿]+/g, " ")
    .trim()
    .slice(0, max);

/** Pure and unit-tested without a network: whether the current price is at or below the target. */
export function needsAlert(current: unknown, alertBelow: unknown): boolean {
  return typeof current === "number" && Number.isFinite(current) && typeof alertBelow === "number" && Number.isFinite(alertBelow) && current <= alertBelow;
}

export default defineAgent({
  async run(ctx) {
    const input = (ctx.input ?? {}) as Input;
    const base = (input.priceBase ?? "https://api.pricewatch.example").replace(/\/$/, "");
    const items = (input.items ?? [])
      .filter((i): i is Item => Boolean(i) && typeof i.name === "string" && i.name.trim().length > 0)
      .slice(0, 10);
    if (items.length === 0) return "Nothing is being tracked. Give items: [{name, alertBelow}].";

    let sent = 0;
    let unreachable = 0;
    for (const item of items) {
      const name = clean(item.name, 120);
      let price: number | undefined;
      try {
        const res = await ctx.fetch(`${base}/v1/price?item=${encodeURIComponent(name)}`, { credential: "price-watch-read" });
        const text = await res.text();
        if (res.ok) price = (JSON.parse(text || "{}") as PriceReply).price;
        else unreachable++;
      } catch {
        unreachable++; // one item's watcher being unreachable does not stop the others
        continue;
      }
      if (needsAlert(price, item.alertBelow)) {
        ctx.emit("suggestion.propose", { title: `"${name}" dropped to ${price}`, reason: `At or below your target of ${item.alertBelow}.` });
        sent++;
      }
    }
    if (sent > 0) return `I suggested ${sent} thing${sent === 1 ? "" : "s"}. They wait in your suggestions; nothing happens until you accept one.`;
    return unreachable === items.length ? "None of the tracked items could be read." : "Nothing tracked has dropped to its target yet.";
  },
});
