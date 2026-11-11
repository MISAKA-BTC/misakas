# Selection の受付前共通予約

状態: **IMPLEMENTATION_IN_PROGRESS / RUST_VALIDATION_NOT_RUN / NOT_PUBLICLY_ACTIVATED**。

基点は `6a515bd17`。Studioから追加した対象claimの公開受理、Final、当選時の権利消費とブロック生成に向けた受付計画の接続作業であり、公開完走の記録ではない。

## 接続した範囲

- `PlainBatchOriginalProjectionV2::with_native_selection_v3` が元のTFP1/Source・時刻・受理済みtokenizer登録を再照合し、native custody上限からSelectionの供給者役割数を導く。後から渡した親のTKR1登録も元のprojectionと一致する必要がある。抽選された申立人の人数を供給者数へ代入しない。registry欠落時の保守的sentinelを実際の人数として使わない。
- 各保護枠について、任意の1行のFullIdsと全logits tileを覆う正規順序の最大資料長を、各供給者役割に予約する。全decode行を同時搬送する意味ではない。供給者別のdemand ordinalを数値系・Output・Tokenizerと重ならない帯域で導き、後のconsumerも同じresolverを使えるようにした。
- Selectionの搬送、検査、終端での再検査を数値・FullIds・tokenizerと同じgraph/bookへ追加する。費用・保持量・相対graphを既存の原子的予約へ渡す。新しいprofile/resource identityにSource、役割数、資料上限、局所裁定の数量、予算を固定する。Selectionを含まない旧graphのID/期限の意味を変更しない。
- 共通compilerでもSelectionとTokenizerのSourceを照合する。claim IDだけの一致で別header/Sourceの資料を混ぜない。
- 検証packetを明示的に保持し、archive保存を伴う検証でもinspection期限を適用する。資料Finish、検証、filing、終端/回復の実割当と修復可能な最遅slotを別々に検査する。

局所裁定用のpin/opening・署名はデータ同一性と反証のためのもので、モデル計算の暗号学的な正当性証明ではない。Minerのモデル重み全量配信やHF取得認定は追加していない。

## 追加した回帰コード

Selectionのgeometry/役割/費用変更がidentityへ反映されること、サイズ・work・I/O・workspaceの不足拒否、3供給者役割を含む共通予約と因果順、旧termsの流用拒否、終端再検査費用不足の拒否を対象とする。既存の実ML-DSA companion fixtureによる2保護枠の署名前後比較と、保持期限・受理時刻の拒否もSelection付きprojectionで呼び出す。保存を伴う検証がfiling期限へ逃げない試験も追加した。

fixtureの人数・予算は明示的なローカル入力であり、公開値や性能測定ではない。Rust build/testは通し実装後にまとめる方針に従い未実行。rustfmtとgit diff --checkだけを実施し、回帰PASSとは報告しない。

## 未接続の範囲

新保護caseごとの最初のSelection行/beat固定、正規の供給者権限とrange resolver、実署名carrierの支払/包含、typed資料writer、客観的局所裁定と原子的archive/undo、実行root単位の失効、供給不能時のhold/default/回復、第三者と防御の権利が残る。今回の費用下限は内部controlとnative数量であり、外側carrier mass・全保存/決済の本番資格を保証しない。古いclaim単位first-notice排他やrepairで保護枠を代用しない。

全資料の供給枠を時間順に予約することと、全供給者の応答をruntimeの反証条件にすることは別である。実際の局所反証は必要な認証済みFullIdsと正確な比較tileから独立に提出できるconsumerへ接続する。未完成のprofileを公開quote成功へ変更していない。Studio実送信・公開受付・Panel/Final・当選時の権利消費とブロック生成は未達。

文書検査: [docs](docs-check.log)、[RFC](rfc-integrity.log)、[ADR](adr-integrity.log)。既知エラーを消すbaseline更新は行わない。
