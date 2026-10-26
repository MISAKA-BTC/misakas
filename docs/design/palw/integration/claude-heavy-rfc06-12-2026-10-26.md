# RFC06・07・08・12 重い作業のClaude引継ぎ — 2026-10-26

ユーザー依頼により、重い3件を実Claude Code CLIへ委任する準備を行った。基点は `2de0a526b41559994e9894e6e13a268bb4dc2247`。このcommitはGDN認証workspaceとmesh一回決済の失敗原子性を53 distinct parent PASSで確定した局所成果で、全RFC/campaign完了ではない。

## 実行状態

Claude Code 2.1.153で3件の別sessionを実際に起動したが、全件が最初のAPI要求で `401 OAuth access token has expired` を返した。tool実行は0件、実装開始前に停止している。稼働中とは扱わない。ユーザーへ `claude auth login` による再認証を依頼済み。自動再試行はしない。認証情報をコピー・読み替え・手作業更新する回避は行っていない。

指示・worktree・入力manifestは保持しており、再認証後に同じ担当へ再投入できる。既存のClaude sessionへ手渡す場合も、以下の各指示書をそのまま利用できる。

| 担当 | 具体的な仕事 | worktree / branch |
|---|---|---|
| RFC06 router | SEAT-S4 worker→detach→done→matched→receipt直前までnative Some/Noneと予約ownerを保持し、実Q36 checkpoint再実行へ接続 | `/Users/wata/.codex/worktrees/claude-rfc06-router/MISAKA-testnet` / `codex/claude-heavy-rfc06-router` |
| RFC07 source | normal V5 source chainのtyped Resource/absence/Decode保持、生成前予約・last-use owner、native Fund/StageMessage/FundRound/Receipt/FS callerと試験 | `/Users/wata/.codex/worktrees/claude-rfc07-sources/MISAKA-testnet` / `codex/claude-heavy-rfc07-sources` |
| RFC08 lifecycle | provider replacementとretention中material、cold restart/reorg/prune/import/resumeの既存実consumer接続 | `/Users/wata/.codex/worktrees/claude-rfc08-lifecycle/MISAKA-testnet` / `codex/claude-heavy-rfc08-lifecycle` |

各担当用の完全な指示書：

- [RFC06 router](/Users/wata/.codex/artifacts/rfc06-12-20261025/claude-heavy-jobs-20261026/rfc06-router.md)
- [RFC07 normal source](/Users/wata/.codex/artifacts/rfc06-12-20261025/claude-heavy-jobs-20261026/rfc07-sources.md)
- [RFC08 provider/material lifecycle](/Users/wata/.codex/artifacts/rfc06-12-20261025/claude-heavy-jobs-20261026/rfc08-lifecycle.md)

準備済みのseat-s4 patchとnormal-source native tests patchを引き継ぎ、同じ調査・実装を最初から作り直さない。RFC07 worktreeには統括の未コミットWIP 7ファイルを意図的にコピー済みで、入力manifestにSHAを保存した。これは受入済み・compile済みという扱いではない。

## 統括Codex側に残すもの

RFC12 campaign ledger/identityの修正、履歴収集の証拠、RFC08 actual coinbase残件、rootの統合・型検査・build・実試験を管理する。重い担当のCargoは統括へBUILD_REQUEST.jsonで依頼し、共有targetの同時Cargoを避ける。branch/source/feature/binary SHA/実行commandと親件数を固定して検証する。

公開activation None、通常bootstrap拒否、未承認economics休眠、モデル二階層、claimごとの抽選承認は維持。K3未作成、native39写像未策定、未実行15 gate UNKNOWNを維持し、料金・保持期間・protocol番号や承認を創作しない。別worktreeだけ編集し、push/deploy/公開node操作・他作業のreset/cache削除は行わない。

実起動のsession ID、PID、exit、prompt SHA、ログは `/Users/wata/.codex/artifacts/rfc06-12-20261025/claude-heavy-jobs-20261026/status.json` に保存した。停止済みPIDを稼働証拠として使用しない。
