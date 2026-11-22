// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

pub mod compiler;
pub mod enforcement;
pub mod models;

use compiler::FirewallCompiler;
use enforcement::FirewallEnforcer;

pub struct VMFirewall {
    pub compiler: FirewallCompiler,
    pub enforcer: FirewallEnforcer,
}

impl Default for VMFirewall {
    fn default() -> Self {
        Self::new()
    }
}

impl VMFirewall {
    pub fn new() -> Self {
        let compiler = FirewallCompiler::new();
        let enforcer = FirewallEnforcer::new();
        Self { compiler, enforcer }
    }
}
