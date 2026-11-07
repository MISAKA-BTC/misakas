# 2026-09-21 — LLM Jobs verifier matrix shows QWEN25-A16-2M

`misakascan.com` LLM Jobs “Verifier seats” used the graph-v5@512 class
(`4277d84f…`, display `Qwen 2.5 1.5B @512`). The live row is graph-v7@2097152.

| | ctx512 (removed from the matrix) | ctx2M (shown) |
| --- | --- | --- |
| catalog | `…/graph-v5@512` | `Qwen/Qwen2.5-1.5B/graph-v7@2097152` |
| class id | `4277d84f7d91528c…` | `74c67e63d9c03daa…902f7a` |
| display | QWEN25-A16 · @512 · ~1.7 GB | QWEN25-A16-2M · @2M · ~2.7 GB |

`readySeatsNow` / `requiredReadySeats` still come from `getPalwModelRegistry` for that id.
For graph-v7@2097152 the registry derives **9,992** required seats (canonical `262143+2`,
ADR-0135), not the panel floor of 7 — misakascan was correct to show `0 / 9,992`; the prose
that said "currently 7" was wrong for held 2M.
Cache token: `app.js?v=ctx2m2`.
