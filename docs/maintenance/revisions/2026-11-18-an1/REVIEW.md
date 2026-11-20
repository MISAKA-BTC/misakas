# PALW-AN1 独立査読記録

## 設計・数学の補修

| ID | 指摘 | 反映 |
| --- | --- | --- |
| AN-R01 | 同一Cを別slotへ再包装すると、slotだけの一回消費では不十分 | RegKey→slot一意、s/実仕事/用途別BaseWorkKeyの原子消費 |
| AN-R02 | Cへ割当後slotや自己を含むclosed-set rootを入れると循環 | native管理envelopeと非循環collection IDを分離 |
| AN-R03 | 長いjobで正規input rootを未定義adapterへ委譲 | 直接再計算可能な予算へ参照profileを限定。未実装adapterでpayload rootを信用しない |
| AN-R04 | first-divergenceは算術式だけでは導けない | Init/全命令/型/範囲/index/参照/End/構造と有界checkerを要求 |
| AN-R05 | 一人正直条件が署名数だけへ縮退する | 能動性、実取得または成功停止、完全再実行、包含・裁定・異議時間を明記 |
| AN-R06 | qを事後の当選集合や期待費用から決める | 全関係者の正しい初回照会の事前上限、各実行の予算、停止比較に限定 |
| AN-R07 | AN1へFULL1の固定入力下界を流用 | sigma依存入力のため転用禁止。c_min未取得を明記 |
| AN-R08 | 同batchのnotice/資料caseより先にFinalする余地 | hold・反証・失効・依存取消をmint/consumeより先行 |
| AN-R09 | 空output/treeの定義がない | 現参照shapeは非空。空shapeは別のcanonical規則を要求 |

## 独立コード査読で再現した反例

初回査読対象implementationのSHA-256は`7d2c30f01f6708e9f38410c5a74e9a782212f233b6b0cb24370d3941ae297f3e`。当初の13群・153 assertionsの成功だけでは次の反例を検出できていなかった。

### AN-C01: native受付とCourtの整数型不一致

claimの`trace_count=6153.0`等を、native側は6153との数値等価だけで受理していた。一方、opening側は厳密なintを要求した。このため、誤ったtraceが受理されても正直PanelのCourtがInvalidEvidenceになり、成功へ進み得る反例を再現した。

補修は、受付からCourtまでの件数・長さ・index・値を同じcanonical型/rangeへ揃え、float/bool等の別型を拒否することである。元claimを受け入れた後に全てのopeningが型不一致になる状態を作らない。

### AN-C02: 有界Courtに載らない過大leaf

513byteのleafをcommitした悪意あるtraceについて、PanelがRefutedと返しても、512byte上限のNode Courtへそのrecordを載せられずInvalidEvidenceとなる反例を再現した。正規Tree generatorが過大leafを拒否するだけでは、攻撃者がraw hashで作ったRootを防げない。

補修は、局所反証を作れない資料・構造未充足を、元claimの期待indexに結び付く有限の資料notice/caseへ送り、native holdによって成功を止めることである。無制限のrecord受信でNode予算を破らず、証拠の破棄だけで未充足を解消しない。

## 結果の読み方

修正前と修正後の独立反例結果は別に保存する。確定結果とscriptのhashは[VALIDATION](VALIDATION.md)へ記録する。認証済みfixture、ネットワーク・包含、正直Panel、DB等の未実装条件を、上記の実装不備と混同しない。

全Panelが悪意で検査・closureの前提も偽なら、正しい反証が届かず偽Finalが通り得る。これはPanel＋Courtの条件付き定理の境界であり、今回のテストではその境界例も維持する。
