# Merion for bank operations

What a bank's operations, compliance and support teams can hand to Merion today, what comes back, and what Merion does
not do. Everything here runs in a sealed cell with `0` outbound connections, on hardware the bank controls.

## Ready-made packs

| Pack | Team | You drop in | You get | Reads with | Sample |
|---|---|---|---|---|---|
| [`neft-rtgs-returns`](../../examples/merion-agents/neft-rtgs-returns/README.md) | Payments ops | a returns / rejects report (`.txt`, `.csv`) | returned and rejected lines, beneficiary problems, UTRs, IFSCs, amounts | `text` | yes |
| [`nach-return-report`](../../examples/merion-agents/nach-return-report/README.md) | Collections | a NACH debit return report (`.csv`) | returns by reason, status and sponsor; first rows | `text` + `csv_columns` | yes |
| [`recon-exceptions`](../../examples/merion-agents/recon-exceptions/README.md) | Reconciliation | an exceptions export (`.csv`) | exceptions by type, channel and ageing; first rows | `text` + `csv_columns` | yes |
| [`upi-dispute-mail`](../../examples/merion-agents/upi-dispute-mail/README.md) | Customer care | dispute mail (`.eml`, `.mbox`) | what customers report, reference numbers, amounts, escalation asks | `eml` | yes |
| [`loan-sanction-letter`](../../examples/merion-agents/loan-sanction-letter/README.md) | Credit ops | a sanction letter PDF | terms, conditions, charges, amounts, rates, dates | `pdftotext` | no |
| [`rbi-circular-brief`](../../examples/merion-agents/rbi-circular-brief/README.md) | Compliance | a circular PDF | references, applicability, deadlines, "shall" lines, repeals | `pdftotext` | no |

Packs that already ship and fit bank work: contract clauses, the PDF brief, the security questionnaire (vendor
due diligence), [`bank-sms-ledger`](../../examples/merion-agents/bank-sms-ledger/README.md) and
[`card-statement`](../../examples/merion-agents/card-statement/README.md) (customer-side files), the office packs for
receivables, PO line items and reimbursement claims, and log triage for branch and app logs.

## How a bank would run them

- **Daily files.** A scheduler posts each morning's returns or exceptions file to a webhook trigger; Merion runs the
  pack and keeps the artifact. Compare two days in **Merion history** to see what moved. Merion does not fetch files from
  core banking itself: you give it the file.
- **Sending the result on.** Sharing a brief with another team is an action, so it waits for a person's approval.
- **A model, only if you choose one.** The packs above are extractive. If a team wants a written summary as well, the
  model step sends the *extracted* text to one endpoint the bank declares and approves once; the cell stays offline and
  the key is added on the host ([MODEL.md](MODEL.md)).

## What Merion does not do

- **No OCR.** KYC documents, cheque images and scanned letters have no text layer, so no pack reads them.
- **No decisions.** Packs list and count. They do not approve loans, decide disputes, file returns with a regulator,
  total amounts or reconcile entries.
- **No connection to core banking, NPCI or the RBI.** Files go in; artifacts come out.
- **No compliance claim.** Evidence class is `software-test`: the host's operator could still read a cell's memory.
  Merion is not certified against any RBI direction, PCI DSS or ISO standard. Bring it to your information-security and
  compliance teams before real customer data goes in, and start with masked or synthetic files.

Every sample in these packs is synthetic: `EXMP`, `DEMO`, `SAMP` and `TEST` are not real bank codes, and no real
customer, account or UTR appears.
