// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

pub mod classifier;
pub mod enforcement;
pub mod models;

use classifier::TrafficClassifier;
use enforcement::QoSEnforcer;

pub struct TrafficShaper {
    pub classifier: TrafficClassifier,
    pub enforcer: QoSEnforcer,
}

impl Default for TrafficShaper {
    fn default() -> Self {
        Self::new()
    }
}

impl TrafficShaper {
    pub fn new() -> Self {
        let classifier = TrafficClassifier::new();
        let enforcer = QoSEnforcer::new();
        Self {
            classifier,
            enforcer,
        }
    }
}
