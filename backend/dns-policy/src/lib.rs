// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

pub mod enforcement;
pub mod models;
pub mod resolver;

use enforcement::DnsEnforcer;
use resolver::DnsResolver;

pub struct DnsManager {
    pub resolver: DnsResolver,
    pub enforcer: DnsEnforcer,
}

impl Default for DnsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsManager {
    pub fn new() -> Self {
        let resolver = DnsResolver::new();
        let enforcer = DnsEnforcer::new();
        Self { resolver, enforcer }
    }
}
