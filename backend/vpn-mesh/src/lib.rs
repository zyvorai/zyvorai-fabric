// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

pub mod compiler;
pub mod enforcement;
pub mod models;

use compiler::TunnelCompiler;
use enforcement::WireguardEnforcer;

pub struct VpnMesh {
    pub compiler: TunnelCompiler,
    pub enforcer: WireguardEnforcer,
}

impl Default for VpnMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl VpnMesh {
    pub fn new() -> Self {
        let compiler = TunnelCompiler::new();
        let enforcer = WireguardEnforcer::new();
        Self { compiler, enforcer }
    }
}
