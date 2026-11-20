#!/usr/bin/env bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
# Build the two local images docker-compose.yml expects: zyvor-fabricd's own
# (a plain single-context build) and fluxvm's (which needs the sibling
# guestkit repo as a second BuildKit build context -- not something a
# docker-compose.yml `build:` block can express portably across Docker
# Compose and Podman Compose versions, so it's built here instead).
#
# Usage:
#   ./scripts/build-container-images.sh
#   BUILDER=docker ./scripts/build-container-images.sh
#   FLUXVM_DIR=/path/to/FluxVM GUESTKIT_DIR=/path/to/guestkit ./scripts/build-container-images.sh
#
# Prerequisites: FluxVM and guestkit checked out as sibling directories to
# this repo (../FluxVM, ../guestkit) unless overridden above -- the same
# layout FluxVM's own Dockerfile and CI already assume.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILDER="${BUILDER:-podman}"
TAG="${TAG:-local}"
FLUXVM_DIR="${FLUXVM_DIR:-${ROOT}/../FluxVM}"
GUESTKIT_DIR="${GUESTKIT_DIR:-${ROOT}/../guestkit}"

if ! command -v "${BUILDER}" >/dev/null; then
  echo "ERROR: ${BUILDER} not found" >&2
  exit 1
fi

if [[ ! -d "${FLUXVM_DIR}" ]]; then
  echo "ERROR: FluxVM not found at ${FLUXVM_DIR} (set FLUXVM_DIR)" >&2
  exit 1
fi

if [[ ! -d "${GUESTKIT_DIR}" ]]; then
  echo "ERROR: guestkit not found at ${GUESTKIT_DIR} (set GUESTKIT_DIR)" >&2
  exit 1
fi

build_args=()
if [[ "${BUILDER}" == "podman" ]]; then
  build_args+=(--format docker)
fi

echo "=== Building zyvor-fabricd:${TAG} ==="
"${BUILDER}" build "${build_args[@]}" -t "zyvor-fabricd:${TAG}" "${ROOT}"

echo "=== Building zyvor-fabric-fluxvm:${TAG} (context: ${FLUXVM_DIR}, +guestkit: ${GUESTKIT_DIR}) ==="
"${BUILDER}" build "${build_args[@]}" \
  --build-context "guestkit=${GUESTKIT_DIR}" \
  -t "zyvor-fabric-fluxvm:${TAG}" \
  -f "${FLUXVM_DIR}/Dockerfile" \
  "${FLUXVM_DIR}"

echo ""
echo "=== Built ==="
echo "  zyvor-fabricd:${TAG}"
echo "  zyvor-fabric-fluxvm:${TAG}"
echo ""
echo "Run: docker compose up -d   (or: podman compose up -d)"
