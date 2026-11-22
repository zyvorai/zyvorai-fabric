// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

pub mod compiler;
pub mod enforcement;
pub mod identity;
pub mod models;

use compiler::PolicyCompiler;
use enforcement::PolicyEnforcer;
use identity::IdentityAllocator;

pub struct PolicyEngine {
    pub allocator: IdentityAllocator,
    pub compiler: PolicyCompiler,
    pub enforcer: PolicyEnforcer,
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngine {
    pub fn new() -> Self {
        let allocator = IdentityAllocator::new();
        let compiler = PolicyCompiler::new(allocator.clone());
        let enforcer = PolicyEnforcer::new(allocator.clone());
        Self {
            allocator,
            compiler,
            enforcer,
        }
    }
}
