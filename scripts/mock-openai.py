#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
"""Tiny OpenAI-compatible backend for smoke tests (no GPU, no model).

  mock-openai.py PORT NAME [CUTOFF_PERCENT]

Replies "NAME: ..." to /v1/chat/completions. When the request sets
`return_token_ids` it also returns token IDs and logprobs, like vLLM. About
CUTOFF_PERCENT percent of prompts (chosen by hash of the last user message) end
with finish_reason "length", so two instances can differ in quality.
"""
import hashlib
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT, NAME = int(sys.argv[1]), sys.argv[2]
CUTOFF = int(sys.argv[3]) if len(sys.argv) > 3 else 0


class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, code, body, ctype="application/json"):
        data = body if isinstance(body, bytes) else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("content-type", ctype)
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        self._send(200, {"object": "list", "data": [{"id": NAME, "object": "model"}]})

    def do_POST(self):
        req = json.loads(self.rfile.read(int(self.headers.get("content-length", 0))) or b"{}")
        msgs = req.get("messages") or [{"content": ""}]
        last = str(msgs[-1].get("content", ""))
        if "impartial grader" in str(msgs[0].get("content", "")):
            # Judge mode: rate "big:" answers high and everything else low.
            answer = last.split("## Assistant answer to grade", 1)[-1].strip()
            score = 0.9 if answer.startswith("big:") else 0.3
            reply = "```json\n" + json.dumps({"score": score, "reason": "mock"}) + "\n```"
            return self._send(200, {"id": "mock", "object": "chat.completion", "model": NAME,
                "choices": [{"index": 0, "message": {"role": "assistant", "content": reply}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}})
        cut = int(hashlib.sha256(last.encode()).hexdigest(), 16) % 100 < CUTOFF
        finish = "length" if cut else "stop"
        text = f"{NAME}: reply to {last[:40]}"
        toks = [101 + i for i in range(len(text.split()))]
        want_ids = bool(req.get("return_token_ids"))
        usage = {"prompt_tokens": 12, "completion_tokens": len(toks), "total_tokens": 12 + len(toks)}
        choice = {"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": finish}
        out = {"id": "mock", "object": "chat.completion", "model": NAME, "choices": [choice], "usage": usage}
        if want_ids:
            out["prompt_token_ids"] = [1, 2, 3, 4]
            choice["token_ids"] = toks
            choice["logprobs"] = {"content": [{"token": str(t), "logprob": -0.1 * (i + 1)} for i, t in enumerate(toks)]}
        if req.get("stream"):
            events = []
            for word in text.split(" "):
                events.append({"choices": [{"delta": {"content": word + " "}}]})
            events.append({"choices": [{"delta": {}, "finish_reason": finish}], "usage": usage})
            body = "".join(f"data: {json.dumps(e)}\n\n" for e in events) + "data: [DONE]\n\n"
            return self._send(200, body.encode(), "text/event-stream")
        self._send(200, out)


ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
