#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# ============================================================================
# deploy-k8s-all-remote.sh — Fabric K8s deploy to a remote host (Ragnarok-style)
# ============================================================================
# Flow:
#   1. Rsync repo to remote checkout
#   2. Build zyvor-fabricd (+ fluxvm if FluxVM/guestkit siblings present) unless --quick
#   3. Import images into k3s/rke2/… containerd
#   4. kubectl apply k8s/base + create secrets
#   5. Smoke NodePort :30095 and hostNetwork :9095
#
# Usage:
#   ./scripts/deploy-k8s-all-remote.sh [host] [user] [--quick]
#   ./scripts/deploy-k8s-all-remote.sh USER@HOST [--quick|--uninstall]
#   ./scripts/deploy k8s USER@HOST [--quick]
#
# Env:
#   DEPLOY_HOST, DEPLOY_USER, DEPLOY_PASS / SSHPASS, DEPLOY_DIR
#   IMAGE_TAG=local  FABRIC_ADMIN_PASSWORD  FABRIC_SKIP_FLUXVM=1
#   FABRIC_LAB_DEFAULTS=1 — use Admin@321 for convenient lab deploys
#   (otherwise generate random password; never silent Admin@321)
#   FLUXVM_DIR / GUESTKIT_DIR — local paths to rsync for fluxvm image build
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/deploy-common.sh
source "${SCRIPT_DIR}/lib/deploy-common.sh"

SSH_PORT="${SSH_PORT:-22}"
IMAGE_TAG="${IMAGE_TAG:-local}"
NAMESPACE="zyvor-fabric"
NODE_PORT="${NODE_PORT:-30095}"
FABRIC_IMAGE="zyvor-fabricd:${IMAGE_TAG}"
FLUXVM_IMAGE="zyvor-fabric-fluxvm:${IMAGE_TAG}"

QUICK=false
UNINSTALL=false
POSITIONAL=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick) QUICK=true; shift ;;
    --uninstall) UNINSTALL=true; shift ;;
    -h|--help)
      sed -n '2,20p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *) POSITIONAL+=("$1"); shift ;;
  esac
done
set -- "${POSITIONAL[@]:-}"

HOST="${DEPLOY_HOST:-}"
USER="${DEPLOY_USER:-}"
PASS="${DEPLOY_PASS:-${SSHPASS:-}}"

if [[ $# -ge 1 && "$1" == *@* ]]; then
  USER="${1%%@*}"
  HOST="${1#*@}"
  shift
elif [[ $# -ge 1 ]]; then
  # HOST USER or USER HOST
  if [[ "$1" =~ ^[0-9]+\.[0-9]+\. ]] || [[ "$1" == *.* && "$1" != *@* ]]; then
    HOST="$1"
    USER="${2:-${DEPLOY_USER:-sus}}"
    shift $(( $# >= 2 ? 2 : 1 ))
  else
    USER="$1"
    HOST="${2:-${DEPLOY_HOST:-}}"
    shift $(( $# >= 2 ? 2 : 1 ))
  fi
fi

HOST="${HOST:-${DEPLOY_HOST:-}}"
USER="${USER:-${DEPLOY_USER:-sus}}"

if [[ -z "${HOST}" ]]; then
  echo "ERROR: host required (DEPLOY_HOST or USER@HOST)" >&2
  exit 1
fi

REMOTE="${USER}@${HOST}"
REMOTE_DIR="$(fabric_remote_dir_for_user "$USER")"
# Prefer a dedicated K8s checkout under .deployment when DEPLOY_DIR unset
if [[ -z "${DEPLOY_DIR:-}" && -z "${REMOTE_DIR_OVERRIDE:-}" ]]; then
  if [[ "$USER" == "root" ]]; then
    REMOTE_DIR="/root/.deployment/zyvor-fabric"
  else
    REMOTE_DIR="/home/${USER}/.deployment/zyvor-fabric"
  fi
fi
SUDO="$(fabric_sudo_prefix_for_user "$USER")"

SSH_OPTS=(
  -o StrictHostKeyChecking=accept-new
  -o ConnectTimeout=30
  -o ServerAliveInterval=15
  -o ServerAliveCountMax=120
  -p "$SSH_PORT"
)

timestamp() { date +"%H:%M:%S"; }
now_epoch() { date +%s; }
format_duration() {
  local total="$1" mins=$((total / 60)) secs=$((total % 60))
  printf "%02dm %02ds" "${mins}" "${secs}"
}

PHASE_NUM=0
if $QUICK; then
  PHASE_TOTAL=3
else
  PHASE_TOTAL=5
fi
STEP_STARTED_AT=0
RUN_STARTED_AT="$(now_epoch)"

start_phase() {
  PHASE_NUM=$((PHASE_NUM + 1))
  STEP_STARTED_AT="$(now_epoch)"
  deploy_ui_phase "$PHASE_NUM" "$PHASE_TOTAL" "$1"
}
end_phase() {
  local elapsed=$(( $(now_epoch) - STEP_STARTED_AT ))
  deploy_ui_info "$(format_duration "${elapsed}") — done"
}

_ssh() {
  if [[ -n "${PASS}" ]] && command -v sshpass &>/dev/null; then
    SSHPASS="$PASS" sshpass -e ssh "${SSH_OPTS[@]}" "${REMOTE}" "$@"
  else
    ssh "${SSH_OPTS[@]}" "${REMOTE}" "$@"
  fi
}

_rsync() {
  local ssh_cmd="ssh ${SSH_OPTS[*]}"
  if [[ -n "${PASS}" ]] && command -v sshpass &>/dev/null; then
    ssh_cmd="sshpass -e ${ssh_cmd}"
  fi
  # shellcheck disable=SC2086
  rsync -az --delete \
    --exclude='.git' \
    --exclude='target/' --exclude='node_modules/' \
    --exclude='web/node_modules/' --exclude='web/dist/' \
    --exclude='*.qcow2' --exclude='*.iso' --exclude='*.img' \
    --exclude='*.raw' --exclude='*.vmdk' \
    -e "$ssh_cmd" \
    "$@"
}

gen_random_password() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -base64 18 2>/dev/null | tr -d '/+=' | head -c 24
    return 0
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import secrets; print(secrets.token_urlsafe(18)[:24])'
    return 0
  fi
  echo "error: need openssl or python3 to generate admin password (or set FABRIC_ADMIN_PASSWORD / FABRIC_LAB_DEFAULTS=1)" >&2
  exit 1
}

GENERATED_ADMIN_PASS=false
if [[ -n "${FABRIC_ADMIN_PASSWORD:-}" ]]; then
  ADMIN_PASS="${FABRIC_ADMIN_PASSWORD}"
elif [[ -n "${ZYVOR_FABRICD_ADMIN_PASSWORD:-}" ]]; then
  ADMIN_PASS="${ZYVOR_FABRICD_ADMIN_PASSWORD}"
elif [[ "${FABRIC_LAB_DEFAULTS:-}" == "1" ]]; then
  ADMIN_PASS='Admin@321'
else
  ADMIN_PASS="$(gen_random_password)"
  GENERATED_ADMIN_PASS=true
fi
ADMIN_USER="${FABRIC_ADMIN_USERNAME:-admin}"

fabric_build_metadata "$REPO_DIR"

if $UNINSTALL; then
  PHASE_TOTAL=1
  start_phase "Uninstall Fabric K8s namespace + checkout"
  _ssh "
    ${SUDO} kubectl delete namespace ${NAMESPACE} --wait=false 2>/dev/null || true
    rm -rf '${REMOTE_DIR}'
    echo Done
  " || true
  end_phase
  exit 0
fi

echo ""
deploy_ui_banner "Fabric K8s → ${REMOTE}" "${FABRIC_VERSION:-dev} · ${FABRIC_COMMIT:-?}"
deploy_ui_kv "📁" "Remote tree" "${REMOTE_DIR}"
deploy_ui_kv "🏷️" "Images" "${FABRIC_IMAGE} / ${FLUXVM_IMAGE}"
deploy_ui_kv "🔌" "NodePort" "${NODE_PORT}"
$QUICK && deploy_ui_note "--quick: skip image build (import/apply only)"

# ── [1] Rsync ──
start_phase "Rsync sources"
_ssh "mkdir -p '${REMOTE_DIR}'"
_rsync "${REPO_DIR}/" "${REMOTE}:${REMOTE_DIR}/"
# Optional FluxVM + guestkit siblings for image build
LOCAL_FLUXVM="${FLUXVM_DIR:-${REPO_DIR}/../FluxVM}"
LOCAL_GUESTKIT="${GUESTKIT_DIR:-${REPO_DIR}/../guestkit}"
REMOTE_PARENT="$(dirname "${REMOTE_DIR}")"
if [[ "${FABRIC_SKIP_FLUXVM:-}" != "1" && -d "${LOCAL_FLUXVM}" && -d "${LOCAL_GUESTKIT}" ]]; then
  echo "  Syncing FluxVM + guestkit siblings for fluxvm image build..."
  _ssh "mkdir -p '${REMOTE_PARENT}/FluxVM' '${REMOTE_PARENT}/guestkit'"
  _rsync "${LOCAL_FLUXVM}/" "${REMOTE}:${REMOTE_PARENT}/FluxVM/"
  _rsync "${LOCAL_GUESTKIT}/" "${REMOTE}:${REMOTE_PARENT}/guestkit/"
fi
end_phase

# Detect build + runtime tools
CTR_BUILD=$(_ssh 'if command -v podman >/dev/null 2>&1; then echo podman; elif command -v docker >/dev/null 2>&1; then echo docker; elif command -v nerdctl >/dev/null 2>&1; then echo nerdctl; else echo none; fi' | tr -d '\r')
K8S_RUNTIME=$(_ssh env "SUDO=${SUDO:-}" bash -s <<'REMOTE_K8S_RUNTIME'
if [ -x /usr/local/bin/k3s ] || command -v k3s >/dev/null 2>&1; then echo k3s
elif [ -x /usr/local/bin/rke2 ] || command -v rke2 >/dev/null 2>&1; then echo rke2
elif command -v microk8s >/dev/null 2>&1; then echo microk8s
elif [ -n "${SUDO}" ] && ${SUDO} ctr version >/dev/null 2>&1; then echo containerd
else echo generic; fi
REMOTE_K8S_RUNTIME
)
K8S_RUNTIME=$(echo "$K8S_RUNTIME" | tr -d '\r')
echo "  builder=${CTR_BUILD}  runtime=${K8S_RUNTIME}"

if [[ "${CTR_BUILD}" == "none" ]] && ! $QUICK; then
  echo "ERROR: need podman, docker, or nerdctl on remote to build images" >&2
  exit 1
fi

# ── [2] Build images ──
if ! $QUICK; then
  start_phase "Build container images"
  _ssh "
    set -e
    cd '${REMOTE_DIR}'
    export TAG='${IMAGE_TAG}'
    export BUILDER='${CTR_BUILD}'
    export FLUXVM_DIR='${REMOTE_PARENT}/FluxVM'
    export GUESTKIT_DIR='${REMOTE_PARENT}/guestkit'
    if [ -x scripts/build-container-images.sh ] && [ -d \"\$FLUXVM_DIR\" ] && [ -d \"\$GUESTKIT_DIR\" ] && [ \"${FABRIC_SKIP_FLUXVM:-}\" != \"1\" ]; then
      ./scripts/build-container-images.sh
    else
      echo 'Building zyvor-fabricd only (FluxVM siblings missing or FABRIC_SKIP_FLUXVM=1)'
      ${CTR_BUILD} build -t ${FABRIC_IMAGE} .
    fi
  " 2>&1 | sed -e 's/^/  [img] /'
  end_phase

  start_phase "Import images into cluster"
  case "${K8S_RUNTIME}" in
    k3s)
      K3S_BIN=$(_ssh "command -v k3s 2>/dev/null || echo /usr/local/bin/k3s" | tr -d '\r')
      _ssh "
        ${CTR_BUILD} save ${FABRIC_IMAGE} | ${SUDO} ${K3S_BIN} ctr -n k8s.io images import -
        ${SUDO} ${K3S_BIN} ctr -n k8s.io images tag localhost/${FABRIC_IMAGE} docker.io/library/${FABRIC_IMAGE} 2>/dev/null || true
        if ${CTR_BUILD} image exists ${FLUXVM_IMAGE} 2>/dev/null || ${CTR_BUILD} inspect ${FLUXVM_IMAGE} >/dev/null 2>&1; then
          ${CTR_BUILD} save ${FLUXVM_IMAGE} | ${SUDO} ${K3S_BIN} ctr -n k8s.io images import -
          ${SUDO} ${K3S_BIN} ctr -n k8s.io images tag localhost/${FLUXVM_IMAGE} docker.io/library/${FLUXVM_IMAGE} 2>/dev/null || true
        fi
      " 2>&1 | sed -e 's/^/  [import] /'
      ;;
    rke2)
      RKE2_BIN=$(_ssh "command -v rke2 2>/dev/null || echo /usr/local/bin/rke2" | tr -d '\r')
      _ssh "
        ${CTR_BUILD} save ${FABRIC_IMAGE} | ${SUDO} ${RKE2_BIN} ctr -n k8s.io images import -
        if ${CTR_BUILD} image exists ${FLUXVM_IMAGE} 2>/dev/null || ${CTR_BUILD} inspect ${FLUXVM_IMAGE} >/dev/null 2>&1; then
          ${CTR_BUILD} save ${FLUXVM_IMAGE} | ${SUDO} ${RKE2_BIN} ctr -n k8s.io images import -
        fi
      " 2>&1 | sed -e 's/^/  [import] /'
      ;;
    microk8s)
      _ssh "
        ${CTR_BUILD} save ${FABRIC_IMAGE} | microk8s ctr image import -
        if ${CTR_BUILD} image exists ${FLUXVM_IMAGE} 2>/dev/null || ${CTR_BUILD} inspect ${FLUXVM_IMAGE} >/dev/null 2>&1; then
          ${CTR_BUILD} save ${FLUXVM_IMAGE} | microk8s ctr image import -
        fi
      " 2>&1 | sed -e 's/^/  [import] /'
      ;;
    containerd)
      _ssh "
        ${CTR_BUILD} save ${FABRIC_IMAGE} | ${SUDO} ctr -n k8s.io images import -
        if ${CTR_BUILD} image exists ${FLUXVM_IMAGE} 2>/dev/null || ${CTR_BUILD} inspect ${FLUXVM_IMAGE} >/dev/null 2>&1; then
          ${CTR_BUILD} save ${FLUXVM_IMAGE} | ${SUDO} ctr -n k8s.io images import -
        fi
      " 2>&1 | sed -e 's/^/  [import] /'
      ;;
    *)
      echo "  WARN: runtime=${K8S_RUNTIME} — import skipped; ensure images are visible to the cluster"
      ;;
  esac
  end_phase
else
  deploy_ui_note "Skipping image build/import (--quick)"
fi

# ── Apply manifests ──
start_phase "Apply manifests + secrets"
_ssh "
  set -e
  cd '${REMOTE_DIR}'
  ${SUDO} kubectl apply -f k8s/base/namespace.yaml
  ${SUDO} kubectl apply -f k8s/base/fabricd-configmap.yaml

  # Credentials: Kubernetes Secret only (never systemd .admin_password).
  # Preserve existing Secret unless FORCE_SECRET=1.
  if [ \"${FORCE_SECRET:-}\" = \"1\" ] || ! ${SUDO} kubectl get secret zyvor-fabric-secrets -n ${NAMESPACE} >/dev/null 2>&1; then
    JWT_SECRET=\$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)
    ${SUDO} kubectl create secret generic zyvor-fabric-secrets \\
      --from-literal=admin-username='${ADMIN_USER}' \\
      --from-literal=admin-password='${ADMIN_PASS}' \\
      --from-literal=jwt-secret=\"\$JWT_SECRET\" \\
      -n ${NAMESPACE} \\
      --dry-run=client -o yaml | ${SUDO} kubectl apply -f -
    echo '[k8s] Applied Secret zyvor-fabric-secrets'
  else
    echo '[k8s] Keeping existing Secret zyvor-fabric-secrets (FORCE_SECRET=1 to replace)'
  fi

  if [ \"${FABRIC_SKIP_FLUXVM:-}\" = \"1\" ]; then
    echo '[k8s] FABRIC_SKIP_FLUXVM=1 — skipping fluxvm DaemonSet'
  else
    ${SUDO} kubectl apply -f k8s/base/fluxvm-daemonset.yaml
  fi
  ${SUDO} kubectl apply -f k8s/base/fabricd-daemonset.yaml
  ${SUDO} kubectl apply -f k8s/base/fabricd-service.yaml

  ${SUDO} kubectl set image daemonset/zyvor-fabricd zyvor-fabricd=${FABRIC_IMAGE} -n ${NAMESPACE} 2>/dev/null || true
  if ${SUDO} kubectl get daemonset fluxvm -n ${NAMESPACE} >/dev/null 2>&1; then
    ${SUDO} kubectl set image daemonset/fluxvm fluxvm=${FLUXVM_IMAGE} -n ${NAMESPACE} 2>/dev/null || true
  fi

  ${SUDO} kubectl rollout restart daemonset/zyvor-fabricd -n ${NAMESPACE} 2>/dev/null || true
  ${SUDO} kubectl rollout restart daemonset/fluxvm -n ${NAMESPACE} 2>/dev/null || true

  ${SUDO} kubectl rollout status daemonset/zyvor-fabricd -n ${NAMESPACE} --timeout=300s || true
  ${SUDO} kubectl get pods,svc,secret -n ${NAMESPACE} || true
  echo \"Login: ${ADMIN_USER} / (Secret zyvor-fabric-secrets key admin-password)\"
" 2>&1 | sed -e 's/^/  [k8s] /'
end_phase

# ── Smoke ──
start_phase "Smoke health checks"
sleep 3
code_np=$(_ssh "curl -sf -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1:${NODE_PORT}/health 2>/dev/null || echo 000" | tr -d '\r')
code_hn=$(_ssh "curl -sf -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1:9095/health 2>/dev/null || echo 000" | tr -d '\r')
echo "  NodePort :${NODE_PORT}/health → ${code_np}"
echo "  hostNetwork :9095/health → ${code_hn}"
end_phase

elapsed=$(( $(now_epoch) - RUN_STARTED_AT ))
echo ""
deploy_ui_kv "💚" "UI / API" "http://${HOST}:${NODE_PORT}/  (also :9095)"
if [[ "${FABRIC_LAB_DEFAULTS:-}" == "1" ]] && [[ -z "${FABRIC_ADMIN_PASSWORD:-}" ]] && [[ -z "${ZYVOR_FABRICD_ADMIN_PASSWORD:-}" ]]; then
  deploy_ui_kv "🔑" "Login" "${ADMIN_USER} / Admin@321 (FABRIC_LAB_DEFAULTS=1 · Secret zyvor-fabric-secrets)"
elif $GENERATED_ADMIN_PASS; then
  deploy_ui_kv "🔑" "Login" "${ADMIN_USER} / (generated · Secret zyvor-fabric-secrets)"
  echo "  Retrieve: kubectl -n ${NAMESPACE} get secret zyvor-fabric-secrets -o jsonpath='{.data.admin-password}' | base64 -d; echo"
else
  deploy_ui_kv "🔑" "Login" "${ADMIN_USER} / (from env · Secret zyvor-fabric-secrets)"
fi
deploy_ui_kv "⏱" "Total" "$(format_duration "${elapsed}")"
if [[ "${code_np}" == "200" || "${code_hn}" == "200" ]]; then
  deploy_ui_info "Deploy OK"
else
  deploy_ui_warn "Health not 200 yet — check: kubectl -n ${NAMESPACE} get pods -o wide"
fi

fabric_save_deploy_last "$REPO_DIR" "$HOST" "$USER" "k8s" || true
