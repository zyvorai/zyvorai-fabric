# GPU (PCI Device) Passthrough

Pass a physical PCI device — including a GPU — through to a VM using VFIO and IOMMU.

Zyvor Fabric's passthrough support is **generic PCI hotplug**: it can attach any host PCI
device (a GPU, a NIC, an NVMe controller, ...) to an already-running VM once that device is
bound to the `vfio-pci` driver. There is no GPU-specific API, no vGPU/mediated-device support,
and no `fabricctl gpu` command — driver binding, ROM handling beyond a simple on/off flag, and
guest driver setup are all done by hand, outside the platform.

---

> **Merion cells.** For an agent runtime session you do not attach devices by hand: set `"gpus": N` and `"cell_backend":
> "qemu"` in the agent manifest and FluxVM picks free GPUs that are already bound to `vfio-pci` (see
> `agent-runtime/README.md`, "GPUs"). The binding steps below still apply.

## Prerequisites

### Hardware

- CPU with IOMMU support (Intel VT-d or AMD-Vi)
- A device you can dedicate to the VM (for a GPU: one separate from the host's own display GPU)
- Motherboard with IOMMU support enabled in BIOS/UEFI

### Enable IOMMU

Add the appropriate kernel parameter:

```bash
# Intel CPU
GRUB_CMDLINE_LINUX_DEFAULT="quiet intel_iommu=on iommu=pt"

# AMD CPU
GRUB_CMDLINE_LINUX_DEFAULT="quiet amd_iommu=on iommu=pt"
```

Update bootloader and reboot:

```bash
sudo update-grub    # Debian/Ubuntu
sudo grub2-mkconfig -o /boot/grub2/grub.cfg    # Fedora/RHEL
sudo reboot
```

### Verify IOMMU

```bash
dmesg | grep -i iommu
find /sys/kernel/iommu_groups/ -type l
```

---

## List host PCI devices

```bash
curl -H "Authorization: Bearer $TOKEN" http://localhost:9095/api/system/pci-devices
```

```json
[
  {
    "address": "0000:01:00.0",
    "vendor_id": "10de",
    "device_id": "1b80",
    "vendor_name": "NVIDIA Corporation",
    "device_name": "GP104 [GeForce GTX 1080]",
    "class_name": "VGA compatible controller",
    "iommu_group": 14,
    "driver": "nvidia",
    "numa_node": 0,
    "attached_to": null
  }
]
```

This lists **every** PCI device on the host, not just GPUs — filter on `class_name` (e.g. `VGA
compatible controller`, `3D controller`) to find candidates. There is no `fabricctl` command for
this or any other step below; it's REST-only.

---

## Bind the device to VFIO

The platform will not rebind a device's driver for you — doing so automatically risks yanking a
device the host itself depends on (its GPU console, a storage or network controller sharing an
IOMMU group, etc). Bind it yourself first, ideally with `driverctl` so the override survives a
reboot:

```bash
sudo driverctl set-override 0000:02:00.0 vfio-pci
```

Or by hand, for a one-off/non-persistent bind:

```bash
echo "0000:02:00.0" | sudo tee /sys/bus/pci/devices/0000:02:00.0/driver/unbind
echo "10de 1c03" | sudo tee /sys/bus/pci/drivers/vfio-pci/new_id
```

Confirm the device now shows `"driver": "vfio-pci"` in the `GET /api/system/pci-devices` list
above — attaching it will be rejected otherwise.

---

## Attach the device to a running VM

Create and start the VM normally first (there is no `gpu_passthrough`/`gpus` field on VM
creation) — the device is attached as a hotplug operation against an already-running VM:

```bash
curl -X POST http://localhost:9095/api/vms/gaming-vm/devices/pci \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"address": "0000:02:00.0", "rombar": true}'
```

`rombar` is optional and only a boolean (enable/disable the device's own option ROM) — the API
has no way to supply a custom ROM file. If you actually need a non-default ROM dumped from the
card, that's outside the platform: dump it yourself (`echo 1 > rom; cat rom > file; echo 0 >
rom` under `/sys/bus/pci/devices/<address>/`) and feed it to QEMU directly, which this endpoint
does not support.

Detach it the same way:

```bash
curl -X DELETE http://localhost:9095/api/vms/gaming-vm/devices/pci/0000:02:00.0 \
  -H "Authorization: Bearer $TOKEN"
```

---

## Multiple devices / SLI groups

Attach devices one at a time with repeated `POST .../devices/pci` calls — there is no batch
"pass N GPUs at once" request shape.

For SLI/CrossFire, keep the devices in the same IOMMU group in mind when planning what else
shares that group (anything else in the group either has to also be passed through or isn't
available to the host at all once one member is bound to vfio-pci):

```bash
for d in /sys/kernel/iommu_groups/*/devices/*; do
    n=${d#*/iommu_groups/*}; n=${n%%/*}
    printf 'IOMMU Group %s ' "$n"
    lspci -nns "${d##*/}"
done
```

---

## Guest driver setup

This part is entirely outside the platform's API — it's the same guest-OS-level work as any
KVM/QEMU passthrough setup:

### NVIDIA on Windows

Install NVIDIA drivers normally. If you hit "Error 43", NVIDIA's consumer driver is detecting a
virtualized GPU and refusing to initialize — the usual fix is a CPU vendor-id-hiding flag QEMU
supports (`-cpu ...,hv_vendor_id=...`/hidden-KVM-signature style options). **Zyvor Fabric doesn't
expose that knob today** — there is no API or config field for it, so this workaround currently
requires modifying the QEMU invocation outside the platform, or isn't achievable at all through
the supported surface.

### NVIDIA on Linux

```bash
sudo apt install nvidia-driver-xxx    # Debian/Ubuntu
sudo dnf install akmod-nvidia         # Fedora
nvidia-smi                            # Verify
```

### AMD GPUs

Some AMD GPUs have a reset bug. Workaround:

```bash
# Kernel parameters
amd_iommu=on iommu=pt video=efifb:off

# Install vendor-reset module
git clone https://github.com/gnif/vendor-reset
cd vendor-reset && make && sudo make install
sudo modprobe vendor-reset
```

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| `400 Bad Request` on attach | Device isn't bound to `vfio-pci` yet — check `GET /api/system/pci-devices`, bind it (see above), retry |
| Black screen in guest | Verify IOMMU groups don't split the device from something it needs; try a different display port |
| Device not releasing after VM shutdown | `sudo driverctl unset-override <address>` (or `modprobe -r vfio-pci && modprobe vfio-pci`), then rebind to the host driver if you want it back |
| IOMMU group contains other devices | Use ACS override (`pcie_acs_override=downstream,multifunction`) — not recommended for production |

---

## Security considerations

- A passed-through device has direct memory access (DMA) to VM memory
- IOMMU provides isolation between the device and the rest of host memory
- Always verify driver signatures in the guest OS
- Do not pass through devices on multi-tenant hosts without understanding the DMA risk

## Not supported today

- **vGPU / mediated devices** (NVIDIA GRID, Intel GVT-g) — no support of any kind; only a whole
  physical PCI device can be passed through, not a slice of one.
- **Per-device stats** — there's no GPU/PCI-device-specific metrics endpoint; `GET
  /api/vms/:name/metrics` reports VM-level CPU/memory/disk, not per-passthrough-device data.
- **Automated driver bind/unbind** — deliberately manual (see above); the API refuses to do it
  for you.
- **Custom ROM files** — only the on/off `rombar` flag is exposed, not a `romfile` path.
