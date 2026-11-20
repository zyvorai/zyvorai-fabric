// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

pub mod collector;
pub mod enforcement;
pub mod models;

use collector::MetricsCollector;
use enforcement::AlertEvaluator;

pub struct NetMonitor {
    pub collector: MetricsCollector,
    pub evaluator: AlertEvaluator,
}

impl Default for NetMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl NetMonitor {
    pub fn new() -> Self {
        let collector = MetricsCollector::new();
        let evaluator = AlertEvaluator::new();
        Self {
            collector,
            evaluator,
        }
    }
}
