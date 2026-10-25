# ADR-0080: The answer is long; the verified unit is short

**Status:** Superseded in part — 分割 claim 案は 2026-09-03 に撤回。

一つの回答を複数 claim に分割する方式は実装しない。claim をまたぐ出力の連結を認証できず、同じ prompt の後続 claim は DuplicateWork になり、分割で receipt 負担と報酬・weight の歪みが増えるため。後継は [ADR-0082](0082-the-close-is-flat-in-the-context.md) の有界 court。

既存の close chunk-group transport と、分割で経済量を増やさない条件は後継設計に引き継ぐ。
