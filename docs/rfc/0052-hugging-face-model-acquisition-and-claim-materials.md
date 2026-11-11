# RFC-0052 — Hugging Face 固定モデル取得と claim 固有資料の分離

**後続決定が優先:** [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)により、以下の独立取得検査・定期可用性record・期限・HF取得不能の裁定をburn停止・解除へ置換する。本文は採択時の履歴として残す。HF取得とclaim資料の分離は継続するが、取得可否認定の実装を続けない。

Status: **Accepted design / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-11
Decision: [ADR-0220](../adr/0220-hugging-face-model-acquisition-and-claim-materials.md)
Current owners: [RFC03](../rfc-v2/0003-model-onboarding-and-artifacts.md)、[RFC16](../rfc-v2/0016-miner-execution-commit-and-material-supply.md)、[RFC17](../rfc-v2/0017-noninteractive-positive-verification.md)、[RFC18](../rfc-v2/0018-interactive-negative-court-and-defaults.md)、[RFC20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)。

## 1. 明示指示と修正範囲

2026-11-11、ユーザーは「この方針で進める RFCにあるhugging faceから取得証明する方式に変えて進めて またRFcの修正も行なって」と指示した。本書とADRはその明示的採択を記録する。設計変更のために同じ採択を再要求しない。実装・検証・公開有効化の完了とは分ける。

既存の[ADR-0067 Decision 4](../adr/0067-classes-are-chain-data-kernels-are-the-build.md)は、モデルの内容をclass/artifact rootで固定し、取得元をHugging Face・mirror等とする。RFC03には固定revisionの取得と再現可能な変換がある。一方、RFC-0050/RFC16の全資料供給を「モデル重みまでMISAKA独自providerで全量再配布する必要がある」と解釈すると、既存の取得機構を重複実装する。今回、その必須経路を明示的に分離する。これは停止原因を再現したという主張ではなく、ユーザーが指示した供給責務・実装優先順位の限定改定である。

**新HF profileではHugging Faceをモデル重み・固定設定・tokenizerの唯一の必須公開元とし、匿名取得できる固定版を登録済み内容へ照合する。MISAKA独自のモデル重み全量配信は正常系の必須条件にしない。claim固有の入力・初期依存・実行trace・openingと局所反証の供給、資料不足による成功停止は維持する。**

## 2. 「取得証明」の意味

本書の取得証明は、対象ファイルを実取得し、固定サイズ・ファイルhash、必要な再現変換後のprogram/class/artifact/tokenizer/container identityを照合した記録をいう。単なるURL、Hubの存在表示、HEAD、ローカルcacheの存在、HTTP 200、他者の署名だけでは取得成功としない。

ファイルhashはデータ同一性を認証する。署名は記録者と対象を認証する。どちらもLLM計算の正しさ、署名者が物理的にdownloadした行為、将来の配信継続、他の選出者の取得成功を証明しない。Nodeは外部HTTPを合意入力として実行せず、モデル全量を取得しない。完全再実行承認と局所裁定は従来どおりclaim単位で行う。

## 3. 固定する取得情報

取得recipeには少なくとも次を明記する。

| 情報 | 必須条件 |
| --- | --- |
| repository / revision | repo IDと完全なimmutable commit hash。branch/tag/latestを実行時に再解決しない |
| required files | path、exact bytes、hash algorithmとdigest。weights全shard、index、必要config/tokenizerを列挙 |
| native pins | 対象class ID、artifact inventory root、tokenizer ID、program graph root。PALWART3を使う場合はcontainer rootも別に照合 |
| conversion recipe | 変換器版、量子化・丸め・layout・shape・calibration入力・全依存の固定情報。upstream float checkpointとnative実行artifactを混同しない |
| resource limits | download総量、各file、disk、RAM、変換work、timeout/retry、取得後の保持と検証期限 |

取得先を選ぶoff-chain recipeやroot-fetch hookは権限ではない。候補recipeの自己申告rootを期待値にせず、元claimと登録済みclass/bindingのrootを最終期待値にする。既存class登録wireへURLを追加したり、旧rootの意味を変更したりしない。recipe、取得ログ、変換結果には同じclaim/class/artifactとdescriptor digestを記録する。設定や版を修正して別rootになった場合、元claimが成功したと表示しない。

Hugging Faceの[snapshot download](https://huggingface.co/docs/huggingface_hub/guides/download)はrevision指定の取得に使える。実装済みの固定revision fetcherも同じ条件を満たせば再利用する。tool名だけで条件達成としない。

### 3.1 HF公開契約の登録・定期検査・期限

ユーザーの追加指示により、新HF profileのモデル公開契約は必須とする。モデル品質・階層の審査とは別の契約であり、モデルclassの無審査登録と1.0/0.3は維持する。HF公開契約が未成立・期限切れでもclass登録の取消やモデルの降格はしない。

これはPRINCIPLES §9の「公開状況・取得可能性をチェーンが審査しない」という旧一般規則に対する、ユーザーの後続明示指示による限定改定である。公開契約のcanonical確認を単なる名称変更で旧規則に適合すると説明しない。改定範囲はこのHF契約だけとし、モデル品質審査・階層変更へ広げない。

版付き`HFModelReference`の固定identityは`repo_id`、`full_commit_sha`、`required_file_manifest_root`、`weights_root`、`tokenizer_root`、`execution_profile_id`と必要なconversion pinsに加え、network/chain scope、class ID、native inventory root、必要なPALWART3 container rootを束縛する。claimはこのimmutable reference IDをpinする。更新可能な`availability_valid_until`はこのidentity digestに含めず、別の版付き可用性recordに置く。初回・更新recordの署名はreference ID、canonicalな直前record ID（初回は初回専用marker）、検査policy、観測点、`availability_valid_until`を束縛する。読取APIで参照と期限を一緒に表示する場合も、固定identityと更新recordを区別する。

nativeに受理する公開契約は、原登録者と独立した検査者が匿名で全必須ファイルを実取得し、exact size/hashとnative rootsを照合した署名付き記録を必要とする。HF private/gated判定のAPI観測も固定revision・観測点へ束縛するが、API応答や検査者の宣言だけを外部世界の暗号学的真実としない。正規HF hostの同じrepo/full SHAについて`private=false`かつ`gated=false`を明示的に確認し、unknown/欠損を成功記録にしない。登録・更新署名はさらに専用purpose、canonical accepted-parent block/state root、観測点と期限のclockを束縛する。

登録検査者の資格・独立性・署名対象・費用・選び方と信頼前提を、claim Panelの抽選前提とは別の版付きpolicyに明示する。記録を作る検査者の少なくとも一人が、その観測時点で正直に全量照合したという前提を必要とする。claimごとの「正直な一人」をこの別集合へ自動的に流用しない。BFT、validator集合、DA委員会を新設しない。未定の検査者policyや数値を既存quorumの流用で埋めず、実装・公開有効化の残件として記録する。

定期可用性検査と更新記録を必須にし、頻度・有効期間・取得費用・異議受付を有限policyで定める。軽いAPI/HEAD確認は全量検査記録を更新しない。検査者の署名と期限はNodeがcanonical recordとして検査し、Node自身がHFへ照会して判定を分岐させない。更新は同じ固定file/native rootsを対象にし、変化は別referenceとして扱う。過去の完了記録を永久有効にしない。reorgで孤立した登録・更新記録は使用せず、元claimがpinした同一referenceのcanonicalな後継だけを参照する。

受理時にはprofileが要求する検証期間を覆う有効期限を確認し、Final時もそのclaimに適用される有効な公開契約・検査記録と未解決caseを確認する。途中の取得失敗は期限内でもcaseを作れる。更新が原claimのnotice/hardを延長したり、未解決の取得異議を消したりしない。mirror/cacheは任意の認証済み高速化・回復手段であって、HF公開条件の失効を隠す代替登録にはしない。

## 4. 取得と検証の分担

| 資料 | 正常系の供給元と検証 |
| --- | --- |
| モデル重み・構造・設定・tokenizer | Hugging Face固定版、hash照合、必要な再現変換、native root照合。認証済みcache/mirrorは任意の最適化 |
| program・実行数値規則 | 固定recipeと登録classに一致する形式を取得・導出し、claimに束縛 |
| prompt・canonical token列・seed・初期KV等のclaim固有依存 | Minerの署名固定資料。公開検証に必要な範囲をMISAKA資料経路で供給 |
| Miner初回trace・中間値・Merkle opening | Minerの固定rootに対するclaim固有供給。HFのモデルファイルから供給済みとは推定しない |
| 検証承認・反証 | 独立選出Panelまたは適格な第三者が作成し、既存資格・費用・期限でMISAKAへ提出 |

未保有の選出Panelは、元claimのnative pinsを解決し、有限予算の外部取得jobで固定版を取得して全hashを照合する。変換が必要なら固定recipeで再構築し、native class/artifact/tokenizerを照合してからbackendをロードする。次に同じclaim固有資料を認証し、全体再実行・全必要関係・trace/output照合へ進む。既存のproducer slot loopやNodeの合意fold内へdownload/全モデル実行を入れない。

抽選母集団をモデル保持者やHF取得成功者へ狭めず、失敗後の再抽選をしない。取得ログだけでPanel承認を作らず、一人の完全承認、必須局所検査、未解決Q、原期限その他のFinal gateを維持する。

## 5. 取得不能・期限・責任

HFは可用性の無条件保証ではない。publicかつungatedで匿名取得可能なことを新profileの必須条件とし、独立登録検査と定期検査の記録を設ける。[gated model](https://huggingface.co/docs/hub/models-gated)では利用者ごとの権限が必要になる。削除、通信障害、rate limit、権限不足、hash不一致、変換不一致、予算不足をそれぞれ記録する。特別な許可を持つPanelだけへ選出を変更しない。

必要資料を得られない適格者は、元claim/manifest/native rootへ結び付いた資料不足caseを開始できる。通常の取得元がHFであることを理由に受付を拒否しない。caseは成功を保留し、元notice/hard内で認証された回復・取得後検査へ進む。元期限を更新せず、回復できなければ未検証Abortとする。私的なHTTP失敗、時間経過、他Panelの成功だけでMiner有罪や正直な申立人のdefaultを決めない。

HF障害だけからMinerに独自の全重み再配布サービスを必須化しない。任意のcache/mirrorによる回復は許すが同一rootと予算・期限を守る。個別に引き受けた供給義務の違反、算術不正、外部取得障害は分ける。悪意の虚偽未着caseを有限に終結するnative手順は依然必要であり、HF利用や一件のreceiptで解決済みとしない。

帯域、全fileのhash、変換、cold load、全再実行、claim資料、反証と包含の費用は残る。独自providerの転送量を0にしてHF取得費用まで0と扱わない。HF経由の正常完了は期限内の外部取得・変換・検証・包含が成立する条件付きである。

## 6. RFC-0051とG14の修復

RFC-0051はモデル重み再配布の規約ではなく、受付前tokenizer rawの有限な有料native登録契約である。HFで同じtokenizer bytesを取得できても、native rendering/EOG検査と受理済みregistration rowを作ったことにはならない。

したがって`G14_ROUTE_ABSENT(FpTokenizer)`の解消は別の必須工程として維持する。既存のOpen/Part/Finish、署名、原資、予約、native全検査、登録rowの先行受理を利用する。HF取得を理由にrowを裸で挿入したり、ローカル検査をnative成功へ昇格させない。旧EOF certificate、TKR1、旧署名・wire・費用・GCの意味は変更しない。

## 7. 実装と確認

1. 最初の未成立条件であるFpTokenizer routeとG14資源・費用・期限を修復する。対象claimの実native pinsと固定HF source manifest・conversion recipeも照合する。巨大なモデル内部DAの完成をこの修復の前提にしない。
2. HF公開契約の登録時独立全量検査、定期検査・期限・canonical recordを受付に接続し、モデル未保有のPanelでbackend missから既存root-fetch/固定取得/変換へ接続する。重みの独自provider公開を前提にしない。
3. 独立cache・初期artifactなしの実取得、全file hash、同じnative root、claim固有資料取得、全体検証を一つの記録で追う。既存cacheの再利用をcold取得と表示しない。
4. pinned revision不一致、欠損shard、file/hash/変換root不一致、認可失敗、容量・期限超過を試験し、承認や成功Finalへ進まないことを確認する。
5. tokenizer routeのnative登録、同じ期限内候補のquote/受付、選出Panel検証、Finalと権利使用を公開T12で別途確認する。

既存方式の読み取り結果、実装、ローカル試験、HFからの実取得、公開T12受理を別々に記録する。モデル二階層1.0/0.3、無審査登録、計算暗号proof禁止、旧claim、旧75/41と一人の反証優先を変更しない。公開profileの数値とactivationを本書だけで設定しない。

## 8. 成功条件と明示的な非要件

新HF profileの成功Finalは少なくとも次の全条件を必要とする。

```text
SuccessFinal => HFPublicAndUngated
             && ManifestAuthenticated
             && AvailabilityProcedureSatisfied
             && IndependentReplayApproved
             && NoUnresolvedCase
             && ExistingNativeLocalChecksAndFundingAndDeadlines
```

HFPublicAndUngatedはcanonicalな登録・定期検査・期限と当該claimの適格異議の処理に基づく。Nodeが現在のHTTPを呼び出す述語ではない。AvailabilityProcedureSatisfiedは本人の取得後検査機会を含み、他人のdownload記録だけから推定しない。AV-01〜03に相当する取得・選択的秘匿・虚偽未着の終結は、HF化だけで解決済みとしない。

初期HF profileでは、モデル重みを対象とするMISAKA独自全量配布、消失訂正符号化・分散配布、Nodeによるモデル資料サンプリング取得を必須経路から外す。claim固有traceは巨大になり得るので、その資料供給・保持・局所opening・費用上限は残す。旧profileの義務や既受理claimへ遡及適用しない。
