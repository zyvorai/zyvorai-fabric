// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

// The device's push token is untrusted. Resolve and pin only explicitly approved
// public HTTPS destinations before sending any notification.
import { lookup as systemLookup } from "node:dns/promises";
import { request as systemRequest } from "node:https";
import { BlockList, isIP } from "node:net";

const blocked4 = new BlockList();
const blocked6 = new BlockList();
for (const [prefix, bits] of [
  ["0.0.0.0", 8], ["10.0.0.0", 8], ["100.64.0.0", 10], ["127.0.0.0", 8],
  ["169.254.0.0", 16], ["172.16.0.0", 12], ["192.0.0.0", 24],
  ["192.0.2.0", 24], ["192.168.0.0", 16], ["198.18.0.0", 15],
  ["198.51.100.0", 24], ["203.0.113.0", 24], ["224.0.0.0", 4], ["240.0.0.0", 4],
]) blocked4.addSubnet(prefix, bits, "ipv4");
for (const [prefix, bits] of [
  ["::", 3], ["4000::", 2], ["8000::", 1], ["2001:db8::", 32],
  ["2001:10::", 28], ["2002::", 16],
]) blocked6.addSubnet(prefix, bits, "ipv6");

export function isPublicIp(address) {
  const family = isIP(address);
  return family !== 0 && !(family === 4 ? blocked4.check(address, "ipv4") : blocked6.check(address, "ipv6"));
}

export function approvedUrl(raw, allowedHosts) {
  if (typeof raw !== "string" || raw.length > 2048) throw new Error("invalid push URL");
  const url = new URL(raw);
  if (url.protocol !== "https:" || url.username || url.password || url.hash ||
    (url.port && url.port !== "443") || isIP(url.hostname) ||
    url.hostname.startsWith("[") || url.hostname.endsWith(".")) {
    throw new Error("push URL must be a public HTTPS host without credentials or a custom port");
  }
  if (!Array.isArray(allowedHosts) || !allowedHosts.some((host) =>
    typeof host === "string" && host.toLowerCase() === url.hostname)) {
    throw new Error("push host is not explicitly approved");
  }
  return url;
}

export async function sendWebhook(rawUrl, notification, {
  allowedHosts = [], dnsLookup = systemLookup, httpsRequest = systemRequest,
} = {}) {
  const url = approvedUrl(rawUrl, allowedHosts);
  const addresses = await dnsLookup(url.hostname, { all: true });
  if (!Array.isArray(addresses) || addresses.length === 0 ||
    addresses.some(({ address }) => !isPublicIp(address))) {
    throw new Error("push host did not resolve exclusively to public addresses");
  }
  const { address } = addresses[0];
  const family = isIP(address);
  const body = JSON.stringify(notification);
  if (Buffer.byteLength(body) > 64 * 1024) throw new Error("push notification too large");
  await new Promise((resolve, reject) => {
    const req = httpsRequest(url, {
      method: "POST", agent: false, timeout: 5000,
      headers: { "content-type": "application/json", "content-length": Buffer.byteLength(body) },
      // HTTPS still verifies the certificate against url.hostname; DNS is never re-resolved.
      lookup: (_host, _options, callback) => callback(null, address, family),
    }, (res) => {
      res.resume();
      if (res.statusCode >= 200 && res.statusCode < 300) resolve();
      else reject(new Error(`push endpoint answered HTTP ${res.statusCode}`));
    });
    req.on("timeout", () => req.destroy(new Error("push endpoint timed out")));
    req.on("error", reject);
    req.end(body);
  });
}
