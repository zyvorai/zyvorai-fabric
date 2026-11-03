# Solvor, 60 seconds

Record with a synthetic file only (the repo has samples under `examples/merion-agents/*/sample.*`). Nothing personal on screen.

| Time | Shot | Say |
|---|---|---|
| 0:00 | Solvor opens on Agent Home; switch to **Use cases**, "Drop anything. Get answers." | "This is Solvor. Every file is read inside a sealed cell with no network." |
| 0:08 | Drag `examples/merion-agents/card-statement/sample.csv` onto the drop zone | "Drop a statement." |
| 0:14 | The suggested use case, then the sealed-cell animation | "It picks the use case and reads it in a throw-away microVM." |
| 0:24 | The result: totals by category; the green pill "0 outbound connections" | "You get the answer and the proof: zero outbound connections." |
| 0:34 | Toolbar: **Read email from browser** on the test page (`make verify-page`) | "It can read an email from your browser tab, only when you click." |
| 0:42 | The preview: code and account number hidden, use case pre-selected | "You review everything first. Codes and account numbers are redacted." |
| 0:50 | Result again | "Nothing was sent to the mail site, and Solvor never sends or deletes anything." |
| 0:55 | Settings caption: "evidence class: software-test" | "It is honest about the limits: whoever runs the host can read a cell's memory." |

Before recording: `make run MERION_HOST=... MERION_TOKEN=...` against a real host, and confirm every step on screen actually works. Do not show
Siri or the microphone until they are verified (see the TODO).
