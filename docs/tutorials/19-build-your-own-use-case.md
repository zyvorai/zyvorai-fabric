# Tutorial 19: Build your own Keep use case

Tutorials 17 and 18 run use cases we ship. This one builds your own and deploys it in one step,
first without code, then as a TypeScript agent.

**Level:** Beginner  
**Time:** 25 minutes  
**Needs:** A Fabric + Keep runtime with FluxVM (`./scripts/deploy keep user@host` sets one up),
Node 20+ for the CLI, `cd sdk/agent-runtime && npm ci` once.

> **Honesty.** Summaries from a declarative use case are **extractive**: no model is called and
> nothing in the file runs. Evidence class stays `software-test`. Zero CONNECT means the cell
> made no outbound connection, not "the operator cannot read the VM".

---

## Part 1: a use case with no code

Take an invoice as plain text and pull out the totals, the due date and any repeated lines.

### Option A: the console

1. Sign in, open **Keep** (`/app/merion`), and choose **Deploy your own use case → New use case**.
2. Name it `Invoice check`, choose **Text files**, extensions `txt`.
3. Add rules: *Lines that mention…* `total, amount due`; *Most repeated lines*; *Counts*.
4. Paste a few lines of a sample invoice into **Sample text** and click **Deploy use case**.

It appears in the picker with a `custom` badge. Click **Run Invoice check**: the cell comes up, the
guest reads your file, the summary is built, and the cockpit shows **0 CONNECT**.

### Option B: one command

`examples/merion-agents/invoice-check/` is the same thing as a `pack.json`:

```bash
export MERION_API=http://127.0.0.1:9096 MERION_TOKEN=...      # or ssh -L 9096:127.0.0.1:9096 user@host
./scripts/merionctl deploy examples/merion-agents/invoice-check --dry-run   # what it would do
./scripts/merionctl deploy examples/merion-agents/invoice-check --test      # deploy, run on the sample, require 0 CONNECT
```

Change a rule in `pack.json` and run the same command: the use case is replaced in place.

### What the runtime checks

A spec is data. The extractor is one of two fixed choices, the rules are bounded, unknown fields
are rejected, and you cannot shadow a built-in id. See [PACKS.md](../merion/PACKS.md) for every field
and limit.

Try to break it: add a rule kind that does not exist, or a `command` field. Both are refused.

---

## Part 2: an agent with your own code

When rules are not enough, write a TypeScript agent. It runs **inside the cell**, never on the host.

```bash
mkdir my-agent && cd my-agent
cat > agent.ts <<'TS'
import { defineAgent } from "@zyvor/fabric-agent";
export default defineAgent({
  async run(ctx) {
    ctx.emit("hello", { text: "hello from the cell" });
    return { ok: true };
  },
});
TS
cat > pack.json <<'JSON'
{ "kind": "agent", "name": "my-agent",
  "manifest": { "template": "agent-node", "egress_mode": "deny", "confinement": "strict" },
  "goal": { "title": "Say hello", "text": "Say hello." } }
JSON
```

Deploy and start a session in one command:

```bash
export MERION_POLICY_SEED=$(cat ~/.config/zyvor/keep-signer.seed)   # made by ./scripts/deploy keep
../scripts/merionctl deploy . --run
# ✓ deploy agent my-agent (signed)
# ✓ start a session and goal
# Open /app/merion/<session> in the console
```

In Keep mode the deploy is signed over the exact bytes it sends. `merionctl doctor` tells you if the
runtime is in Keep mode and has your public key registered.

### From the console instead

```bash
../scripts/merionctl bundle .        # writes my-agent.merionpack.json (signed with your seed)
```

In `/app/merion`, **Deploy an agent pack**, choose the file, **Deploy pack**. The console uploads the
signed bytes untouched; your seed never leaves your machine.

---

## Check it worked

```bash
./scripts/merionctl doctor           # runtime, Keep mode, signers, FluxVM, use-case counts
./scripts/merionctl doctor --smoke   # also runs the CSV cleanup once in a real cell
```

## Where next

- All pack fields: [PACKS.md](../merion/PACKS.md).
- Install everything on a host: `./scripts/deploy keep user@host` (needs FluxVM already running).
- Use cases that need the network or a browser are `agent` packs with an egress allowlist:
  [Tutorial 16](16-merion-workstation.md).
