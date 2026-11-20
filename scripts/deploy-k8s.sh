#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# Local / CI deploy: kubectl apply k8s/base for zyvor-fabric.
# Credentials come from Kubernetes Secret zyvor-fabric-secrets.
# Env:
#   BUILD_IMAGES=true  — build images first
#   IMAGE_TAG=local
#   FABRIC_ADMIN_PASSWORD / ZYVOR_FABRICD_ADMIN_PASSWORD — explicit password
#   FABRIC_LAB_DEFAULTS=1 — use Admin@321 for convenient lab deploys
#   (otherwise a random password is generated; never silent Admin@321)
#   FORCE_SECRET=1 — recreate secret from defaults/env
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
NAMESPACE="zyvor-fabric"
IMAGE_TAG="${IMAGE_TAG:-local}"
NODE_PORT="${NODE_PORT:-30095}"

cd "$REPO_DIR"

echo "Deploying Zyvor Fabric to Kubernetes (namespace=${NAMESPACE})..."

kubectl apply -f k8s/base/namespace.yaml
kubectl apply -f k8s/base/fabricd-configmap.yaml

if [ "${FORCE_SECRET:-}" = "1" ] || ! kubectl get secret zyvor-fabric-secrets -n "${NAMESPACE}" &>/dev/null; then
  FABRIC_ADMIN_USERNAME="${FABRIC_ADMIN_USERNAME:-admin}" \
    ./scripts/k8s-set-admin-secret.sh --apply
else
  echo "Secret zyvor-fabric-secrets already exists (set FORCE_SECRET=1 to replace)"
fi

if [ "${BUILD_IMAGES:-}" = "true" ]; then
  echo "Building container images (IMAGE_TAG=${IMAGE_TAG})..."
  TAG="${IMAGE_TAG}" ./scripts/build-container-images.sh || {
    echo "Full image build failed — building zyvor-fabricd only..."
    BUILDER="${BUILDER:-podman}"
    if ! command -v "${BUILDER}" >/dev/null 2>&1; then
      BUILDER=docker
    fi
    "${BUILDER}" build -t "zyvor-fabricd:${IMAGE_TAG}" .
  }
fi

kubectl apply -f k8s/base/fluxvm-daemonset.yaml
kubectl apply -f k8s/base/fabricd-daemonset.yaml
kubectl apply -f k8s/base/fabricd-service.yaml

echo "Waiting for DaemonSets..."
kubectl rollout status daemonset/fluxvm -n "${NAMESPACE}" --timeout=180s || \
  echo "WARN: fluxvm DaemonSet not ready (need /dev/kvm + zyvor-fabric-fluxvm:${IMAGE_TAG})"
kubectl rollout status daemonset/zyvor-fabricd -n "${NAMESPACE}" --timeout=180s

echo ""
echo "Status:"
kubectl get pods,svc,secret -n "${NAMESPACE}"
echo ""
if [[ "${FABRIC_LAB_DEFAULTS:-}" == "1" ]] && [[ -z "${FABRIC_ADMIN_PASSWORD:-}" ]] && [[ -z "${ZYVOR_FABRICD_ADMIN_PASSWORD:-}" ]]; then
  echo "Login: admin / Admin@321  (FABRIC_LAB_DEFAULTS=1 · Secret zyvor-fabric-secrets)"
else
  echo "Login: admin / (from Secret zyvor-fabric-secrets)"
  echo "Retrieve password:"
  echo "  kubectl -n ${NAMESPACE} get secret zyvor-fabric-secrets -o jsonpath='{.data.admin-password}' | base64 -d; echo"
fi
echo "Access:"
echo "  NodePort: http://<node-ip>:${NODE_PORT}/health"
echo "  hostNetwork: http://<node-ip>:9095/health"
