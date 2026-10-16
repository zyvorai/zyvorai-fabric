# Keep packs: `pack.json` reference

A **pack** is a directory with a `pack.json`. One file drives everything: the console,
`merionctl deploy`, `fabric-agent pack`, and the docs. Three kinds:

| `kind` | What it is | Deploy with |
|---|---|---|
| `usecase` | A declarative use case: an extractor and a few summary rules. **No code.** | console form / paste JSON, or `merionctl deploy <dir>` |
| `agent` | A TypeScript agent (`agent.ts`) with a manifest and an optional signed policy. Runs **inside the cell**. | `merionctl deploy <dir>`, or `merionctl bundle` then the console's **Deploy a pack** |
| `builtin` | Documents a use case the runtime already ships (`pdf-brief`, `csv-clean`, ...). Nothing to deploy. | `merionctl deploy <dir> --test` runs it |

A `pack.json` with no `kind` (the older `name` + `manifest` + `goal` shape) is an `agent` pack.

## `usecase`

```json
{
  "kind": "usecase",
  "name": "invoice-check",
  "title": "Invoice check",
  "description": "Pull totals and due dates out of a plain-text invoice.",
  "accepts": ["txt"],
  "extract": "text",
  "summary": [
    { "kind": "keyword_sections", "title": "Totals", "keywords": ["total", "amount due"], "max_lines": 5 },
    { "kind": "top_repeated_lines", "title": "Repeated lines", "top": 5 },
    { "kind": "stats", "title": "Size" }
  ],
  "artifact_title": "invoice-summary.md",
  "sample_file": "sample.txt"
}
```

| Field | Notes |
|---|---|
| `name` | 1-40 lowercase letters, digits, `-`. Becomes the use-case id. Cannot be a built-in id. |
| `accepts` | 1-6 lowercase extensions. Each fixed-format extractor takes only its own (table below); `text` cannot read binary formats. |
| `extract` | One of the extractors below. **A fixed list, never a command.** |
| `max_bytes` | Upload limit, per extractor (table below). |
| `summary` | 1-20 rules (below). |
| `artifact_title` | Ends in `.md`; default `summary.md`. |
| `model` | Optional model step: the host sends the extracted text to one endpoint. See [MODEL.md](MODEL.md). |
| `sample` / `sample_file` | Optional sample text (`text`, `html` and `eml` only, up to 200 KB) so the use case runs in one click. |

Extractors:

| `extract` | Reads | Upload limit (default / largest) | Notes |
|---|---|---|---|
| `text` | Any text file | 200 KB / 300 KB | Not `pdf`, `docx`, `xlsx`, `pptx` or `zip` |
| `pdftotext` | `.pdf` | 8 MiB / 32 MiB | Needs poppler in the template. A scanned PDF has no text layer and is refused; export its pages as images and use `ocr` |
| `ocr` | `.png`, `.jpg`, `.jpeg`, `.tif`, `.tiff` | 4 MiB / 16 MiB | Photos and screenshots, read by `tesseract` (English, `--psm 4`) in the cell; needs `tesseract-ocr` in the template. Quality depends on the picture, so check amounts against the original. HEIC is not read (share as JPEG). No bundled sample |
| `docx` | `.docx` | 4 MiB / 16 MiB | Paragraph and table text |
| `xlsx` | `.xlsx` | 4 MiB / 16 MiB | First sheet only, as CSV (up to 5000 rows), so `csv_columns` and `table` read it |
| `pptx` | `.pptx` | 4 MiB / 16 MiB | Slide text in presentation order (`## Slide N`) and each slide's speaker notes (`Notes: ...`); slide-number fields dropped. The legacy binary `.ppt` is not read |
| `html` | `.html`, `.htm` | 2 MiB / 8 MiB | Visible text; scripts and styles are dropped |
| `eml` | `.eml`, `.mbox` | 2 MiB / 8 MiB | From, Date, Subject and the text body of up to 500 messages |

`docx`, `xlsx`, `pptx`, `html` and `eml` run a fixed Node script that Keep writes into the cell; it needs
`node` in the template (the agent templates have it). A damaged file is refused with a 400, never
summarised. A **zip** upload is a container for any use case: the files inside it that the use case
accepts each run in their own cell, as a batch (see [TRIGGERS.md](TRIGGERS.md)).

Rules (all bounded, evaluated on the host over the extracted text, no I/O):

| `kind` | Fields | Output |
|---|---|---|
| `keyword_sections` | `title`, `keywords` (1-40), `max_lines` (1-20, default 5) | Lines that mention any keyword (case-insensitive) |
| `top_repeated_lines` | `title`, `top` (1-20, default 5) | Most repeated lines, digits collapsed so near-duplicates group |
| `stats` | `title` (optional) | Line, non-empty line, word and character counts |
| `csv_columns` | `title`, `columns` (1-10), `top` (1-10, default 5) | Distinct count and most common values per column |
| `regex_extract` | `title`, `pattern`, `group` (optional), `max_matches` (1-50, default 10) | Matched values (or one capture group), most frequent first |
| `json_path` | `title`, `paths` (1-10) | Values at paths like `vendor.name`, `items[0].id`, `items[*].sku` |
| `table` | `title`, `max_rows` (1-50, default 10) | The first rows of a CSV as a Markdown table |

`regex_extract` uses Rust's `regex`: matching is linear in the input, and backreferences and
look-around are refused, so a pattern cannot hang the host. A pattern is at most 200 characters and is
compiled when you deploy, so a bad one fails then. Output cells are defanged (no markup, no `|`).

Unknown fields and unknown rule kinds are rejected. At most 50 custom use cases per runtime;
the whole spec is at most 64 KB.

## `agent`

```json
{
  "kind": "agent",
  "name": "infra-ops",
  "entry": "agent.ts",
  "manifest": { "template": "agent-node", "egress_mode": "ask", "confinement": "strict" },
  "goal": { "title": "Read alerts and VM inventory", "text": "Investigate; remediation waits for approval." }
}
```

`manifest` is passed to `POST /v1/agents` unchanged, so it uses the runtime's field names.
If the pack has a `merion.policy.yaml` it is applied after the deploy.

## Signing

In Keep mode (`ZYVOR_AGENT_MERION_MODE=1`) the runtime only accepts a deploy signed by a trusted
signer. The signature is Ed25519 over the **exact bytes** of the request body (and over the
policy YAML for the policy).

```bash
export MERION_POLICY_SEED=$(openssl rand -hex 32)      # your key; keep it private
node sdk/agent-runtime/src/cli.js pack keys           # public key: register it on the runtime
                                                      # as ZYVOR_AGENT_POLICY_TRUSTED_SIGNERS
```

`fabric-agent pack` signs with Node's built-in crypto, so no Rust toolchain is needed. A Rust
test pins a Node-made signature against `merion_sign_policy`, so the two cannot drift. The seed
stays on your machine; the console only ever receives a finished, signed `.merionpack.json`.

## Where the boundary is

| Piece | Runs where |
|---|---|
| `usecase` extractor | In the cell: a fixed command, on a file at a fixed path |
| `usecase` rules | On the host, over bounded text; data only |
| `agent` code | In the cell only. Never on the host. |

Every run, either kind, keeps strict confinement (gateway-only egress, `deny_udp`) and the
0-CONNECT check: a connect freezes the session and returns `409`.
