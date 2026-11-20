# Dataplane observe-all pack

Ops scripts and helpers on top of the observe/control pack in
[`docs/dataplane-observe-pack.md`](dataplane-observe-pack.md)
(implemented in `policy_control`, not a separate `policy_observe` module).

| Feature | How |
|---------|-----|
| Explain dest:port | `fabricctl dataplane explain VM 1.1.1.1 --port 443` |
| Dry-run Guard | `fabricctl dataplane dry-run VM` |
| Templates list | `GET /api/dataplane/templates` |
| Follow | `scripts/dataplane-follow.sh 5` |
| Time-boxed Guard | `scripts/dataplane-timers.py set --vm web-1 --ttl 600` |
| Fail-closed chaos spec | `scripts/dataplane-chaos-failclosed.sh` |
| Doctor | `scripts/dataplane-doctor.sh` |
| Support bundle | `scripts/dataplane-bundle.sh /tmp/out` |
| GitOps CR | `examples/devops/gitops/dataplane-policy.yaml` |
| Terraform locals | `examples/devops/terraform/dataplane.tf` |

Not Cilium Hubble gRPC. Not Cilium-private maps.
