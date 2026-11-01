# 採択済みwide Q24とA0の統合境界

2026-11-01。[RFC-0043](../../../rfc/0043-wide-q24-llm-output.md) / [ADR-0211](../../../adr/0211-wide-q24-llm-output.md)は、ユーザーが2026-10-31に両案を明示採択した既存の決定である。新しい設計採択や公開網有効化ではない。旧i32の値・class・scheme・wireを維持し、i64 Q24は別profileとして実装する。

## 今回取り込んだ資料

元checkout `/Users/wata/Downloads/MISAKA-testnet` の採択済み本文、旧提案URLの参照stub、[学習済みLLM登録実験](../../../evidence/models/trained-llm-cohort-2026-10-31.md)とそのraw evidenceを保存した。[取り込みmanifest](../../../evidence/positive-court-a0-2026-10-31/integration/wide-design-and-external-evidence-import-20261101.json)に元ファイルのSHA-256とサイズを記録し、既存evidence manifestの全73件を再照合した。原ログ・失敗run・digest・実測値は変更していない。採択本文には、実験が元checkoutでの実績であることを示すA0注記だけを追加した。

元実験はSmolLM2-360M/135Mの登録context8192、Pythia-70Mのcontext2048を維持している。Pythiaは別のi64 Q24 profileで再試験している。この資料の保有だけで、A0のPositive Court実装や最大context全位置の実行・費用が検証済みとはならない。実験側のRust sourceと保持binaryの完全な対応も、evidenceのfile hash一致だけでは証明されない。

## A0に残る実装

- `50339b43a`でadopted schemeと最終post logitsだけのi64 commitを識別するstructural foundationを統合。独立static review後、新7件と既存golden3件・golden_v2の11件、対象Clippyが成功した。[source/原log](../../../evidence/positive-court-a0-2026-10-31/integration/wide-structural-and-goldens-20261101.json)、[レビュー/Clippy](../../../evidence/positive-court-a0-2026-10-31/integration/wide-structural-review-and-clippy-20261101.json)。native admissionの既存scheme whitelistは変更していない。
- 別domainのデータroot/openingとi128 Q48の選択primitiveを`82146b3b9 / 74aac75f8`へ統合した。profile/旧入口11件・source料金2件・wide data/selection5件が同sourceで成功し、Core libraryのstrict Clippyも等価cleanup後に成功。[stage2記録](../../../evidence/positive-court-a0-2026-10-31/integration/profile-wide-stage2-review-20261101.json)。明示V2 Step leafとCPU producerは後続で統合した。capture・cell・Court・資料費用の全経路接続はまだ必要。
- `f862774d3`で登録program由来の明示profile resolverを追加。旧factsの74-byte保存形式を維持し、旧class record/StepSpace/binding入口は未対応wideを拒否する。canonical tokenizer/CUのゼロ、元の最大context、anchor由来prompt/seed/full jobを維持する。factsだけを読むnative callerへの接続は別途必要。
- A1の明示V2全範囲検査を後続で統合した。A3の一回のPanel再計算からの証拠生成・native受付への接続は引き続き必要。旧i32入口へi64をcastして受理しない。
- 同一候補での実checkpoint、元の最大文脈長、role別費用、cold/warm、複数Node・再起動の確認。

未接続consumerは拒否を維持する。元checkoutの広い未コミット差分、公開preset、既存の生成物pinや他作業の変更を一括コピーしない。実native job/role/資金/P/全consumer接続も別に必要であり、構造認識や登録成功をclaimの信用認証へ読み替えない。

## 追加統合と検証の範囲（2026-11-01）

- lowerer明示32/64指定と独立ref2のfinal i64認識はRoot `a19f6d19d` の31件で成功。旧goldenとresident/streamed変換、Q24単位・旧境界を超える値・独立3way fixtureを含む。[原記録](../../../evidence/positive-court-a0-2026-10-31/integration/wide-lowerer-20261101.json)。実学習済みcheckpointの登録試験ではない。
- schema1 recordのfull-layout wrapper `5d14e927c` とSDK pin `d6dc6f11b`、A1 V2 Step最終修正を統合。fixtureのcarry shape誤りをtest-only `017bff082` で直した後、record6件とStep/data11件は標準test stackで成功。[結合記録](../../../evidence/positive-court-a0-2026-10-31/integration/wide-sdk-record-joined-fixed-20261101.json)にはSDK標準stackの停止も保持し、全体PASSにしない。
- 同じSDK binaryは明示16MiB test stackで16件成功した。[原記録](../../../evidence/positive-court-a0-2026-10-31/integration/wide-sdk-runtime-pack-explicit-stack-20261101.json)。追加4件の標準stack個別実行は3件成功・1件停止。backtraceはtestnet-12 Params生成中の既存`apply_object_inner`のstack probeを指す。[切り分け](../../../evidence/positive-court-a0-2026-10-31/integration/sdk-standard-stack-diagnosis-20261101.json)。未最適化coreの結果であり、標準stack問題の解消・本番最小環境の認定ではない。
- `9689b3369` の明示wide CPU runnerはA1 canonical leafを用い、旧state/intermediateは4B、final i64だけ8Bとする。one-pass borrowed observer、checked logical quote、result寿命のhost leaseを保持する。owner固定sourceの49件・Clippy成功を取り込み、Root結合確認は別途実行中。capture2/Hello2は次のincrementで、実LLM・RSS・native資格は含まない。
- `afc37f144` はwide metadataを認証済みclass/artifact/layoutからfull record照合して導出し、V1 decodeのflat/Int32 fallbackへ既知wide program/headerが入ることを拒否する。旧I8/I16を含むnarrow経路は維持し、新しいauthorityを発行しない。この追加のRoot試験は実行中。

8B上界の全Court sizingとPanel証拠組立て、DA/decode reader、private court2の全経路の受付、P/資金/全consumerと実trained cohort再現は未完了。guardやdata APIの追加だけでfenceを有効化しない。

Rootの後続確認ではmetadata8・decode4・wide runner10・専用positive-panel adapter8の30件が標準test stackで成功した。[count audit](../../../evidence/positive-court-a0-2026-10-31/integration/wide-run-safety-count-audit-20261101.json) は、先行node-only指定でadapterが0件だったことを保持し除外する。A1のlayout不要row-profile viewを`941b78c86`まで統合した後、[SDK/Core/exec strict library Clippy](../../../evidence/positive-court-a0-2026-10-31/integration/wide-row-sdk-core-clippy-20261101.json) が成功。viewの`row_max_context`はrooted row由来であり、layout preimageとの一致・step geometry・admission・native権限を代替しない。


後続 `4753ee4be` では、明示V2のwide最終出力をsource-only Node全範囲検査へ接続し、A3 PALWTRC2 captureと最小work検査順序修正を統合した。同じsourceの標準test stack試験67件が成功した。[監査](../../../evidence/positive-court-a0-2026-10-31/integration/wide-node-capture-count-audit-20261101.json)。初回lint失敗と誤った再検査は保持し、7箇所の参照修正後 `b89631b38` のSDK/Core/exec library strict Clippyは成功した。[記録](../../../evidence/positive-court-a0-2026-10-31/integration/wide-node-capture-clippy-corrected-20261101.json)。別候補の旧16MiB SDK結果をこれらへ合算しない。

`5a2f18ba9` のobject dispatch分割は旧本文・guard・rollbackのsource同一性を検査した段階で、標準stack/SDK/golden再試験は未完了。A4 pure8B sizingとA1 purewide admissionはowner固定sourceの試験・レビュー段階、A3 Hello2は固定plan CPU subsetを実装中であり、native受付の許可ではない。実trained cohortの保持資料inventoryを再照合したが、新たな同候補での登録・最大context全位置・role別費用は未測定。Panelの再計算値からの証拠組立て、実native job/roles/資金/P/全consumer、復旧とcold/warm実測を完了条件として維持する。


Root `c85d5a9c9` ではSDK全16件と、旧GEN価格全14・旧close/range・新wide sizing13・状態/carriage golden・T12を含むintegration53件が標準stackで成功した。旧裁定・archive・receipt7件とscalar/golden/deadline/G14の14件も成功したが、新admissionは8成功/2失敗で、全体PASSではない。[初回監査](../../../evidence/positive-court-a0-2026-10-31/integration/wide-admission-stack-first-count-audit-20261101.json)。2件はArtifactをoperandと誤算入した試験期待を修正中で、モデル/program/context/PWUや計算規則は変更しない。厳格library Clippyと新sizing target Clippyは成功した。[原記録](../../../evidence/positive-court-a0-2026-10-31/integration/wide-admission-stack-clippy-20261101.json)。標準stackの以前のSDK停止経路は今回成功したが、全branchの最小stack保証・実LLM性能認定ではない。


後続 `7a6935de6` はtest-only operand oracle修正、macOS子process試験のblocking修正、actual fold回帰3件、DATA funds clock整合性、Hello2 prepared CPUを統合し、標準stackで107件成功した。[監査](../../../evidence/positive-court-a0-2026-10-31/integration/hello2-source-clock-corrected-count-audit-20261101.json)。初回失敗・0件filter除外・正しいtarget再実行を保持し、別候補の69件/67件等を加算しない。exec/positive-accounting-node libraryとHello2 target strict Clippyも成功した。汎用artifact startup、stream-backed Panel record署名/Node reader、native受付/実資金/P/全consumer、実trained LLMの登録と最大文脈長を維持した同候補再現は未完了。
