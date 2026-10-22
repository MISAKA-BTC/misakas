# T4 — T01〜T15 node campaign、Mac Studio、2026-10-22

- branch `studio/t4-campaign`。base は `origin/g14/integration-r7` @ `558619700`（`34d68d7e2` を含む）。
  - `6b46c0fa4`: テスト側のみ（`consensus/src/pipeline/virtual_processor/tests/g14_t_campaign/`、`consensus/Cargo.toml` の feature 追加）。
  - `97d470a31`: 生ログ 5 本（gzip）をこのディレクトリに置いた。
- ハードウェア: Mac13,1（Mac Studio M1 Max）、32 GB、10 cpu、rustc 1.93.0。最低要件ハードウェアではない。他 agent の build と同時に実行した。
- どの T 項目も完了ではない。値が未承認の経路は本番で拒否される。

## 実行結果

| run | 内容 | 結果 |
|---|---|---|
| 1 | 修正前（lane は未 compile・未実行で書かれていた） | 15 PASS / 6 FAIL / 12 ignored |
| 2 | 1 回目の harness 修正後 | 18 PASS / 3 FAIL |
| 3 | `6b46c0fa4` | **21 PASS / 0 FAIL** / 12 ignored |
| 4 | `--features g14-t-campaign-merged --ignored` | 12 件とも FAIL。11 件は blocker 文で止まる panic、1 件は T15-full の拒否（どちらも想定どおり） |
| 名指し | T14 legacy `g14r_lg14a_a_pruned_import_after_…` | PASS |

## ignored の扱い

- 名前を実在の API に合わせられたのは T15-full の 1 件だけ（`palw_t12_arm_full_activation_at_v1`）。
- 残り 10 件が想定した API（`K::PaidIngressOpen`、`palw_shared_duty_v1`、`palw_funded_original_v1` など）は存在しない。実在するもの（tag 178 `SharedCourtOpV1`、tag 255 `MaterialIntakeV1`、`funded_escrows_v1`、tag 201–204）は TIR native の shared original にしか作用せず、kernel-route claim には作用しない。
- これらの経路の値の取得元はコンストラクタが `#[cfg(test)]` 専用なので、node build では必ず typed-missing になる。
- 想定 API の本体は OFF の feature `g14-t-campaign-intended-api` の下に残した（r7 では compile できないので有効にしない）。ignore 理由は実際の blocker に書き換え、`--ignored` では `BLOCKED: …` で panic する。
- 新しい active test `value_gates`: V1〜V5・V8 が `None` / `Err(Schedule)` であること、16,000 での full-activation が拒否されることを node crate から確認する。

## T01〜T15

| T | 実行できる部分（PASS） | 残る部分の blocker（ignored） |
|---|---|---|
| T01 | 誤った challenger、報酬は 1 回だけ、demand bond は焼却 | V2・V3・V4。kernel claim に shared original が無い |
| T02 | 沈黙は default で、固定 penalty のみ | 本番に `CourtSharedContext` を作る経路が無い。tag 178 に OpenDuty が無い。V6 |
| T03 | 偽署名・偽の Panel 完了報告は金額にも culpability にも効かない | — |
| T04 | flood は別々に上限がかかり、proof は飢えない | V5・V4・V1（M-C 実測） |
| T05 | 共謀者が全 slot を埋め直しても外部者 1 人が有罪にできる | V4、shared original |
| T06 | notice・応答とも hard 以内 | V2・V9、lease の期限が state に無い |
| T07 | −1/=/+1 を 3 つの期限で確認 | V2・V3・V8 |
| T08 | 第三者 relay が現在の鍵の署名を運ぶ | 設計未定: 指名 court signer が未実装 |
| T09 | 1 つの bond で 2 claim を合算予約 | V2、lease |
| T10 | fold だけで済む嘘を有罪にできる | V2（schedule の段階で拒否） |
| T11 | N=3 で PASS | 10,000 規模は未実行 |
| T12 | N=3 の集中で PASS | V2・V3・V10 |
| T13 | 古い claim の default は penalty のみ、独立 claim は Final | — |
| T14 | restart、reorg（両方向と深い同点）、pruned import・IBD、legacy 回帰 | V2・V3・V4・V8 |
| T15 | 旧 claim は旧規則のまま、armed ruleset は別 id で拒否 | full-activation は arming record 未 pin で拒否（最初の拒否は `palw_probabilistic_constraints_v1`）。テストは目標状態の `is_ok` のまま ignore |

## product bug

見つかっていない。7 件の失敗はすべて harness 側の誤りで、assertion は弱めていない。

1. T07: ledger は block の object を tick より先に fold するので、deadline と同じ block の応答は合法（`k2_ledger_route.rs` の規則）。期待値を直し、−1・=・+1 の 3 点を必須にした。
2. T11・T12: 割り当ては cover 後に消えるので、cover の前に読むよう直した。
3. T11 の slash 183,333,333: その producer は嘘の claim の seat でもあり、通過 receipt に署名した G-K1 責任。seat でない正直な producer は slash 0、seat を兼ねる producer は 0 より大きく claim collateral 未満、と分けて確認する。
4. T12: demander 条件（全 Panel の外）が厳しすぎた。claim ごとの外部者に直した。
5. T13: card 0 が B の seat を兼ね、B 用の signer lock が加算されていた。A の残額は `ledger_reserved_of` で厳密に確認する。
6. T14 pruned: pruning point 以下の delta 行を読もうとしていた。`agree`、UTXO 一致、main と IBD の完全比較に直した。
7. T14 reorg: 8 DAA を超える同点 reorg は `palw_reorg_strict_economic_win` が拒否する（V6 対策の設計どおり）。2 DAA 以内で両方向を確かめ、深い同点は incumbent が残ることを assert した。

## 値 pin 後に実装が要るもの（16000 に入れる前に）

- 値の取得元のコンストラクタが `#[cfg(test)]` 専用 → 本番 build で決定値を読む経路。
- kernel-route claim の shared original（T01・T05）。
- 本番で `CourtSharedContext` を作る経路と、tag 178 の OpenDuty（T02）。
- lease の期限を state に持つ（T06・T09）。
- 指名 court signer（T08、設計未定）。

## 未実行

- 10,000 規模の T11 と、規模を上げた T12。
- core crate の `shipped_presets_have_pinned_fingerprints`。
