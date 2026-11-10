# RFC-0051 — 受付前 tokenizer 資料の有料搬送と native 登録

Status: **Draft / Review Required / NOT ADOPTED / NOT ACTIVATED**  
Date: 2026-11-11  
Decision proposal: [ADR-0219](../adr/0219-funded-preclaim-tokenizer-registration.md)

本書は公開 T12 の実装調査から作成した変更案であり、ユーザーによる採択記録ではない。番号はこの checkout で未使用だったものを使用する。採択済みの [実装計画](../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md) 全体を置換しない。

## 1. 問題と現在の到達点

対象候補、class、artifact root、container root は [公開読戻し記録](../evidence/palw/public-t12-studio-binding-quote-2026-11-11.json) に固定する。PALWART3 binding は公開受理済み。同じ未署名候補の DAA 21,019 の quote は `quoted=false`、`G14_ROUTE_ABSENT (FpTokenizer)`、plan なしだった。

現在の拒否には、class semantics が未スケジュールで、対象 class の raw byte 数・key-event 上限の宣言が未受理であることが関係する。宣言と既存 EOF の受理は第一段階である。それだけで期限内の quote が成立するとは限らない。実ファイルを使った従来 planner の比較は、出荷設定で 8,567,151 / 許容 3,120 DAA、Batch 比較で 72,565 / 許容 6,880 DAA と拒否している。この比較の包含時刻と署名欄はローカル fixture である。

既存の `ArtifactInventoryOpen/Part/Finish` は raw の長さとハッシュを認証し、Finish 後に raw を保持しない。全 rendering table と EOG を確認する `install_verified_tokenizer_registration_v1` には production caller がない。既存 family raw 取得には受理済み claim が必要なため、この claim の受付前の登録入力にはできない。実 raw は 2,104,556 bytes で、現行 ObjectChunk の総上限 1,600,000 bytes を超える。

未受理 row の注入、ローカルファイルの存在、EOF certificate、Panel の署名を、全 rendering 検査済みの native row に読み替えて解決しない。本案はその入力経路を有限の新しい契約として定義する。

## 2. 変更範囲

別 version の `TokenizerRegistrationOpen/Part/Finish` と専用の pending/raw/accepted 状態を追加する案とする。名称は論理名であり、wire tag、fence、domain、params fingerprint の実値は中央 registry への衝突検査後に割り当てる。既存 EOF、TFP1、BCT1/BCT2、staged movement の既存 tag の意味を変更しない。

既存の署名付き court carrier 搬送と native の実手数料検証を利用するが、旧 action として復号できる形式にはしない。新 action の dispatch は新 fence と採択済みの有限 policy の両方を必須とする。policy がない場合は拒否し、公開 default を埋めない。

登録するのは tokenizer bytes と既登録 rendering table の対応である。モデルの品質、公開資格、tier、計算結果の正しさを認定しない。モデル二階層、claim ごとの検証・認証・Final 条件を維持する。

## 3. 認証と状態遷移

### Open

署名対象を network、version、policy ID、class ID、artifact root、container root、semantics ID、既受理 EOF inventory ID、tokenizer ID、table root、vocab、rendering rule、EOG opening の digest、opener bond、nonce、exact raw bytes、受付期限、完了期限、保持期限、最大登録サービス料金 `max_registration_charge` に固定する。accepted block や将来の transaction ID を事前署名に要求しない。Open ID は専用 purpose domain でこの canonical 署名前 body を hash した値とする。network を含み、署名 bytes や将来の carrier txid を ID に含めない。

class 登録者が署名した既受理の semantics と binding を前提とする。資料搬送の opener は別の適格 bond でもよいが、class の宣言を変更する権限は得ない。opener の署名・費用は資料搬送の契約だけを認証する。EOG は raw JSON から推測せず、署名済み semantics の table root への opening で認証する。

受理 parent 上の全 pins、EOF、現在の opener 資格、期限、原資、個人・全体の pending 上限を検査する。全 part、最終パース、保持・回収の容量と手数料を同じ native quote で計上し、実容量の予約と pending row の挿入を原子的に行う。未予約の見積もりや署名だけでは Open を受理しない。

pending は `(class_id, semantics_id, opener, Open ID)` で識別し、最初の Open に class 全体の排他権を与えない。別 opener が自身の枠と料金を使う登録を許す。accepted key は既存の `(class_id, semantics_id)` のままとする。全段階で Open に保存した opener 公開鍵と現在の bond 資格を検査する。予約義務の間はその義務に対応する release hold を保持するが、別の正当な失格・退役を覆さない。資格を失った opener の新 Part/Finish は拒否し、期限後に有限契約を回収する。別 wallet が carrier 料金を支払う場合、その UTXO 所有者の通常取引署名と opener の登録サービス同意をそれぞれ必要とする。

### Part

各署名は専用 Part purpose domain の network、version、Open ID、policy、part index、offset、payload digest と opener を束縛する。payload の上限は既存と同じ 65,536 bytes とし、全長は Open の exact raw bytes に限定する。canonical な part 数と最終 part 長を checked arithmetic で導出する。

同じ index の同じ bytes の再送では進捗・費用・予約を二重計上しない。異なる bytes、余分な part、境界外 offset、別 Open・policy・署名者を拒否する。受信だけではなく、native 受理した part を branch に結び付けて保持する。順不同の合法 part を固定 index で格納できるようにし、欠けた範囲を RPC で読み戻す。

この indexed raw spool は旧 EOF の順次 offset checkpoint と別形式にする。旧 EOF fold の hash 継続行を順不同用へ読み替えない。

### Finish

Finish の署名は専用 Finish purpose domain の network、version、Open ID、policy、EOG opening digest、完了期限、opener を束縛する。Finish の実 accepting parent 上で Open と全 part が同じ branch に先行受理されていることを要求する。同じ処理単位内の最後の Part と Finish は許さず、次の accepted parent から Finish を行う。Open の parent を現在 parent の代用にしない。

exact EOF、raw hash、EOG opening、現在 parent 上の class/semantics/container pins を再検査する。pins が変化・消失していたら新しい意味で継続せず拒否する。scratch と work 枠を取得してから parser を実行し、重複 JSON key、vocab 外 ID、不正 rendering、全 table root の不一致を拒否する。既存の `derive_tokenizer_rendering_v2` の全 ID 順検査を使う。

Finish の実 accepting parent と point を使って private writer を呼ぶ。sender が完成 row や検査結果を指定する入口は作らない。検査・費用・予約消費・accepted row の書込みは原子的に実施する。失敗時に「検証済み」の行や部分的な経済効果を残さない。accepted key に先行成功があれば上書きせず、同一 pins に対する後続契約は `Superseded` として予約解放・保持後回収へ進む。これを二件目の検査成功や追加料金徴収としない。

claim は登録 row が自身の受理点より先行することを既存の DAA/blue-score 条件で検査する。この登録だけで元候補を受理したり、claim の anchor や ID を更新したりしない。

## 4. 有限の費用・容量 policy

policy は少なくとも raw bytes、vocab、key events、EOG IDs、総 rendering bytes、各 part/署名/control bytes、parser scratch/work、保持 raw・metadata・journal bytes、pending 件数、予約期間、rollback/pruning 余裕、単価と総手数料上限を持つ。すべて有限値とし、未定値・ゼロを無制限の代わりにしない。

個別上限だけでなく、各 block/DAA の同時転送・署名・parser・retained bytes・reorg 再実行の合計を同じ予約 book で制限する。Node 固有の一時 RAM 不足は local capacity failure とし、合法な chain を無効と判定する理由にしない。policy による合意上の超過とはエラーを区別する。

work の入力単位は raw bytes、JSON key-event 上限、vocab leaves、EOG IDs、総 rendering bytes、署名回数、hash bytes とする。有限で正の policy 係数との checked な積和から quote を一意に導出し、実行時間や Node の機材を合意入力にしない。Open は通常 Finish と同じ原期限内の一回の recovery Finish の枠を予約する。取り逃した通常枠は recovery 枠でのみ再試行でき、両方を失えば未完 expiry とする。再送で元期限や枠を延長しない。reorg replay は元の受理操作の再生であって、新しいサービス枠・charge ではない。無効入力にも最大検査費用が発生するため、受理前の work scanner と既存 mempool/peer の local work 制限を接続する。

候補 block の総 work scanner は parser より前に、無効・重複 action も含む全 carrier の費用を数える。同じ予約 Finish slot に複数の不正 carrier を詰めても、一本分の予約だけで複数回 parser を実行できないことを回帰検査する。local mempool/peer 制限をこの合意上の block 上限の代わりにしない。

現 rooted verifier の安全上限（raw 8 MiB 等）を値引きや性能認定に転用しない。scratch の現算式は `raw_bytes * 128 + 4 * eog_count + 16,384`。実 raw だけでも約 257 MiB を予約する。これは計測 RSS ではなく保守的な予約量である。合法な最大入力、巨大文字列、疎な ID、重複 key、最後の leaf だけ不一致を最低要件機材で測る。

旧 EOF の rent 11,929,756 sompi と deposit 35 sompi は hash 継続用の実ファイル見積もりであり、本案の料金に流用しない。新 quote は転送、native 認証、raw 保持、全パース、復元と GC の費用を分けて出す。

料金は Open carrier の実 native 手数料から登録サービス費用として一度だけ取り分け、残りだけを通常の collectible fee とする案とする。Open で約束する保持・検査サービスは Open で前払いし、Part/Finish から同じサービス代を再徴収しない。後続 carrier の通常送信手数料とは分ける。`max_registration_charge` は登録サービス代だけの上限で、未知の将来 carrier の通常手数料を含まない。wallet は別の通常手数料上限を各送信で確認する。

同じ `(Open ID, policy ID, charge kind)` の charge ID と算式を transaction fee projection、fold、miner collectible、再編成の undo/redo で共用する。重複・拒否された action に新しい登録 charge を作らない。失敗 carrier の通常取引手数料の扱いは既存規則を維持し、追加サービス料金と混同しない。旧 branch の取り分けを rollback した後の再採用は新 branch の一回の charge として再現する。

初期版の登録 charge は **burn** とする提案であり、新 escrow、報酬請求権、架空の支払先を作らない。通常 UTXO/署名検証、native action の原子的受理と charge の導出、その受理結果を使う collectible/coinbase 照合の順を必須とする。Open が不受理なら追加 burn はゼロ。既存規則がその carrier 自体を無効とする場合は包含を拒否し、通常取引として扱える場合だけ既存の通常 fee を適用する。費用不足を「既に支払った」と見なして row を書く fallback を設けない。

opener の署名上限を超えて引かず、claim の将来報酬を原資にしない。未利用予約を期限で解放しても、既に購入した期間のサービス料金を架空の refund として発行しない。本案は追加の slashable deposit を導入せず、release hold は既存 bond の有限な義務保持に限定する。実装前にこの料金分解と既存 native fee accounting の接続をレビューする。

公開単価・総額・TTL・同時件数・activation 高さは本案で採択しない。[parameter register](../governance/parameter-register.md) に旧新値、導出、機材、測定、影響と承認を別途記録する。未確定の profile を出荷 preset に追加しない。

## 5. 失敗・復元・互換性

期限内の未完登録は accepted row を作らず、期限後に予約を解放する。part の native raw spool は所定の保持期限後に回収する。正常 Finish でも必要な復元期限より前に raw を削除しない。登録失敗だけでモデル失効、tier 変更、claim 不正の slash を行わない。

Open の受付期限、Part/Finish の完了期限、native raw spool の回収期限、accepted row の寿命を別欄にする。回収期限は `完了期限 + policy の保持余裕` に固定し、Open の quote が最遅終端までの全期間を価格付けする。正常 Finish・Superseded・未完 expiry のいずれでも同じ有限回収期限を使う。pending の予約解放は raw の即時削除を意味しない。accepted row は既存 immutable row の寿命を維持し、raw の回収と同時に消さない。

各 Open/Part/Finish の undo/redo、fork 差替え、restart、snapshot、pruned IBD が同じ accepted row と費用を復元することを必須とする。peer が送る完成 row は既存の認証済み state root と bounded import の規則で検査し、裸の row や孤立したローカル raw を権限にしない。

合意 state の raw GC は accepted DAA と固定した回収期限で決め、Node がローカルに観測した pruning の進み具合には依存させない。古い認証済み snapshot から再生する場合は、既存規則で保持する carrier block bodies と native undo/redo journal から Open/Part/Finish または expiry/GC を再現する。GC 後の認証済み snapshot なら、そこに束縛された accepted row または未完契約の削除結果を引き継ぐ。裸の row から検査成功を復元しない。

履歴 block bodies と undo journal の物理回収は既存の pruning/認証済み state-root 規則に従い、raw GC の副作用で早めない。pruning が停止した Node の通常履歴保管を、この追加サービスが無期限に価格保証するとはしない。新契約の有限料金は native raw spool と予約期間に対応し、通常履歴は既存 carrier/storage policy の対象として bytes と書込み量を別途計上する。この区分で最低要件の disk 上限を満たせなければ公開 profile を有効化しない。raw state・journal・snapshot の物理コピーを容量から落とさず、同じ logical charge の複数回徴収にはしない。

合意 policy の Open cutoff とローカル RPC の停止は別である。新 Open を止める合意上の切替でも、既受理契約の Part/Finish・料金・保持・回収には Open 時の policy を適用し続ける。既存の状態に基づく現在の opener 資格検査は維持する。旧 EOF の bytes・hash・費用・snapshot 意味は変えず、旧 row を新しい検査済み row に自動昇格しない。

## 6. 実装と検証の順序

1. 専用 codec/domain/policy、bounded native scanner、quote を実装する。署名・全 field 置換・truncation・overflow・上限境界を検査する。
2. 実 fee 検証、Open/Part/Finish fold、予約・journal・GC・復元を接続し、公開と同じ dispatcher を通す。partial write、二重 charge、異なる branch の流用を拒否する。
3. RPC と CLI に quote、署名対象、欠損 part、費用、受理 block/DAA と registration ID の読戻しを接続する。
4. 実 tokenizer と最大合法入力で cold/restart/reorg/pruning/IBD、容量不足、未完、重複・競合送信を検証する。ローカル成功を公開受理として記録しない。
5. 全体 parser と既存の逐次 tokenizer 裁定を同じ有効・無効入力で比較し、EOF、重複 key、rendering、EOG、key-work の判定差を解消する。別 version の Batch quote/admission 共通 planner に accepted registration を接続し、全 branch の公開回復が 6,880 DAA に収まることを検証する。未完の Output/Selection/DA を省いて quote 成功にしない。
6. 独立レビュー、数値台帳、全ノードの移行計画と明示的 activation 承認後、公開 T12 の同一 pins で登録 row を読み戻す。その後に対象候補の `quoted=true`・空 refusal・非空 plan を確認する。

本案の採択だけでは T12 への送信・有効化・quote 成功・main push を完了としない。元候補の executor bond は別途 1 BILI 不足している。補充経路は本案に追加しない。新しい bond を使う場合は別 claim ID として扱う。

## 7. 比較した代替案

- 旧 EOF certificate を検査済み記録へ昇格する案: 全 rendering/EOG の検査と入力保持がないため不採用案。
- raw を一取引へ詰める案、既存 ObjectChunk の上限だけを引き上げる案: 実サイズと処理・保持・再現費用の契約を解決しないため不採用案。
- keeper のローカル検査、任意の Panel receipt を合意記録にする案: 他 Node の独立再現を満たさないため不採用案。
- 現行の claim 後 family raw を先に使う案: 受付前登録に対して循環するため不採用案。
- 本案の有限 preclaim stream: 公開入力から全検査を再現できるが、新しい状態・費用・容量契約を伴う。採択、実装、測定、有効化の各段階を分ける。
