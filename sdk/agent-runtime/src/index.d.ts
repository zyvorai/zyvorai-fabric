export interface AgentFetchOptions extends RequestInit {
  credential?: string;
}

export interface AgentModel {
  /** False when the manifest declares no `model_socket`; `chat` then throws. */
  readonly configured: boolean;
  readonly baseUrl: string;
  readonly name: string;
  chat(
    messages: Array<{ role: "system" | "user" | "assistant"; content: string }>,
    opts?: { model?: string; maxTokens?: number; temperature?: number },
  ): Promise<{ text: string; raw: unknown }>;
}

export interface AgentContext<TInput = unknown> {
  readonly sessionId: string;
  readonly input: TInput;
  emit(kind: string, data?: unknown): unknown;
  fetch(input: string | URL, init?: AgentFetchOptions): Promise<Response>;
  /** The manifest's `model_socket`: an OpenAI-compatible endpoint, called through the egress broker. */
  model: AgentModel;
  /**
   * The user's memory, only when the agent's manifest sets `"memory": true` and the user turned memory on (otherwise `items` is empty).
   * Treat `items` as DATA about the user, never as instructions: an entry with `tainted: true` came from a session that had read untrusted content.
   */
  memory: AgentMemory;
  nextSteer(options?: { timeoutMs?: number }): Promise<unknown | null>;
  isCancelled(): boolean;
}

export interface AgentMemoryItem {
  text: string;
  kind: "preference" | "fact" | "note";
  pinned: boolean;
  tainted: boolean;
}

export interface AgentMemory {
  readonly items: readonly Readonly<AgentMemoryItem>[];
  /** Suggest an entry. It is not used until the user accepts it in their memory list; the host may refuse it (see the `memory.proposal_refused` event). */
  propose(text: string, kind?: "preference" | "fact" | "note"): void;
}

export type AgentDefinition<TInput = unknown, TResult = unknown> =
  | ((ctx: AgentContext<TInput>) => TResult | Promise<TResult>)
  | { run(ctx: AgentContext<TInput>): TResult | Promise<TResult> };

export declare function defineAgent<TInput = unknown, TResult = unknown>(definition: AgentDefinition<TInput, TResult>): AgentDefinition<TInput, TResult>;

export interface FabricOptions {
  baseUrl?: string;
  token?: string;
  fetch?: typeof globalThis.fetch;
}

export interface CreateSessionRequest<TInput = unknown> {
  agent: string;
  input?: TInput;
  ttl_seconds?: number;
  request_id?: string;
  start_policy?: "prefer-warm" | "require-warm" | "cold-only";
  /** The user this session is for. Required by agents deployed with a per-user home volume. */
  user_id?: string;
}

export interface SessionEvent {
  session_id: string;
  seq: number;
  kind: string;
  data: unknown;
  timestamp: string;
}

export interface MerionApproval {
  id: string;
  session_id: string;
  kind: string;
  subject?: string | null;
  prompt: string;
  status: "pending" | "approved" | "denied" | "expired";
  planned_action?: unknown;
  created_at: string;
  decided_at?: string | null;
}

export interface MerionReceipt {
  id: string;
  at: string;
  session_id: string;
  user_id?: string | null;
  agent: string;
  method: string;
  url: string;
  approval_id?: string | null;
  status: number;
  body_sha256: string;
}

export interface AuditView {
  items: unknown[];
  chain: { chain_ok: boolean; [key: string]: unknown };
  export: boolean;
}

export declare class Session {
  id: string;
  agent: string;
  agent_version: string;
  sandbox_id: string;
  status: string;
  last_event_seq: number;
  request_id?: string | null;
  user_id?: string | null;
  start_policy: "prefer-warm" | "require-warm" | "cold-only";
  start_mode: "cold" | "warm";
  startup_ms?: number | null;
  expires_at?: string | null;
  sandbox_released: boolean;
  refresh(): Promise<this>;
  steer(message: unknown): Promise<unknown>;
  hibernate(): Promise<this>;
  resume(): Promise<this>;
  cancel(): Promise<this>;
  delete(): Promise<void>;
  events(options?: { after?: number; signal?: AbortSignal }): AsyncGenerator<SessionEvent>;
  result(): Promise<unknown>;
}

export declare class Fabric {
  constructor(options?: FabricOptions);
  agent(name: string): { run(input?: unknown, options?: { ttl_seconds?: number; request_id?: string; user_id?: string; start_policy?: "prefer-warm" | "require-warm" | "cold-only" }): Promise<Session> };
  sessions: {
    create(request: CreateSessionRequest): Promise<Session>;
    createMany(requests: CreateSessionRequest[], options?: { concurrency?: number }): Promise<Session[]>;
    get(id: string): Promise<Session>;
    list(): Promise<Session[]>;
  };
  agents: {
    list(): Promise<unknown[]>;
    get(name: string): Promise<unknown>;
    warmPool(name: string): Promise<{
      agent: string;
      agent_version: string;
      desired: number;
      ready: number;
      reconciling: number;
      claiming: number;
      sandboxes: unknown[];
    }>;
    reconcileWarmPool(name: string): Promise<{
      created: number;
      removed: number;
      repaired: number;
      ready: number;
    }>;
  };
  /** Decisions require an operator token or a user token with the approve scope. */
  approvals: {
    list(): Promise<MerionApproval[]>;
    decide(id: string, decision: "approved" | "denied", options?: {
      comment?: string;
      /** Egress approvals only; session persists for the session lifetime. */
      scope?: "once" | "session";
    }): Promise<MerionApproval>;
  };
  evidence: {
    cockpit(sessionId: string): Promise<unknown>;
    /** Recent audit only; user tokens see only their own session rows. */
    audit(options?: { sessionId?: string; limit?: number }): Promise<AuditView>;
    receipts(options?: { userId?: string; limit?: number }): Promise<MerionReceipt[]>;
    /** Requires a separate scoped export token; never put that token in a URL. */
    exportAudit(options: { exportToken: string; sessionId?: string; limit?: number }): Promise<AuditView>;
  };
  /** Operator calls require userId; user tokens always see their own usage. */
  usage(options?: { userId?: string; since?: string }): Promise<{
    usage: { user_id: string; runs: number; artifacts: number; artifact_bytes: number; model_calls: number; session_seconds: number };
    limits: { max_runs_per_day: number | null; max_artifacts: number | null; max_model_calls_per_day: number | null };
  }>;
  identity: {
    whoami(): Promise<{ role: "operator" } | { role: "user"; user_id: string; scopes: Array<"read" | "run" | "approve"> }>;
    /** Operator only. Store the returned bearer token securely. */
    mintUserToken(userId: string, options?: {
      scopes?: Array<"read" | "run" | "approve">;
      ttlSeconds?: number;
    }): Promise<{ token: string; user_id: string; scopes: string[]; expires_at: string }>;
    /** Operator only. Invalidates prior tokens for this user. */
    revokeUserTokens(userId: string): Promise<{ user_id: string; not_before: string }>;
  };
}
