# zyvor-fabric Documentation

Private cloud control plane driving VM lifecycle over FluxVM's REST API — no libvirt, no systemd dependency for VM lifecycle itself.

## Start here

| Goal | Document |
|------|----------|
| **Product README** | [README.md](../README.md) |
| **Naming / clone URL** | [NAMING.md](NAMING.md) |
| **Product positioning** | [POSITIONING.md](POSITIONING.md) |
| **Product overview + metrics** | [PRODUCT_OVERVIEW.md](PRODUCT_OVERVIEW.md) |
| **Comparison matrix** (vs. libvirt/virsh, Proxmox VE) | [guides/decision-support/comparison-matrix.md](guides/decision-support/comparison-matrix.md) |
| **FAQ** | [quick-reference/faq.md](quick-reference/faq.md) |
| **Kubernetes** (DaemonSets, Helm, lab deploy) | [KUBERNETES.md](KUBERNETES.md) |
| **Docker / Podman** | [DOCKER.md](DOCKER.md) |
| **Quick start** (dev build) | [QUICKSTART.md](../QUICKSTART.md) |
| **Web UX** (marketing + `/app` console) | [web-ui.md](web-ui.md) · [ux.md](ux.md) |
| **User journeys & acceptance criteria** | [USER_STORIES.md](USER_STORIES.md) |
| **FluxVM VM driver** — config, capability matrix, gaps | [guides/vm-drivers/fluxvm.md](guides/vm-drivers/fluxvm.md) |
| **VM edge dataplane** (Network Fabric schema v4) | [fluxvm-dataplane.md](guides/vm-drivers/fluxvm-dataplane.md) · [Tutorial 09](tutorials/09-edge-dataplane.md) |
| **Service Fabric v6** (Maglev VIP LB + HA + identity/L7 policy) | [ebpf-service-fabric.md](ebpf-service-fabric.md) · [examples](examples/service-fabric-v3/) · [phase 5](service-fabric-phase5.md) · [phase 6](service-fabric-phase6.md) |
| **AI Workloads** (Beta: Janus / NVIDIA, OpenAI gateway, MIG, admit) | [ai-workloads.md](ai-workloads.md) · [Tutorial 15](tutorials/15-ai-workloads.md) |
| **Enterprise identity** (SCIM 2.0) | [scim-identity.md](scim-identity.md) |
| **OpenStack compatibility** (Keystone/Nova/…) | [openstack-compat.md](openstack-compat.md) · [Tutorial 08](tutorials/08-openstack-clients.md) |
| **Host maintenance evacuation** | [host-lifecycle.md](host-lifecycle.md) |
| **Container Groups** (FluxVM Secure Containers workloads) | [container-groups.md](container-groups.md) |
| **Agent Runtime** (sandboxed sessions, harness, schedules, session CI) | [agent-runtime/README.md](../agent-runtime/README.md) |
| **Merion approvals and telemetry** (Slack/Teams, OpenTelemetry, Android) | [chatops-approvals](merion/design/chatops-approvals.md) · [otlp-export](merion/design/otlp-export.md) · [Android app](../integrations/android-merion/README.md) |
| **Federation** (one view across Fabric daemons) | [federation.md](federation.md) |
| User page index | [user/PAGE_INDEX.md](user/PAGE_INDEX.md) |
| Full documentation catalog | [index.md](index.md) |

## Deploy cheatsheet

| Mode | Command | Docs |
|------|---------|------|
| **Ship stack (easiest)** | `./scripts/ship USER@HOST` | [Deploy guide](deploy.md#bare-metal-systemd--easiest-path) · [DEVOPS.md](DEVOPS.md) |
| Bare metal remote | `./scripts/deploy remote USER@HOST` | [Deploy guide](deploy.md) |
| **Prod readiness** | `FABRIC_TOKEN=… ./scripts/test-production-readiness.sh` | [production.md](deployment/production.md) · [DEVOPS.md](DEVOPS.md) |
| Lab verify | `./scripts/test-lab-verify.sh` | [DEVOPS.md](DEVOPS.md) |
| Lab deploy from GitHub | push to `main`, or run the Lab deploy workflow | [DEVOPS.md](DEVOPS.md) · [`.github/workflows/lab-deploy.yml`](../.github/workflows/lab-deploy.yml) |
| Kubernetes lab | `./scripts/deploy k8s USER@HOST` | [KUBERNETES.md](KUBERNETES.md) |
| Kubernetes local | `make k8s-deploy` | [KUBERNETES.md](KUBERNETES.md) |
| Helm | `helm upgrade --install … ./charts/zyvor-fabric` | [KUBERNETES.md](KUBERNETES.md#c-helm) |
| Docker / Podman | `make docker-up` | [DOCKER.md](DOCKER.md) |
| Operator (CRDs) | `operator/charts/zyvor-fabricd-operator` | [operator/README.md](../operator/README.md) |

Lab K8s ports: **30095** (NodePort UI/API), **9095** (hostNetwork), **7788** (FluxVM localhost).

## User stories

Persona-based journeys with acceptance criteria: **[USER_STORIES.md](USER_STORIES.md)**

| Persona | Focus |
|---------|-------|
| Alex (Private Cloud Admin) | VM lifecycle via the FluxVM driver |
| Morgan (Platform Engineer) | K8s operator, Helm, Terraform |
| Jordan (Developer) | CLI/API for VM operations |

## Ecosystem

Part of the [Zyvor platform stack](https://zyvor.dev):

| Product | Role |
|---------|------|
| **FluxVM** | Disposable-VM engine — Fabric's VM backend |
| **GuestKit** | Offline VM disk assurance / customization |
| **Axiom** | k8s-native private cloud control plane |
| **Ragnarok** | AI-powered KubeVirt VM management |
| **hypercluster** | Kubernetes bootstrap |
| **machina** | Bare-metal hypervisor OS |
| **hermes** | Application layer for K8s |
| **forge** | AI infrastructure on K8s |
| **packetwolf** | Network intelligence |

See also: [../README.md](../README.md)
