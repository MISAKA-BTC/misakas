# ADR-0081: Long context — the input is a state chain

**Status:** Superseded in part — prefill 分割案は 2026-09-03 に撤回。

prompt を prefill claim の state chain に分割する方式は実装しない。既存 checkpoint は一つの claim 内に拘束され、claim をまたぐ state 連結を保証しないため。後継は [ADR-0082](0082-the-close-is-flat-in-the-context.md) の履歴を運ばない court と seat 側再計算。

単独で実装された Merkle prompt ids（`palw_prompt_ids_v1`）はこの撤回に含めず、ADR-0082 の規則に従う。
