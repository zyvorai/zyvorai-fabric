# node22-agent

The cell template every Keep use case runs in: Ubuntu 24.04, Node 22, `poppler-utils` (for `pdftotext`), `tesseract-ocr` with the English data (for photos and screenshots) and the
FluxVM guest agent. Nothing else. Node runs the fixed extractor scripts for `.docx`, `.xlsx`, `.pptx`, `.html`,
`.eml` / `.mbox`; poppler reads PDFs; tesseract reads images. A template baked before OCR was added still works for everything else: an image
run is refused with "The template has no tesseract". All of the one-click use cases and the scenario packs in
[`examples/merion-agents/`](../../../examples/merion-agents/) run in it.

## Bake it (one command, on the FluxVM host)

```bash
./scripts/merion-bake-node22-agent.sh            # download Node, build the image, register the template
./scripts/merion-bake-node22-agent.sh --dry-run  # show the plan and what is missing; change nothing
```

Needs on the host: `fluxvm` (or `fluxctl`) on the PATH with the control plane running, `sudo`, `curl`, `python3`,
several GiB free under `/var/lib/fluxvm/images`, and a `fluxvm-guest-agent` binary. Prefer a **static** guest
agent: a binary built against a newer glibc than the guest fails with `GLIBC_x.y not found`.

Overrides (environment):

| Variable | Default | Meaning |
|---|---|---|
| `MERION_NODE_VERSION` | `22.11.0` | Node release to bake in |
| `MERION_NODE_TAR` | `/tmp/node.tar.xz` | Where the Node tarball is cached (downloaded if missing) |
| `MERION_NODE_BASE_IMG` | the Ubuntu noble URL in `build.json` | Use a local base image instead of downloading one |
| `MERION_NODE_GUEST_AGENT` | `/usr/local/bin/fluxvm-guest-agent` | Guest agent binary to bake in |
| `MERION_NODE_OUT` | `/var/lib/fluxvm/images/node22-agent.qcow2` | Image to write |
| `MERION_NODE_TEMPLATE_DIR` | `/var/lib/fluxvm/templates/node22-agent` | Where the template is registered |

Re-running skips the build if the image already exists (add `--force` to rebuild) and re-registers the template.

## Files

| File | Role |
|---|---|
| `build.json` | The image build spec (`fluxvm build-image --spec`). The bake script fills in the paths |
| `spec.json` | The template FluxVM registers: 1 vCPU, 2 GiB, tap network in a netns |
| `fluxvm-guest-agent.service` | The guest agent's systemd unit, baked into the image |

`vcpus` must be 1 and `network.mac` is required with `netns: true` (see
[Tutorial 11](../../../docs/tutorials/11-agent-runtime-quickstart.md)).

## Related

- Firecracker cells: `./scripts/merion-bake-fc-rootfs.sh` turns this image into a flat rootfs (`node22-fc`).
- Browser cells: [`../browser-agent/`](../browser-agent/README.md).

## Guest networking

The image configures its NIC with systemd-networkd and DHCP (`/etc/systemd/network/20-keep.network`, matching `en*`). A cloud image ships **no** network configuration, so before this the interface stayed down: use-case cells never noticed (the host reaches them over
vsock), but an **agent session** reaches the runtime's egress broker over IP and ended with `sandbox did not report a default gateway`. With the NIC up, FluxVM's tap+netns DHCP gives the guest an address and a default route. A template baked before this change still works for use cases.
