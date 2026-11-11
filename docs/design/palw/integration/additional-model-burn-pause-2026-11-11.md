# 追加モデルのburn停止・解除 — 実装追補

採択: [RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../../../adr/0221-additional-model-burn-pause.md)。**ADOPTED_DESIGN_NOT_ACTIVATED**。

元の[2026-11-10計画](public-one-honest-vertical-implementation-plan-2026-11-10.md)は原文保持する。[HF取得追補](hugging-face-cold-panel-2026-11-11.md)の独立登録検査・定期可用性認定・有効期限・HF取得不能裁定を実装残件から削除し、本書のburn停止へ置き換える。

独自モデル重み全量配信は廃止候補として依存を棚卸しする。モデル用の消失訂正・分散再配布・Node可用性サンプリングは必須経路から除外する。モデルroot/hash照合、claim固有入力/trace/Merkle opening、Nodeの局所courtは維持する。共用HTTP providerや局所証拠用のモデルopeningを一括削除しない。

## 最小実装と境界

`consensus/core/src/palw_model_pause_v1.rs`の純粋なplannerで、受理済みclass・artifact root・正規登録者、明示genesis/annual指定、期待旧state/sequenceを解決する。Pauseは誰でも、Resumeは登録者の認証を必要とし、別々の正の有限burn条件を導出する。返り値は未署名候補であり、native受理・実支払・状態書込みの権限ではない。

登録者欄がないことだけをgenesis免除の根拠にしない。実genesis登録と明示指定に基づいて除外する。追加モデルが年次指定へ入った時点で旧pause rowは無効となり、対象外化による永久停止を防ぐ。旧Frozen制裁はこのResumeで解除しない。

対象テスト2件が成功した。追加モデルのPause/Resume、異なる必要認証とburn、古いsequence/no-op/overflow/別scope/root、登録前DAA、登録者不明、genesis exact root、genesis catalogと年次指定境界を確認した。これは未署名plannerの試験であり、実署名の拒否や支払・native state保存を試験したものではない。

## 続く接続

1. wire/purposeと署名対象（直前canonical操作IDのpinを含む）、実carrier feeからの一回burn、認証state保存、journal/reorg・snapshot/IBD、資源料金を原子的に接続する。現plannerはscope/state/sequenceの段階であり、署名とbranch認証は未完成。
2. quote/rehearsal/実受付、全成功Final writer、rewardと生成権のconsumerへ同一gateを接続する。旧ModelLineなど別のidentity体系を対象へ含める場合は、共有classのowner/tierを流用せずnative登録者と固定rootを解決する。
3. canonical prefix順を維持する。現行pre-object deadline Finalは同blockの後続Pauseより先行する。post-object kernel tick等はPause後の状態を見る。PAUSED時にdeadline sweepが単にskipしてclaimを索引から落とすことを防ぐ。
4. 原期限と責任を保持して失敗/未検証終結を処理し、反証・供給応答を継続する。停止は計算不正の有罪根拠ではない。
5. 停止/解除burn額、処理上限、全reward境界、公開移行を確定してから有効化する。現時点で公開presetや残高を変更しない。

新TFP1のFpTokenizer/TKR1・共通planner・資料供給・選出Panel承認・公開quote以降も未完であり、burn plannerの実装をそれらの成功証跡にしない。
