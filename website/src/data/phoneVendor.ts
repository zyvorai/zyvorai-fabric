/**
 * Copy for /keep/phones. Kept apart from docs/merion/marketing.json on purpose: that file is shared with the
 * Zyvor web app, and this page is GitHub-site only for now. Facts mirror docs/merion/VENDORS.md.
 */

export type Step = {
  id: string;
  title: string;
  who: string;
  body: string;
  call: string;
  screen: 'signin' | 'enrol' | 'ask' | 'approve' | 'done';
};

export const DAY: Step[] = [
  {
    id: 'signup',
    title: 'Sign up',
    who: 'Vendor',
    body: 'The vendor app logs the user in with the vendor’s own account system. The gateway picks a shard in the user’s region, once, and never moves them.',
    call: 'vendor login → gateway places the user',
    screen: 'signin',
  },
  {
    id: 'enrol',
    title: 'Enrol the phone',
    who: 'Phone + vendor',
    body: 'After a strong login the app makes a P-256 key pair in the phone’s keystore. Only the public key goes to the gateway, which enrols it on the user’s shard. A stolen user token cannot enrol a key of its own.',
    call: 'POST /v1/users/{id}/devices  (operator token, from the gateway)',
    screen: 'enrol',
  },
  {
    id: 'ask',
    title: 'Ask for something',
    who: 'Keep',
    body: 'The app asks the gateway; the gateway calls the user’s shard with a short-lived token that reaches only that user’s data. The agent works inside a sealed cell with no network beyond the policy.',
    call: 'POST /v1/demos/{use_case}  (user token)',
    screen: 'ask',
  },
  {
    id: 'approve',
    title: 'Approve on the phone',
    who: 'Phone',
    body: 'When the agent wants to send, buy or delete, the shard pushes through the vendor’s relay. The person reads what is being approved, confirms with biometrics, and the keystore signs that exact decision. The shard refuses anything the enrolled key did not sign.',
    call: 'POST /v1/approvals/{id}  { decision, device_id, signature }',
    screen: 'approve',
  },
  {
    id: 'done',
    title: 'See what happened',
    who: 'Keep',
    body: 'Runs, artifacts and the audit trail come back through the same path. The number of outbound connections the cell made is reported with every run.',
    call: 'GET /v1/inbox  ·  GET /v1/artifacts  (user token)',
    screen: 'done',
  },
];

export const OWNS: {who: 'vendor' | 'keep'; title: string; body: string}[] = [
  {who: 'vendor', title: 'Accounts, login, step-up', body: 'Your identity system, biometrics and second factor. Keep believes the user id the gateway puts in a token.'},
  {who: 'vendor', title: 'Placement and regions', body: 'Which shard a user lives on. A user is pinned to a region and never moved.'},
  {who: 'vendor', title: 'Push delivery', body: 'Keep posts a signed message to a relay you run. You turn it into FCM, Mi Push, HMS, OPPO or vivo push.'},
  {who: 'vendor', title: 'App, billing, model choice', body: 'The UI, quotas policy, and which model answers: Qwen, DeepSeek, GLM, a local server or your own.'},
  {who: 'keep', title: 'A sealed cell per job', body: 'A microVM on hardware you control, with deny-by-default egress enforced on the host.'},
  {who: 'keep', title: 'Signed policy and vault', body: 'Policy you can diff in git. The agent never holds an API key; keys are added on the host.'},
  {who: 'keep', title: 'Phone-signed approvals', body: 'Approvals only an enrolled phone key can sign, in a tamper-evident audit journal.'},
  {who: 'keep', title: 'Isolation, quotas, usage', body: 'User tokens that reach only one user’s data, per-user limits, and a usage report to bill from.'},
];

export type Status = 'built' | 'reference' | 'gap';

export const STATUS: {area: string; state: Status; note: string; href?: string}[] = [
  {area: 'Sealed cell, signed policy, vault, egress control, audit', state: 'built', note: 'Deny-by-default egress on the host, a tamper-evident audit journal', href: '/docs/merion/'},
  {area: 'Many users on one shard: tokens, isolation, quotas, usage, revocation', state: 'built', note: 'Two users in real cells, side by side, on the lab host', href: '/docs/merion/TENANCY'},
  {area: 'Phone-signed approvals and device enrolment (server side)', state: 'built', note: 'Covered by unit and CI end-to-end tests. Not yet run with a waiting agent on the lab host', href: '/docs/merion/mobile/'},
  {area: 'The vendor’s choice of model', state: 'built', note: 'Tested against a stub OpenAI-compatible endpoint', href: '/docs/merion/MODELS'},
  {area: 'Document use cases, triggers, batch, ready-made scenarios', state: 'built', note: '7 built-in use cases and scenario packs, run live', href: '/docs/merion/SCENARIOS'},
  {area: 'Vendor gateway: login, placement, token minting, push relay', state: 'reference', note: 'Reference code, tested; not a product', href: 'https://github.com/zyvorai/fabric/tree/main/reference/vendor-gateway'},
  {area: 'Node phone client and test vectors', state: 'reference', note: 'Reference client; vectors pin the signed text', href: '/docs/merion/mobile/'},
  {area: 'Benchmark script for cold start and concurrency', state: 'reference', note: 'One lab host measured; run it on yours', href: '/docs/merion/VENDORS#sizing-measure-do-not-guess'},
  {area: 'An Android app', state: 'gap', note: 'Not built. The guide has a signing sketch and test vectors to start from'},
  {area: 'Push adapters for FCM, Mi Push, HMS, OPPO, vivo', state: 'gap', note: 'Placeholders: each needs the vendor’s own credentials'},
  {area: 'Warm-pool and hibernate latency', state: 'gap', note: 'Not measured. Sessions report startup_ms so you can'},
  {area: 'Vault keys held by the phone’s secure chip; confidential cells', state: 'gap', note: 'Not available. Hardware-gated (Keep 0.2). Evidence class stays software-test'},
];

export const MODELS = ['Qwen', 'DeepSeek', 'GLM', 'A local server (vLLM)', 'The vendor’s own model'];

export const CAN_SAY = [
  'Each user’s agent runs in a sealed cell with no network except what policy allows, and the cell reports how many outbound connections it made.',
  'The agent never holds an API key. Keys are added on the host, for hosts you list.',
  'A user’s token reaches only that user’s data; another user’s objects look like they do not exist.',
  'With signing required, an approval is accepted only if the user’s enrolled phone key signed that exact decision.',
];

export const CANNOT_SAY = [
  'Not even we can read your data. The evidence class is software-test: the vendor’s operators can still read a cell’s memory and the host’s secrets. Confidential hardware is Keep 0.2 and not available.',
  'Your phone’s secure chip holds your vault. The phone key signs approvals; it does not unlock the vault.',
  'Compliant with any national or industry rule. Keep makes no compliance claim, in China or anywhere else.',
  'Your data never leaves the country, unless the shard, the model endpoint and the push path all stay there.',
];

export type UseCase = {id: string; title: string; files: string[]; drop: string; get: string; built?: boolean};

/** Ready-made use cases a phone user can run. Mirrors docs/merion/SCENARIOS.md ("Phone-user packs"). */
export const USE_CASES: UseCase[] = [
  {id: 'chat-export-digest', title: 'Chat digest', files: ['.txt'], drop: 'An exported chat', get: 'Who talks most, plans and times, open questions, money, links'},
  {id: 'bank-sms-ledger', title: 'Bank alerts ledger', files: ['.txt'], drop: 'Saved bank and card SMS alerts', get: 'Money out and in, amounts, merchants, declined lines. One-time codes are not listed'},
  {id: 'card-statement', title: 'Card statement', files: ['.csv'], drop: 'A statement export', get: 'Most common categories and merchants, the first rows'},
  {id: 'calendar-week', title: 'Calendar week', files: ['.ics'], drop: 'A calendar export', get: 'Events, start times, places, attendees'},
  {id: 'contacts-audit', title: 'Contacts tidy-up', files: ['.vcf'], drop: 'A contacts export', get: 'Card count, names with duplicates first, numbers, emails'},
  {id: 'travel-itinerary', title: 'Trip summary', files: ['.eml', '.mbox'], drop: 'A booking or boarding-pass email', get: 'Flights, stays, booking references, amounts'},
  {id: 'subscription-finder', title: 'What am I paying for?', files: ['.mbox', '.eml'], drop: 'A month of billing mail', get: 'Renewals, trials ending, what will be charged, who charges'},
  {id: 'receipt-pdf', title: 'Receipt and warranty', files: ['.pdf'], drop: 'A receipt or warranty PDF', get: 'Totals, dates, warranty and return terms'},
  {id: 'pdf-brief', title: 'PDF brief', files: ['.pdf'], drop: 'Any text PDF', get: 'A one-page brief with headings and key lines', built: true},
  {id: 'csv-clean', title: 'CSV clean-up', files: ['.csv'], drop: 'A messy CSV', get: 'A cleaned file and a change summary', built: true},
];
