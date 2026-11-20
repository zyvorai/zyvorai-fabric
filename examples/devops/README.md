# DevOps examples (Fabric)

Copy these into your platform repo or call them from CI. They assume:

- Fabric API on `:9095`
- FluxVM on `:7788` (sibling [zyvorai/fluxvm](https://github.com/zyvorai/fluxvm))
- One writer (GitOps **or** Terraform **or** `fabricctl apply`)

## Local / pipeline gates

```bash
# unit (no daemon)
python3 -m unittest examples.devops.test_contract examples.devops.test_examples
bash scripts/test-devops-gate.sh

# live stack (compose or bare metal; lab is usually HTTPS + self-signed)
unset FABRIC_URL   # auto-picks https://127.0.0.1:9095 then http
export FLUXVM_URL=http://127.0.0.1:7788
bash scripts/devops-gate.sh
# or full lab pack:
#   ./scripts/test-lab-verify.sh
# production readiness (read-only; requires FABRIC_TOKEN):
#   FABRIC_TOKEN=… ./scripts/test-production-readiness.sh
fabricctl apply -f examples/devops/apply-vm.yaml
```

## GitHub Actions

See `github-actions/fabric-gates.yml`. Live job needs a self-hosted runner with KVM.

## Kubernetes GitOps

Operator must already point at fabricd. Then:

```bash
kubectl apply -k examples/devops/gitops
kubectl get vm -n team-web
```

## Terraform

```bash
cd examples/devops/terraform
terraform init
terraform plan -var="token=$FABRIC_TOKEN"
```

## Ansible

```bash
ANSIBLE_LIBRARY=sdk/ansible/plugins/modules \
  ansible-playbook examples/devops/ansible/site.yml
```

Probe contract: [docs/contracts/fabric-fluxvm-readyz.json](../../docs/contracts/fabric-fluxvm-readyz.json).
