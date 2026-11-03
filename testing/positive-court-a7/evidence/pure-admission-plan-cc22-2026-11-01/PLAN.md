# A7 実3classのpure admission追加harness案

2026-11-01、固定Root `cc22b0da13784eb03a023cbd1781969577883d9e` のread-only調査。**今回の成果はexact APIと実行計画のみ**。追加Cargo・型検査・モデル変換・pure admission実行はしていない。RootのCPU/capture実行とは別の検査であり、モデル品質・階層の審査を追加しない。

## exact API

以下は固定commitの `consensus/core/src/` 内。引数・返却型・source hashは[api-and-inputs.json](api-and-inputs.json)にも保存する。

| 工程 | API | 返却／意味 |
|---|---|---|
| narrow pure gate | `palw_tir_admission_v1::verify_class_admission_v10(bundle, rules, registration, certified, chain_certified)` | `Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError>` |
| explicit wide pure gate | `palw_tir_admission_v2::verify_tir_class_admission_v2`、同じ引数 | 同じ返却型。local rulesのcourt semantics version 2が必要 |
| narrow constructor | `palw_tir_admission_v1::palw_tir_post_genesis_registration_v1` | `Result<PalwConsensusObjectV2, PalwClassAdmissionError>` |
| wide constructor | `palw_tir_admission_v2::palw_tir_post_genesis_registration_v2` | 同じtag61 DTO。元wide profileのcanonical全文一致を要求 |
| rules-at-height preflight | `palw_tir_registration_preflight_at_v1` / `..._v2` | `(params, bundle, object, daa_score, chain_certified)`。Paramsからrulesを取得。署名・collateral・target・processor share policyの検証ではない |
| narrow sizing | `palw_tir_close_size_v1::palw_tir_worst_closes_work_v1(space, inventory, job, sizing)` と `palw_tir_close_range_v1::palw_tir_worst_closes_range_work_v1` | `Result<(Vec<PalwTirCloseBoundV1>, u64), String>` |
| wide sizing | `palw_tir_close_size_v2::{palw_tir_worst_closes_work_v2, palw_tir_worst_closes_range_work_v2}(space, job, sizing)` | `Result<PalwTirCloseBoundsV2, String>`。`bounds()` と `sizing_work()`。V1 boundへ変換するAPIはない |

constructorの10引数は両版とも `class, canonical, artifact_root, share_permille, initial_target, slash_value_per_pwu, activation_daa, registrant_bond, signature, ladder`。constructorは署名を検査せず、そのまま格納する。返ったDTOは登録・native権限・creditの成功ではない。

## 入力と未保有項目

既存3モデルのdeclared artifact・元revision・SHA・program/layout/tokenizer/class IDを維持する。base/early artifactへのfallback、モデルの選び直し、再変換、context/値/fee/tierの変更をしない。

原 `registration-fixture.json` はclass-only exportで、3件とも `registration_performed:false`。上記constructorのshare・target・slash・activation・bond・signatureを含まない。調査したretained native JSONはtxid/sink/state root等の要約で、真正な元carrier/object bytesは確認できなかった。旧Downloadsの実験sourceもcarrier等をメモリ内で作り、保存は要約だけ。旧private wide環境・Params変更とtest harness keyによる新規署名を現Rootへ流用しない。

従って、真正carrier、または出所を保持した実登録フィールドが揃うまでconstructorとfull pure gateの実cohort正例は **`NOT_RUN_MISSING_REGISTRATION_INPUTS`** とする。test用bond/signature、share0、架空origin、合成family認証で成功を作らない。旧private登録のsummaryを現native V2成功へ読み替えない。

full gateにはさらに固定されたactual Params・DAA・bundle・ladder・genesis/chain family-certified factsが必要。shareが非0なら既存のfamily coverageを適用する。欠損を空リストで隠したり、shareを0へ変えて迂回しない。モデル単位の追加審査は行わない。

## 先行できるclass-only解析

1. fixtureのbounded読込、元program/layoutのstrict復号、既存declared artifactとの完全一致を確認する。startupの割当前上限が未成立のloaderは引き続きquoteの外として扱い、外枠processのtimeout/RSS観測を別記録する。
2. narrowは既存facts/canonical API、wideは `ResolvedTirJobProfileV2::new(...).ctx()` と `PalwTirStepSpaceV2::new` を使う。元class ID、layout/history上限、profile、canonical全文、geometry、canonical/worst leaf countを出力する。resolverの成功だけでadmissionとしない。

葉数解析のcaller指定diagnostic capと、真正なbundleから取得するprotocol ladderは別欄へ保存する。後者が不明ならladder適合はNOT_CHECKED。短CPU試験のleaf capや任意に大きなdiagnostic capでprotocol適合を代替しない。

| class | 元layout最大位置 | 登録canonical P,D | canonical実行位置数 | canonical token budget |
|---|---:|---|---:|---:|
| Smol360 / Smol135 | 8192 | 1023,2 | 1024 | 8193 |
| Pythia70 wide | 2048 | 255,2 | 256 | 2049 |

`palw_tir_attempt_v1` の既存規約ではlayoutは `P+D-1` の位置数、contextは `P+D` のtoken数を数える。`layout.max_context+1` はこの既存canonical規約であり、元モデルのlayout/historyを書き換えるものではない。CPU短jobのP16/D1とそのtoken budgetを登録canonicalへ流用しない。

3. full-context sizingはcanonical自体を保持し、pure verifier内部と同じ**解析用別probe** `P=1, D=original layout.max_context, max_context_tokens=u32::MAX` で行う。実行位置数は元layout最大位置のまま。この内部解析probeを新しいjob権限やモデルcontext拡大として渡さない。model CPUを実行して全位置coverageを確認した結果でもない。

## 8B close envelopeと失敗記録

wideは `PalwTirCloseSizingV2 { form: Multiproof, court: model_rules.court.is_some(), cap: 1<<26, stop_above: None }`。court設定・window・element/range twinは元rulesの導出に合わせる。rules未指定ならその依存を記録し、都合のよいParamsを選ばない。element/range双方を分析する場合もwork値と結果を別記録する。

このcapはsizing workと一部metadata cardinalityの上限で、RAM/native resourceの予約ではない。read map・range cache・serialization buffer等の同時保持はcaller側で別途予算を確認する。

V2は全step laneを保守的に8Bで見積もる。実際のmixed-width commitment wireを全8Bへ変更するものではない。`stop_above: Some(...)` の成功prefixは全点の成功としない。work-cap超過のexact文字列は `the close sizing exceeds its work cap`、pure gateでは `TirExceeds { limit: "IR close sizing work", ... }`。format/geometry/overflow/未対応Composite等とは分ける。

pure V2のenvelopeは `M=max(old_coarse, max(typed close_bytes))`。既存court byte ceiling、carriable bytes、chunk policyに照合し、dissected root claimには別の1-carrier上限がある。V1への型変換や4Bの見積りでwideを通さない。CPU benchmarkの4GiB/8GiB capはこのprotocol ceilingと無関係で、料金や上限を変えてadmission成功を作らない。

program/context/unrolled nodes、MAC・state/live bytes・cone work、F7、ladder、DA reach、operand count等の既存検査を維持する。最初に失敗したstage、error型、got/cap、source/rules/入力hashを記録し、後段はNOT_REACHEDとする。typed sizing成功だけをpure gate成功へ昇格しない。

## 真正入力が揃った後の段階

真正carrierの元フィールドを保持してconstructor、V1 narrow / explicit V2 wide pure gate、返却catalog/schema1 rowと元classの一致を別々に確認する。local court2選択は明示コピーとして記録し、実Params preflight結果も別欄に保存する。V1のwide拒否、V2のnarrow拒否、canonical全文・class/root/count変異、sizing cap不足、途中prefix、typedboundの取り違えを負例にする。署名の真偽をpure gateが検査したとは扱わない。

固定cc22ではParams validatorはcourt1以外を拒否し、native foldは旧V1経路を使う。pure V2データAPIやlocal rules court2はParams/private presetを有効化しない。nativeには真正署名・bond/collateral/UTXO・origin・target/share policy・activation・資金・受付・永続化・再起動等の実接続が別に必要。採択済みRFC43の通常class登録配線はRoot/A1が別途実装中で、後続候補で再確認する。新Assignment・P・fundingの未採択設計とこの通常登録実装を混同せず、この案で新しいauthorityを作らない。SecurityCertified/信用・決済も別のNOT_RUN条件である。
