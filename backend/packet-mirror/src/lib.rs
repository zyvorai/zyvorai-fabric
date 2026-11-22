// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

pub mod compiler;
pub mod enforcement;
pub mod models;

use compiler::MirrorCompiler;
use enforcement::MirrorEnforcer;

pub struct PacketMirror {
    pub compiler: MirrorCompiler,
    pub enforcer: MirrorEnforcer,
}

impl Default for PacketMirror {
    fn default() -> Self {
        Self::new()
    }
}

impl PacketMirror {
    pub fn new() -> Self {
        let compiler = MirrorCompiler::new();
        let enforcer = MirrorEnforcer::new();
        Self { compiler, enforcer }
    }
}
