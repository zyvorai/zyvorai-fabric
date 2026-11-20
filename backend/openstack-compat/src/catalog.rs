// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

use serde_json::{json, Value};

pub fn catalog(base: &str) -> Value {
    json!([
        {
            "type": "identity",
            "name": "keystone",
            "endpoints": [ep(base, "identity", "identity")]
        },
        {
            "type": "compute",
            "name": "nova",
            "endpoints": [ep(base, "compute/v2.1", "compute")]
        },
        {
            "type": "image",
            "name": "glance",
            "endpoints": [ep(base, "image", "image")]
        },
        {
            "type": "network",
            "name": "neutron",
            "endpoints": [ep(base, "network", "network")]
        },
        {
            "type": "volumev3",
            "name": "cinder",
            "endpoints": [ep(base, "volume/v3", "volume")]
        }
    ])
}

fn ep(base: &str, path: &str, iface_name: &str) -> Value {
    let url = format!("{base}/{path}");
    json!({
        "id": format!("ep-{iface_name}"),
        "interface": "public",
        "region": "RegionOne",
        "region_id": "RegionOne",
        "url": url
    })
}
