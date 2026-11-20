# RFC-0055 独立レビュー記録

- 記録日: 2026-11-17
- 対象: [RFC-0055](../../../rfc/0055-global-admission-court-calendar-and-one-bps.md)
- source基準: `MISAKA-BTC/misakas pre@c3da7c6eaf5bf87f8e032d5b122c603d720a8398`
- 判定: **対象snapshotの文書整合を確認。native実装・実機性能・公開activationは未検証。**
- 対象設計の状態: `ADOPTED_DESIGN_NOT_ACTIVATED`
- レビュー範囲: 件数非固定の受付、全工程予約、唯一の適格正直者の救済、Node内部の資源上限、時計・包含、Final・権利・資金・復旧。

## 1. 判定と前提

最新のユーザー指示は、courtを一件・三件・五件などの固定件数で制限することではなく、過剰な提出・計算負荷でNodeの管理下資源が無制限に増えない構造である。現行本文はこの指示に合わせ、共通checkerの費用上界と全体のflow/stock予約から受付量を導く。120秒は計画・集計・difficulty更新の区間であり、30秒の専用court枠や一周期一claimは規範ではない。

レビューした範囲で、修正後の本文に追加の重大な文書矛盾は検出しなかった。ただし、これは条件を満たす実装・profileの存在、十分な処理量、最低Nodeのスペック、1 BPSの実到達、一般的な安全性・活性の証明を意味しない。本文が要求するqualificationとnative接続は後述の未完了ゲートである。

モデルのcanonical整数計算が完全一致するというユーザー前提を維持した。数値誤差を反例にはしていない。一方で、整数の一致だけから記録の完全性、入力・source・状態接続の認証、Nodeの資源上界が成立したとは扱わない。

本レビューではRFC本文と保存済み抽象モデルの範囲・結果・hash対応を確認した。レビュー担当による本番コード変更、nativeテスト実行、実機測定、activationは行っていない。主題別正本への統合、registry/params/indexの網羅確認、変更前後の全体差分確認は統合作業の記録と区別する。

## 2. 旧固定案からの是正履歴

以下は同一作業中のレビューと修正の履歴である。旧案の数値は現行profileの採択値や実測値ではない。

| 段階・論点 | レビュー上の問題または要求 | 現行本文での扱い |
| --- | --- | --- |
| 初期の120秒、通常一件＋反証三件という例 | 一周期の処理上限をcaseの生涯上限や先着人数へ変えると、遅い唯一の正直者を排除する。interactive courtの全手順を30秒で完了できるとも限らない。 | §0/4/7/8で固定件数・固定30秒枠を撤回。全actorの最大合法継続を複数区間へ事前予約する。 |
| ユーザーによる固定件数の撤回 | 件数を決め打ちせず、全モデル合算の実際の資源要求から受付量を変える必要がある。 | 共通cost vector、flow/stock、期限、既存予約を用いる。難易度追従の遅れや上限でもhard admissionが保護する。 |
| 未知modelの追加 | modelごとのbenchmarkや人手のallowlistを新たな入口にしない。 | §2/14で共通checker・codec・meter・binary・機器profileを版ごとにqualificationし、対応kernelで表せる未知modelはdescriptorで自動判定する。 |
| 実行入口と状態効果 | 予約済みというだけでは、別actor・別task・別世代・期限外の実行や二重消費を防げない。 | §8の`ExecuteCourtTask`と原子的debit/once markerに、本人・役割・世代・前提・期間・費用・資金を明記した。 |
| carrierの短い寿命と重い検査 | 生成期限だけを短くして別の包含猶予を無計上で許すと、burst上限を誤る。固定30秒の計算猶予とも整合しない。 | §10でLをreleaseからcanonical包含までの総寿命とした。検査・伝播・包含の両期限を予約し、stagingも費用・保持・入力root・prestateへ束縛する。 |
| 局所不合格とcase終結 | 無効packet、資料不足、局所関係が成立した正規裁定を混同すると、Qや他人の救済を消せる。 | §8で`EvidenceRejected`、`MaterialPending`、`LocalPredicateHolds`を分離。native終結条件を満たす当該CaseIdだけを終結させる。 |
| Nodeの軽いcheckerとPanelの準備 | Nodeの局所検査が有界でも、cold Panelの取得・全再実行・証拠作成やproviderの供給が期限内に終わるとは限らない。 | §7.1で役割別容量に取得・完全再実行・比較・局所化・opening・実供給を追加。同じ物理資源の重複計上を禁止した。 |
| bootstrapと合意入力 | 負のepochを想定する、収集開始後にDを変更する、各hostの空きRAMや遅さで共通Dを変えると規則が一意にならない。 | §6.2/6.5でepoch 0/1は新規admissionなし、D2/low_runを先に初期化。b>=1から完了epochに基づきb+2を更新し、local状態を合意controllerへ入れない。 |

初期レビューで重視した原期限の維持、通知を全時間帯に受け付けること、source取消後の権利再利用禁止も現行§5/8/11に残っている。固定件数を撤回しても、保護予約や費用・資格を撤回したわけではない。

## 3. 最新本文で確認した安全条件

ここでの「確認」は規範が明記され、関係する節と矛盾しないことの確認である。実装試験の合格を表す表ではない。

| 項目 | 確認内容 | 根拠 |
| --- | --- | --- |
| 件数と経済的機会 | model数や固定court数を容量としない。新規Dは受付候補の資格であり、既存P_m/CCU/生成権/有限creditや一claim当たりの権利数を再定義しない。 | §0/1/4/6/11 |
| 共通checker | WellFormed/Init/全Step/End、値の出どころ、状態接続、出力・停止・仕事量を対象にする。Minerが最初に分解とtraceへcommitし、異議後のPanel作成値へすり替えない。 | §2/3 |
| 多資源の上限 | CPUだけでなく署名/hash/parse/展開/wire/I/O/control/carrierを合算し、RAM・queue・保持・journal・GC等は占有期間付きstockにする。空・重複itemにも非ゼロ費用がある。 | §4 |
| 一人の救済と期限 | 最後のnotice、最遅応答、他actorの最大合法case、防御、未検査状態、終結・保持を予約する。先着順でactorの機会を奪わず、全期限区間と因果順序を確認する。 | §5/7/8 |
| 取得・Panel・provider | Nodeだけでなく各役割の実サービスを計上し、共通cost式とshapeから導く。Nodeのmodel保持・全推論・GPU fallbackへ変えない。 | §1/7.1 |
| 原子的受付・実行 | actual prefixの資金・資格・費用・予約を再検査し、quoteを予約権としてinstallしない。原claim・資金・book・一回性を同じcheckpointで更新する。 | §6.4/7.3/8 |
| 全時間帯control | 適格notice/MaterialRequest/Qは固定のnormal/negative時間帯を待たない。新D、新PoW、ExecCreditを既受理救済へ要求しない。 | §8 |
| Qの維持 | 不正署名や他人の予約IDは本人の保護予約を消費しない。誤ったpacketや不足資料で既存Q・他case・他actorを消さず、重い証拠の検査待ちもQを維持する。 | §8/11 |
| Node内部保護 | decode前にlength/count/展開・費用とmemory leaseを確認する。error/log/undo/clone/decompressionも有界。cancel通知だけではworkerやbufferのleaseを返さない。 | §9 |
| 合意とlocal拒否の分離 | hostの空きRAM・遅さは共通difficultyの入力にしない。local資源不足や復旧不能を永久block-invalid・算術有罪・ChallengerDefaultへ変えない。 | §6.5/9/12 |
| 1 BPSと並列DAG | 全kindでpermitを共有し、L内に複数permitを許す。canonical foldで一回消費し、全fork・悪意packet・IBDまで1 BPS内とは主張しない。救済搬送を未認証workに依存させない。 | §10 |
| Finalとsource取消 | 同batchのnotice/Q/反証/依存取消をFinal・発行・予約・消費より先に適用する。元expiryを延ばさず、消費済みunitの再発行を禁止する。 | §11 |
| 原資と解放 | 将来収入を既存責任の必要原資にしない。料金条件を原署名へ固定し、hard/Final/Abort後にも残る保持・金融・GC責任を独立に精算する。 | §12 |
| 復旧・移行 | 空のbookで受付を再開しない。reorgは同じjournalでundo/replayし、旧世代workerとleaseを管理する。旧義務を元期限内に収容できなければ新profileを有効化しない。 | §13/15 |

## 4. 抽象モデル結果と証拠の範囲

保存済みの[model-results.json](model-results.json)は、2026-11-17T05:49:31.969324+00:00の実行について30件中30件成功、failure/error/skipは0件と記録する。[check_protocol_model.py](check_protocol_model.py)の費用・容量・時間・件数は非規範の合成fixtureであり、実測値ではない。

レビュー時に本文とscriptをhashし、保存結果の以下の値と一致することを確認した。

| 対象 | SHA-256 |
| --- | --- |
| RFC-0055本文 | `0804ca748b7872171a262dfa12dc2166b2e4125a6022ad64b623e0daf6b70fe8` |
| 抽象モデルscript | `29805e4214d84e40e09faa7f80b7e6bc27b2e5f1e39f12c03361be2b144f4339` |

途中の保存結果は追記前の本文hashを参照していたため、その時点では最新本文への結果と扱わなかった。上記の最終保存結果でsnapshot対応を確認した。hash一致は結果と対象ファイルの対応を示すもので、scriptがRFCの全条項を検証することを意味しない。

抽象モデルは、合成vectorの受付・混合資源、有限のtoy graphの割当、actor別予約、stock保持、lease寿命、原子的rollback、整数difficultyとbootstrap、共有permit、一回性、反証優先、費用保存、snapshot復元などを対象とする。`EvidenceRejected`が実行費を使っても自動的にQを終結させない抽象化も含む。

次は入力・仮定または明示的な対象外である。

- 共通checkerのqualification、完全な合法継続graph、native費用導出、認証・canonical順序は外部入力。
- ClockBridgeから保守的なrelease/deadlineが得られることを仮定。実伝播、schedule認証、clock同期、実包含は未検証。
- Panelのcold取得・全再実行・provider容量と、それぞれの実予約は未検証。nativeな三分類と完全なQ lifecycleも対象外。
- 合成Finalの一unit投影は既存経済契約の全面検証ではなく、一claim一権利への変更でもない。
- テスト用JSON identityとdeep copyはwire/暗号の同値性やスケールする実装ではない。
- native/Rust/Studio/RPC接続、実LLM、最低機器のRSS/CPU/I/O、登録一万/五万件での実性能、持続1 BPS、非同期allocation、IBD/GC/WALの全経路、正式な安全性・公平性・活性は示していない。

レビュー担当はこのテストを再実行していない。保存済み結果とその適用範囲の確認を、native実装や実機の合格へ読み替えない。

## 5. 残る実装・測定・公開有効化ゲート

以下は本文で要求される条件の実証が残るという意味であり、未知modelごとのbenchmarkや人手審査を復活させる要求ではない。

| ID | 必要な証拠 | 現在の扱い |
| --- | --- | --- |
| G01 | 全対応kernel/codecの分解が整数意味、Init/Step/End、read-from・状態・停止を保存し、不正記録から有界な裁定可能箇所を得られること。 | nativeな完全性・soundnessは本レビュー未証明。 |
| G02 | descriptor/compiler/meterの上界が最大合法shape、悪意入力、planner、失敗/undoでも実費を覆うこと。 | 実装照合と境界検証が必要。自己申告costは不可。 |
| G03 | 最低機器でchecker、認証・通常取引、DB/index/root/journal、cold I/O、最大state、reorg/restart/IBD/compactionが共存する供給点とsafe margin。 | checker microbenchmarkやp99のみでは合格にしない。CPU/RAM等の最低値をこの記録から確定しない。 |
| G04 | preflight前の無制限allocationがなく、buffer/worker/I/O/clone/error/undoまでleaseが一回だけ所有・返却されること。allocation失敗も有界に処理されること。 | 実ランタイム・非同期処理・障害試験が必要。OS/機材の任意障害まで無停止とは保証しない。 |
| G05 | cold Panelの資料取得・全再実行・局所化・opening、providerの実供給と物理資源の競合を含め、最後の適格正直者の全手順が元期限内に収まること。 | 役割別サービス上界・供給同意・実ネットワーク条件が必要。Node checkerの軽さで代用しない。 |
| G06 | canonical clock/close frontier、ClockBridge、beacon停止時継続、fallback、L内の検証・伝播・包含が同時に成立すること。 | 本文の必要条件を実profileで満たす証拠は未確認。合計120秒の余裕だけでは不十分。 |
| G07 | Attempt/FreePrompt/TFO3/atomic batch/quote/Studio/RPCの全入口が同じnative admission・資本・work一回性・実資金・bookへ接続されること。 | route迂回、siblings、古いquote、bootstrapを含む実装試験が必要。 |
| G08 | nativeのQ/終結分類、同batch反証優先、source取消、credit/生成権の発行・予約・消費、expiry、費用保存、支払一回性が連動すること。 | 抽象状態の試験を実権利・実資金の接続完了とは扱わない。 |
| G09 | crash/reorg/restart/IBDでbook・資金・時計・一回性・lease・保持を復元し、physical GC完了までdiskを解放扱いにしないこと。旧義務を削らない移行が可能なこと。 | native journal・snapshot・storage・移行試験が必要。 |
| G10 | 同じqualification済み規格で未知modelを個別測定なしに受け付け、実LLMで正常Final、反証、精算、権利消費、carrier受理まで通ること。最大pending/retention/GCと未完了分母を含む持続負荷も測ること。 | 全拒否・全Abortだけの成功では完了にしない。実用的な成立率・待ち時間・持続1 BPSは未実証。 |

## 6. レビューの結論

最新本文は、**新規提出を全体の空き資源に合わせて絞り、既受理claimの救済・終結・保持を先に確保する**設計として整合している。動的件数、全工程の期限予約、Node内部のmemory/queue/work上限、反証優先、残責任を保つ解放が別々の条件として定義された。

これにより「1 BPSだから安全」「courtを三件にすれば安全」「Nodeで一箇所通ればFinal」「unknown modelのたびに測定が必要」という解釈は採らない。有限な共通profileで表せない、資源・原資・期限を予約できない、または実装/測定ゲートが未達の経路は、新規受付・公開有効化の根拠にしない。

本記録は文書設計の独立レビュー完了を示す。公開activationの承認、native実装の完了、測定済み最低スペック、PALW全体の無条件成立宣言ではない。後続作業は[実装計画](../../../design/palw/integration/unified-admission-implementation-plan-2026-11-17.md)と[ADR-0223](../../../adr/0223-global-admission-court-calendar-and-one-bps.md)へ接続する。
