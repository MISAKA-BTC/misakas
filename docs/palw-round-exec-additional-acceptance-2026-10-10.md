# Round / EXEC の追加受入条件 — 2026-10-10

**Status: 追加受入条件として採用。実装・検証・activationは未完了。**

計算量比例のRound権利とbondごとの経済上限を両立させ、運営者への身分上の優先権を設けない。
本決定は設計・受入条件の追加であり、既存networkのパラメータ、wire、fingerprint、activationを変更しない。
以下の条件と既存の全activation gateが成立するまで、新しい経済規則と統合EXECを有効化しない。

このcheckoutは2026-09-22のコードを基点としており、参照先のRFC-0008・ADR-0176を含まない。
そのため本書を独立した追加条件として記録する。後続branchへ統合する際は、RFC-0008の実装仕様と
bond会計の受入表にも接続し、本書の採用を既存コードの実装済み証拠として扱わない。

## 1. 維持する基本設計

- Round候補ticketはモデル名ではなく、検証済みCanonicalWorkから生成する。
  既存の100,000 credit単位と将来seedによる端数処理を基準とする。
- 120は120秒・1秒1枠の場合の共有window容量であり、claimごとの固定配布数ではない。
  window長・幅が異なる設定では、その設定の共有容量を使う。
- EXEC_SLICEは承認された計算計画と決定的な区間境界に従う。
  モデル名、任意の分割数、header数で仕事量・報酬・Round権利を増やさない。
- EXEC_TXとEXEC_SLICEのfork-choice weight / blue score / DAA寄与は0。
  root weightをEXECへ配分せず、sliceの検証・報酬はroot単位に集約する。

## 2. BUDGET — 抽選前の権利制限と共通会計

設計原則を次のように置く。これは実装済み公式ではない。

```text
T_earned ≈ VerifiedWork / 100,000
T_candidate <= min(T_earned, BondRemainingRoundRights)
T_executed <= T_allocated <= T_candidate
sum(T_allocated in a window) <= shared_window_capacity
```

`BondRemainingRoundRights`の単位、対象期間、予約・消費・解放時点、失効、最早再利用時刻、
端数、抽選落選時の扱い、繰越の可否をversioned仕様として確定する。
root / slice / claim / receipt / Roundから派生する権利を同じ資本の帰属会計へ束縛する。
予約・行使・Final・失効・reorgで二重計上や早期枠回復を起こさない。

bond上限は共有window抽選の候補集合を作る前に適用する。
行使不能な候補を大量投入して当選枠を占有し、当選後だけ上限を検査する方式は受け入れない。
抽選直前の残余予算と既存予約を検査し、別windowへの同時予約で上限を超えない。

Round発行権・実行回数・取引手数料・claim報酬とADR-0176のQ/B/R/F予算の関係を明文化する。
特にfee-only RoundをB_maxの対象とするか、別の明示的な実行上限で制約するかを確定する。
市場依存の手数料収入をR_maxへ含める場合は、その範囲と超過時の処理・会計を定義する。
未決定のままfee-only経路を上限の迂回路として残さない。

## 3. WINDOW — 公開競争と分割不変性

将来seedによる公開された決定的な割当を基準とする。
運営者、genesis bond、登録順、特別鍵、既存minerに身分上の優先権を与えない。
資本量・正当な計算量による差と、運営者への特別扱いを区別する。

同じ正当な仕事量と同じ総拘束資本について、bond分割、operator鍵の増設、claim分割、
同一計算の再提出、root/sliceへの再包装だけで候補総量や期待獲得枠を増やさない。
共通所有者をchainが識別できるという仮定に依存せず、資本の重複計上と仕事の再利用を防ぐ。
分割による丸め・候補cap・抽選差も評価し、許容誤差とその根拠を仕様に固定する。

## 4. ECON — 計算と資本の両方が意味を持つ範囲

同額bond・同期間の小型/大型モデルについて、上限到達前と到達後、非飽和と飽和を比較する。
モデルのパラメータ数だけでなく、canonical work、token数、計算費用、処理時間を記録する。
小型モデルの複数jobと大型モデルの単一jobを、同じ総workでも比較する。

必須指標はcandidate / allocated / executed ticket、window占有率、期待手数料とwork報酬、
計算費用あたりの収益、資本拘束費用、上限到達時間、上限後の限界収益とする。
ticket数を増やすことだけで大型モデルの採算成立を宣言しない。
需要・手数料・計算費用・参加者構成の感度分析を行い、採用パラメータと許容基準を
結果の判定前に固定する。大型モデルへの固定優遇や採算の無条件保証を導入しない。

## 5. 必須受入試験と有効化判定

| Gate | 必須証拠 | 現状 |
| --- | --- | --- |
| BUDGET | 単位・期間・予約・消費・失効・再利用・fee帰属の仕様と、全writer/reader/undoの一致 | 未検証 |
| WORK | 同額bondで軽い/重い正当な仕事を比較。上限未到達時の計算比例と上限時の制限を確認 | 未検証 |
| WINDOW | 120共有枠等の飽和、巨大候補集合、上限済みbond、同時window予約で枠占有と予算超過を防止 | 未検証 |
| NEUTRALITY | 同条件の運営者/genesis/新規minerの役割を入れ替え、身分・登録順の優先権がないことを確認 | 未検証 |
| SPLIT | bond/operator/claim分割、丸め、候補cap、重複job・再提出で権利と期待機会が増幅しない | 未検証 |
| SLICE | 区間重複・slice数増加・root prefix再利用・duplicate Finalで権利/報酬/weightが二重に増えない | 未検証 |
| RECOVERY | restart/reorg/expiry/conviction/retirementで予算とone-use履歴が正しく復元される | 未検証 |
| ECON | 上限前後・window飽和前後の計算費用/資本費用/需要別評価と、事前に固定した基準の達成 | 未検証 |

本書の追加だけではPASSにしない。対象commit、ruleset、パラメータ、再現手順、結果、
未解決事項をgateごとに保存し、既存のlane隔離・liveness・公開検証・DA・互換性gateと併せて判定する。
未解決のBUDGET / ECONまたはその他の必須gateを残したまま新規則を有効化しない。

## 参照

- [既存Round ticket実装](../consensus/core/src/palw_execution_quanta_v1.rs)
- [RFC-0008: 統合EXEC設計](https://github.com/MISAKA-BTC/misakas/blob/pre/docs/rfc/0008-palw-claim-backed-consensus-blocks.md)
- [RFC-0008実装仕様・activation gates](https://github.com/MISAKA-BTC/misakas/blob/pre/docs/design/palw/rfc-0008-implementation-spec.md)
- [ADR-0176: bondと共通期間による上限](https://github.com/MISAKA-BTC/misakas/blob/pre/docs/adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)
