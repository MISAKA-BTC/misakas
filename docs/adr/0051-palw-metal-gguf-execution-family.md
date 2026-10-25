# ADR-0051: The Metal/GGUF execution family — native-speed inference as half the work, quorum-verified beside the deterministic floor

**Status:** Superseded — 2026-08-26 に ADR-0053 で撤回。

Metal/GGUF の別実行 family は実装しない。対象モデルを整数 runtime で裁定できるようになり、別 family の必要性がなくなったうえ、提案した family cap・専用 panel・admission の安全条件がコードで成立していなかったため。後継は [ADR-0053](0053-palw-one-execution-family.md) の単一実行 family。
