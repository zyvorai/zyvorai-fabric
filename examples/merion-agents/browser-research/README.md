# browser-research pack

Read-only Keep browser research agent: open allowlisted hosts, snapshot a11y
refs, write a markdown artifact. Never asks for passwords in chat.

## Manifest sketch

```json
{
  "template": "browser-agent",
  "resources": { "vcpus": 2, "memory_mib": 7900 },
  "home_volume": { "name": "browser-research-home", "guest_path": "/home/agent", "per_user": true },
  "confinement": "strict",
  "egress_mode": "sentinel",
  "egress_allow_hosts": ["example.com", "github.com"],
  "browser_port": 9222,
  "browser": {
    "enabled": true,
    "allow_hosts": ["example.com", "github.com"],
    "block_file_url": true,
    "max_tabs": 4,
    "snapshot_only": true,
    "downloads": "deny"
  }
}
```

## Goal plan (suggested)

1. Open `https://example.com/` via MCP `browser_open`
2. `browser_snapshot` — collect titles/refs (no HTML)
3. Write artifact `research.md` summarizing what was visible
4. Stop — no purchases, no logins

## Honesty

Evidence class stays `software-test`. Host can still see the guest VM.
