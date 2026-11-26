# RFC-0007 — BFTによるモデル追加・採用

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、モデル追加をBFT承認の一経路へ変更する。
- 判断理由：[ADR-0007](../adr/0007-bft-only-model-admission.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 採用経路

**新しいモデルを追加・採用する経路は `BFT_DECISION` のみとする。** `KERNEL_AUTO` / `AUTO_APPROVED`による新規成功、全自動検査PASSによる採用、BFTを待たないfallback、timeoutによる採用を廃止する。

kernelへの自動分解・変換・shape推論・資源推定は技術準備として残す。これらを「モデル自動追加」と表示しない。BFT提案の前に自動審査のtimeoutやG-COLL/G-COST/G-USE全PASSを待つ必要はない。

## 2. 登録・認可・claimを分ける

候補提出は不変descriptorと必要資料を固定する準備状態であり、採掘資格を作らない。正式な新規モデル追加は有効なBFT YES決定を根拠に行い、正規登録とMOE3市場作成を同じ原子遷移で成立させる。採用決定と実効化を分ける場合、実効化まで新規採掘を許さない。

既登録モデルを新しい承認方針へ移す場合は、現在のBFT採用recordを明示的に作り、市場を重複作成しない。市場、残高、origin又は過去の自動PASSは新規BFT承認を代替しない。

genesisの初期manifestはチェーンの初期状態として固定する。起動後のモデル追加にgenesis扱いを付ける経路はない。初期モデルもBFTの停止・解除・廃止管理に含める。forkという名称で係数や市場を変える特別経路は設けない。

## 3. BaseCompatible — YESで免除できない条件

| 条件 | 必須内容 |
| --- | --- |
| identity | artifact、重み、program、tokenizer、入力adapter、Readout、数値profileの正規digest |
| execution | shape/type、丸め、overflow、生成・停止、有限contextと資源上界 |
| kernel | 必要primitiveと状態遷移を有効なTIR/kernelと有界checkerが解釈できる |
| trace | 元入力から全Step・Readout・End・四Rootまでの検証関係 |
| context | network/genesis、policy、generation、kernel契約、現行状態の一致 |
| useful computation | 試行IDのechoや採掘専用の無関係枝を正規の有用推論と扱わない |

未知kernel、壊れたcodec、非決定的実行、認証root不一致、実行上界不明をYESで通さない。一方、未知の高速算法の有無、実用性score、G-COLLの定理の有無は採否判断の資料であり、数学的証明や一律の測定合格を必須にしない。

## 4. 提案と測定資料

提案は対象model/descriptorと実行契約を確定する。利用可能な測定・推定をその対象へ結び付け、測っていない値はUNKNOWNとする。BFTは必要に応じて品質、処理量、資源、再利用・予測のリスクを検討する。資料の欠落を合格と表記しないが、測定証拠の添付は票の有効条件にしない。

MWA1を採用するprofileでは、共通meterが導くModelWorkProfileと上界を提案へ固定する。BFTは任意の計算量倍率やモデル別単価を設定しない。MWA1自体はRFC-0013の提案状態に従う。

## 5. roster・票・定足数

proposal開始前にnativeが固定したrosterへ、MODEL_ADMISSION権限を明示付与する。登録者がrosterを選ばない。TB1 holderや他の役割を兼ねる場合も、権限・鍵用途・署名domainを分離する。

論理条件は`n=3f+1, f>=1, q=2f+1`。同じproposalに相異なるq人のYESで承認、q人のNOで不採用、期限までにいずれも満たさなければ未決終了とする。無応答をYES又はNOへ変換しない。少数NOを単独vetoにしない。

```text
ModelAdmissionBallot = {
  action=MODEL_ADMISSION, domain, codec,
  chain_id, genesis_hash, proposal_id, canonical_open_anchor,
  immutable_descriptor_hash, active_kernel_contract_digest,
  work_profile_ref_if_required,
  policy_id, policy_version, admission_generation,
  roster_epoch, roster_root, deadline, decision={YES,NO}
}
```

曖昧なモデル名だけの承認、ownerへの包括承認、別network/世代/期限への票の転用を拒否する。正直な署名者は同一VoteKeyでYES/NOの両方へ署名せず、restart/reorg後も署名journalを保持する。

## 6. 状態遷移

候補は `PROPOSED → TECHNICALLY_COMPATIBLE → VOTING` と進める。技術的拒否なら `REJECTED_TECHNICAL`、q NOなら `REJECTED_BY_VOTE`、期限切れなら `EXPIRED_UNDECIDED`。q YESは `BFT_APPROVED`であり、実効化条件を満たして初めて新規claimへ使える。

```text
EffectiveNewModelAdmission =
    ProfileActive
    AND BaseCompatible
    AND ValidCurrentBftYes
    AND CurrentPolicyAndGeneration
    AND NoApplicableStop
    AND NotRetired
```

初期genesisは明示された不変bootstrap recordを起点とし、稼働後の免除を作らない。過去のAUTO recordは読み取り互換だけを残し、新policyでの新規採掘には明示BFT決定を要求する。

承認は `assurance=BFT_POLICY_APPROVAL` と記録する。G-COLL/G-COST/G-USEの元UNKNOWN/FAILをPASSへ書き換えない。採用recordの再署名・別subset・説明文の変更は新しいTicket、市場、仕事キー、報酬を生まない。

## 7. 停止・解除・再審

NOは当該proposalの不採用であり、別の有効な既存承認を取り消す停止票ではない。停止には専用MODEL_STOP、解除には対象StopId集合・直前control record・世代に束縛した正規解除を使う。

同batchの新停止は承認・解除より優先する。owner変更、alias、再登録、世代increment、burn又はtimeoutだけで停止を消さない。BurnResumeは独立の安全停止・恒久廃止を解除しない。

BFTは全モデルを管理するが、既存descriptorの重みや数値意味を上書きしない。変更したinstanceは別提案とする。停止・廃止の資産効果はRFC-0017の限定scopeに従う。

## 8. 個別claimの検証

BFT採用はモデルの利用認可であり、Minerの各計算の承認ではない。Panel完全再実行、Court、元期限、Final、独立Gate C、TB1、正規Ticket、権利消費、報酬会計を省略しない。

BFTの集合交差は所定の故障仮定の下で相反決定を防ぐ認可上の性質であり、モデルの有用性・計算困難性・判断の正しさを証明しない。相反する有効quorumを検出したら対象proposalの新規成功を保留し、到着順で都合よく選ばない。

## 9. 移行と受入条件

activation前に署名codec、domain、roster権限、期限、容量、journal、旧claim cut、読み取り互換を確定する。既受理claimの元期限・責任・署名を変えず、旧自動承認を新BFT票として偽装しない。

必須試験は、技術互換＋q YESでの追加、q未満/NO/期限切れの拒否、AUTO PASSだけでは追加不能、未知kernel＋YESの拒否、二重票、別scope、停止優先、genesisへの正規停止、reorg/restart、登録と市場の原子性、不正claimと無Gate C受賞の拒否である。
