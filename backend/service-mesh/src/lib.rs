// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

pub mod compiler;
pub mod enforcement;
pub mod health;
pub mod models;

use compiler::ServiceCompiler;
use enforcement::ServiceEnforcer;
use health::HealthChecker;

pub struct ServiceMesh {
    pub compiler: ServiceCompiler,
    pub enforcer: ServiceEnforcer,
}

impl Default for ServiceMesh {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceMesh {
    pub fn new() -> Self {
        let health_checker = HealthChecker::new();
        let compiler = ServiceCompiler::new(health_checker);
        let enforcer = ServiceEnforcer::new();
        Self { compiler, enforcer }
    }
}
