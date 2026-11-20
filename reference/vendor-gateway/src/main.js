#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// node src/main.js gateway.json
import { readFileSync } from "node:fs";
import { createGateway } from "./gateway.js";

const path = process.argv[2] ?? process.env.GATEWAY_CONFIG ?? "gateway.json";
const config = JSON.parse(readFileSync(path, "utf8"));
for (const key of ["relaySecret", "stateFile", "shards", "defaultRegion"]) {
  if (!config[key]) throw new Error(`${path}: "${key}" is required`);
}
if (Boolean(config.oidc) === Boolean(config.jwtSecret)) {
  throw new Error(`${path}: configure exactly one of "oidc" or "jwtSecret"`);
}
const { server } = createGateway(config, { log: (...a) => console.error(...a) });
const port = Number(config.port ?? process.env.PORT ?? 8443);
server.listen(port, config.host ?? "127.0.0.1", () => console.error(`vendor gateway on :${port}, ${config.shards.length} shard(s)`));
