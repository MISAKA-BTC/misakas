# A1: explicit wide-only pure TIR admission V2

2026-11-01。Root base `4753ee4be`。Branch `codex/a1-wide-admission-v2`。
既存 `codex/a1-row-profile-metadata-v2` を保存し、同じA1 checkoutの新branchで作業した。
この段階はsource実装・静的レビューのみ。Cargo・rustc・Clippy・試験実行は未実施。
A0のgrant前は全target/cacheへアクセスせず、新target/cacheも作成していない。

## 依存と明示API

A4の未build sourceを選択統合した。

| A4 original | A1 selected commit | 内容 |
| --- | --- | --- |
| `77fced38a` | `ab8325fb1` | TIR-only typed V2 close sizing |
| `82cde2aec` | `479688659` | 同意味の不要borrow除去 |
| `13e1fe213` | `2e85254f1` | mutable legacy spaceへのwide改変を旧pricing入口で拒否 |

新moduleは `consensus/core/src/palw_tir_admission_v2.rs`。

```rust,ignore
verify_tir_class_admission_v2(bundle, rules, registration, certified, chain_certified)
palw_tir_post_genesis_registration_v2(/* V1と同じ引数順 */)
palw_tir_registration_preflight_at_v2(params, bundle, object, daa_score, chain_certified)
```

verifierは既存tag61 DTOだけを取り、既存 `(PalwClassCatalogEntryV2, PalwTirClassRecordV1)`
をDATAとして返す。exact wide Q24・最終post I64とローカルrulesのcourt semantics version2を
要求する。held history・GEN・Composite pricingは未対応。opaque artifact rootだけで
compositeを識別できるとは扱わず、公開のform選択を設けずordinary Multiproofに固定する。

constructorは元full-class profileのcanonical contextと完全一致を要求し、immutable
`PalwTirStepSpaceV2` でcanonical leaf数を数える。PWUへbyte幅係数は掛けない。
元program/layout/dtype/commit/state/最大contextを変更せず、canonical tokenizer/CUのゼロを保つ。
署名bytesをそのまま既存tag61へ載せ、既存message/domain/ML-DSA context/class IDを使う。
constructorの成功はfull admission・signature検証ではない。

preflightは既存rules-at-DAA解決を使いpure V2 verifierを呼ぶ。Params validatorは従来の
court version1限定のままで、実presetのversion1はV2 verifierで拒否される。ローカルrulesの
version2選択はParams/private environment/native dispatch/public presetの有効化ではない。
signature、Active bond、collateral、target、processorのexact share policyはこのpure結果の外。

## 費用上界と全点走査

```text
O_i = checked(program_bytes + 16_384 + source_opened_bytes_i)
O   = max_i O_i
S   = max(full_typed_bounds.close_bytes)
M   = max(O, S)
entry.court_cost.max_close_bytes = M
```

A4 Element/Range twinは同じ既存demand fenceで選択し、既存sizing-work capを使う。
`stop_above=None` に固定して全点を走査し、空結果・全Errを拒否する。成功prefixを全点合格に
扱う経路や旧4B pricingへのfallbackはない。OVER_CAPだけを既存work-limit errorへ写し、
unsupported/overflow/format/profile等は別拒否として保持する。

Mは既存court byte ceiling、raw carriable ceiling、catalogの既存
`palw_close_fits_court_v1` をすべて満たす必要がある。dissected root claimは別に既存の
単一carrier上限を満たす。root claimをMへ合算しない。MAC/operand上限は同じ既存fieldを使い、
新V2では返却catalogのworst operand数もその既存上限と明示照合する。V1の導出は変更しない。

Oはelement粒度の旧概算でprogram全文を含み、Sは元programを参照するCourtClosed全DTOの
serialized上界である。両者の包含関係を仮定せずmaxで包絡する。Sは全step laneを保守8Bとし、
実codecのnarrow/Fixed/Histは4Bを維持する。既に含まれる26-byte leaf header、frame、pathsを
再加算せず、Sへ新たな1.2倍やその除算を行わない。既存catalog判定内の換算は保持する。
Mはexact実wire bytesや実hash CPU・native RAM・通貨料金の実測値ではない。既存V1より厳しい
保守的拒否があり得るが、元context/programを縮小・改変して受付を通さない。

根拠は既存 `palw_tir_admission_v1.rs:666`、`palw_court_v2.rs:2993`、
`palw_mode_v2.rs:553,1644`、`docs/spec/palw/04b-tensor-ir.md` §10.3、
`docs/design/palw/tir/phase-f-integration.md` §2.12.1、RFC-0043のwide8B保守上界である。

## 旧契約と検証の範囲

旧v10 verifier・constructor・factory・preflightの本体は不変。共通低層helper3個の可視性だけを
`pub(crate)` へ広げた。Params、native acceptance/fold、tag61、signature、genesis、公的/私的
armingを変更しない。新native authority、wire/schema/domain、rate/currency、モデル単位の
審査・係数・昇格条件は追加していない。

全旧program/format/primitive/token feedback/history/layout/context/unrolled/distinct tiles、
位置MAC/state/cone/peak、universal interval analysis、tile MAC/Fixed replay/F7、canonical/prompt、
ladder/DA reach/PWU/class ID/family-share検査を新wide経路にも維持する。range解析を実際の一jobの
成功で置き換えない。最長jobは費用用の別yardstickであり、入力canonicalを変更しない。

新unit test sourceは10件。旧narrow I8/I16/I32/Idxのrow/IDと旧入口wide拒否、元wideのtag61/
full row/canonical/PWU、canonical改変、held/GEN/version/全主要limit、F7 H-reductionとwindow、
Mの両支配・空・overflow・root/32chunk上限・error分類・旧4B予算と実8B leafの境界を含む。
大規模DA reach・9種類以上tileなどの既存大fixtureは重複せず、該当gateのsource保持を確認する。
A4の試験sourceも依存として保存するが、いずれも実行成功とは記載しない。

source freezeと静的レビューの記録は `validation/wide-admission-v2-source/` に保存する。
この段階では実binary/archiveを新たに生成・検証していない。Cargo/Clippy/試験・full mixed
CourtClosed serialization・全reader/owner費用/実native受付は、別grantと接続確認が必要である。
