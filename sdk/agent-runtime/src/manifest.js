// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

const EGRESS_MODES = ["deny", "ask", "sentinel"];

function positiveInt(flag, value) {
  const n = Number(value);
  if (!Number.isInteger(n) || n <= 0) throw new Error(`${flag} must be a positive integer`);
  return n;
}

/**
 * Parse `host[:METHOD,METHOD][:/path,/path][:maxBytes]` into an egress rule, e.g.
 * `api.example.com:GET` or `api.example.com:POST:/v1/messages:65536`.
 */
export function parseRule(text) {
  const [host, methods = "", paths = "", max = ""] = text.split(":");
  if (!host) throw new Error(`--rule ${text}: host is required`);
  const rule = { host };
  const methodList = methods.split(",").filter(Boolean);
  const pathList = paths.split(",").filter(Boolean);
  if (methodList.length > 0) rule.methods = methodList.map((m) => m.toUpperCase());
  if (pathList.length > 0) rule.path_prefixes = pathList;
  if (max !== "") rule.max_body_bytes = positiveInt(`--rule ${text} max bytes`, max);
  return rule;
}

/**
 * Build the agent manifest sent to POST /v1/agents from parsed CLI flags.
 * Fields the runtime treats as optional are only included when a flag sets
 * them, so a deploy that uses none of the newer flags produces the same
 * manifest (and therefore the same version id) as before they existed.
 */
export function buildManifest(flags, runtime) {
  const manifest = {
    template: flags.template,
    runtime,
    credentials: flags.credential,
    egress_allow_hosts: flags.allowHost,
    allow_private_networks: flags.allowPrivateNetwork,
    runtime_port: Number(flags.runtimePort || 8080),
    ttl_seconds: flags.ttl ? Number(flags.ttl) : null,
    max_concurrent_sessions: flags.maxConcurrency ? Number(flags.maxConcurrency) : null,
    idle_hibernate_seconds: flags.idleHibernate ? Number(flags.idleHibernate) : null,
    warm_pool_size: flags.warmPool ? Number(flags.warmPool) : 0,
  };

  if (flags.egressMode !== undefined) {
    if (!EGRESS_MODES.includes(flags.egressMode)) {
      throw new Error(`--egress-mode must be one of ${EGRESS_MODES.join(", ")}`);
    }
    manifest.egress_mode = flags.egressMode;
  }
  if (flags.egressApprovalTimeout !== undefined) {
    manifest.egress_approval_timeout_seconds = positiveInt("--egress-approval-timeout", flags.egressApprovalTimeout);
  }

  const wantsHome = flags.homeVolume || flags.perUserHome || flags.homeVolumeName || flags.homePath;
  if (wantsHome) {
    const home = {};
    if (flags.homeVolumeName) home.name = flags.homeVolumeName;
    if (flags.homePath) home.guest_path = flags.homePath;
    if (flags.perUserHome) home.per_user = true;
    manifest.home_volume = home;
  }

  if (flags.vcpus !== undefined || flags.memoryMib !== undefined) {
    if (flags.vcpus === undefined || flags.memoryMib === undefined) {
      throw new Error("--vcpus and --memory-mib must be given together");
    }
    manifest.resources = {
      vcpus: positiveInt("--vcpus", flags.vcpus),
      memory_mib: positiveInt("--memory-mib", flags.memoryMib),
    };
  }

  if (flags.confinement !== undefined) {
    if (!["off", "strict"].includes(flags.confinement)) {
      throw new Error("--confinement must be off or strict");
    }
    if (flags.confinement === "strict") manifest.confinement = "strict";
  }
  if (flags.confidential !== undefined) {
    if (!["off", "auto", "required"].includes(flags.confidential)) {
      throw new Error("--confidential must be off, auto, or required");
    }
    if (flags.confidential !== "off") manifest.confidential = flags.confidential;
  }
  if (flags.innerContainer !== undefined) {
    if (!["off", "strict"].includes(flags.innerContainer)) {
      throw new Error("--inner-container must be off or strict");
    }
    if (flags.innerContainer === "strict") manifest.inner_container = "strict";
  }
  if (flags.persistent) manifest.persistent = true;
  if (flags.browserPort !== undefined) manifest.browser_port = positiveInt("--browser-port", flags.browserPort);
  if (flags.dlp) manifest.dlp = true;
  if (flags.taintTrust.length > 0 || flags.taint) {
    manifest.taint = { trusted_hosts: flags.taintTrust };
  }
  if (flags.rule.length > 0) manifest.egress_rules = flags.rule.map(parseRule);

  if (flags.skill.length > 0) manifest.skills = flags.skill;
  if (flags.skillScope) manifest.skill_scope = flags.skillScope;
  return manifest;
}
