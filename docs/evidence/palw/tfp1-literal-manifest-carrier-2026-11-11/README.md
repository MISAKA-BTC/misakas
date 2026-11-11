# TFO3 の literal manifest と HF 参照

2026-11-11。基点 `93436ac2f`。**実装中・Rust検証未実行**。

## 変更

- TFO3 wire version 1 は従来の byte 列を維持する。明示的な version 2 は、既存の inner / prompt / signed companion に、固定幅 ManifestV1、u16 長、最大732 bytesの HF descriptor を追加する。全体の既存上限を維持し、空 descriptor、切詰め、余分な末尾を拒否する。
- pairing は実取引内の manifest / descriptor の offset を保持する。外部からの別 body や追加 heap buffer で置き換えない。Node はモデル全量を取得しない。
- `prepare_tfp1_pairing_v1` は version 2 の場合、署名済み companion の manifest root、全scopeの subject / codec / 上限、literal prompt、descriptor の内容と class / program / tokenizer / inventory を照合する。既存の履歴再演も同じ入口を使う。
- `prepare_tfp1_material_pairing_v2` は metadata を必須にし、同じ native FP / Source と選択親の class / container へ参照を解決する。同じ受付点・literal promptから初回資料量を導出し、全10scopeのmanifest宣言サイズとcodec/chunk形状、HF / InputTokens rootを照合する。料金policyの上限が大きいだけでは、実際のmanifestを小さく偽装できない。
- 同入口は一時的なFP/context/layoutハッシュ用bufferを事前予約する。受付policyのjobはSourceのjobと一致させ、資源不足を契約拒否と別の型で返す。実claim資料の量は返すが、資料配送やモデル取得可否の証拠は作らない。

モデル取得証明、可用性審査、Minerによるモデル重み全量配信は追加していない。ここで搬送するのは小さな参照情報とclaim資料の目録であり、claimの入力・trace・opening本体の供給義務とは別である。

## 検証範囲

旧wire互換、新版のexact EOF・サイズ上限、bare / FPA1 / FPR2+FPA1実取引からのborrow、署名済み目録と参照の差し替え拒否の回帰コードを追加。実登録tokenizerを使う既存fixtureに、各scopeの長さ変更、root / chunk変更、別Original ID、等長prompt差し替えの拒否を追加した。fixtureのFP署名・資金は既存の構造fixtureであり、公開受付の実証ではない。

採択済み[実装計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md) §1に従い、Cargo check / build / test は縦接続の完成後にまとめる。現段階ではコード読解、rustfmt、差分確認のみ。文書検査の結果は [validation.json](validation.json) に記録する。

## 残作業

完全policy resolverとG14全工程planner、claim / escrow / frame / bookの原子的な本番受付、FpTokenizer route、Node候補filter・RPC・Studio新版producerは未接続。今回の新版codecを作っただけで既存公開claimが新版へ変換されたとは扱わない。元claimを無断で置換・再時刻付けしない。

公開T12のquote・署名・受付、同一claimのPanel / Final、当選権利消費と対応ブロックは未証明。配備・公開照会・取引送信はこの作業では行っていない。
