# RFC-0053 — 追加モデルのburnによる停止・登録者による解除

Status: **Accepted design / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-11
Decision: [ADR-0221](../adr/0221-additional-model-burn-pause.md)
Supersedes in part: [RFC-0052](0052-hugging-face-model-acquisition-and-claim-materials.md)の取得証明・独立登録検査・定期可用性認定・取得不能の裁定。

## 1. 採択と境界

ユーザーの後続明示指示「取得できない証明などはなくしてburnで停止方式に変える」「genesisモデルとforkモデルはこの対象外に」「追加モデルをこの対象にして停止できるようにする」を採択する。HFへの到達性をチェーンが認定する制度を廃止し、追加モデルの停止権をburnで購入する制度へ変更する。正常なモデルも停止できることは仕様であり、不正申告という裁定を設けない。

HFからの固定版取得、file hash、変換後のclass/inventory/tokenizer identityの照合は、Panelが同じ計算対象を実行するためのoff-chain作業として残る。取得の成否・public/gated状態・取得receipt・第三者署名・定期検査・`availability_valid_until`は、モデルのACTIVE/PAUSED判定や解除条件に使わない。NodeはHFを呼ばない。HFをモデルの通常取得元とする方針と、MISAKA独自のモデル全量再配布を必須としない方針は維持する。

HF取得不能の証明、登録検査者集合、可用性への投票・多数決・第三者裁定は導入しない。従来のHF可用性caseをburnの代わりに受理しない。claimの独立再実行承認、claim固有資料の供給case、局所不正反証は別の責務として残す。

後続のユーザー指示に基づく機能別の扱いを、次のとおり固定する。

| 仕組み | 変更方針 |
| --- | --- |
| MISAKA独自のモデル重み全量配信 | 廃止候補。必須経路にはしない。共有コード・旧profileの依存を確認してから削除を判断する |
| モデル用の消失訂正符号化・分散再配布 | 必須経路から除外 |
| Nodeによるモデル可用性サンプリング | 必須経路から除外 |
| HF取得不能の証明・再試行裁定 | 停止制度から廃止。Pause/Resumeはburnと権限・canonical状態で処理する |
| モデルroot・hash照合 | 維持。固定対象と取得bytesの同一性を確認する |
| claim固有の入力・trace・Merkle opening | 維持。Minerの供給・保持義務と資料caseを残す |
| Nodeの局所court | 維持。有界な局所証拠・必要な認証openingを検査する |

「廃止候補」は全配信コードの削除完了を意味しない。claim資料や局所courtと共有するprovider・Merkle・保存処理を一括削除しない。局所courtに必要なモデル由来operandのopeningも、モデル全量配信とは別に維持する。off-chain downloaderの通常の通信再試行は許すが、その結果を停止・解除の裁定に使わない。

## 2. 対象モデル

- 対象はcanonicalに登録された追加モデルの固定class ID。artifact root、登録者identityとの対応は受理済みnative登録から取得し、申告者の自己申告を使わない。
- genesis指定モデルと、その時点で有効な年次fork指定モデルは対象外。係数を引数として渡すだけで判定せず、既存のgenesis/fork指定とnative tier導出を使う。catalog root経由の指定も含む。
- 対象外モデルのPause/Resumeは拒否し、この契約のburnを消費しない。未知モデルも拒否する。
- 追加モデルが年次forkで指定された時点から、既存pause rowは稼働gateに影響しない。対象外化によって解除操作もできず永久停止する実装を禁止する。過去rowとburn履歴は巻き戻し用に保持できる。

モデルclassの取消、品質審査、tierの降格・昇格はしない。追加モデルの0.3、指定モデルの1.0、年次forkだけが1.0への経路であることは維持する。PRINCIPLES §9の旧一般的な停止・失効禁止と衝突する部分は、この追加モデルの可逆な稼働停止に限り後続ユーザー指示で改定する。旧`Frozen`等の独立した制裁をこの解除で取り消さない。

## 3. 状態と操作

初期状態はACTIVE。専用の版付きrowは少なくともclass ID、登録artifact root、状態、単調なsequence、最終操作のcanonical識別を持つ。旧class statusやtier欄を別の意味に流用しない。

| 操作 | 署名権限 | 必要条件 | 結果 |
| --- | --- | --- | --- |
| Pause | 誰でも。carrier支払者の実UTXO署名 | 対象追加モデルがACTIVE、停止burnの全額支払 | PAUSED、sequence + 1 |
| Resume | native登録者identityの正規鍵 | 対象追加モデルがPAUSED、解除burnの全額支払 | ACTIVE、sequence + 1 |

停止理由、HF HTTP応答、取得receipt、他者の承認は入力条件にしない。Pause済みへのPause、ACTIVEへのResume、古いsequence、別branchの参照、root/登録者のすり替え、sequence overflowは拒否する。失敗操作でこの制度のburnや状態更新だけを残さない。通常のcarrier手数料の扱いは既存規則に従う。

操作bodyは専用purpose、network/genesis domain、class/artifact、action、期待旧state/sequence、適用policy ID、正確なburnと署名者が許可する上限、受理期限を束縛する。Pauseの主体はcarrier支払者であり、このbody全体を含む取引への実UTXO署名で認証する。別の無資格keyによる重複署名は要求しない。Resumeには同じbodyへの登録者の追加認証を必要とし、登録者と異なる支払者も許す。登録者でない支払者の取引署名だけでResumeしない。登録者の鍵を喪失しても第三者判定による解除へ迂回しない。

`ValidatedTransaction`という型だけで支払者署名を確認済みと推定しない。署名不要scriptの支出も表現できるため、native handoffではbodyを実際に束縛する許可済み署名付き資金支出を確認する。

bodyは期待する直前canonical操作ID（受理済みcarrier txidと操作位置、未操作はnetwork/class固有の初期marker）も束縛し、現prefix rowと照合する。同じstate/sequenceでも別の前操作へ到達した分岐に署名を流用しない。同一の前操作がreorg後に再採用され、scope・sequence・policy・期限も一致する場合の再採用は許すが、旧branchのburnを保持したまま二重計上しない。署名が有効でも受理は常に現在のcanonical状態から再検査する。純粋なstate/sequence plannerだけではこのbranch認証は完成しない。

## 4. burnとcanonical順序

burn額と通常手数料を区別する。実検証済みcarrierの支払から、native fee accountingで当該操作のburnを一度だけ取り分ける。burnは他者への報酬ではなく供給からの消却とし、coinbase収入にも重複加算しない。任意の`burned=true`や残高の自己申告、署名済みという事実だけでは支払を認めない。

必要な認証・支払・旧状態照合・sequence更新・burn accountingは同じnative checkpointで原子的に確定する。mempool受付は停止ではない。「即時」はcanonicalに受理された操作から効力を持つという意味である。reorgでは状態とburnを一緒にundo/redoし、再生で追加burnを計上しない。

同じ処理単位の競合は既存の決定論的native適用順と、その時点のprefix状態で解決する。各新規受付・成功Final・新規報酬確定の直前に同じeffective pause述語を使う。先行Pauseがあれば停止、先行Resumeがあれば通常gateを再評価する。後続Pauseはそれより前に確定した効果を遡及取消ししないが、以後の新しい効果を止める。実装ごとにwall clock、RPC応答順、mempoolの到着順で判定しない。Finalとrewardを別経路で書く全consumerを監査し、一部のrouteだけで有効化しない。

現行`block_preparation`のdeadline sweepはobject loopより前、kernel tickは後に走る。この順序を維持する場合、pre-objectのFinalは同blockのPauseより先行する効果であり、Pause後のkernel処理や新規rewardは停止状態を読む。「同じblockにPauseがあれば全処理を遡及取消し」とはしない。停止中にdeadlineを消費する経路は単にskipして索引からclaimを脱落させず、原期限に基づく未検証終結と責任・資料保持を明示して保存する。

## 5. 停止中のclaim・報酬・権利

PAUSEDは対象モデルの新規claim受付、成功Final、新規報酬確定を止める。quoteは停止理由を返し、quote後に状態が変わった場合も実受付で再確認する。成功の旧条件を省くものではない。

既存claimの資料供給義務、不正反証、異議への応答、責任追及、slash可能な既存違反の裁定、失敗・未検証Abort、保管・GCは継続する。pause rowを書いたことだけでMinerを有罪とせず、証拠や責任を消さない。notice/hard、保持、責任、権利の期限を延長・短縮・リセットしない。停止中に原期限を過ぎたclaimをResume後に成功へ復活させない。

既にcanonicalに確定した支払・UTXO・生成済みblockを遡及取消ししない。未使用の生成権とpending payoutをどの境界から「新規報酬確定」と扱うかは、実装台帳でproducerごとに固定する必要がある。停止中の新しいreceipt spend・coinbase確定を抜け道にせず、既決済出力の通常支出とは分ける。未確定の境界がある間は公開activationしない。停止期間を理由とした新規補償発行は導入しない。

## 6. 有限資源・経済前提

停止burnと解除burnは別の正の有限パラメーターとする。額・価格policyの版、carrier上限、受理期限、同時処理数、状態/journal/IBD保存費用は未確定であり、本書は数値を採択しない。無制限やゼロ料金で暫定有効化しない。単一model rowの更新でも署名検査・索引・journal・rollbackを有限に計上する。停止と解除を連打する候補の負荷も含める。

固定burnは資金力のある攻撃者の繰返し停止を防がない。モデル登録者に反復解除の費用を課し、競合停止による便益がburnを上回る攻撃も可能である。この制度は可用性保証や正直な申告の選別ではない。HF取得成功を示して無償解除する例外を作らない。

## 7. 実装・有効化条件

1. native登録とgenesis/annual指定に基づく対象判定、別state、期待sequence、必要署名/burnを導く純粋なplannerを実装する。plannerの候補結果を受理済みrowとして使わない。
2. 専用wire/purposeを正式に割り当て、実署名と支払、state保存とundo/redo、認証snapshot、pruning/IBD、mempool/peer work制限へ接続する。
3. quote/rehearsal/実受付、成功Final、新規報酬/生成権の全writerへ同じgateを接続する。claim反証と責任追及を止めない試験を含める。
4. duplicate、競合Pause/Resume、wrong signer、費用不足、対象外モデル、年次指定境界、再起動/reorg、同一処理単位のFinal/報酬競合を検証する。
5. 数値台帳・経済リスク・実測と公開移行を整えてから有効化する。現行公開presetの値や高さを本採択だけで変更しない。

新HF公開契約の独立検査・定期更新・有効期限の実装は、残件から削除する。claimごとの再実行と資料供給、G14のFpTokenizer/TKR1、同じ公開TFP1のquote/受付/Finalは継続対象である。
