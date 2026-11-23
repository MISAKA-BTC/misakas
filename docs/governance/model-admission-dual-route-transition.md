# MAD2 — 二経路モデル採用の適用と移行

2026-11-23、ユーザー明示指示による **ADOPTED_DESIGN_NOT_ACTIVATED**。
正本は[RFC-0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)と[ADR-0235](../adr/0235-model-admission-kernel-auto-and-bft-decision.md)。

## 1. 優先順位

明示的にMAD2を有効化する新規採掘profileについて、モデル採用規則は本契約とRFC0067を優先する。旧MAS1の「UNKNOWNは全経路で採掘不可」「BFTは停止専用」「全モデルにG-COLL定理とG-COST/G-USE合格を必須」はKERNEL_AUTOに限定する。BFT_DECISIONでは証拠提出なしのYES/NO決定を使用できる。

現在の旧profile、署名済みclaim、snapshot、過去の検査世代を遡及変更しない。過去のUNKNOWN測定はUNKNOWNのまま残る。新規承認recordは元の採用判断を参照し、数学的PASSを偽装しない。

## 1.1 originと管理権限

GENESIS_ORIGINALは不変genesis manifestで特定し、BFTは採用・変更・削除・停止・解除へ関与できない。追加モデルは自動又はBFT YESで採用し、いずれもBFT管理・停止対象とする。FORK_DESIGNATEDはBFT YESでも追加でき、Optionなし・Miner=Rを継承するが、BFT停止・解除の対象である。

genesis同様の経済扱いを、genesis由来の停止免除へ読み替えない。BFTによるGENESIS再分類、rule/family経由の停止、同root複製での免除付与を拒否する。個別claimの不正拒否・既存検証はgenesisにも適用する。

## 2. 引き継ぐ条件

共通技術互換、モデルと重みの認証、決定的な実行とkernel coverage、Panel/Court、独立Gate C、TB1、Ticket一意性、一回会計、停止・元期限・責任を引き継ぐ。暗号学的な計算validity proof、DNS-BFT、HF取得証明制度を復活させない。

追加モデル登録と市場原子生成、MOE2の30%買付・20%永久LP・余剰burn・残余、通常取引5%/3%、coreの対象worker報酬100%は変更しない。

## 3. 入口と解除

全consumerは次を共通参照する。

    BaseCompatible AND 現行実行profile
    AND (
        元genesisの不変bootstrapと一致
        OR (
            MAD2有効化 AND 現行世代
            AND (追加モデルのAUTO又はBFT YES OR forkモデルのBFT YES)
            AND 適用停止なし
        )
    )
    AND 個別claimの既存gate

BFT承認を記録する場合、TheoremRuleと評価証拠は必須fieldにしない。旧MAS1の全PASS predicateを最後に重ねない。現在のSTOPをYESで消さず、BFT経路に適合した明示解除を必要とする。ROSTER権限、投票domain、proposal・generation・期限を固定し、承認と停止を同じnative順序で適用する。

## 4. 実装・activation前の確認

1. 新policy/codec/admission basisを版付きで実装する。旧enum・旧署名意味を変えない。
2. 既存kernelへの自動lowering・審査入口と、技術互換だけを共通要件にするBFT入口を接続する。
3. MODEL_ADMISSION権限を明示付与し、署名済みYES/NO、重複・別scope・期限・reorgを検査する。
4. RPC、Studio、miner、Panel、Node、Final、Gate C、未使用権利消費、reward、import/IBDで同じeffective gateを使う。
5. 技術失敗は両経路で拒否し、未証明の採用判断はq YESで通せることを短時間試験で確認する。
6. 実モデルに正規の投票を行い、その結果を記録する。文書を追加しただけでQF1 REVIEW_HOLDを解除しない。
7. genesisへのBFT制御拒否、BFT forkの追加と停止、追加モデルの両経路への停止、originのsnapshot/IBD復元を確認する。
8. 受賞・決済・再起動・reorgを通常ブロックで検証し、公開activationを別途決定する。

BFT経路の開始条件へ、G-COLLの定理や全G-COST/G-USE測定の完成を再挿入しない。自動経路の開始条件はその規則に従う。非報酬評価は任意の判断資料として再利用できるが、評価traceに後付け報酬を与えない。

## 5. 文書と履歴

現在入口のRFC/ADR-v2は20冊/50冊を維持し、新規記録は旧名前空間のRFC0067/ADR0235として追記する。元の統合payload、実測ログ、runtime、公開fence、配備、支払いを本改訂で変更しない。

[改訂記録と検査](../maintenance/revisions/2026-11-23-dual-route-admission/README.md)と[必要性監査](../design/palw/consensus/verification-responsibilities-and-simplification-2026-11-23.md)を参照する。
