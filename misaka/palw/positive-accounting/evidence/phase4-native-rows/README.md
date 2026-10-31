# Phase4: aux151 readonly信用入力の検証

2026-10-31。A0 `327393e8e` の固定row型/typed reader（本branch取込み `e4e01019d`）と、
P statement DTO `069d54007`（取込み `9fd78d4e3`）に対するA5の後続検証。
phase1/phase2/phase3の原ログ・対象sourceは変更していない。

## 検証対象と結果

| 記録 | 対象 | 結果 |
| --- | --- | --- |
| `common-native-rows.log` / `common-native-rows-result.json` | 共通契約 suite。base `e4e01019d`、P DTO取込み前 | 29試験成功（14 common / 5 funding / 2 namespace / 5 rows / 3 TIR） |
| `credit-rows-tests.log` / `credit-rows-tests-result.json` | node crateの `--no-default-features --lib rows::`。base `9fd78d4e3` とJSON記載の4 source SHA-256 | 7試験成功・exit 0 |
| `rows-clippy-initial.log` / `rows-clippy-initial-result.json` | 同じrow bridgeのlibのみ、`--no-default-features --lib --no-deps -- -D warnings` | exit 0・警告なし |

対象commitと範囲が異なるため、これらを合算したnative全体の同一candidate合格数として扱わない。
`credit-rows-tests-result.json` は正確なcommand、環境、base commit、raw log、row bridge/core/commonの4 source hashを保持する。
clippy JSONには同じ既知環境・base・4 source hashを追記した。元command/exit/raw logは保持している。
full node suite、実process、native ingress/coin lock、全native consumer、reorg/IBDの再試験記録ではない。

独立したread-only監査は `independent-review.json` に保存した。拒否の迂回経路は見つからず、
分類・7件の原出力・記録source hashの一致を確認した。監査担当によるbuildや試験再実行は行っていない。

## 実装した境界

node `rows::PositiveCreditRows` は実 `PalwChainStateV2` からcore `PositiveRowsV2::read/bundle`を呼ぶ。
独自codecやstate setterは追加していない。通常はcoreのsigned bundleを使い、失敗時だけread-only probeで
欠落componentとmalformed key/valueを特定する。Witness/Coverageには共通 `validate_witness_bundle`を使う。

7試験は次を確認する。

- public genesisでのkernel route欠落と、既存public carriageによるempty routeのrow欠落を区別する。
- ClaimIndexが指すResult欠落を識別し、旧subjectへfallbackしない。
- 切詰めvalue、誤version、key/body不一致を「欠落」へ変換しない。
- 実coreによる署名bundle読取はdata onlyであり、Matched/Passed/資金残高からnative信用を作らない。
- Result/funding署名失敗とcross-row plan不一致を区別する。
- positive witness/coverageの欠落をlocal計算完了と扱わない。
- Witness署名とcoverage linkを共通validatorで照合する。

各失敗理由は全7 `CreditUse` で一致し、read前後でstate rootを変えない。
test fixtureの`AcceptedOrigin`はA0既存row-codec試験の**DTO data**であり、実native Origin capabilityではない。
public carriageのローカルowned snapshot importはnative採用・peer import検証・実UTXO/lockの成立を意味しない。

## まだ認可できない理由

`SignedCreditInputs` はprivate fieldsを持つdata-only read結果である。
DTOから作るconstructor、任意boolのowner入力、native成功stubはなく、`NativeRowCreditGate::Authorization`は`Infallible`。
署名bundleが正常でも次の8 native ownerは未実装としてtyped missingを返す。

1. accepted origin
2. accepted prefix / import世代
3. registered expectations
4. local replay
5. live funding / capital
6. dependencies / exports
7. current Court / resources
8. actual P draw Certified

P statement DTOの署名認証は実抽選の`Certified`ではない。
`PublicCheckCertified`、旧S/K/V subject、一致したlocal replay、保存済みPassedでも代用しない。
native ownerが同じ現viewで作るassociated private capabilityの結合はA0/A4/A5/A6の次の統合段階に残る。

本番preset・有効化設定は変更していない。
