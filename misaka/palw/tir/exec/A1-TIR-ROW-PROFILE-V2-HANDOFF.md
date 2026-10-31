# A1: bounded schema-1 row profile metadata

2026-11-01。Base: Root `017bff082`。Branch: `codex/a1-row-profile-metadata-v2`。
既存A1 checkoutで専用branchを使用した。登録rowのlayout preimageを持たない読取のため、
private field・非serializeの `ResolvedTirRowProfileV2` を追加する。

```rust,ignore
ResolvedTirRowProfileV2::resolve(expected_class_id, authenticated_artifact, &row, limits)
```

key・artifact・rowはcallerが同一の認証済みnative state viewから取得する。artifactは
`classes[expected_class_id].artifact_root` 由来であり、rowには含まれない。
このAPIが確認するのは入力の整合性で、入力自体のprovenanceやnative権限ではない。

## 照合と再導出の境界

入力の宣言／実program長、formatとcallerのraw cap、row dissection長を最初に確認する。
schema1、programの実長とgraph hashを確認した後、元bytesをcanonical decodeする。
既存decoderがstrict Borsh・byte-identical re-encode・構造validatorを実行する。
exact schemeを分類し、logits dtype、token_bound、held、旧tiled predicate、prim_set_id、
logits_vocab、dissected全listを再導出して照合する。listは順序・重複も含めて完全一致を求める。

既存 `palw_tir_class_id_from_digests_v1` helperを意味不変で抽出した。
既存domainと `version(u16LE) || graph_root || layout_digest || artifact || tokenizer` の
順序を保ち、`PalwTirClassV1::class_id` も同helperに委譲する。導出IDをnative map keyと
`row.facts.class_id` の両方に照合する。program hashは前段で検査済みである。

| 情報 | 根拠と検査 |
| --- | --- |
| scheme / dtype / vocab / token_bound / held / old tiled bit / prim set / dissected | 元canonical programから再導出して照合 |
| graph root / program length | 元bytesと照合 |
| class ID | 既存式を再計算しnative key・row内IDと照合 |
| layout digest / tokenizer | rooted row由来。既存class IDに束縛される |
| max_context | rooted row由来。`1..=program.history_bound`だけを検査し、`row_max_context()`で区別 |
| 正確なlayout.max_context・checkpoint・h_tile・commit/state tiles・StepSpace・leaf数 | layout preimageが必要。本viewには存在しない |

layoutなしでは、範囲内で変わったrow max_contextとlayoutの一致を独立確認できない。
rooted rowを入力とする前提を保ち、完全row再導出には既存full-layout resolverを用いる。
既存tag169/TirLayoutのlayoutが不在なら完全joinはfail closedにする。layout公開が現在
V1 StepSpaceを要求してwideを拒否する挙動は、この変更の範囲に含めない。

## 旧dtypeとwire lane

row専用 `TirRowTraceProfileV2` は `FlatLegacy4Byte`、`TiledLegacy4Byte`、`TiledI64Q24`。
legacyは従来committable dtypeのI8・I16・I32・Idxをすべて維持する。算術dtypeは
`logits_dtype()`、wire幅は `lane_width_bytes()` で分離する。既存のI32専用job resolverの
enumや受理条件を再定義しない。wideはexact schemeと最終post I64を要求する。

wideのschema1 `facts.tiled=false` はそのまま保つ。これは旧exact scheme predicateであり、
tiled family bitではない。metadataのlogits-data tilesはflat=1、両tiled=`ceil(vocab/4096)`。
DAのdraw clamp等policyやStep layout geometryとは別であり、旧readerへ渡す許可にならない。
flatのcontext trace IDとprogram scheme IDが異なる既存仕様も維持する。

raw facts・row変換・program・layout・StepSpace・canonical job・native許可のgetterは設けない。
job tokenizer/CUの書換えや新wire tag/domain/schema、registration factory、V1 reader、
admission dispatch、FullRelationのI64受理や公開有効化を追加しない。

## 資源と検証

`TirRowProfileLimitsV2` はlocal input/result capで、heap/work leaseではない。
program/row/dissectedをcloneせず、bounded decoder・re-encode・validatorとgraph/cone metadataの
一時領域・hash workはownerが別に予約する。dissectionは最大16block・512node/block・
8input/nodeの既存format上限下でcommitごとにgraphを走査する。モデル位置・history長・
重みのtensor計算を行わない。出力list capはこの一時graph workを包含しない。

実行証拠は `validation/row-profile-metadata-v2/` に保存する。凍結source/tree、限定unit test、
strict Core library Clippy、実unit-test binary、既存static RocksDB archive、link build-script
outputのSHA256を記録する。A1既存cacheだけを使用しRoot/A0 target・新target・C++ buildを
使わない。過去のphaseのテスト数は合算せず、raw log末尾も変更しない。
