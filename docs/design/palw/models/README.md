# design / palw / models

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../../../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../../../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../../../governance/qwen-fow1-profile-transition.md)。QF1のRFC、profile JSON、trace schema、小規模codec参照を追加した。モデル推論の実装や実用性保証とは区別する。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用定理・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../../../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../../../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../../../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。モデルのlowering、Readout、安い短絡、実用suite、UNKNOWN保留、rosterに束縛した停止証明の入口を追加する。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

文書の適用版・対象・採用状態を確認してください。配置は採用・有効化ではありません。

[総合入口](../../../README.md) / [状態](../../../STATUS.md)。

## 資料

- [beyond-weight-files.md](beyond-weight-files.md)
- [k2-real-scale.md](k2-real-scale.md)
- [kernel-k2-tir-v1.md](kernel-k2-tir-v1.md)
- [palw-base0-ptq-pipeline-scope.md](palw-base0-ptq-pipeline-scope.md)
- [palw-practical-runtime-plan-2026-08-26.md](palw-practical-runtime-plan-2026-08-26.md)
- [palw-qwen25-class-phase0.md](palw-qwen25-class-phase0.md)
- [palw-two-class-plan-2026-08-21.md](palw-two-class-plan-2026-08-21.md)
- [rfc-0004-part2-typed-roots.md](rfc-0004-part2-typed-roots.md)
- [rfc06-genesis-declaration-index-2026-10-13.md](rfc06-genesis-declaration-index-2026-10-13.md)
- [versioned-kernels.md](versioned-kernels.md)
