// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

import { Fabric } from "@zyvor/fabric-agent";

const fabric = new Fabric({
  baseUrl: process.env.FABRIC_AGENT_URL,
  token: process.env.FABRIC_AGENT_TOKEN,
});

const targets = ["qemu", "firecracker", "cloud-hypervisor"];
const sessions = await fabric.sessions.createMany(
  targets.map((target) => ({
    agent: "research-agent",
    input: { prompt: `Review ${target}` },
    request_id: `runtime-review:${target}`,
  })),
  { concurrency: 2 },
);

for (const session of sessions) {
  console.log(session.id, session.request_id, session.status);
}
