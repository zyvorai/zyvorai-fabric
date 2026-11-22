#!/bin/bash
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
set -e

# Create dev directories
mkdir -p /tmp/zyvor-fabricd/images

# Run daemon in dev mode
cd backend
RUST_LOG=debug cargo run --bin zyvor-fabricd
