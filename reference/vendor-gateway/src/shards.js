// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// Shards and placement. A shard is one Keep host (FluxVM + agent-runtime). Keep does not move a user's
// cells between hosts, so a user is placed once, in their region, and stays there.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";

/** Rendezvous (highest random weight) hashing: stable when shards are added, and spreads users evenly. */
export function pickShard(userId, shards) {
  if (shards.length === 0) return null;
  let best = null;
  let bestScore = "";
  for (const s of shards) {
    const score = createHash("sha256").update(`${s.id}\0${userId}`).digest("hex");
    if (score > bestScore) { best = s; bestScore = score; }
  }
  return best;
}

export class Placement {
  /** @param shards [{id, region, url, token, capacity?}]  @param file where assignments are kept */
  constructor(shards, file) {
    this.shards = shards;
    this.file = file;
    this.assigned = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : {};
  }

  byId(id) { return this.shards.find((s) => s.id === id) ?? null; }

  /** The user's shard, placing them in `region` on first sight. Returns null when the region has none. */
  shardFor(userId, region) {
    const known = this.assigned[userId];
    if (known) return this.byId(known.shard);
    const candidates = this.shards.filter((s) => s.region === region && !s.full);
    const shard = pickShard(userId, candidates);
    if (!shard) return null;
    this.assigned[userId] = { shard: shard.id, region, placed_at: new Date().toISOString() };
    const tmp = `${this.file}.tmp`;
    writeFileSync(tmp, JSON.stringify(this.assigned, null, 2));
    renameSync(tmp, this.file);
    return shard;
  }

  usersOn(shardId) {
    return Object.entries(this.assigned).filter(([, v]) => v.shard === shardId).map(([u]) => u);
  }
}

/** Mints and caches short-lived user tokens, so the operator token never leaves the gateway. */
export class TokenBroker {
  constructor({ ttlSeconds = 900, scopes = ["read", "run", "approve"], fetchImpl = fetch } = {}) {
    this.ttl = ttlSeconds;
    this.scopes = scopes;
    this.fetch = fetchImpl;
    this.cache = new Map();
  }

  async tokenFor(shard, userId) {
    const key = `${shard.id}/${userId}`;
    const hit = this.cache.get(key);
    if (hit && hit.expires > Date.now() + 60_000) return hit.token;
    const res = await this.fetch(`${shard.url}/v1/user-tokens`, {
      method: "POST",
      redirect: "manual",
      headers: { "content-type": "application/json", authorization: `Bearer ${shard.token}` },
      body: JSON.stringify({ user_id: userId, scopes: this.scopes, ttl_seconds: this.ttl }),
    });
    if (!res.ok) throw new Error(`shard ${shard.id} would not mint a token (HTTP ${res.status})`);
    const { token } = await res.json();
    this.cache.set(key, { token, expires: Date.now() + this.ttl * 1000 });
    return token;
  }

  forget(shardId, userId) { this.cache.delete(`${shardId}/${userId}`); }
}
