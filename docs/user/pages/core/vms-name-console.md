# VM Console

## Purpose

VM Console — a real, live console into a running VM, from the browser, without SSH or any other client installed.

Two transports share one page: a text **Terminal** (PTY / xterm.js) and a graphical **VNC** session (noVNC). Both use your existing Fabric session — no separate console password or exposed host port.

## When to use it

- To reach a VM that isn't network-reachable yet (no IP assigned, no SSH configured)
- To watch or interact with boot output, a serial console, or a full graphical desktop
- To debug a VM that's otherwise unresponsive over the network
- To confirm a guest came up after create/start without leaving the Fabric UI
- During installers or desktop environments where a text shell is not enough (use **VNC**)

## How to get there

- Route / id: `/app/vms/:name/console`
- From a VM's detail page, click **Console** in the header or open the Console tab
- Nav: reached via **Core → Virtual Machines**, not linked directly from the top nav
- Shortcuts: [Favorites](favorites.md) and [VM Browser](vm-browser.md) also offer Console / detail jumps

## Operate from the console (UX)

The page has two tabs:

1. **Terminal** — interactive shell (xterm.js) streamed over the VM's PTY. Type as you would in any terminal; output renders live.
2. **VNC** — graphical framebuffer (noVNC) for installers, desktops, or anything that draws to the screen.

Operator checks:

3. Confirm the VM is **running** on its detail page before expecting input; wait a few seconds after start if the session connects then stalls (QEMU/FluxVM still coming up).
4. Prefer Terminal for recovery shells and cloud-init logs; switch to VNC when you need a GUI or boot menu.
5. If the frame stays black or disconnects: VM stopped, FluxVM unreachable, or console/VNC backend not ready — check Dashboard VM-driver health and retry.
6. Closing the browser tab ends your view of the session; it does not stop the guest.

**Empty / fail:** Error banner, empty table, or failed toast — confirm you are signed in, `zyvor-fabricd` is healthy (`/readyz` on `http://127.0.0.1:<port>` or `https://<host>`), and any backend this page needs (FluxVM, storage, network) is reachable. See [Admin basics](../../admin-basics.md).

**Success:** Live data loads without error; creates/updates appear in the list or detail view and any confirmation toast clears cleanly.

## Related pages

- [Virtual Machines](vms.md)
- [Favorites](favorites.md)
- [VM Browser](vm-browser.md)
- [Create VM](create.md)
- [Dashboard](home.md)
- [Getting Started](../../getting-started.md)
- [Page index](../../PAGE_INDEX.md)
