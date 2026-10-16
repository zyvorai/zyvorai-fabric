#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""keep-approve: a stand-in phone in the terminal, for demos and tests. NOT a real phone.

    MERION_API=http://127.0.0.1:19096 MERION_TOKEN=<user token> scripts/merion-approve.py --key phone.key --device demo-phone

It shows what is waiting for you (what the host read out of the request: recipients, subject, text) and asks. Approve or deny signs the
decision with the key file (the same `keep-approval-v1` payload a real phone signs; see docs/merion/mobile/README.md) and sends it. The key sits
in a file on THIS machine, so this proves the flow, not the phone's hardware protection (a real phone keeps the key in its Secure Enclave or
StrongBox). Nothing is decided without an answer from you, unless you pass --decision for a scripted run.

    --once            look once and exit (default: keep watching until Ctrl-C)
    --decision D      approved or denied: answer every waiting approval without asking (tests)
"""
import argparse, json, os, subprocess, sys, tempfile, time, urllib.error, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
PHONE = os.path.join(HERE, "..", "sdk", "agent-runtime", "src", "phone-cli.js")


def call(host, token, method, path, body=None):
    req = urllib.request.Request(host.rstrip("/") + path, method=method, data=body, headers={"Authorization": "Bearer " + token, "content-type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        try:
            return e.code, json.loads(e.read() or b"null")
        except ValueError:
            return e.code, None


def show(a):
    print("\n" + "=" * 60)
    print(f" {a.get('kind', 'approval').upper()}  {a.get('subject') or ''}")
    p = a.get("preview")
    if p and p.get("fields"):
        w = max(len(f["label"]) for f in p["fields"])
        for f in p["fields"]:
            first, *more = str(f["value"]).split("\n")
            print(f"   {f['label']:<{w}}  {first}")
            for m in more:
                print(f"   {'':<{w}}  {m}")
    else:
        print("   " + str(a.get("prompt") or "(no details from the host)"))
    print("=" * 60)


def decide(host, token, key, device, a, decision):
    if not a.get("sign"):
        print("  the host sent no signing challenge for this approval; not deciding it", file=sys.stderr)
        return False
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(a, f)
        path = f.name
    try:
        body = subprocess.run(["node", PHONE, "decide", key, device, path, decision], check=True, capture_output=True, text=True).stdout
    finally:
        os.unlink(path)
    status, res = call(host, token, "POST", f"/v1/approvals/{a['id']}", body.encode())
    if status == 200:
        print(f"  {decision}.")
        return True
    print(f"  the host answered {status}: {(res or {}).get('error', res)}", file=sys.stderr)
    return False


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("--host", default=os.environ.get("MERION_API", os.environ.get("KEEP_API", "http://127.0.0.1:9096")))
    p.add_argument("--key", required=True, help="the device key file (keep-phone keygen)")
    p.add_argument("--device", required=True, help="the device id it was enrolled under")
    p.add_argument("--once", action="store_true")
    p.add_argument("--decision", choices=["approved", "denied"])
    a = p.parse_args(argv)
    token = os.environ.get("MERION_TOKEN", os.environ.get("KEEP_TOKEN", ""))
    if not token:
        sys.exit("set MERION_TOKEN to your user token")
    seen = set()
    while True:
        status, inbox = call(a.host, token, "GET", "/v1/inbox")
        if status != 200:
            sys.exit(f"the host answered {status} for the inbox")
        waiting = [x for x in inbox.get("pending_approvals", []) if x["id"] not in seen]
        for x in waiting:
            seen.add(x["id"])
            show(x)
            if a.decision:
                decide(a.host, token, a.key, a.device, x, a.decision)
                continue
            while True:
                ans = input("  Approve (y), deny (n), or skip (s)? ").strip().lower()
                if ans in ("y", "yes"):
                    decide(a.host, token, a.key, a.device, x, "approved"); break
                if ans in ("n", "no"):
                    decide(a.host, token, a.key, a.device, x, "denied"); break
                if ans in ("s", "skip"):
                    seen.discard(x["id"]); break
        if a.once:
            return
        time.sleep(2)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
