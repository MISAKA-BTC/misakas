# A7 固定3モデルの実ファイルinventory

2026-11-01。固定Root `4753ee4beae5e245095954c6ef6e9b96fa05f361`、branch `codex/a7-real-llm-inventory-4753`。3担当が既存ファイルを並列にstream hashし、A7親担当が重複を除く808現存ファイルを再hashした。全808件のsize/SHA-256が担当記録と一致し、親担当の読取前後stat/realpathも安定していた。原evidence manifestの73件もsize/SHA-256が全件一致した。[親担当audit](parent-audit.json)に機械可読の結果とAPI source hashを保存した。

今回確認したのは実checkpointから既存converted/declared artifactまでの所在・同一性・宣言profile。Cargo、build、download、変換、推論、新native V2登録・admission、最大context全位置実行、Court算術判定、SecurityCertified/creditは実行していない。hashはデータ同一性の記録であり、計算の正しさやモデル品質・階層の審査を意味しない。

| 既存モデル | 固定revision | 元max context | 実checkpoint bytes | declared artifact bytes | 出力profile |
|---|---|---:|---:|---:|---|
| SmolLM2-360M | `f8027fd0eaeea54caa13c31d31b9fdc459c38b49` | 8192 | 723674912 | 436395392 | i32 Q24 v1 |
| SmolLM2-135M | `93efa2f097d58c2a74874c7e644dbc9b0cee75a2` | 8192 | 269060552 | 180336320 | i32 Q24 v1 |
| Pythia-70M | `a39f36b100fe8a5377810d56c3f4789b9c53ac42` | 2048 | 166029852 | 103402560 | i64 Q24 v2、自然対数単位 × 2^24 |

全path、realpath、checkpoint/config/tokenizer、converted/declared artifact、class ID、stat、照合相手、欠損、履歴は個別JSONに保存した。

- [SmolLM2-360M](smollm2-360m.json)：42ファイル、112比較、欠損・不一致・読取中変化0。
- [SmolLM2-135M](smollm2-135m.json)：61ファイル、126比較。参照欠損・identity不一致・読取中変化0。非identityのHub snapshot差1。
- [Pythia-70M](pythia-70m.json)：713 path中709現存と56照合。元narrow失敗に伴う4欠損を保持。wide主対象の必要ファイルは現存し、照合不一致0。

モデルごとのファイル数には重複したcohort共通資料を含むため、合計を親担当のunique数として扱わない。Pythiaのpartial chunk等も原状のまま個別hashを記録している。欠損4件は元narrowの `probe-corrected.palwtir`、`run/model.palwtir`、`run/pack/pack.json`、`run/registration-fixture.json` であり、wideの不足や新しい破損ではない（正確なpathはaudit参照）。

## 主対象のSHA-256

| モデル | checkpoint `model.safetensors` | declared artifact |
|---|---|---|
| 360M | `7aaff6661428bed033abba9522bec81938678642cca3181fe752b6ca9e1e540f` | `d17a82c83fcd86736feb60b05e492ebae79f5e2507600bc35ec2c024a2fe36c8` |
| 135M | `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1` | `48c72278b0d9b45c7ab92b6e70f5fbcef47fa86cc8644cd4307798354639fe90` |
| 70M wide | `ebfa4e2f18696ebd83716a0d39fe2c025f2ff8483f72a83ca59c475692fc9d15` | `2d1010327664743f240ea8d5bc089daed5a704d4340fea322d1f9cafbe009651` |

既存file sourceは `/Users/wata/Downloads/misaka-trained-llm-cohort-20261031/` 以下。Smolは各 `HuggingFaceTB__SmolLM2-*/run/`、Pythia主対象は `EleutherAI__pythia-70m/run-wide-q24-v2/`。manifestに保持されたrevision/LFS情報と実payload hashを照合したが、今回networkからpublisher情報を再取得していない。LFS pointerのGit blob IDとcheckpoint payloadのSHA-256を区別している。

## 保存した差分・制限

Smol135のretained `info.json` とcheckpoint `hub-info.json` は `spaces` 一覧だけが異なる。revision/config/siblingのbyte/hash記録は一致する。2ファイルのbyte差を訂正・上書きせず保存した。

Smolのbase convertedとdeclaredではlayout/logits scheme宣言に伴ってprogram/graph rootが異なる。base側のpack graph rootをdeclared classの根拠へ流用せず、各programを別hashとして照合した。declared class IDは既存program/layout/tokenizerと記録済みartifact inventory rootから再計算した。weight inventory rootの全再構築や算術再実行は行っていない。

Pythiaの元narrow変換はi32 Q24範囲超過でFAILED。wide初回のnative試験もstack overflowでFAILEDであり、後の明示64MiB stackによるhistorical private登録retryとは分けて保持する。過去短jobのlogits最大値・登録PASSは今回の実行成功に数えない。

Pythia plain/declared containerのprovenance `logits_scale` は `5.960464477539063e-8` / `5.960464477539064e-8` と1 f64 ULPの表記差がある。pack scale_bitsは `3e70000000000000`、programは一致する。metadataはclass identityの根拠ではない。この発見から旧metadataを変更せず、次段階でparser profileを明記する。

このread-only調査はatomic filesystem snapshotではない。stream hashと前後statで観測中の安定性を確認したが、外部publisherの追加認証や実checkpointから変換値が正しいことまで示さない。短job履歴、元max context宣言、全位置実行、登録・admission、claim creditは別の判定である。

## 引継ぎ

[local repro harness案](local-repro-proposal.md)に、4753の明示wide CPU runner・capture2・profile/metadata API、独立expected、narrow保持、role別quoteと実測、負例、次段階の完了条件を記した。Rootの実行candidate・利用target指定とconsumer接続が次の実行段階の条件になる。今回はこのinventoryと案の範囲を完了した。

旧667件のevidenceはbranch `codex/a7-79c3-fixture-fixed`、commit `8b5f5d5e317ecd1f8aff3060c5de181309cb0b63` のまま保持。旧 `run.json` SHA-256 `4c0e05c233b337e4c0f7cb87bdeef5b76fd7e0f7ae20228c04fe3e399789672c` をGit objectから再照合した。旧667件は新4753/実LLMのPASS件数に加算しない。

親担当の再照合は、このworktreeで `python3 testing/positive-court-a7/evidence/real-llm-inventory-4753-2026-11-01/audit_inventory.py`。ローカル実データが必要で、書込みは同dirの `parent-audit.json` のみ。
