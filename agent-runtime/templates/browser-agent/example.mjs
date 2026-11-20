#!/usr/bin/env node
// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
// Thin demo client for the Keep a11y driver (Playwright stays private).
import http from "node:http";

const PORT = Number(process.env.ZYVOR_BROWSER_DRIVER_PORT || "9230");
const url = process.argv[2] || "https://example.com/";

function call(body) {
  return new Promise((resolve, reject) => {
    const data = JSON.stringify(body);
    const req = http.request(
      {
        hostname: "127.0.0.1",
        port: PORT,
        path: "/v1/tool",
        method: "POST",
        headers: {
          "content-type": "application/json",
          "content-length": Buffer.byteLength(data),
        },
      },
      (res) => {
        const chunks = [];
        res.on("data", (c) => chunks.push(c));
        res.on("end", () => {
          try {
            resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
          } catch (e) {
            reject(e);
          }
        });
      },
    );
    req.on("error", reject);
    req.write(data);
    req.end();
  });
}

const opened = await call({ tool: "open", url });
console.log("open", opened);
const snap = await call({ tool: "snapshot", interactive: true });
console.log("snapshot nodes", (snap.nodes || []).slice(0, 8));
await call({ tool: "close" });
