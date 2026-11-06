# A1 → A0: 未commit中間値を含む全lane exact検査の接続案

2026-10-31。**調査・接続案。未採択・未実装・無効。** 対象sourceはA1
`41c88e18a`。現all-node-commit derivation、native最小path、登録program、dtype、
layout、最大文脈、wire ID、署名domainを変更していない。実LLM対応・性能認定の
完了報告ではない。別derivation/資料codec/資源予約の確定はA0の責務とする。

## 結論と現在の制約

既存の公開 `DemandTarget::Node` と `eval_demanded` は、**元の未commit i64/i128
nodeを直接対象にできる**。結果は `Vec<i128>`。したがって元programを全commit化
せず、Nodeが全nodeの全laneを再計算する算術部分は既存APIで組める。

ただし公開Court入口は既存step treeのcommit leafを対象とする。任意nodeを認証済み
資料から検査する公開入口は未接続であり、現 `ExactPart` にwide値や偽のleaf indexを
詰めれば動く、という案ではない。

| 確認事項 | 既存source |
| --- | --- |
| i64/i128は `committable()` でない | `../core/src/types.rs:94` |
| commit属性だけ変更してもvalidationが拒否 | `../core/src/validate.rs:561` |
| 通常Linear loweringがi64 MatMulを生成 | `../lower/src/lower/mod.rs:2803` |
| narrowingがi128 Mul/Divを生成 | `../core/src/library/mod.rs:160` |
| 任意node target / i128結果 | `../core/src/demand.rs:184`, `:288` |
| whole-node oracle | `../core/src/interp.rs:349` |
| Court targetはlegacy leaf / 4-byte lane | `consensus/core/src/palw_tir_court_v1.rs:592`, `:618`, `:919`（repo相対） |

現initial profileの「全nodeがcommit済み」という条件を単に外してはならない。
通常のlowered programをそのまま全commit化できるとも、学習済みモデル群が現profileで
検査可能とも扱わない。wide **中間値** とwide **logits** は別の境界である。この案は
元々のi32 tiled logits/greedyだけを終端に使い、wide logitsはRFC-0043 / ADR-0211の
別profileの実装との明示接続が必要（本branch未取込・公開網未有効）。
旧WIPの一括importは行わない。

## 1. Nodeが導出するschedule

論理項目は次の二種類。ここでは正式型名・ID・codecを割り当てない。

1. **既存leaf項目**: 元のcommit node tile、Fixed checkpoint、Hist tile。
   既存のG14検査とlogits bridgeを維持する。
2. **未commit node項目**: `(position, occurrence, node, dtype, resolved_shape,
   first_lane, lane_count)`。登録programから導出し、Panelのrelation listは採用しない。

順序はposition → 実行するoccurrence → 元のnode順 → lane range順。
commit nodeでは元layoutのtile分割を使い、未commit nodeでは新profileの有限な
target-lane上限で `[0, elements)` を欠番・重複なく分割する。position末尾の
Fixed/Hist項目は既存順序を維持する。従って既存leafの相対順序は変えない。

`H = min(position+1, block_window)` は `history_length_v1` / `TensorType::resolve`
を使う。positionは `0 .. P+D-1`（最後は `P+D-2`）。postは既存job規則どおり
`position >= P-1` の位置だけを含める。prefill途中の不要postを追加して仕事を増やさない。
同じblockを複数layerで使う場合もoccurrenceを省略しない。

新しい必須relation数は、元のlegacy leaf数に**全未commit nodeの全tile数**を加えたもの。
native bindingの `step_leaf_count` は元のcommit treeの数のまま保つ。
expanded relation cursorとlegacy leaf frontierを別に持ち、片方を他方へ再解釈しない。
shape積・tile数・総relation数・費用のoverflowは受付拒否。最初は有限なplan導出量の
上限を設け、巨大contextを走査する費用も予約する。閉形式カウントの高速化は別途検証する。
出力格納は元dtype幅ではなく `Vec<i128>` の16 bytes/laneを予約し、sourceのwire幅・
decode展開・memo/metadata/clone分とは分けて数える。

`Select`/`Gather`で最終出力が消費しないlaneも、そのlaneを持つ元node自身の項目で
必ず対象にする。未選択側の元nodeを省略しない。未使用parameter byte全体の検査と
混同しない。対象は登録programに定義された実行nodeの仕事であり、Node再計算の重複を
追加creditへ算入するものではない。

## 2. 再計算する値と認証する資料

未commit項目は次の既存APIを用いる。

```text
target = DemandTarget::Node { ctx: DemandContext { pos, occurrence }, node }
request = DemandRequest { target, elements: canonical_complete_tile_indices }
(values_i128, work) = eval_demanded(original_program, validated_info,
                                  request, authenticated_source, remaining_limits)
```

target自身はcommitされていても再計算される。初期案では未commit nodeの値をPanelから
供給しない。Nodeが計算し、必要ならNode所有の監査記録へ反映して破棄する。
既存commit/logitsはその後も元のopeningと再計算結果を照合する。値を破棄できることは、
その計算を省略できることではない。

Panel由来のi64/i128 tensor/rootを新設しなくてもこの方式は成立する。旧4-byte
`PalwStepTileLeafV1`へwide値を切り詰めたり、旧leaf IDにwide nodeの意味を与えない。
将来Panelのwide値を比較対象にするなら、別途A0がdtype/shape/range/値幅とauthorを
固定する明示的な資料形式を定め、Nodeの全lane再計算との比較が必要。

**認証の対象**は次の資料であり、計算の正しさの証明ではない。

- 登録artifact inventoryの原byteと既存opening。param dtype/offsetは元programから導出。
- 正規jobのprompt、input commitmentと既存opening、固定されたgenerated IDsのpin。
- 同じcase/branch/witnessの既存commit/checkpoint/history leafへのopening。
- 正規authorがframe全体へ署名した資料packet。relay署名を値author署名へ置き換えない。

未commit値は算術で導出する。Merkle membershipや署名だけで未検査値を通さない。
Sourceに存在しないwide中間値を要求・受理する入口を作らない。

## 3. 認証済みsourceと帰納境界

既存 `DemandSource` を実装するsourceは任意のpeer callbackにしない。
expanded cursorから、検査済みの連続したlegacy leaf frontierをNodeが導出する。
供給できるstep値は、同じbindingに認証され、既に検査したprefix内のleafだけ。
target自身・未検査future leaf・別position/layer/branchの値は拒否する。

現Courtの `TirSource.before` / `authenticate_operands` はこのlegacy順序制約を持つ。
ただし `TirSource`, `check_carriage`, `authenticate_params/prompt/decode_pin` はprivate。
`check_carriage`は出力leafの認証から始めるため、未commit項目にそのまま使えない。

A0へ依頼する接続は、既存の認証部品を共有する**不透明なread-only source factory**。
Node導出targetとlegacy frontier、登録binding、既存source-only carriage、有限な予約を
入力にし、`DemandSource`と使用unitの記録を返す。出力のlegacy openingを捏造して
任意node APIへ迂回せず、新しいsource-only payloadは旧coneとは明示的に区別する。
既存のdtype/range検査・canonical read-set照合・opening depth/byte/heap上限を保持する。
公開のmutable `supplied` mapや「認証済み」booleanを渡す設計にしない。

Fixed stateは元の初期zero、検査済みcheckpoint、または先行positionのwriter再計算から
供給する。Histは元のwindowとinstance/layerに従い、先行する検査済みrow/tileに結び付ける。
carryは直前occurrenceの検査済みcarry-out。prefix継承や未検証checkpoint開始は追加しない。

これにより各項目は、認証済み原入力と既に検査したprefixだけから導出される。
全ての元node/lane項目と既存leaf項目が完了し、全logits lane・選択・結果rootを照合した
場合だけ終端事実を生成する。Gate C、native credit、経済責任は引き続き別条件。

## 4. 分割・再開・費用

**最初の接続案はlane分割まで**。各laneのreduction全体が一回の有限予約へ収まるjobだけ
受付し、収まらなければ未検証/受付拒否にする。WorkLimitを成功・算術不正へ読み替えない。
各呼出しの `DemandWork` とsource read、byte/heap消費をclaim全体へ累積し、呼出しごとに
上限を復活させない。fresh evaluatorは中間計算を重複し得るため、推論一回相当の費用とは
見積もらない。総RAM/CPUと実LLM最大文脈の実測は未完了。

`eval_demanded_range` はHを縮約するMatMul/ReduceSum/ReduceMaxの部分範囲に限る。
固定Kの一般MatMulやTopK全般を分割するAPIではない。`Engine`/`Frame`はprivateで、
各呼出しは新しいengineを作り、公開されたsuspend/resume tokenもない。

H分割を後で接続する場合、全spanの連続性とterm数だけでは足りない。現算術の
`order_free_add`は正・負の累積を別々にdtype範囲へ制限する。再開stateには少なくとも
元target/context/range、次term、正/負accumulator（ReduceMaxなら最大値と初期化状態）、
直前のNode導出state、累積費用を保持する。正負を打ち消した符号付き合計だけを保存して
overflow判定を省略してはならない。cast/div/round/clampは元primitive境界を維持する。
partial結果の合計や`supplied`をPanelからそのまま受け取る案にはしない。

Node計算済み中間tileの再利用は別の最適化段階。既存 `eval_demanded_range(range=None,
supplied=...)` の停止点は利用候補だが、sourceが返す未commit値はNode自身が検査した
同一context/rangeの値だけに限定する不透明なcache型が必要。Panel mapを許可するために
`supplied`を開放しない。cache欠落・再起動は再計算または未検証とし、保存flagだけで
coverageを復元しない。A6のjournal/undoと費用・signature保持も一体で扱う。

## 5. A0に確定を依頼する契約と試験

| 接続箇所 | 必要な確定・変更 |
| --- | --- |
| derivation | 元program全node/lane、実行occurrence、tile幅、legacy frontier、終端数を固定する別ID。現all-node-commit IDを変更しない |
| material | legacy coneと別のsource-only target/range資料codec。既存outer Result/Witness/author署名とのbindingを明示 |
| core API | 任意node用の認証source factoryと、source使用集合を閉じる検査。旧Courtの受理意味を変更しない |
| 予約 | legacy leaf capとexpanded relation capを分離。全再計算・source reads・認証・metadata・RAM・署名・再送を計上 |
| 復旧・責任 | Node進捗、frontier、費用、packet署名を同branchで復元。case/derivation変更で無料試行枠を作らない |
| 対象外 | wide logits、未承認program version、prefix再利用、公開網有効化、実LLM性能認定 |

最小の手計算fixture案は `[2,3] → I64×2^32 → I128二乗 = [2^66,9×2^64]` の後に
lane0だけsliceし、shift66でi32へ狭めるもの。lane1のI128値を仮に `9×2^64+1` としても
終端は変わらない。現live-node検査はnode参照の到達性であり、これを全lane被覆と見なさない。
新planはlane1も明示的に対象にし、Nodeが正しい値を導出したことを検査記録で確認する。
資料に未認証wide値が混入してもsourceとして読まない。これは提案fixtureで、今回
program/実装を追加・登録したものではない。

受入試験は最低限、最終node/未消費lane省略、range重複・欠番、別layer/position/shape、
wide byte切詰め、同一結果の別モデル/input、HのW-1/W/W+1境界、state/carry差替えを含む。
reduction再開を追加するならchunk1/3/ragged末尾をwhole-node oracleと比較し、
accumulator/reset/term index/古いbranchの差替え、restartでの予算復活を拒否する。

既存の `../core/tests/demand.rs::eval_demanded_equals_eval_cone_everywhere` は全位置・
occurrence・nodeを `Interpreter::eval_cone` と複数tile幅で比較する。checkpoint再計算は
同fileの `replay_between_checkpoints_is_the_run` が比較元となる。これらは算術APIの
差分試験であり、新source codec・公開認証入口・実LLM統合が完成した証拠ではない。

## 今回実行した確認

既存の `eval_demanded_equals_eval_cone_everywhere` を実行し、1件成功（172.88秒）。
raw logは `validation/a1-demand-api-research-2026-10-31.log`。新しいcoverage検査器や
上記の提案fixtureは実装しておらず、この結果をその受入試験として扱わない。
