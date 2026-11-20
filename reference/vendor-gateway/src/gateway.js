// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// The vendor gateway. It is deliberately thin: identity in, a user token out, a shard chosen once.
// Isolation is the runtime's job (a user token reaches only that user's data), so a bug here cannot
// widen a user's reach beyond what the runtime allows that token.

import http from "node:http";
import { Readable } from "node:stream";
import { toUserId, verifyJwt } from "./jwt.js";
import { OidcVerifier } from "./oidc.js";
import { defaultAdapters, handleRelay } from "./relay.js";
import { Placement, TokenBroker } from "./shards.js";

/** The only runtime routes exposed to a phone. Everything else is a 404 here (and 403 at the runtime). */
const EXPOSED = ["sessions", "approvals", "artifacts", "audit", "demos", "usage", "inbox"];

/**
 * What may follow `/api/` on its way to a shard: plain path segments only. No dot segments, no `%` (so no
 * encoded slashes or dots), no backslashes. The upstream URL is then built from a fixed base, so a request can
 * choose which exposed route it reaches but never which host.
 */
const SAFE_REST = /^[a-z]+(?:\/[A-Za-z0-9._~-]+)*\/?$/;

const send = (res, status, body) => {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
};

class BodyTooLargeError extends Error {}

function refuseRedirect(response) {
  if (response.status >= 300 && response.status < 400) throw new Error("shard redirect refused");
  return response;
}

async function readBody(req, limit = 1 << 20) {
  if (Number(req.headers["content-length"]) > limit) throw new BodyTooLargeError("body too large");
  const chunks = [];
  let size = 0;
  for await (const c of req) {
    size += c.length;
    if (size > limit) throw new BodyTooLargeError("body too large");
    chunks.push(c);
  }
  return Buffer.concat(chunks);
}

/** Per-user token bucket. */
class RateLimiter {
  constructor(perMinute) { this.perMinute = perMinute; this.buckets = new Map(); }
  allow(key, now = Date.now()) {
    const b = this.buckets.get(key) ?? { tokens: this.perMinute, at: now };
    b.tokens = Math.min(this.perMinute, b.tokens + ((now - b.at) / 60_000) * this.perMinute);
    b.at = now;
    const ok = b.tokens >= 1;
    if (ok) b.tokens -= 1;
    this.buckets.set(key, b);
    return ok;
  }
}

/**
 * @param config { jwtSecret, adminKey, relaySecret, defaultRegion, shards, stateFile, ratePerMinute }
 * @param deps   { fetchImpl?, adapters?, log?, dnsLookup?, httpsRequest? }
 */
export function createGateway(config, { fetchImpl = fetch, adapters, log = () => {}, dnsLookup, httpsRequest } = {}) {
  if (Boolean(config.oidc) === Boolean(config.jwtSecret)) {
    throw new Error("configure exactly one login verifier: oidc or jwtSecret");
  }
  const oidc = config.oidc ? new OidcVerifier(config.oidc, { fetchImpl }) : null;
  const placement = new Placement(config.shards, config.stateFile);
  const tokens = new TokenBroker({ fetchImpl });
  const limiter = new RateLimiter(config.ratePerMinute ?? 120);
  const relayAdapters = adapters ?? defaultAdapters({ log, webhookAllowedHosts: config.pushWebhookAllowedHosts, dnsLookup, httpsRequest });

  const operator = async (shard, path, init = {}) =>
    refuseRedirect(await fetchImpl(`${shard.url}${path}`, {
      ...init,
      redirect: "manual",
      headers: { "content-type": "application/json", ...(init.headers ?? {}), authorization: `Bearer ${shard.token}` },
    }));

  /** Who is calling and where their data lives, or a response has been sent. */
  async function identify(req, res) {
    let claims;
    try {
      const token = (req.headers.authorization ?? "").replace(/^Bearer /, "");
      claims = oidc ? await oidc.verify(token) : verifyJwt(token, config.jwtSecret);
    } catch {
      send(res, 401, { error: "invalid or missing login" });
      return null;
    }
    const userId = toUserId(claims.sub);
    if (!limiter.allow(userId)) { send(res, 429, { error: "slow down" }); return null; }
    const shard = placement.shardFor(userId, claims.region ?? config.defaultRegion);
    if (!shard) { send(res, 503, { error: "no capacity in your region yet" }); return null; }
    return { claims, userId, shard };
  }

  async function proxy(req, res, who, url) {
    const rest = url.pathname.replace(/^\/api\//, "");
    const first = rest.split("/")[0];
    if (!EXPOSED.includes(first)) return send(res, 404, { error: "not found" });
    if (!SAFE_REST.test(rest) || rest.split("/").some((seg) => seg === "." || seg === "..")) {
      return send(res, 400, { error: "bad path" });
    }
    const base = new URL(who.shard.url);
    const target = new URL(`/v1/${rest}`, base);
    target.search = url.search;
    if (target.origin !== base.origin) return send(res, 400, { error: "bad path" });
    const hasBody = !["GET", "HEAD"].includes(req.method);
    const bodyBytes = hasBody ? await readBody(req, 70 * 1024 * 1024) : undefined;
    const attempt = async () => {
      const token = await tokens.tokenFor(who.shard, who.userId);
      const headers = { authorization: `Bearer ${token}` };
      for (const h of ["content-type", "accept"]) if (req.headers[h]) headers[h] = req.headers[h];
      return fetchImpl(target, { method: req.method, headers, body: bodyBytes, redirect: "manual" });
    };
    let upstream = await attempt();
    if (upstream.status === 401) { tokens.forget(who.shard.id, who.userId); upstream = await attempt(); }
    refuseRedirect(upstream);
    res.writeHead(upstream.status, { "content-type": upstream.headers.get("content-type") ?? "application/json" });
    if (upstream.body) Readable.fromWeb(upstream.body).pipe(res); else res.end();
  }

  async function devices(req, res, who, url) {
    const strong = who.claims.acr === "strong";
    const parts = url.pathname.split("/").filter(Boolean); // api, devices, [id]
    if (req.method === "GET" && parts.length === 2) {
      const token = await tokens.tokenFor(who.shard, who.userId);
      const r = refuseRedirect(await fetchImpl(`${who.shard.url}/v1/users/${who.userId}/devices`, { redirect: "manual", headers: { authorization: `Bearer ${token}` } }));
      res.writeHead(r.status, { "content-type": "application/json" });
      return res.end(await r.text());
    }
    // Enrolling a key lets it approve things, so it needs a strong login (biometric, second factor).
    if (!strong) return send(res, 403, { error: "enrolling or removing a device needs a strong login" });
    if (req.method === "POST" && parts.length === 2) {
      const body = (await readBody(req)).toString();
      const r = await operator(who.shard, `/v1/users/${who.userId}/devices`, { method: "POST", body });
      res.writeHead(r.status, { "content-type": "application/json" });
      return res.end(await r.text());
    }
    if (req.method === "DELETE" && parts.length === 3) {
      const r = await operator(who.shard, `/v1/users/${who.userId}/devices/${encodeURIComponent(parts[2])}`, { method: "DELETE" });
      res.writeHead(r.status);
      return res.end();
    }
    return send(res, 404, { error: "not found" });
  }

  const server = http.createServer(async (req, res) => {
    try {
      const url = new URL(req.url, "http://gateway");
      if (url.pathname === "/healthz") return send(res, 200, { ok: true });

      if (url.pathname === "/relay/push" && req.method === "POST") {
        const raw = (await readBody(req)).toString();
        const [status, body] = await handleRelay({ secret: config.relaySecret, adapters: relayAdapters, rawBody: raw, signature: req.headers["x-zyvor-signature"] });
        return send(res, status, body);
      }

      if (url.pathname.startsWith("/admin/")) {
        if (!config.adminKey || req.headers["x-admin-key"] !== config.adminKey) return send(res, 401, { error: "admin key required" });
        if (url.pathname === "/admin/shards") {
          return send(res, 200, { shards: config.shards.map(({ token, ...s }) => ({ ...s, users: placement.usersOn(s.id).length })) });
        }
        if (url.pathname === "/admin/usage") {
          const user = url.searchParams.get("user_id");
          const at = placement.assigned[user];
          if (!at) return send(res, 404, { error: "unknown user" });
          const r = await operator(placement.byId(at.shard), `/v1/usage?user_id=${encodeURIComponent(user)}`, { method: "GET" });
          res.writeHead(r.status, { "content-type": "application/json" });
          return res.end(await r.text());
        }
        return send(res, 404, { error: "not found" });
      }

      if (url.pathname.startsWith("/api/")) {
        const who = await identify(req, res);
        if (!who) return;
        if (url.pathname === "/api/devices" || url.pathname.startsWith("/api/devices/")) return await devices(req, res, who, url);
        return await proxy(req, res, who, url);
      }
      return send(res, 404, { error: "not found" });
    } catch (e) {
      log("gateway error", e);
      if (e instanceof BodyTooLargeError) return send(res, 413, { error: "body too large" });
      return send(res, 502, { error: "upstream failure" });
    }
  });
  return { server, placement, tokens };
}
