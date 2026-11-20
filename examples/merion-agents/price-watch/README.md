# price-watch

The connector half of [`docs/merion/connectors/README.md`'s "plain API-key connector"](../../../docs/merion/connectors/README.md#a-plain-api-key-connector-with-no-oauth-at-all-price-watch) example, paired with the "keeps working" half, [`calendar-suggestions`](../calendar-suggestions/). Checks each tracked item's current price against a target you set and proposes a suggestion (no model, nothing learned) when it has dropped to or below it. `needsAlert` is the whole decision and is unit-tested without a network (`sdk/agent-runtime/test/price-watch.test.js`).

`api.pricewatch.example` is a placeholder host (RFC 2606's reserved `.example` domain) — this is not an integration with a real vendor. Point [`price-watch.credentials.json`](../../../docs/merion/connectors/price-watch.credentials.json) at whatever price-tracking API you actually use (its `host`, `header` and `path_prefixes`) and this agent needs no changes.

Meant to run on a schedule, once a day, for one person:

```
POST /v1/schedules   (operator)
{"agent": "price-watch", "cron": "0 8 * * *", "user_id": "ana",
 "input": {"items": [{"name": "noise-cancelling headphones", "alertBelow": 150}]}}
```

Tested against a fake `ctx.fetch` only; there is no real vendor behind it to run against.
