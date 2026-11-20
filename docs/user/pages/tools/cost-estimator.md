# Cost Estimator

## Purpose

Storage Cost Estimator — a what-if calculator that projects cloud storage cost (AWS S3, Azure Blob, GCS) for your VM fleet and compares it against an on-premises baseline. It's a planning tool: figures come from the inputs you set, not from your actual billing or usage.

## When to use it

- To budget or justify a move to cloud storage before you commit to it
- To see how fleet size, average VM disk size, or snapshot retention changes projected storage spend
- To put a number on "how much would we save vs. on-prem" for a proposal or ticket

## How to get there

- Route / id: `/cost-estimator`
- Nav: **Tools → Cost Estimator** (sidebar, command palette, or desktop nav)

## Operate from the console (UX)

1. **Configuration** — set Number of VMs (1–1000), Average VM Size (10 GB–2 TB), and Storage Duration (1–36 months) with sliders. Toggle **Include snapshots** to add 50% to the total storage figure.
2. Click **Calculate Costs**. The page first tries the backend (`POST /api/cost/estimate`); if that call fails or returns no estimates, it falls back to a client-side calculation using fixed per-GB rates (AWS S3 $0.023, Azure Blob $0.018, GCS $0.020, on-prem $0.10).
3. Results show three provider cards (Monthly / Annual / Total for the chosen duration), with the cheapest option badged.
4. A savings panel compares the cheapest cloud option against the on-prem estimate, with a percentage saved.
5. A horizontal bar chart compares monthly cost across all three cloud providers plus on-prem.
6. **Copy Estimate to Clipboard** copies a plain-text summary of your inputs and the results — handy for pasting into a ticket or email.

5. **Empty / fail:** Check health, auth, and domain dependencies.
6. **Success:** Live data loads; mutations complete without error toasts.

## Related pages

- [API Playground](playground.md)
- [Webhooks](webhooks.md)
- [VM Compare](vm-compare.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
