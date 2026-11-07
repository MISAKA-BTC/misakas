# RFC全25件の確認範囲

対象は今回のZIP内の番号付きRFCで、一覧に欠番はない。全件を全文横断検索・構造/改定/規範の照合対象とし、オンチェーンcourtに関係する条項と現行の適用範囲を重点読解した。
これは全ファイルの全履歴ログ・すべてのfixture・全Rust実装の正しさを個別に形式検証したという意味ではない。「新規指摘なし」は機能の完成・安全性を保証するPASSではない。

| RFC | 原本（ZIP内） | 行数 | 主な役割 | 確認した整合性の観点 | 指摘・扱い |
|---|---|---:|---|---|---|
| 0001 | `docs/rfc/0001-palw-inference-surface-gaps.md` | 431 | 推論・生成制御・サービング境界 | 局所実行・UI/CLIの利便機能と合意上の意味変更を区別。既存model identityを変える最適化を当然視しない。 | 直接の新規指摘なし |
| 0002 | `docs/rfc/0002-palw-tensor-ir.md` | 4,748 | Canonical TIR、局所court、履歴dissection | exact算術、root・operand認証、root/round/choice/close、期限固定・資料取得の前提。長い実装履歴を有効化証拠へ昇格させない。 | F04・F05・F06・F09の基盤 |
| 0003 | `docs/rfc/0003-palw-generative-model-classes.md` | 2,621 | Generative classes・共通TIR・media | 0003全体の共通部分と、0022で延期された画像・動画固有部分を区別。旧media工程表を現行必須機能とは扱わない。 | P05／範囲整理 |
| 0004 | `docs/rfc/0004-palw-model-improvement.md` | 1,155 | モデル改善・蒸留・評価 | 0022による延期と登録不変性の後続規則を優先。旧モデル更新の説明を現在のroot上書き許可とは読まない。 | P05／範囲整理 |
| 0005 | `docs/rfc/0005-palw-ml-vm.md` | 15 | VM案 | 不採用の短い文書。汎用VMをcourt不足の代替にする経路として読まない。 | P05 |
| 0006 | `docs/rfc/0006-palw-layer-sharded-panels.md` | 1,328 | Sharded Panel、DA、外部訴追 | off-chain資料取得とon-chain強制開示、receiptとcourtの責任、open session中の中立終了抑止、枠・resource未決値。 | F08・P02・P03 |
| 0007 | `docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md` | 1,497 | 確率的検出・receipt・exact court | Panelが行う検査とnodeの認証、暗号学的validity proof禁止、receipt不足の中立終了とcourt義務を区別。 | P01・P03 |
| 0008 | `docs/rfc/0008-palw-claim-backed-consensus-blocks.md` | 356 | EXEC・native settlement・依存責任 | court停止時も進むclock、worker/sponsor/providerの署名horizon、支払相対material保持、複数責任の会計。 | F02・F07・F08 |
| 0009 | `docs/rfc/0009-palw-remote-miner.md` | 326 | Remote/node-less miner | providerはbyte配信者、keysはMiner側。PC停止後のcourt署名と責任移転のprofile境界を重点確認。 | F04 |
| 0010 | `docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md` | 246 | Permissionless Panel binding | 当面Panel=1、正の検査evidenceとFinalのAND条件。未割当・beacon不在・receipt不足を対話defaultと混同しない。 | P03・F07の限定 |
| 0011 | `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md` | 746 | モデルonboarding・kernel境界 | boundedな資源admission、G14不足のfail-closed、モデル入手不介入への後続改定と旧資格gateを区別。 | P04／資源・G14前提 |
| 0012 | `docs/rfc/0012-palw-only-consensus-and-native-evm-settlement.md` | 607 | PALW-only consensus・native EVM | DNS-BFT廃止方針、refund・safe settlement・liabilityの時計を分離。court delayとmaturityは別の責任。 | F07・F08・F09の周辺 |
| 0013 | `docs/rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md` | 394 | Layoutとresource-bounded tools | 再現可能なlayout/root/資源見積りと、登録・検証可能性・実モデル稼働を区別。toolの成果をcourt上限証明にしない。 | P04／測定の範囲 |
| 0014 | `docs/rfc/0014-panel-independent-fraud-prosecution.md` | 1,027 | Panel非依存の公開訴追・10月19日改定 | 今回の主対象。3120/10000、手番・署名・default、第三者relay、fees・容量・収録機会、採択と実装の区別。 | F01〜F11の中心 |
| 0015 | `docs/rfc/0015-panel-free-permissionless-verification.md` | 966 | Panel=0将来mode・共通bond予算 | Panel=0は当面延期。共通発行枠回復d+Wと担保責任は別。hard deadline・G14はmodeを越えて保持。 | F07・P05 |
| 0016 | `docs/rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md` | 672 | Bond加重のブロック機会・Final容量 | V+の機会配分とcourt収録を同一視しない。burst上限と、court停止時にも維持するheartbeat/DAAを確認。 | F02・F03の周辺 |
| 0017 | `docs/rfc/0017-model-eligibility-examination-and-renewal.md` | 45 | モデル階層・旧審査廃止 | 階層0.3/1.0、モデル単位審査廃止、claim単位Gate C維持。shape/court資源admissionはモデル公開審査と別。 | P04 |
| 0018 | `docs/rfc/0018-dag-parallel-multi-model-consensus.md` | 429 | DAG・Gate C・safe frontier | FinalとSecurityCertified、Final後の公開告発window、合意credit、pruningと独立進行への0023改定。 | F07・F08 |
| 0019 | `docs/rfc/0019-annual-model-upgrade.md` | 446 | 年次カタログ・段階展開 | モデル追加のみ、階層によるcourt免除なし、claimの受理DAAはaccepting chain block基準、fenceと運用を区別。 | F09・P04 |
| 0020 | `docs/rfc/0020-long-context-calibration-memory.md` | 187 | 較正・変換・忠実度のメモリ | 合意外のtool設計。bit同一性・32K依存を維持。較正時間/メモリをcourt verifierの実測として流用しない。 | 監査対象への誤適用なし |
| 0021 | `docs/rfc/0021-design-freeze-and-documentation-governance.md` | 23 | 設計凍結・文書管理 | RFC/ADRの明示的採択・置換、変更とパラメーター調整、実装/試験/activationの区別。 | F11 |
| 0022 | `docs/rfc/0022-post-mainnet-media-and-model-improvement-scope.md` | 28 | 初期mainnetの適用範囲 | media・モデル改善の延期と、共有TIR/検証の維持。旧記録やmergeを有効化にしない。 | P05 |
| 0023 | `docs/rfc/0023-unresolved-claim-liability-and-independent-progress.md` | 65 | 未解決責任・独立claim進行 | 共有frontierを維持する段階1のL_classと、別fenceの段階2を分離。期限後保留・pruning・後発不正の扱い。 | F08 |
| 0024 | `docs/rfc/0024-court-door-3200-to-ctx-2m.md` | 593 | Door短縮・1位置化・先読み・事前取得 | 本文の数値foldと付録Cのhash-only候補、3120/3200、16分岐仮定、自己完結な認証・最悪bytes。 | F01・F05・F06・F09・F10 |
| 0025 | `docs/rfc/0025-scalable-objective-court-many-models-mass-disputes.md` | 698 | 多数モデル・大量court・容量/延長/IBD | SC64・予約・inclusion・one-outsider・E_max・署名済みterms・障害終了・snapshotを統合して照合。 | F01〜F11／P06 |

## 併せて確認した資料

PRINCIPLES、設計凍結規程、RFCのREADME/INDEX/TOPICS、該当Spec、統合残作業記録、t12分岐数の回帰テストsource、TIRの署名/phase/fold/期限検査、ML-DSA署名長の定数。正確な抜粋は [source-evidence.md](source-evidence.md) を参照。

## 完成扱いしていないもの

旧RFCの「tested」表示、別branchの実装記録、0024/0025添付のtoy test、ユーザー報告のfixture性能は、それぞれの範囲を越える実装・本番保証に使っていない。今回の12チェックは参照・算術のみで、実ノード試験ではない。

## 監査の再現資料

全ファイルの行数・bytes・SHA-256・見出しは [inventory.json](inventory.json)、全文横断照合のヒットは [cross-rfc-normative-scan.json](cross-rfc-normative-scan.json)、算術・参照存在の結果は [checks.json](checks.json)。
