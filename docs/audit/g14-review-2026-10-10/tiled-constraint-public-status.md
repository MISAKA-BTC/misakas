# Public constrained selection follow-up

2026-10-11。基点 `4f1120d9e80d289702af67acd7177ce311674164`。G14 全域の objective と ADR-0177 D7 の「正しい登録モデルを保有する verifier」という前提を維持する。

## 実装

False-Valid の事前判定は、processor の gate / fold と同じ block-context session rule を使うようにした。旧 helper は `RefusedUnderSession` を固定しており、public-filer fence が有効な実ブロックでも constrained direct proof を事前拒否した。既存の height-free API の振る舞いは保ち、processor の実読取だけ新しい helper に接続した。activation、adjudicator、consensus object、state-root encoding は変更しない。

tiled constraint の node filer は、認証された最初の public event に含まれる生成 ID 列と、host の認証済み token table から違反候補を選ぶ。禁止 token / premature stop は table と公開 prefix から選び、許可 token 同士の選択違反は verifier 自身の同じ job の生成 ID を候補に使う。候補位置の committed / beating tile を最大2つ選び、既存の signed reservation / DA demand / reacquisition 経路で公開 material を取得する。以前の history watermark より古い既存応答も backfill する。

選択済み tile だけ cache に保持する。binding / row / tile / prefix rendering / EOG list / job identity が全て認証された後、既存 court predicate で不正が成立した proof だけを contradiction tag 14 として reporter の commit・offence・reveal に渡す。verifier 自身の値は court proof に入らない。公開 row が候補 lane の勝ちを証明しない場合は算術局所化へ戻す。reorg は選択状態を破棄して元の claim から再取得する。大きい job / pin / rendering proof は直接 evidence の100 KB ceiling を守って拒否し、未実装の multipart terminal carriage が必要であることを返す。

## 検証範囲

最終結果は添付 recipe / raw logs に記録する。node の filer 30 tests（既存28件と新規2件）、LG14-B 16 tests、False-Valid 12 tests、receipt history 1 test は PASS。新規 tiled test は position 2 の禁止 token、premature stop、異なる tile の admitted beating lane、公開値が勝ちを示さない own-replay nominee、foreign job、偽 tile、欠けた disclosure、wrong table、未選択 tile の cache 拒否と後続 row の learn を確認する。通常 node check は PASS（38.57 s）。この test は合成 profile の builder / book / engine の検証であり、登録済み tiled class の actual service conviction の証拠ではない。

旧 session rule のままの実serviceは、pagination 修正後も **0 PASS / 3 FAIL、680.67 s、exit 101**。3件とも producer の kind-4 conviction / reveal は到達したが、Panel 全員の追及が完了しなかった。失敗ログを保存する。session rule 修正後の actual service は **3 PASS、444.71 s、exit 0**。forbidden / premature stop / beaten token の全ケースで、ordinary bonded verifier が公開2,065 blocks を fresh DB で再検証し、元 job body / metadata がない状態から公開 job を再取得して、position 2 の tag 15 反証を自動生成した。producer と全5 Valid signers の conviction・正の collateral 徴収・reporter commit / offence / reveal の実 block 包含・予約解放・service 終了を確認した。builder / book / engine の59件と合わせ、この round の node 検証は **62 unique PASS**。service 実行後の再ビルドは cache test のみの追加で、production / service source は同じだが、その前の service binary hash は未保存であり最終 binary hash との同一性は主張しない。追加 cache test の初回は fixture を input bootstrap 状態にしたため、後続 tile の learn を拒否した。fixture を実際の mismatched replay 状態に直し、再試験で PASS。全 Panel の conviction 条件、期限、反証 kind / position の assertion は弱めていない。

追加 core 回帰は filing 5件、offence attribution 19件、decode constraint 19件の **43 unique PASS**。node と合わせ今回の最終結果は **105 unique PASS**（過去 round と重複加算しない）。初回 core ビルドは空き容量不足で exit 101、自分の一時キャッシュ内の使用済み object を整理し、core test の debug symbols だけを減らした再ビルドは exit 0（7m 38s）。test の assertion・最適化・意味は変更していない。raw logs、source / 最終 binary hash、再実行コマンドを recipe に保存した。shipping harvest 361 pins の再検証はこの round では実行していない。

## 残り

1. 登録 tiled family の通常入口から、公開 job/input・位置別 disclosure・actual service conviction / withheld-tile DA default を検証する。
   最初の対象は既存 LG14-B fixture の Qwen25 A16 family とする。通常 registry の canonical derivation と class registration を使い、合成 profile への置換で済ませない。TIR backend の free-prompt は現時点で `TIR_FREE_PROMPT_CLOSED_V1` を返すため、この constrained FP service の実証には使えない。
2. 最大 job / pin / rendering / terminal proof の分割搬送と admission、session 数、包含期限、累積 RAM を閉じる。
3. 登録 tokenizer / model / table の association、out-of-vocabulary token、stop 後の ID / context、initial / inherited-prefix state、全 active family の計算・job/input/output/state/DA 違反を監査し、足りない公開 court を実装する。
4. restart / reorg / pruned import と socket RPC/P2P、producer 自動 responder、最大 profile と経済条件を一人の verifier で検証する。

shipping activation は引き続き拒否される。公開 table を host に予め持たせた合成 fixture、手動で供給した public answer、狭い builder test から G14 全域の保証を主張しない。
