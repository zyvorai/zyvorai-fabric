---
sidebar_position: 8
---

# Scenarios

Ready-made use cases for common jobs. Each is a `pack.json` under
[`examples/merion-agents/`](../../examples/merion-agents/): copy one, change the keywords, deploy it. None of them needs
code, and the extractive ones call no model, so the cell reports `0` outbound connections.

**Prerequisite:** a cell template. Bake `node22-agent` once on the FluxVM host:
`./scripts/merion-bake-node22-agent.sh` ([template](../../agent-runtime/templates/node22-agent/README.md)).

```bash
./scripts/merionctl deploy examples/merion-agents/<name> --test     # deploy, then run its sample
```

## The scenarios

| Pack | You drop in | You get | Reads with | Sample |
|---|---|---|---|---|
| [`status-page-watch`](../../examples/merion-agents/status-page-watch/README.md) | a saved vendor status page (`.html`) | what is down, degraded, under maintenance, recovered, and the times | `html` | yes |
| [`mailbox-triage`](../../examples/merion-agents/mailbox-triage/README.md) | a mail export (`.mbox`, `.eml`) | subjects, senders, and lines about replies, money, meetings | `eml` | yes |
| [`api-facts`](../../examples/merion-agents/api-facts/README.md) | a JSON document | the fields you named, by path | `text` + `json_path` | yes |
| [`expense-sheet`](../../examples/merion-agents/expense-sheet/README.md) | an Excel sheet (`.xlsx`) | top categories and vendors, the first rows | `xlsx` | no |
| [`nda-review`](../../examples/merion-agents/nda-review/README.md) | a Word contract (`.docx`) | term, confidentiality, liability and governing-law passages; durations and amounts | `docx` | no |
| [`invoice-model-brief`](../../examples/merion-agents/invoice-model-brief/README.md) | an invoice PDF | rule sections **plus a generated summary** | `pdftotext` + model | no |
| [`meeting-notes-model`](../../examples/merion-agents/meeting-notes-model/README.md) | a transcript (`.txt`, `.vtt`) | decisions and actions **plus a generated summary** | `text` + model | yes |

### Phone-user packs

For what a person exports from their phone, and what a phone vendor's app can hand to Keep. All are extractive: no model
reads the file, and the cell reports `0` outbound connections.

| Pack | You drop in | You get | Reads with | Sample |
|---|---|---|---|---|
| [`chat-export-digest`](../../examples/merion-agents/chat-export-digest/README.md) | an exported chat (`.txt`) | who talks most, plans and times, open questions, money, links | `text` | yes |
| [`bank-sms-ledger`](../../examples/merion-agents/bank-sms-ledger/README.md) | saved bank and card SMS alerts (`.txt`) | money out and in, every amount, merchants, declined or international lines. One-time codes are not listed | `text` | yes |
| [`card-statement`](../../examples/merion-agents/card-statement/README.md) | a statement (`.csv`) | most common categories and merchants, the first rows | `text` + `csv_columns` | yes |
| [`calendar-week`](../../examples/merion-agents/calendar-week/README.md) | a calendar export (`.ics`) | events, start times, places, attendees | `text` | yes |
| [`contacts-audit`](../../examples/merion-agents/contacts-audit/README.md) | a contacts export (`.vcf`) | card count, names with duplicates first, numbers, emails | `text` | yes |
| [`travel-itinerary`](../../examples/merion-agents/travel-itinerary/README.md) | a booking or boarding-pass email (`.eml`, `.mbox`) | flights, stays, booking references, amounts | `eml` | yes |
| [`subscription-finder`](../../examples/merion-agents/subscription-finder/README.md) | a mail export (`.mbox`, `.eml`) | renewals, trials ending, what will be charged, amounts, who charges | `eml` | yes |
| [`receipt-pdf`](../../examples/merion-agents/receipt-pdf/README.md) | a receipt or warranty PDF | totals, dates, warranty and return terms, amounts | `pdftotext` | no |
| [`receipt-photo`](../../examples/merion-agents/receipt-photo/README.md) | a photo or screenshot of a receipt (`.png`, `.jpg`) | totals, dates, warranty and return terms, amounts | `ocr` | no |
| [`bill-photo`](../../examples/merion-agents/bill-photo/README.md) | a photo or screenshot of a utility, phone or card bill | amount due, due dates, account and reference lines, charges | `ocr` | no |

| [`fuel-receipt-photo`](../../examples/merion-agents/fuel-receipt-photo/README.md) | a photo of a fuel receipt | litres, rate, total, date | `ocr` | no |
| [`school-fee-receipt-photo`](../../examples/merion-agents/school-fee-receipt-photo/README.md) | a photo of a school fee receipt | receipt number, student and term, fees paid, balance | `ocr` | no |

More everyday packs:

| Pack | You drop in | You get | Reads with | Sample |
|---|---|---|---|---|
| [`payslip-text`](../../examples/merion-agents/payslip-text/README.md) | a payslip as text | month, earnings, deductions, net pay, amounts | `text` | yes |
| [`kindle-highlights`](../../examples/merion-agents/kindle-highlights/README.md) | a Kindle `My Clippings.txt` | books ranked by clippings, highlights vs notes, dates | `text` | yes |
| [`android-call-log`](../../examples/merion-agents/android-call-log/README.md) | a call-log CSV | calls by type, who you talk to, numbers | `text` + `csv_columns` | yes |
| [`insurance-claim-mail`](../../examples/merion-agents/insurance-claim-mail/README.md) | insurer emails (`.eml`, `.mbox`) | claim numbers, status, amounts, what they need from you | `eml` | yes |
| [`takeout-my-activity`](../../examples/merion-agents/takeout-my-activity/README.md) | Google Takeout `MyActivity.json` | products used, what you did, dates | `text` | yes |

Photos are read by OCR (English, `tesseract` in the cell), so check the amounts against the original; a scanned PDF and HEIC are not read. Not covered: vendor-specific
exports whose layout changes between versions (location history, health, screen time), where a pack would be guessing.
These packs read personal data. The cell is sealed and reports `0` outbound connections, but the evidence class is
`software-test`: the host's operator could still read a cell's memory ([VENDORS.md](VENDORS.md)).

### Mac and Windows packs

For files a person exports from a Mac or a Windows PC, run by the person or by an IT team. Keep does **not** connect to
the machine or drive its desktop: the cell is a sealed Linux microVM, and the pack reads a file you export. All are
extractive (no model), and the cell reports `0` outbound connections.

| Pack | You run, then drop in | You get | Sample |
|---|---|---|---|
| [`mac-system-report`](../../examples/merion-agents/mac-system-report/README.md) | `system_profiler SPHardwareDataType SPSoftwareDataType > report.txt` | model, chip, cores, memory, macOS and kernel version, firmware, System Integrity Protection, uptime. Serial number, UUID and names are not listed | real layout |
| [`homebrew-audit`](../../examples/merion-agents/homebrew-audit/README.md) | `brew list --versions` or `brew outdated --verbose` | package count, packages keeping old versions, outdated packages, toolchains present | documented layout |
| [`mac-log-triage`](../../examples/merion-agents/mac-log-triage/README.md) | `/usr/bin/log show --last 5m --style compact \| head -c 190000` | processes with errors or faults, most repeated errors, sandbox denials, kernel and thermal trouble | real layout |
| [`mac-update-history`](../../examples/merion-agents/mac-update-history/README.md) | `softwareupdate --history` | what was installed, versions, dates, betas, Command Line Tools | real output |
| [`windows-systeminfo`](../../examples/merion-agents/windows-systeminfo/README.md) | `systeminfo > si.txt` | OS and build, install date, last boot, model, BIOS, memory, domain, hotfix KBs. Host name and IP addresses are not listed | documented layout |
| [`windows-hotfixes`](../../examples/merion-agents/windows-hotfixes/README.md) | `Get-HotFix \| Export-Csv -NoTypeInformation` | KB numbers, kinds of update, who installed them, dates | documented layout |
| [`windows-installed-software`](../../examples/merion-agents/windows-installed-software/README.md) | an installed-programs CSV (registry `Uninstall` keys) | top publishers and programs, first rows | documented layout |
| [`windows-event-log`](../../examples/merion-agents/windows-event-log/README.md) | `Get-WinEvent ... \| Export-Csv -NoTypeInformation` | counts by level, provider and event id; the error and warning rows | documented layout |

**About the samples.** The macOS samples follow output captured from a real Mac (with placeholder names and identifiers);
`brew` was not run because of a local toolchain licence prompt, so its sample follows Homebrew's documented layout. **The
Windows samples were written from the commands' documented layouts and have not been checked against an export from a
real Windows machine.** Try a pack on your own export before relying on it, and adjust the patterns in `pack.json` if your
Windows language or version words a label differently.

Not covered: a live desktop (an agent clicking through a Mac or Windows session), `.evtx` and `.reg` files (binary or
UTF-16), and Keep running on a Mac or Windows host: FluxVM needs Linux/KVM, and macOS guests are only permitted on Apple
hardware. The output describes a real machine and can name hosts, accounts and software, and the evidence class is
`software-test`, so treat it as sensitive ([VENDORS.md](VENDORS.md)).

### Office packs

For the paperwork around invoices, purchase orders, staff and claims. They read a file you export or save, on a Mac or a
PC alike, and they summarise it: they are **not** your books of account. They do not validate a GSTIN, work out tax, post to
an accounting or payroll system, add or compare figures, or send mail. The official invoice, ledger or return stays in your
own systems. Amounts recognise `₹`, `Rs`, `INR`, `USD`, `EUR`, `GBP`, `$`, `€` and `£`, including `1,25,000` grouping.

| Pack | You drop in | You get | Sample |
|---|---|---|---|
| [`receivables-ageing`](../../examples/merion-agents/receivables-ageing/README.md) | a mail export (`.mbox`, `.eml`) | invoice numbers (most mentioned first), overdue and unpaid lines, payments received, due dates, amounts, who is writing | yes |
| [`po-line-items`](../../examples/merion-agents/po-line-items/README.md) | a purchase order as text (`.txt`) | PO number, GSTINs by format, HSN or SAC codes, lines with amounts, open points | yes |
| [`employee-ledger`](../../examples/merion-agents/employee-ledger/README.md) | a monthly ledger CSV | rows per employee and month, the first rows | yes |
| [`reimbursement-claims`](../../examples/merion-agents/reimbursement-claims/README.md) | a mail export (`.mbox`, `.eml`) | who is claiming, amounts, approved or paid, pending or declined, categories | yes |

Samples use made-up ids and figures. These files carry business and personal data (amounts, tax ids, salaries), so keep
Aadhaar, full bank numbers and PAN out of them, and note the evidence class is `software-test`: the host's operator could
still read a cell's memory ([VENDORS.md](VENDORS.md)).

### Developer-tool packs

For what developers and teams already export: GitHub CLI output, Xcode logs, VS Code settings. Keep does **not** call GitHub or drive
the tools: you run the command, save the output and drop it in. They list and count; they are not scanners, linters or reviewers.

| Pack | You run, then drop in | You get | Sample |
|---|---|---|---|
| [`github-prs`](../../examples/merion-agents/github-prs/README.md) | `gh pr list --state all --json number,title,author,state,createdAt,mergedAt,labels` | states, authors, labels, merges per month, titles | real layout |
| [`github-issues`](../../examples/merion-agents/github-issues/README.md) | `gh issue list --state all --json number,title,author,state,labels,createdAt` | open vs closed, authors, labels, issues per month | real layout |
| [`github-actions-log`](../../examples/merion-agents/github-actions-log/README.md) | `gh run view <id> --log-failed` | `##[error]` lines, failing job and step, exit codes, repeated compiler errors and warnings | real layout |
| [`dependabot-alerts`](../../examples/merion-agents/dependabot-alerts/README.md) | `gh api repos/OWNER/REPO/dependabot/alerts` | states, severities, ecosystems, packages, manifests, advisories | real layout, trimmed |
| [`git-log-digest`](../../examples/merion-agents/git-log-digest/README.md) | `git log --pretty=format:'%h\|%an\|%ad\|%s' --date=short` | commits per author and month, commit prefixes, merges | real layout |
| [`xcodebuild-log`](../../examples/merion-agents/xcodebuild-log/README.md) | `xcodebuild ... > build.log` | build result, errors by file:line (no directories), repeated messages, failed targets and tests | documented layout |
| [`xcode-crash-log`](../../examples/merion-agents/xcode-crash-log/README.md) | a legacy `.crash` report (text) | app, version, OS, exception, crashed thread, frames | documented layout |
| [`vscode-extensions`](../../examples/merion-agents/vscode-extensions/README.md) | `code --list-extensions --show-versions` | count, publishers, names | documented layout |
| [`vscode-settings-audit`](../../examples/merion-agents/vscode-settings-audit/README.md) | `settings.json` | settings that are set, telemetry and trust lines, secret-looking setting *names* (values not shown) | documented layout |

### Browser and desktop-app packs

| Pack | You drop in | You get | Sample |
|---|---|---|---|
| [`bookmarks-digest`](../../examples/merion-agents/bookmarks-digest/README.md) | a bookmarks HTML export (Safari, Chrome, Edge, Firefox) | top sites, folders, titles | documented layout |
| [`browser-history-takeout`](../../examples/merion-agents/browser-history-takeout/README.md) | Google Takeout `BrowserHistory.json` | top sites, how pages were reached, titles | documented layout |
| [`mac-apps-inventory`](../../examples/merion-agents/mac-apps-inventory/README.md) | `system_profiler SPApplicationsDataType` | apps, where each came from, first signer, kind, top-level folder | real layout |
| [`mac-launch-items`](../../examples/merion-agents/mac-launch-items/README.md) | `launchctl list` | non-Apple items, exit statuses, labels with a non-zero status | real layout |
| [`windows-services`](../../examples/merion-agents/windows-services/README.md) | `Get-Service \| Export-Csv -NoTypeInformation` | status and start-type counts, names | documented layout |
| [`windows-scheduled-tasks`](../../examples/merion-agents/windows-scheduled-tasks/README.md) | `schtasks /query /fo csv /v` | tasks, state, last result, run-as account | documented layout |
| [`sales-register-sheet`](../../examples/merion-agents/sales-register-sheet/README.md), [`inventory-sheet`](../../examples/merion-agents/inventory-sheet/README.md), [`attendance-sheet`](../../examples/merion-agents/attendance-sheet/README.md) | an Excel sheet (`.xlsx`, first sheet) | rows per customer, location or employee and status; the first rows. No arithmetic | built by CI (no text sample) |

**Already covered by earlier packs:** Apple Mail and Outlook mail exports (`.mbox`, `.eml`) work with `mailbox-triage`,
`receivables-ageing`, `subscription-finder`, `reimbursement-claims` and `travel-itinerary`; Apple and Outlook calendars (`.ics`) with
`calendar-week`; WhatsApp exports (Android and iPhone layouts) with `chat-export-digest`; Excel with `expense-sheet` and the sheets above.

**Not covered, and why:** Siri (it has no export; see [RECIPES.md](RECIPES.md) for calling Keep from a Shortcut), PowerPoint (`.pptx` needs a new
reader, not a pack), legacy `.ppt`, Outlook `.msg` / `.pst`, `.evtx` and browser history databases (binary), passwords and keychain exports (never
read), and live GitHub or Xcode access. "Real layout" samples follow output captured from a real run with names replaced; "documented layout" samples
were written from the tool's documented output and **have not been checked against a real export**, so try a pack on your own file first.

### Bank operations packs

For a bank's payments, collections, reconciliation, care, credit and compliance teams. All are extractive: no model
reads the file, and the cell reports `0` outbound connections. What they do not do (no OCR, no decisions, no
compliance claim) is in [BANK-OPERATIONS.md](BANK-OPERATIONS.md).

| Pack | You drop in | You get | Reads with | Sample |
|---|---|---|---|---|
| [`neft-rtgs-returns`](../../examples/merion-agents/neft-rtgs-returns/README.md) | a returns / rejects report (`.txt`, `.csv`) | returned and rejected lines, beneficiary problems, UTRs, IFSCs, amounts | `text` | yes |
| [`nach-return-report`](../../examples/merion-agents/nach-return-report/README.md) | a NACH debit return report (`.csv`) | returns by reason, status and sponsor, first rows | `text` + `csv_columns` | yes |
| [`recon-exceptions`](../../examples/merion-agents/recon-exceptions/README.md) | a reconciliation exceptions export (`.csv`) | exceptions by type, channel and ageing, first rows | `text` + `csv_columns` | yes |
| [`upi-dispute-mail`](../../examples/merion-agents/upi-dispute-mail/README.md) | dispute mail (`.eml`, `.mbox`) | what customers report, reference numbers, amounts, escalation asks | `eml` | yes |
| [`loan-sanction-letter`](../../examples/merion-agents/loan-sanction-letter/README.md) | a sanction letter PDF | terms, conditions, charges, amounts, rates, dates | `pdftotext` | no |
| [`rbi-circular-brief`](../../examples/merion-agents/rbi-circular-brief/README.md) | a regulator circular PDF | references, applicability, deadlines, "shall" lines, repeals | `pdftotext` | no |

The two model packs send the extracted text to an endpoint **you** allow, after **you** approve it once. Out of the
box they are refused, because the vault has no such credential. See [MODEL.md](MODEL.md).

Already built in (no pack needed): `pdf-brief`, `contract-clauses`, `security-questionnaire`, `meeting-actions`,
`log-triage`, `sbom-summary`, `csv-clean`. See [demos/](demos/README.md).

## Ways to feed them

| You want | Use |
|---|---|
| One file now | the console at `/app/merion`, or `merionctl run <pack> <file>` |
| A month of files | `merionctl run <pack> a.xlsx b.xlsx c.xlsx` (one cell per file), or one **zip** of them |
| Files that arrive in a directory | a **folder trigger**: `merionctl trigger add-folder <pack> inbox 30` |
| A system that can POST | a **webhook trigger**: `merionctl trigger add-webhook <pack>` |
| To see what changed since last time | **Keep history → Runs → Compare selected** |
| A ping when it finishes | set `ZYVOR_AGENT_APPROVAL_WEBHOOK`; runs send `run.finished` / `run.failed` |

All of it is described in [TRIGGERS.md](TRIGGERS.md).

## Recipes

**Incident log bundle.** Zip the logs from a host, then `merionctl run log-triage incident.zip`. Each `.log` and
`.txt` inside runs in its own sealed cell, and you get one `triage.md` per file under one batch id.

**Vendor contracts in bulk.** `merionctl run contract-clauses a.pdf b.pdf c.pdf` for PDFs, or deploy
[`nda-review`](../../examples/merion-agents/nda-review/README.md) for Word files. Compare two versions of one contract
in **Keep history**.

**What am I paying for?** A user shares a month of billing mail (an `.mbox`) and runs `subscription-finder`: renewals,
trials about to end, and every amount that will be charged. Run it again next month and compare the two runs in **Keep
history**.

**Daily status check.** A cron job fetches the vendor status page and posts it to a webhook trigger for
`status-page-watch`. Keep does not fetch pages itself; you give it the file.

**A drop folder for expenses.** Set `ZYVOR_AGENT_WATCH_ROOT`, add a folder trigger for `expense-sheet`, and save
each month's `.xlsx` into that folder. Each file runs once.

## Test them on a real host

`agent-runtime/tests/demos-ci.sh` uses a FluxVM stand-in. To run the scenarios in **real cells**, bake the template
and point the live script at your runtime:

```bash
export MERION_API=http://127.0.0.1:9096 MERION_TOKEN=...
./scripts/merion-live-scenarios.sh            # built-ins, scenario packs, batch, zip, webhook trigger, history
./scripts/merion-live-scenarios.sh --quick    # a smaller run
```

It deletes the custom use cases it created. Evidence class stays `software-test`: a passing run shows the runtime,
the cell and the extractors work, not that the host cannot read the VM. The model step is not part of this script
because it needs an endpoint you allow; [MODEL.md](MODEL.md) describes how to test it.

## Making your own

Copy the closest pack, change `title`, `accepts` and the rules, and follow
[Tutorial 19](../tutorials/19-build-your-own-use-case.md). The extractors and rules are listed in
[PACKS.md](PACKS.md).
