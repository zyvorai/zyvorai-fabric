// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

pub mod compiler;
pub mod enforcement;
pub mod models;

use compiler::NatCompiler;
use enforcement::NatEnforcer;

pub struct NatGateway {
    pub compiler: NatCompiler,
    pub enforcer: NatEnforcer,
}

impl Default for NatGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl NatGateway {
    pub fn new() -> Self {
        let compiler = NatCompiler::new();
        let enforcer = NatEnforcer::new();
        Self { compiler, enforcer }
    }
}
