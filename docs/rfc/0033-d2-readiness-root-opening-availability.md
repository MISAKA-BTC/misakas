# RFC-0033 — D2 readiness rootのdigest opening取得

Status: **Draft / Review Required**。ADR-0188 Bのcodec選択は承認済みだが、本書の供給方式・義務・料金・期限は未採択。判断案は[ADR-0200](../adr/0200-d2-readiness-root-opening-availability.md)。[正本RFC-0026](0026-court-policy-master-contract.md)、[モデル二階層](../PRINCIPLES.md)を維持する。

## 1. 未解決の条件

Bのrootとleaf数だけから、外部検証者はseatがcommitしたdigestとその認証pathを復元できない。検証者が自身のモデルを持っていても、誤ったdigestをcommitしたseatのpathを知ることにはならない。正しいdigestの計算やrootのdecodeだけで、外部からの反証可能性を成立済みにしない。

現codecはroot/pathの同一性と既存登録byteの再検査だけを行うpure predicateで、nativeのready行、権限、slashを作らない。この境界を保ったまま、pathの供給を閉じる必要がある。本書はモデル登録・係数・昇格・利用資格の審査を新設しない。対象はliteral seatが自ら署名したデータcommitmentと応答義務だけである。

## 2. C1：F02に束縛したdigest/path開示court（推奨候補）

- literal seatの署名はnetwork、native draw/beacon/class/seat/span、root、leaf数、元の保持契約・profileへ束縛する。DTOの自己申告をauthorityにしない。
- drawで固定されたinventory座標だけを要求できる。応答はclaimed digestとMerkle pathだけで、seatにモデルbyteを要求しない。正当なopeningは全利用者で共用し、署名の必要なfresh応答義務とは分ける。
- rootの受理前に、全許容draw座標の要求・応答・再取得・不正/重複/拒否・終了後保持まで、原資・現在のauthority・全分岐packet・実費・保持horizonを原子的に予約する。未決draw上限/価格/供給profileなら拒否する。
- 開示の応答期限は前提成立と合意状態の正規の処理機会から決める。実供給・明示した配送収録前提がなければdefault/slashにせずholdする。Panelの成功申告やwall clockで代用しない。
- 完成openingはroot/count/draw座標へ認証する。誤ったdigestの客観的反証は検証者自身の登録openingと既存predicateを使う。無効pathと非供給の効果を、モデル計算不正やmodel reviewへ読み替えない。

## 3. C2：補助materialの完成公開後にrootを受理

元draw全体の正規digest materialを、有限・有料・保持付きの補助経路で先に公開し、rootとの一致・完全性をnativeに検査してからrootを受理する。top-level root objectは小さいままだが、全digestの転送・保持・再取得・native処理費用が必要である。未完成の先着bodyに独占slotを与えず、partial公開を可用性と認定しない。料金・保持契約・全material量は未採択で、既存のwire上限を上げて収めない。

## 4. 必須検証

偽digest rootだけを公開してopeningを拒むseat、全外部枠の共謀/再充填、外部一人の要求、誤path/別draw/別署名、サービス未供給時のno-slash、restart/reorg/pruned IBD、全資本/保持、最遅受付・全deadline境界を実native/nodeで確認する。C2なら全materialの欠落・部分公開・資金不足の拒否と再取得を含む。いずれもpure codec試験だけでは完了しない。

未決のdraw上限・応答期間・料金・保持義務を仮値で埋めない。本番採択・pre統合・武装・配備は別の承認を要する。
