// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

export function defineAgent(definition) {
  if (typeof definition === "function") return definition;
  if (!definition || typeof definition.run !== "function") {
    throw new TypeError("defineAgent() requires a function or { run(ctx) }");
  }
  return definition;
}

export class Fabric {
  constructor({ baseUrl = "http://127.0.0.1:9096", token, fetch: fetchImpl = globalThis.fetch } = {}) {
    if (typeof fetchImpl !== "function") throw new TypeError("fetch implementation is required");
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.token = token;
    this.fetch = fetchImpl;
    this.sessions = {
      create: async (request) => new Session(this, await this.request("POST", "/v1/sessions", request)),
      createMany: async (requests, { concurrency = 4 } = {}) => {
        if (!Array.isArray(requests)) throw new TypeError("requests must be an array");
        if (!Number.isInteger(concurrency) || concurrency < 1) {
          throw new TypeError("concurrency must be a positive integer");
        }
        return mapConcurrent(requests, concurrency, (request) => this.sessions.create(request));
      },
      get: async (id) => new Session(this, await this.request("GET", `/v1/sessions/${id}`)),
      list: async () => (await this.request("GET", "/v1/sessions")).items.map((v) => new Session(this, v)),
    };
    this.agents = {
      list: async () => (await this.request("GET", "/v1/agents")).items,
      get: async (name) => this.request("GET", `/v1/agents/${encodeURIComponent(name)}`),
      warmPool: async (name) => this.request("GET", `/v1/agents/${encodeURIComponent(name)}/warm-pool`),
      reconcileWarmPool: async (name) => this.request("POST", `/v1/agents/${encodeURIComponent(name)}/warm-pool`, {}),
    };
    this.approvals = {
      list: async () => (await this.request("GET", "/v1/approvals")).items,
      decide: async (id, decision, { comment, scope } = {}) => {
        if (decision !== "approved" && decision !== "denied") {
          throw new TypeError("decision must be approved or denied");
        }
        if (scope !== undefined && (decision !== "approved" || !["once", "session"].includes(scope))) {
          throw new TypeError("scope must be once or session on an approved decision");
        }
        return this.request("POST", `/v1/approvals/${encodeURIComponent(id)}`, {
          decision, ...(comment === undefined ? {} : { comment }), ...(scope === undefined ? {} : { scope }),
        });
      },
    };
    this.evidence = {
      cockpit: (sessionId) => this.request("GET", `/v1/sessions/${encodeURIComponent(sessionId)}/cockpit`),
      audit: ({ sessionId, limit } = {}) => this.request("GET", `/v1/audit${query({ session_id: sessionId, limit })}`),
      receipts: async ({ userId, limit } = {}) =>
        (await this.request("GET", `/v1/receipts${query({ user_id: userId, limit })}`)).items,
      // Full export is a distinct, scoped capability. Never place the export token in a URL.
      exportAudit: ({ exportToken, sessionId, limit } = {}) => {
        if (!exportToken) throw new TypeError("exportToken is required");
        return this.request("GET", `/v1/export/audit${query({ session_id: sessionId, limit })}`, undefined,
          { "x-merion-export-token": exportToken });
      },
    };
    this.usage = ({ userId, since } = {}) =>
      this.request("GET", `/v1/usage${query({ user_id: userId, since })}`);
    this.identity = {
      whoami: () => this.request("GET", "/v1/whoami"),
      // Operator only. The caller must keep the returned token out of logs and URLs.
      mintUserToken: (userId, { scopes, ttlSeconds } = {}) => {
        if (!userId) throw new TypeError("userId is required");
        if (scopes !== undefined && (!Array.isArray(scopes) || scopes.length === 0 ||
          scopes.some((s) => !["read", "run", "approve"].includes(s)))) {
          throw new TypeError("scopes must be a nonempty list of read, run or approve");
        }
        return this.request("POST", "/v1/user-tokens", {
          user_id: userId,
          ...(scopes === undefined ? {} : { scopes }),
          ...(ttlSeconds === undefined ? {} : { ttl_seconds: ttlSeconds }),
        });
      },
      revokeUserTokens: (userId) => {
        if (!userId) throw new TypeError("userId is required");
        return this.request("POST", `/v1/users/${encodeURIComponent(userId)}/revoke-tokens`, {});
      },
    };
  }

  agent(name) {
    return {
      run: (input = {}, options = {}) => this.sessions.create({ agent: name, input, ...options }),
    };
  }

  async request(method, path, body, extraHeaders = {}) {
    const headers = { accept: "application/json", ...extraHeaders };
    if (body !== undefined) headers["content-type"] = "application/json";
    if (this.token) headers.authorization = `Bearer ${this.token}`;
    const response = await this.fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!response.ok) {
      const payload = await response.json().catch(() => ({}));
      throw new Error(payload.error || `Fabric Agent Runtime returned HTTP ${response.status}`);
    }
    if (response.status === 204) return undefined;
    return response.json();
  }
}

function query(params) {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null) search.set(key, String(value));
  }
  return search.size ? `?${search}` : "";
}

export class Session {
  constructor(client, value) {
    this.client = client;
    Object.assign(this, value);
  }

  async refresh() {
    Object.assign(this, await this.client.request("GET", `/v1/sessions/${this.id}`));
    return this;
  }

  async steer(message) {
    return this.client.request("POST", `/v1/sessions/${this.id}/steer`, { message });
  }

  async hibernate() {
    Object.assign(this, await this.client.request("POST", `/v1/sessions/${this.id}/hibernate`, {}));
    return this;
  }

  async resume() {
    Object.assign(this, await this.client.request("POST", `/v1/sessions/${this.id}/resume`, {}));
    return this;
  }

  async cancel() {
    Object.assign(this, await this.client.request("POST", `/v1/sessions/${this.id}/cancel`, {}));
    return this;
  }

  async delete() {
    await this.client.request("DELETE", `/v1/sessions/${this.id}`);
  }

  async *events({ after = 0, signal } = {}) {
    const headers = { accept: "text/event-stream" };
    if (this.client.token) headers.authorization = `Bearer ${this.client.token}`;
    const response = await this.client.fetch(`${this.client.baseUrl}/v1/sessions/${this.id}/events?after=${after}`, {
      headers,
      signal,
    });
    if (!response.ok || !response.body) throw new Error(`event stream returned HTTP ${response.status}`);

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true }).replace(/\r\n/g, "\n");
        for (;;) {
          const cut = buffer.indexOf("\n\n");
          if (cut < 0) break;
          const block = buffer.slice(0, cut);
          buffer = buffer.slice(cut + 2);
          const parsed = parseSse(block);
          if (parsed?.data) yield JSON.parse(parsed.data);
        }
      }
    } finally {
      reader.releaseLock();
    }
  }

  async result() {
    let cursor = 0;
    for await (const event of this.events({ after: cursor })) {
      cursor = event.seq;
      if (event.kind === "session.result") return event.data;
      if (event.kind === "session.failed") throw new Error(event.data?.error || "agent session failed");
      if (event.kind === "session.cancelled") throw new Error("agent session cancelled");
      if (event.kind === "session.expired") throw new Error("agent session expired");
    }
    const latest = await this.refresh();
    if (latest.status === "completed") return undefined;
    throw new Error(`session stream ended with status ${latest.status}`);
  }
}

function parseSse(block) {
  const out = {};
  for (const line of block.split("\n")) {
    if (!line || line.startsWith(":")) continue;
    const cut = line.indexOf(":");
    const key = cut < 0 ? line : line.slice(0, cut);
    const value = cut < 0 ? "" : line.slice(cut + 1).replace(/^ /, "");
    if (key === "data") out.data = out.data ? `${out.data}\n${value}` : value;
    else out[key] = value;
  }
  return out;
}


async function mapConcurrent(items, concurrency, fn) {
  const results = new Array(items.length);
  let next = 0;
  async function worker() {
    for (;;) {
      const index = next++;
      if (index >= items.length) return;
      results[index] = await fn(items[index], index);
    }
  }
  const workers = Array.from({ length: Math.min(concurrency, items.length) }, () => worker());
  await Promise.all(workers);
  return results;
}
