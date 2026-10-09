# Demo recordings

Regenerate with `./scripts/record-demos.sh` (needs `python3`, Pillow and `ffmpeg`). Each demo is a
`.cast.jsonl` (what a command printed, and when) rendered to `.gif`, `.mp4` and `.webm` by
`scripts/demo/cast.py`. Re-render without re-running anything: `RENDER_ONLY=1 ./scripts/record-demos.sh`.

| Demo | What it is | Honesty label |
|---|---|---|
| `keep-speculate` | A real run of the Keep runtime (`scripts/keep-e2e.sh`). The agent proposes a change, a person approves or denies it, and the demo prints what FluxVM was asked to do. | Keep is real. **FluxVM is the CI stub**, so no VM boots; the changeset body is a response captured from a real FluxVM. The frame says so. |

Rules for adding one:

- A real run records a command's actual output and timing. Do not edit the cast.
- A scripted replay is allowed only when the lab cannot run the thing, and its first frame must say
  **illustrative**. It must not print numbers that look measured (latencies, sizes, savings).
- Keep each GIF under 2 MB; the `.mp4` and `.webm` are for the site.
