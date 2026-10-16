// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * Shared Fabric API client for Keep packaged agents.
 *
 * Calls go through the host egress broker with `credential: "fabric-api"`.
 * The host injects `Authorization: Bearer <FABRIC_API_TOKEN>` — the guest never
 * sees the token (same honesty model as Keep 0.1 vault: host-readable).
 *
 * Set `input.fabricBase` (or env via deploy notes) to the fabricd origin, e.g.
 * `https://fabric.example:9095`. Credential injection requires HTTPS.
 */

export type AgentFetch = (
  input: string | URL,
  init?: RequestInit & { credential?: string },
) => Promise<Response>;

export interface FabricClientOptions {
  /** fabricd origin, no trailing slash. */
  baseUrl: string;
  fetch: AgentFetch;
  /** Credential grant name (default `fabric-api`). */
  credential?: string;
}

export class FabricClient {
  readonly baseUrl: string;
  private readonly fetchFn: AgentFetch;
  private readonly credential: string;

  constructor(opts: FabricClientOptions) {
    this.baseUrl = opts.baseUrl.replace(/\/$/, "");
    this.fetchFn = opts.fetch;
    this.credential = opts.credential || "fabric-api";
  }

  async request(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<{ ok: boolean; status: number; json: unknown; text: string }> {
    const url = path.startsWith("http") ? path : `${this.baseUrl}${path.startsWith("/") ? "" : "/"}${path}`;
    const init: RequestInit & { credential?: string } = {
      method,
      credential: this.credential,
      headers: body !== undefined ? { "content-type": "application/json" } : undefined,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    };
    const res = await this.fetchFn(url, init);
    const text = await res.text();
    let json: unknown = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = null;
    }
    return { ok: res.ok, status: res.status, json, text };
  }

  get(path: string) {
    return this.request("GET", path);
  }

  post(path: string, body?: unknown) {
    return this.request("POST", path, body);
  }

  put(path: string, body?: unknown) {
    return this.request("PUT", path, body);
  }

  delete(path: string) {
    return this.request("DELETE", path);
  }
}

/** Resolve fabricd base URL from session input. */
export function fabricBaseFromInput(input: Record<string, unknown> | undefined): string {
  const fromInput = typeof input?.fabricBase === "string" ? input.fabricBase.trim() : "";
  if (fromInput) return fromInput.replace(/\/$/, "");
  // Documented default for lab; override in production.
  return "https://127.0.0.1:9095";
}
