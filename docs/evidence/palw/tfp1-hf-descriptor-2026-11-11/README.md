# 通常TFP1のHF参照資料と登録rootの接続

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前のgoalターンは状況説明のみで、実装進捗はなかった。今回は[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)に従い、新ordinary profileのArtifact資料をモデル重み全量から固定HF参照descriptorへ分離するコードを追加した。Studioの同一claim受付・Final・当選時権利消費・対応blockは **NOT_PROVEN**。

## 接続したコード

- [descriptor](../../../../misaka/palw/positive-contract/src/public_one_honest/hf_artifact.rs): 独立codec `tir-hf-artifact-reference/v1/whole`、最大732 bytes、固定repository/full commit、source manifest・conversion recipeのdigestと6つのnative識別子をcanonical bodyへ束縛する。未認証長によるheap確保をしない。Artifact subjectはinventory、content rootはdescriptor bytesであり、旧inventory codecの意味を変更しない。
- [native resolver](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_hf_artifact.rs): 呼出元が認証した親stateのclass・program・tokenizer・inventory・container・binding manifestと照合する。モデル参照をunsigned manifestへ組み込む際のbyte計上はdescriptorのみ。別layout、別claim/sourceをreplayへ混入できないよう照合する。resolverそのものは受付権限でもHF取得の証拠でもない。
- [通常replayと参照publisher](../../../../misaka/palw/remote/src/positive_panel_sources/legacy_tfp1_replay.rs): 新codecならnative resolverの結果を必須にし、署名されたmanifest entryと完全一致させる。取得前の合計byte予算へdescriptorを加算し、そのcanonical bytesだけを `hf-artifact` 経路で取得する。publisherも同じpairingを通し、I/O・保持予約callbackより後にencoding/writeする。trace publisherは共通pairingを利用する。
- [transport](../../../../misaka/palw/remote/src/public_material.rs): 新pathを許可する。一般body取得側もArtifactを新codec・最大732 bytesに限定する。既存wide profileのfull artifact取得関数へfallbackしない。

backend側の重み変更検出、inventory再計算、program/tokenizer、prompt、cold state、依存、output、全traceの照合を維持する。モデル全量配信をMinerへ戻さず、局所court用の限定的openingは廃止しない。新たなHF可用性認定・取得証明・多数決・有効期限を追加しない。

## 検証と残件

canonical wireの不正長・改変、native root差替え、binding欠落、別layout/claim、署名manifestへのrecipe束縛、remote拒否境界を回帰コードに記述する。Rustのbuild/testは[採択済み作業順](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い通し接続後に実行する。整形・差分・文書検査の結果は `validation.json` に保存する。

これは**model取得・変換runnerを接続し終えた状態ではない**。source manifest/recipeの実preimage、HF file hash、変換依存、PALWART3 container/bindingへの再構成を同じ参照に照合し、daemonのbackend missからこのconsumerへ渡す必要がある。[過去の固定HF取得と同一Studio候補のlocal再計算](../hf-cold-model-replay-2026-11-11/README.md)はその部分的証拠であり、今回のコード実行や遠隔選出Panel承認の証拠ではない。

さらに、完全profileのmanifest規則・資源/費用/期限、G14 quote、共同予約と原子的intake、node候補filter、PMR dispatcher以後を接続する必要がある。descriptorのentryを作れることを、nativeが新profileを受理すると読み替えない。PublicOriginalProfile拒否を外さず、配備・署名送信・公開probeは行っていない。
