// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

//! OpenStack-compatible control plane for Zyvor Fabric.
//!
//! Mount the router returned by [`router`] onto `zyvor-fabricd`:
//!
//! ```ignore
//! .nest("/identity", openstack_compat::identity_router(cloud.clone()))
//! .nest("/compute", openstack_compat::compute_router(cloud.clone()))
//! .nest("/image", openstack_compat::image_router(cloud.clone()))
//! .nest("/network", openstack_compat::network_router(cloud.clone()))
//! .nest("/volume", openstack_compat::volume_router(cloud.clone()))
//! ```
//!
//! Or use [`router`] which already nests those prefixes.

pub mod catalog;
pub mod compute;
pub mod identity;
pub mod image;
pub mod network;
pub mod store;
pub mod volume;

pub use store::Cloud;

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn identity_router(cloud: Cloud) -> Router {
    identity::router(cloud)
}
pub fn compute_router(cloud: Cloud) -> Router {
    compute::router(cloud)
}
pub fn image_router(cloud: Cloud) -> Router {
    image::router(cloud)
}
pub fn network_router(cloud: Cloud) -> Router {
    network::router(cloud)
}
pub fn volume_router(cloud: Cloud) -> Router {
    volume::router(cloud)
}

/// Combined OpenStack surface for a single Fabric daemon port.
pub fn router(public_url: impl Into<String>) -> Router {
    let cloud = Cloud::new(public_url);
    router_with_cloud(cloud)
}

pub fn router_with_cloud(cloud: Cloud) -> Router {
    Router::new()
        .route("/", get(root_versions))
        .nest("/identity", identity::router(cloud.clone()))
        .nest("/compute", compute::router(cloud.clone()))
        .nest("/image", image::router(cloud.clone()))
        .nest("/network", network::router(cloud.clone()))
        .nest("/volume", volume::router(cloud))
}

async fn root_versions() -> Json<Value> {
    Json(json!({
        "versions": [
            {"id": "identity.v3", "status": "CURRENT", "links": [{"rel": "self", "href": "/identity/v3/"}]},
            {"id": "compute.v2.1", "status": "CURRENT", "links": [{"rel": "self", "href": "/compute/v2.1/"}]},
            {"id": "image.v2", "status": "CURRENT", "links": [{"rel": "self", "href": "/image/v2/images"}]},
            {"id": "network.v2.0", "status": "CURRENT", "links": [{"rel": "self", "href": "/network/v2.0/networks"}]},
            {"id": "volume.v3", "status": "CURRENT", "links": [{"rel": "self", "href": "/volume/v3/"}]}
        ]
    }))
}
