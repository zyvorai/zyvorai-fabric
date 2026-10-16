#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Run the Keep scenarios against a LIVE runtime, in real cells (needs the node22-agent template:
# ./scripts/merion-bake-node22-agent.sh). Unlike agent-runtime/tests/demos-ci.sh this uses no stand-in.
#
#   export KEEP_API=http://127.0.0.1:9096 KEEP_TOKEN=...
#   ./scripts/merion-live-scenarios.sh            # everything (a few minutes: each run boots a cell)
#   ./scripts/merion-live-scenarios.sh --quick    # built-ins + one extractor each, no batch or zip
#
# Needs: curl, python3, node (for merionctl deploy). Custom use cases it creates are deleted afterwards.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KEEPCTL="$ROOT/scripts/merionctl"
: "${KEEP_API:=${ZYVOR_AGENT_URL:-http://127.0.0.1:9096}}"
export KEEP_API
QUICK=0
[[ "${1:-}" == "--quick" ]] && QUICK=1
AUTH=()
[[ -n "${KEEP_TOKEN:-${ZYVOR_AGENT_TOKEN:-}}" ]] && AUTH=(-H "Authorization: Bearer ${KEEP_TOKEN:-$ZYVOR_AGENT_TOKEN}")
export KEEP_TOKEN="${KEEP_TOKEN:-${ZYVOR_AGENT_TOKEN:-}}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/keep-live.XXXXXX")"
PASSED=0; FAILED=0
CREATED=()
cleanup() {
  for id in ${CREATED[@]+"${CREATED[@]}"}; do curl -s -o /dev/null -X DELETE "${AUTH[@]}" "$KEEP_API/v1/demos/$id"; done
  rm -rf "$WORK"
}
trap cleanup EXIT

ok()   { PASSED=$((PASSED + 1)); echo "  ok   $*"; }
bad()  { FAILED=$((FAILED + 1)); echo "  FAIL $*"; }
json() { python3 -c 'import json,sys
d=json.load(sys.stdin)
for part in sys.argv[1].split("."):
    d = d[int(part)] if part.isdigit() else d[part]
print(d if not isinstance(d,(dict,list)) else json.dumps(d))' "$1"; }
api()  { curl -s "${AUTH[@]}" "$@"; }
# check <name> <haystack> <needle>
has()  { if grep -qF -- "$3" <<<"$2"; then ok "$1"; else bad "$1 (missing: $3)"; echo "       got: $(head -c 300 <<<"$2")"; fi; }
body_of() { api "$KEEP_API/v1/artifacts/$(json artifacts.0.id <<<"$1")" | json body; }

echo "==> live scenarios against $KEEP_API"
api -f "$KEEP_API/healthz" >/dev/null || { echo "runtime not reachable at $KEEP_API" >&2; exit 2; }

# ---- fixtures -----------------------------------------------------------------
python3 - "$WORK" <<'PY'
import sys, zipfile
w = sys.argv[1]
def sst(*s): return "<sst>" + "".join(f"<si><t>{x}</t></si>" for x in s) + "</sst>"
with zipfile.ZipFile(w + "/exp.xlsx", "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("xl/workbook.xml", '<workbook><sheets><sheet name="March" sheetId="1"/></sheets></workbook>')
    z.writestr("xl/sharedStrings.xml", sst("date", "category", "vendor", "amount", "Travel", "Meals", "AirCo", "Bistro"))
    rows = [(0, 1, 2, 3), (None, 4, 6, None), (None, 4, 6, None), (None, 5, 7, None)]
    body = "".join('<row r="%d">' % (i + 1) + "".join('<c r="%s%d" t="s"><v>%d</v></c>' % ("ABCD"[j], i + 1, v) for j, v in enumerate(r) if v is not None) + "</row>" for i, r in enumerate(rows))
    z.writestr("xl/worksheets/sheet1.xml", "<worksheet><sheetData>" + body + "</sheetData></worksheet>")
with zipfile.ZipFile(w + "/nda.docx", "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("word/document.xml", "<w:document><w:body>" + "".join("<w:p><w:r><w:t>%s</w:t></w:r></w:p>" % t for t in [
        "This Agreement is governed by the laws of Portugal.",
        "The term of this Agreement is two (2) years and renews for 30 days at a time.",
        "Confidential Information must not be disclosed. Damages of EUR 50,000 apply."]) + "</w:body></w:document>")
with zipfile.ZipFile(w + "/logs.zip", "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("app/a.log", "2026-09-25 10:00:01 ERROR db timeout id=1\n2026-09-25 10:00:02 ERROR db timeout id=2\n2026-09-25 10:00:03 INFO ok\n")
    z.writestr("app/b.log", "2026-09-25 11:00:01 WARN slow request took 900ms\n2026-09-25 11:00:02 ERROR disk full\n")
    z.writestr("app/readme.md", "ignored")
PY
printf 'name,qty\nAnn,1\nBob,=cmd|calc\n' > "$WORK/a.csv"; printf 'name,qty\nCy,2\n' > "$WORK/b.csv"; printf 'name,qty\nDee,3\n' > "$WORK/c.csv"

# ---- built-in use cases, on their bundled samples --------------------------------
echo "== built-in use cases (each boots a cell)"
for id in csv-clean log-triage meeting-actions sbom-summary pdf-brief contract-clauses security-questionnaire; do
  [[ "$QUICK" == 1 && "$id" != csv-clean && "$id" != pdf-brief ]] && continue
  r=$(api -X POST -F note=none "$KEEP_API/v1/demos/$id")
  if [[ "$(json egress_connects <<<"$r" 2>/dev/null)" == "0" ]]; then ok "$id runs in a real cell, 0 CONNECT"; else bad "$id: $(head -c 300 <<<"$r")"; fi
done

# ---- scenario packs -----------------------------------------------------------------
echo "== scenario packs"
deploy() { KEEP_API="$KEEP_API" "$KEEPCTL" deploy "$ROOT/examples/merion-agents/$1" "${@:2}" >"$WORK/deploy-$1.out" 2>&1; }
run() { api -X POST -F "file=@$2" "$KEEP_API/v1/demos/$1"; }
for p in status-page-watch mailbox-triage api-facts; do CREATED+=("$p"); done
CREATED+=(expense-sheet nda-review invoice-model-brief meeting-notes-model)

EXTRA_PACKS=(payslip-text kindle-highlights android-call-log insurance-claim-mail takeout-my-activity neft-rtgs-returns nach-return-report recon-exceptions upi-dispute-mail chat-export-digest bank-sms-ledger card-statement calendar-week contacts-audit travel-itinerary subscription-finder receipt-pdf mac-system-report homebrew-audit mac-log-triage mac-update-history windows-systeminfo windows-hotfixes windows-installed-software windows-event-log receivables-ageing po-line-items employee-ledger reimbursement-claims github-prs github-issues github-actions-log dependabot-alerts git-log-digest xcodebuild-log xcode-crash-log vscode-extensions vscode-settings-audit bookmarks-digest browser-history-takeout mac-apps-inventory mac-launch-items windows-services windows-scheduled-tasks sales-register-sheet inventory-sheet attendance-sheet)
for p in "${EXTRA_PACKS[@]}"; do CREATED+=("$p"); done
for pair in "payslip-text:Net Pay 71,300.00" "kindle-highlights:3× Highlight" "android-call-log:incoming (3)" "insurance-claim-mail:3× CLM-2026-0042" "takeout-my-activity:3× Search" "neft-rtgs-returns:3× EXMP0001234" "nach-return-report:Insufficient funds (3)" "recon-exceptions:Unmatched debit (3)" "upi-dispute-mail:2× 506712345678" "status-page-watch:Object storage" "mailbox-triage:Invoice 2041 is overdue" "api-facts:KB-01; MS-07" "chat-export-digest:4× Ana" "bank-sms-ledger:was declined" "card-statement:Dining (3)" "calendar-week:2× Team standup" "contacts-audit:2× Ana Example" "travel-itinerary:2× K7QP2M" "subscription-finder:1× EUR 39.00" "mac-system-report:1× Apple M4" "homebrew-audit:1× openjdk" "mac-log-triage:2× analyticsd" "mac-update-history:5× 26.0" "windows-systeminfo:1× KB5034441" "windows-hotfixes:Security Update (3)" "windows-installed-software:Example Software Inc. (2)" "windows-event-log:Error (2)" "receivables-ageing:4× INV-2026-0142" "po-line-items:1× PO-7781/2026" "employee-ledger:E001 (2)" "reimbursement-claims:2× Rs 4,200" "github-prs:3× MERGED" "github-issues:2× CLOSED" "github-actions-log:1× test Lint" "dependabot-alerts:2× high" "git-log-digest:4× Ana Dev" "xcodebuild-log:BUILD FAILED" "xcode-crash-log:EXC_BAD_ACCESS" "vscode-extensions:2× ms-python" "vscode-settings-audit:1× github.copilot.advanced.apiToken" "bookmarks-digest:3× example.com" "browser-history-takeout:3× LINK" "mac-apps-inventory:2× Apple" "mac-launch-items:1× com.example.oldjob" "windows-services:Running (3)" "windows-scheduled-tasks:SYSTEM (3)"; do
  p="${pair%%:*}"; want="${pair#*:}"
  if deploy "$p"; then
    r=$(api -X POST -F note=none "$KEEP_API/v1/demos/$p")
    if [[ "$(json egress_connects <<<"$r" 2>/dev/null)" == "0" ]]; then has "$p: sample summarised in a real cell" "$(body_of "$r")" "$want"; else bad "$p: $(head -c 300 <<<"$r")"; fi
  else bad "$p: deploy failed: $(head -c 300 "$WORK/deploy-$p.out")"; fi
  # the runtime allows 50 custom use cases and this script deploys about that many: drop each one once it has been checked
  curl -s -o /dev/null -X DELETE "${AUTH[@]}" "$KEEP_API/v1/demos/$p"
  [[ "$QUICK" == 1 ]] && break
done

# Excel packs have no bundled sample (a sample must be text): build small workbooks
python3 - "$WORK" <<'PY'
import sys, zipfile
w = sys.argv[1]
def mk(path, name, rows):
    strings, idx = [], {}
    def si(x):
        if x not in idx: idx[x] = len(strings); strings.append(x)
        return idx[x]
    body = "".join('<row r="%d">' % r + "".join('<c r="%s%d" t="s"><v>%d</v></c>' % ("ABCDEFGH"[c], r, si(str(v))) for c, v in enumerate(row)) + "</row>" for r, row in enumerate(rows, 1))
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("xl/workbook.xml", '<workbook><sheets><sheet name="%s" sheetId="1"/></sheets></workbook>' % name)
        z.writestr("xl/sharedStrings.xml", "<sst>" + "".join("<si><t>%s</t></si>" % x for x in strings) + "</sst>")
        z.writestr("xl/worksheets/sheet1.xml", "<worksheet><sheetData>" + body + "</sheetData></worksheet>")
mk(w + "/sales.xlsx", "Register", [["date","customer","invoice_no","taxable_value","tax","total"],["2026-09-01","Acme Traders","INV-101","10000","1800","11800"],["2026-09-03","Globex Ltd","INV-102","5000","900","5900"],["2026-09-09","Acme Traders","INV-103","2500","450","2950"]])
mk(w + "/stock.xlsx", "Stock", [["sku","description","qty","location","reorder_level"],["A-100","Widget","40","Warehouse A","20"],["B-220","Bracket","8","Warehouse A","15"],["C-330","Cable set","120","Warehouse B","30"]])
mk(w + "/attendance.xlsx", "Sept", [["employee","date","status"],["E001","2026-09-01","Present"],["E001","2026-09-02","Leave"],["E002","2026-09-01","Present"],["E002","2026-09-02","Present"]])
PY
if [[ "$QUICK" == 0 ]]; then
  deploy sales-register-sheet && { r=$(run sales-register-sheet "$WORK/sales.xlsx"); has "sales-register-sheet: real xlsx read" "$(body_of "$r" 2>/dev/null)" "Acme Traders (2)"; } || bad "sales-register-sheet deploy"
  deploy inventory-sheet && { r=$(run inventory-sheet "$WORK/stock.xlsx"); has "inventory-sheet: real xlsx read" "$(body_of "$r" 2>/dev/null)" "Warehouse A (2)"; } || bad "inventory-sheet deploy"
  deploy attendance-sheet && { r=$(run attendance-sheet "$WORK/attendance.xlsx"); has "attendance-sheet: real xlsx read" "$(body_of "$r" 2>/dev/null)" "Present (3)"; } || bad "attendance-sheet deploy"
fi

# deck-outline reads a .pptx (no bundled sample: a sample must be text): build a small deck with speaker notes
python3 - "$WORK/deck.pptx" <<'PY'
import sys, zipfile
z = zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED)
z.writestr("ppt/presentation.xml", '<p:presentation xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId1"/></p:sldIdLst></p:presentation>')
z.writestr("ppt/_rels/presentation.xml.rels", '<Relationships><Relationship Id="rId1" Type="x/slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="x/slide" Target="slides/slide2.xml"/></Relationships>')
z.writestr("ppt/slides/slide1.xml", "<p:sld><a:p><a:r><a:t>Budget plan</a:t></a:r></a:p><a:p><a:r><a:t>Spend 50,000 EUR, owner TBD by Friday 3 Oct</a:t></a:r></a:p></p:sld>")
z.writestr("ppt/slides/_rels/slide1.xml.rels", '<Relationships><Relationship Id="rId9" Type="x/notesSlide" Target="../notesSlides/notesSlide1.xml"/></Relationships>')
z.writestr("ppt/notesSlides/notesSlide1.xml", "<p:notes><a:p><a:r><a:t>Say the number twice</a:t></a:r></a:p></p:notes>")
z.writestr("ppt/slides/slide2.xml", "<p:sld><a:p><a:r><a:t>Welcome</a:t></a:r></a:p></p:sld>")
z.close()
PY
if [[ "$QUICK" == 0 ]]; then
  CREATED+=(deck-outline)
  deploy deck-outline && { r=$(run deck-outline "$WORK/deck.pptx"); has "deck-outline: real pptx read (Node in the cell)" "$(body_of "$r" 2>/dev/null)" "Say the number twice"; } || bad "deck-outline deploy"
fi

# receipt-pdf has no bundled sample: build a small PDF with a text layer and read it with the cell's poppler
python3 - "$WORK/receipt.pdf" <<'PY'
import sys
lines = ["Green Grocer  Receipt", "Order 7731  Date 2025-03-12", "Item  Oat milk  qty 2  4.50", "Subtotal 9.00", "VAT 1.80", "Total EUR 10.80", "Warranty: 2 years, returns accepted within 14 days."]
esc = lambda s: s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")
stream = "BT /F1 12 Tf 50 780 Td 16 TL " + " ".join("(%s) Tj T*" % esc(l) for l in lines) + " ET"
objs = ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        "<< /Length %d >>\nstream\n%s\nendstream" % (len(stream), stream), "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
out = "%PDF-1.4\n"; offs = []
for i, o in enumerate(objs, 1):
    offs.append(len(out)); out += "%d 0 obj\n%s\nendobj\n" % (i, o)
x = len(out)
out += "xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1) + "".join("%010d 00000 n \n" % o for o in offs)
out += "trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, x)
open(sys.argv[1], "w").write(out)
PY
if [[ "$QUICK" == 0 ]]; then
  deploy receipt-pdf && { r=$(run receipt-pdf "$WORK/receipt.pdf"); has "receipt-pdf: real PDF read (poppler in the cell)" "$(body_of "$r" 2>/dev/null)" "Total EUR 10.80"; } || bad "receipt-pdf deploy"
fi

# receipt-photo: a real image read by OCR. Needs Pillow here and tesseract in the cell template; reported as a skip, not a failure, when the
# template was baked before OCR existed (rebuild it with scripts/merion-bake-node22-agent.sh).
if [[ "$QUICK" == 0 ]] && python3 -c 'import PIL' 2>/dev/null; then
  python3 - "$WORK/receipt.png" <<'PY'
import sys
from PIL import Image, ImageDraw, ImageFont
lines = ["GREEN LEAF STORES", "Bill no 20481   Date 12/09/2026", "Item 1  Oat milk   Rs 6.40", "Subtotal Rs 416.40", "Total Rs 437.22", "Returns accepted within 14 days"]
img = Image.new("RGB", (1000, 70 + len(lines) * 52), "white"); d = ImageDraw.Draw(img); f = ImageFont.load_default(34)
for i, l in enumerate(lines): d.text((30, 30 + i * 52), l, fill="black", font=f)
img.save(sys.argv[1])
PY
  CREATED+=(receipt-photo)
  deploy receipt-photo && { r=$(run receipt-photo "$WORK/receipt.png"); if grep -q "has no tesseract" <<<"$r"; then echo "  skip receipt-photo: the cell template has no tesseract"; else has "receipt-photo: a photo read by OCR in the cell" "$(body_of "$r" 2>/dev/null)" "437.22"; fi; } || bad "receipt-photo deploy"
fi

deploy expense-sheet && { r=$(run expense-sheet "$WORK/exp.xlsx"); has "expense-sheet: real xlsx read (Node in the cell)" "$(body_of "$r" 2>/dev/null)" "Travel (2)"; } || bad "expense-sheet deploy"
deploy nda-review && { r=$(run nda-review "$WORK/nda.docx"); has "nda-review: real docx read" "$(body_of "$r" 2>/dev/null)" "governed by the laws of Portugal"; } || bad "nda-review deploy"

# ---- model packs must be refused until the operator allows an endpoint -------------------
for p in invoice-model-brief meeting-notes-model; do deploy "$p" || bad "$p deploy"; done
printf '%%PDF-1.4' > "$WORK/inv.pdf"
code=$(curl -s -o "$WORK/m.out" -w '%{http_code}' "${AUTH[@]}" -X POST -F "file=@$WORK/inv.pdf" "$KEEP_API/v1/demos/invoice-model-brief")
if [[ "$code" == "403" ]] && grep -q "refused by the vault" "$WORK/m.out"; then ok "invoice-model-brief refused by the vault (403), no cell started"; else bad "invoice-model-brief: $code $(head -c 200 "$WORK/m.out")"; fi

# ---- batch, zip, triggers, history --------------------------------------------------------
if [[ "$QUICK" == 0 ]]; then
  echo "== batch, zip, triggers, history"
  out=$("$KEEPCTL" run csv-clean "$WORK/a.csv" "$WORK/b.csv" "$WORK/c.csv" 2>&1)
  if python3 -c 'import json,sys; d=json.loads(sys.argv[1]); assert d["ok"]==3 and d["egress_connects"]==0 and len({r["result"]["session_id"] for r in d["results"]})==3' "$out" 2>/dev/null; then ok "batch of 3: one real cell per file, 0 CONNECT"; else bad "batch: $(head -c 300 <<<"$out")"; fi
  out=$("$KEEPCTL" run log-triage "$WORK/logs.zip" 2>&1)
  if python3 -c 'import json,sys; d=json.loads(sys.argv[1]); assert d["count"]==2 and d["ok"]==2 and d["egress_connects"]==0' "$out" 2>/dev/null; then ok "zip of logs: 2 files, 2 cells, the .md ignored"; else bad "zip: $(head -c 300 <<<"$out")"; fi

  tr=$(api -X POST -H 'content-type: application/json' -d '{"use_case":"csv-clean","kind":"webhook"}' "$KEEP_API/v1/triggers")
  tid=$(json id <<<"$tr"); tsec=$(json secret <<<"$tr")
  r=$("$KEEPCTL" trigger fire "$tid" "$tsec" "$WORK/a.csv" 2>&1)
  if [[ "$(json egress_connects <<<"$r" 2>/dev/null)" == "0" ]]; then ok "webhook trigger: signed call ran csv-clean in a real cell"; else bad "webhook: $(head -c 300 <<<"$r")"; fi
  api -o /dev/null -X DELETE "$KEEP_API/v1/triggers/$tid"

  ids=$("$KEEPCTL" artifacts --use-case csv-clean | grep " clean.csv" | awk '{print $1}' | head -2)
  if [[ "$(wc -l <<<"$ids" | tr -d ' ')" -ge 2 ]]; then
    d=$("$KEEPCTL" diff "$(sed -n 2p <<<"$ids")" "$(sed -n 1p <<<"$ids")" 2>&1)
    has "history: diff between two runs of csv-clean" "$d" "added"
  else bad "history: fewer than 2 csv-clean artifacts"; fi
  chain=$("$KEEPCTL" audit --limit 5 2>&1 >/dev/null)
  has "audit journal chain is intact" "$chain" '"chain_ok": true'
fi

echo
echo "passed=$PASSED failed=$FAILED"
[[ "$FAILED" == 0 ]]
