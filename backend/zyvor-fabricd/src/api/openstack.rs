// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

//! Thin mount of `openstack-compat` onto the Fabric daemon.
//!
//! Added by the OpenStack compatibility drop-in. Safe to keep if the crate
//! is in the workspace; remove this file and the `.nest` lines in
//! `server.rs` to disable.

use axum::Router;
use openstack_compat::Cloud;

pub fn routes(public_url: String) -> Router {
    openstack_compat::router(public_url)
}

pub fn routes_with_cloud(cloud: Cloud) -> Router {
    openstack_compat::router_with_cloud(cloud)
}
