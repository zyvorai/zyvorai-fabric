# Payments: an agent that can buy without holding a card

## Why

Meta's announcement lists shopping through a payment provider as available. The value is real (book, buy, renew), and so is the risk: money
moves, and can rarely be moved back. Keep's rule is that an agent never holds a secret. For a card, that must hold in the strongest form.

## What exists today

- The **vault and egress broker**: a credential is injected by the host, only over HTTPS, only to the host, method and path the descriptor
  allows; `requires_approval` and `require_device_signature` make each use wait for the person's phone key; `approval_kind: "purchase"` exists as a
  distinct approval kind ([credentials.rs](../../../agent-runtime/src/credentials.rs)).
- **Host-rendered previews** of what is being approved (mail and events today; [connectors](../connectors/README.md)), with a digest the
  phone signs.
- **Action receipts** with an idempotency key: an approved request is executed at most once, and a repeat is answered from its receipt
  ([receipts](../receipts/README.md)).
- The browser's `max_checkout_asks` limit and a purchase-click witness scaffold (a keyword guess; see [browser workflows](browser-workflows.md)).
- Nothing that touches a real payment method.

## Principles

1. **The agent never sees a card number, CVV, bank detail or a reusable payment token.** Not in its input, its memory, its logs or a screenshot.
2. **Money moves only through a payment method that limits itself:** single-use or capped, per-merchant, expiring. A leak of the credential
   costs at most its cap.
3. **Every spend is a signed decision on the person's phone, with the host-rendered amount, currency and merchant.**
4. **The host enforces ceilings the agent cannot raise:** per purchase, per day, per merchant, and a total budget per goal.
5. **Refunds, disputes and card changes are not the agent's.**

## Proposal

**A. One provider adapter behind a narrow interface.** The host talks to a provider that can issue a *limited-use virtual card or
payment token* for one purchase (for example a card-issuing product with per-card spending controls, or a wallet's tokenised checkout).
The interface is three calls, all host-side: `quote(merchant, amount, currency) → offer`, `authorize(offer) → limited credential` (only after
the person's signed approval), `settle/void`. The credential is created for exactly the approved amount and merchant, expires quickly, and is
released to the merchant's checkout **by the host** (the browser fill path or the merchant's API through the egress broker), never to the agent.

**B. The approval is the purchase.** The approval's preview is built by the host from the offer: merchant name and domain, item lines if
the merchant API gives them, total, currency, tax and shipping, and the payment method's last four. The signed digest covers all of it.
The idempotency key is derived from the offer id, so a retry cannot buy twice.

**C. Ceilings in policy.** New signed-policy fields: `payments: {per_purchase, per_day, per_merchant, per_goal}`, `currency`, and a merchant
allowlist. Exceeding one refuses the purchase before any approval is opened (nothing to approve).

**D. Start with the smallest real thing.** One provider, test mode only, one merchant that the provider's docs say is safe to test against,
and a fake provider in CI, exactly as Google and Microsoft were done: fixture first, live check by the owner.

## What I would refuse to build

- Storing a card or bank details on the Keep host, or accepting them in the chat.
- Any purchase without a signed approval, including "small" ones, "trusted merchant" ones and recurring ones. (Recurring is a separate design: a
  bounded, revocable grant, not a habit.)
- An agent that can choose the merchant after the person approved.
- Investment, trading or transfers between people's accounts. Those are out of scope entirely; Keep is not a financial advisor or broker.

## How I would verify it

Fake provider in `demos-ci.sh`: quote, approval showing amount/currency/merchant/last four, a flipped or unsigned decision refused, the credential
never present in the agent's input, events, journal or approval store; a retry of the same offer runs once; ceilings refuse before an approval
opens; a changed amount after approval is refused. Then the provider's own **test mode** with the owner's test keys. Real money only with an
explicit go, a tiny amount and the owner watching.

## Size

Adapter interface and fake provider: medium. Policy ceilings: small. Preview and approval kind: small (the machinery exists). Real provider
adapter: medium, and gated on your choice. Three to five PRs.

## Decisions for you

1. **Do you want payments at all in the first year?** A wrong answer costs a person money. Recommendation: yes, but only after the browser
   commit-approval in [browser workflows](browser-workflows.md) exists, because most purchases happen in a checkout page.
2. **Which provider?** I have not checked current terms, countries or availability for any provider; please name one you can open an
   account with, and I will read its documentation before designing the adapter.
3. **Whose money in testing?** Test mode only, at first: agree that no real card is used until a signed go from you.
4. **Recurring payments:** confirm they are out of scope for now.
