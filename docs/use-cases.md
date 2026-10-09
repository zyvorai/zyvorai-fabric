# Where Zyvor Fabric fits

Pick the row that sounds like your problem. Each links to the page that gets you going.

| You want to… | Use | Start here |
|---|---|---|
| Give an AI agent a computer without giving it your network | **Keep**: a sealed FluxVM microVM per run, egress rules the host sets, approvals only a person can give | [Keep](keep/README.md) |
| Let an agent try a change and have a person approve it | **Keep speculate**: the command runs in an isolated copy of the cell; its file changes wait behind an approval | [Keep](keep/README.md), `keepctl speculate` |
| Keep an API key out of the agent's reach | **Keep credentials**: secrets from a file or Vault, injected by the host; optionally handed to FluxVM's broker as a grant | [Keep credentials](keep/README.md) |
| Run a private cloud on Linux servers you own | **Fabric**: VMs, networking, storage and security from one daemon | [Quickstart](../README.md#quickstart) |
| Manage VMs from Kubernetes | The Fabric operator and its `VirtualMachine` CRDs | [Kubernetes](KUBERNETES.md) |
| Drive the same cloud from Terraform or the CLI | The same REST API behind `fabricctl`, the console, the operator and Terraform | [API](api.md) |
| Serve models next to your VMs | OpenAI-compatible inference on the same daemon (single-cluster Beta) | [AI Workloads](ai-workloads.md) |
| Run agents on Macs | **Solvor**, the native Mac client for Keep | [Solvor](../integrations/macos-keep) |
| See what a VM really costs in memory | `fabricctl memory <vm>` shows PSS; `fabricctl balloon` (Beta) reclaims guest memory | [Boundary doc](FLUXVM-FABRIC-BOUNDARY.md) |

## What each piece rests on

- **Fabric decides what should exist. FluxVM makes it exist.** Fabric talks to FluxVM over REST only, so every feature here is a documented FluxVM route. [FLUXVM-FABRIC-BOUNDARY.md](FLUXVM-FABRIC-BOUNDARY.md) lists which routes are used and which were exercised on a real FluxVM and which only against fakes.
- **Keep is the agent workstation.** It adds signed policy, approvals and an audit journal on top of a FluxVM cell. The recorded demo ([how it is made](assets/demos/README.md)) is a real Keep run against a FluxVM stub, labelled as such.

## Not yet

Be wary of anything on a landing page that is not in the table above. Fork fan-out for Keep sessions, a changeset view in the console and the Terraform and operator surfaces for the newer FluxVM features are follow-ups, not shipped.
