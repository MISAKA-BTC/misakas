# ADR-0021: PALW LLM proof-of-work (`algo_id = 4`/`5`), at one block per 120 s

**Status:** Superseded — 旧 algo 4/5 の報酬・PoW 有効化方針は撤回。

出力テキストの commitment はモデルを実行せずに偽造できるため、Ollama ベースの直接 PoW は無効化された。full-logits trace も実行の証明にはならず、この旧方式を報酬・PoW として再実装／有効化しない。後継は [PALW V2](0026-palw-v2-runtime-separated-verification.md)。

旧 worker の devnet・shadow・zero-credit 観測は実装履歴であり、採用計画ではない。
