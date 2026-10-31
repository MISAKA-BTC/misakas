# A6 検証記録 — 2026-10-31

Branch: `codex/rfc42-a6-recovery`。A0共通契約 `a85152edc` / v2/r1をmerge。実装は [A6-RECOVERY.md](A6-RECOVERY.md)。[試験対象sourceのSHA-256](../../../evidence/positive-court-a6-2026-10-31/source-snapshot.json) を固定した。

## 同じcandidateでの結果

- Positive Court: **48 passed / 0 failed**。追加した復旧10件を全て含む。[raw log](../../../evidence/positive-court-a6-2026-10-31/tests.log)
- 既存security certificationとTIR courtのunit回帰: [security](../../../evidence/positive-court-a6-2026-10-31/security-regression.log)、[TIR court](../../../evidence/positive-court-a6-2026-10-31/tir-court-regression.log)。同じCargo生成test executableを使用。
- `rustfmt --edition 2024 --check`（A6の2 Rust module）と `git diff --check`: pass。
- `python3 docs/maintenance/check_docs.py`: [raw log](../../../evidence/positive-court-a6-2026-10-31/docs-check.log)。既知のmissing referencesをbaselineで隠し直していない。strictな全リンク正常性とは別のno-new-regressions検査。

```sh
CARGO_TARGET_DIR=/Users/wata/Downloads/MISAKA-testnet/target CARGO_INCREMENTAL=0 \
  cargo test -p kaspa-consensus-core --lib palw_positive_court_v2 \
  --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
  --config 'profile.dev.package.kaspa-consensus-core.debug=0' \
  -- --test-threads=2 --skip coverage_includes_every_vocabulary_tile_and_ragged_tail
```

全test executableはCargoで作成した。上記packageごとの設定は今回のビルド費用を抑えるためのもので、repositoryのprofileや有効化設定は変更していない。最初に試みた単独rustcビルドや、その前の別candidateを最終成功結果へ合算していない。

既存の8,193語彙case `coverage_includes_every_vocabulary_tile_and_ragged_tail` はunoptimized実行が長いため停止し、上記成功runから明示的に除外した。成功扱いしていない。最初の試験用fixtureはopeningのbytesを含む転送量が4回分に足りず1件失敗した。試験側の転送上限を5 payload分に修正し、work上限4回・料金一回払い・再起動後の超過拒否の期待値を保った上で、同じ最終candidateの48件を再実行した。

## 確認した境界

署名・資料opening・既存exact checkerを実際に呼ぶ。無効応答のwork、重複応答の料金一回限り、全Panelのdedupe、失敗候補のattempt、部分batchのcursor/charged bytes、Panel交代後の責任、保守的な予約量、期限後の履歴再検査、downtime expiry、reorg prefixへの全体rollback、短い履歴/異なるbranch/異なる上限/未認証originの拒否、flag-only importの無信用、実ファイルgeneration、commit acknowledgement喪失時の停止、保持horizon、A0 Recovery port、bounded readerを確認した。

## 未接続・未検証と統合依頼

native `AtomicCourtStore`、実coin hold/支払/undo、row stream/P2P IBD、selected-chain reorg/pruning、実process crash、A4の後続分割搬送状態、複数Node、実LLM費用は未完了。現在の私設ledgerとarchiveをその完成と数えない。

A0/A4には専用recovery module宣言とcoreの共通contract依存を、A4/A5にはchecked transition・資源・実資金・journal headを同じnative transactionで扱うadapterを接続してもらう必要がある。新result/witness wireを旧試作型へ暗黙変換しない。未接続native creditは引き続き拒否する。

残っている仮の成功処理はない。上記native接続自体が未実装。新native wire・公開preset・ネットワーク有効化設定は変更なし。
