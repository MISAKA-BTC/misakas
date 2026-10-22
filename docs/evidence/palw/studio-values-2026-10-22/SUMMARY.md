# studio values — pin と fence（2026-10-22）

## pin した

P3（V6–V9 は決定、V10 は目安）と P4（決定値と目安）を [parameter-register.md](../../../governance/parameter-register.md) と [parameters.json](../../../governance/parameters.json) に記録した。再計算と、先行する独立検証の ADJUST は [pin-2026-10-22.md](pin-2026-10-22.md)。

統合した commit は `studio/values-p3-slash-clock` `cd8142330` と `studio/values-p4-policy-rfc` `a32a95869`（base は `g14/integration-r7` `4d6e3c4cc`）。

## 武装していない

`PALW_T12_FULL_ACTIVATION_DAA` は `None`。arming record と measured record は空。理由は [fence-readiness-2026-10-22.md](fence-readiness-2026-10-22.md)。

DAA 16,000 に今載せられる fence は無い。載せられない理由は、未実施の node E2E と出荷 binary の跨ぎ drill、値以外で拒否される前提 fence、そして V+ と allocation が同時に武装できないことである。

## この commit で実行していないこと

cargo test、node、drill、実測。P1 の計測（`studio/values-p1-measure` `13072773c`）は、X3 の目安がそれより大きいことの参照として P3 の文書にある。この pin はそれを measured record にしていない。

`codex/rfc-completion-integration`（`6549f9be2`）は、この pin に含めていない。その先頭 commit は UNCOMPILED / UNEXECUTED で、残件は horizon の精算、queue の確認、生成 scanner、実 consumer、全部が揃ったあとの一括受入である。受入を実行したことにはしない。
