// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

import { Fabric } from "@zyvor/fabric-agent";

const fabric = new Fabric({
  baseUrl: process.env.FABRIC_AGENT_URL,
  token: process.env.FABRIC_AGENT_TOKEN,
});

const before = await fabric.agents.warmPool("research-agent");
console.log("pool before", before);

const reconciled = await fabric.agents.reconcileWarmPool("research-agent");
console.log("reconciled", reconciled);

const session = await fabric.agent("research-agent").run(
  { prompt: "Explain KVM dirty-page tracking" },
  { request_id: "demo:warm:1", ttl_seconds: 900, start_policy: "prefer-warm" },
);
console.log("session", session.id, session.start_mode, session.startup_ms);
