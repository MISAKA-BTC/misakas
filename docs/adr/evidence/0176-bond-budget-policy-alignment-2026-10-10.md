# ADR-0176 / RFC15 bond予算・総影響保存の文書改定記録 — 2026-10-10

## 対象と結論

`pre`、HEAD `e4f3f6f3d81c75166ea6308bfff0b8aeaf836ca5`の、先行する未commit変更を含む文書群を対象とした。
[ADR-0176](../0176-bond-bounds-palw-production-rewards-and-final-weight.md)を新しい設計前提として採択し、
同額bond・同DAA期間・同共通倍率で、正直な計算と高速偽造の最大claim機会が同じになる規則を追加した。
倍率でclaim容量を増やしても、block・reward・Final weightの総予算は増やさない。
確率的検査・公開取得・独立監視・exact court・徴収可能担保とANDでPALWの成立を評価する。

この記録は文書の参照・改定範囲と差分の検査であり、全既存設計の数学的妥当性レビューや新modeの安全性証明ではない。
新予約/weight会計、共通回復時計、経済測定、実node受入試験とactivationは未完了である。

## 改定範囲

| 対象 | 改定 |
| --- | --- |
| 既存163 ADR、17 RFC（ローカルに実体がある全番号付き文書） | 共通前提とADR176への参照を追加。将来PALWに適用し、旧Status・測定・実装/activation記録は保持 |
| 新ADR176 | claim/block/reward/Final weight予算、倍率と一件配分、共通DAA拘束、責任担保、実効検出/徴収とnet損失、versioned移行・必須試験 |
| public-verifier ADR173 | D12、優先順位、完成証拠へ追加前提を接続 |
| RFC15 | §8.3のclaim容量・三総量cap、§8.4の容量拡大時総影響保存、Final/retired/reversalと全reader/undo、受入/activation条件を改定 |
| ADR27/28/32/38/56/151/167/171 | 客観slash、検査、producer経済、consensus work、class admission、責任担保、旧×1000 packageと確率的検査に個別の改定を追加 |
| RFC2/3/6/7/8/9/10/11/12/13/14 | 正確な実行statement、work一意性、全報酬経路、公的検査/乱数source、remote facts、admission/Final/settlement、資源・G14への接続 |
| 主要入口・設計21文書 | README/索引、architecture、readiness、spec/wiki、実装設計・詳細設計・slash、model/registry/toolingの共通前提を更新 |

改定した202設計文書のうち、172文書は共通bannerを除けば本文が変更前とbyte単位で一致する。
29既存文書はbannerに加え個別の設計/索引改定を含み、新ADRは1文書である。
dated audit/launch/evidenceの測定値を更新せず、存在しないADR0152/0160等の本文を推測で作成していない。
文書ごとの変更前後SHA-256・dispositionは[JSON一覧](0176-bond-budget-policy-alignment-2026-10-10.json)に保存する。

## 現行コードとの区別

ρは現行ではslot/tokenと担保予約の一部に使われる。[ADR167](../0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md)は全体emission budget・rider分配・breakerも記録する。
これらは全producer経路のbond/time予算や共通回復時計を実装した証拠ではなく、現在のFinal weightもρだけで一律に縮小しない。
モデルclassのescrow割引には対象制限がある。350 BILI / ρ=10,000は未採用の検討例である。
新設計では真の計算量を保持し、付与する経済的/consensus creditを別に配分する。
物理blockを小数個発行せず、権利端数・最小credit・丸め・繰越・Final後の会計まで総量保存を検査する。

共通`d + W`は最早の枠回復時計であり、court/DA/retention責任が残る担保解放を意味しない。
有罪・取消・早期Finalで採掘枠を早く返さず、同じ共有担保をclaimごとの損失として重複計上しない。
capだけでは実効検出率が0のときの費用節約を排除できず、49% reporter還流等を控除した経済評価を別途要求する。

## 文書検証

* 全番号付きRFC/ADRの共通前提参照を確認。
* 変更前との比較で、新規の欠落ローカルリンクと不対のcode fenceは0。
* 変更開始時のtracked file hashと比較し、この依頼で変更した既存ファイルは承認範囲のMarkdownのみ。
* この依頼ではソースコード、activation設定、稼働fleet、旧実測値を変更していない。
* `git diff --check`を実施。新規合意実装のRust/実node/経済試験は実施していない。

先行する49% reporterコード改定・Torrent/FPR文書改定とその検証記録は、変更開始時の状態として保持した。
その既存test PASSを、ADR176/RFC15の新会計保証やPanel=0の完成証拠として数えない。
