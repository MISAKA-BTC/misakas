# A1: wide Q24 native registration source handoff

2026-11-02。Base `cc22b0da13784eb03a023cbd1781969577883d9e`、branch
`codex/a1-wide-native-registration`。RFC-0043 / ADR-0211 の既存 tag61 登録接続。
公開 activation、新 Assignment/r1 Result、モデル単位の審査は範囲外。

選択順は Core `4bab4b9d2` → SDK/native `fb204eb8a471e2fb483445afb3b946d238491432`。
19 source files の SHA-256 と相互レビューは
[review.json](validation/wide-native-registration-source/review.json) に記録した。
rustfmt（既存 module 順を維持）と diff check は成功。docs check は既知43件・新規0件。

**source 実装と静的レビューの記録であり、native 受理成功の実測記録ではない。**
A0 の HOLD に従い、この作業では Cargo、rustc、Clippy、build、test、target/cache の参照、
大規模モデル実行を行っていない。Root の実行結果が揃うまでは private native registration
の runtime 検証も未完了。過去の pure admission 10件の Root 実行結果とは別の変更である。

## 選択と旧互換性

新 `palw_tir_registration_dispatch_v1` は trusted Params の fence と DAA を受ける。
既存 program の最終64 bytesが exact wide scheme と異なる入力は追加解析せず旧 v10 / factory
へ渡す。これは deny/route 用の hint であり、program を解析・認証した結果ではない。
既知 wide 候補だけ strict canonical decode、最終 I64、non-held、active court2、primset、
program cap を確認し、既存 pure V2 の full range/cost gate を選ぶ。
unknown/malformed narrow の旧 preflight error と順序を比較する回帰 source を含む。

`PalwStateParamsV2` の full fence mirror は private `#[borsh(skip)]` field。
court1/None は full mirror を保持せず、旧 height setter は wide mirror を消す。
Params sync は高さと full fence を一度に設定し、validation は activation、primset、court、
全 ceilings の一致を要求する。Borsh 復元直後の mirror は None なので trusted top-level Params
から再 sync して validation を行う。既存 Params fingerprints はもともと full fence と court root
を含むため、新 marker/domain を追加しない。公開 preset は変更しない。

SDK は narrow の元 canonical context / constructor を保持し、wide だけ fallible V2 context /
constructor を使う。native processor は従来の target → Active bond / ML-DSA 署名 → admission →
share の順を保ち、wide 時だけ bundle と実 transition params の両 full mirror を照合する。
rehearsal と本 fold は同じ state params で selected metadata factory を使う。
fold は full range/court sizing を再実行せず、元 class/artifact から schema1 row を導出し、
既存 single writer と common registration へ渡す。program/context/commit/ID を改変しない。

## Consumer の境界

| Consumer | この差分の状態 |
| --- | --- |
| SDK 候補 / 署名 object | trusted fence による v10 / V2 選択。tag61/message/context は既存のまま |
| native acceptance | selected admission と実 transition mirror の照合を接続 |
| Params / runtime selector | full court2 mirror を sync / validate。公開 preset 不変 |
| rehearsal / actual fold | 同じ selected factory。既存1件/block、burn、exposure、journal を使用 |
| 永続 row / identity / restore | schema1 / rooted bytes 不変。mirror は trusted Params から復元 |
| layout 不要 metadata view | 既存明示 row-profile を維持。canonical job / layout 認証の代用にしない |
| legacy Attempt / REAL work / prefix | 未対応 wide を書き込み前に拒否。raw facts から旧 job を作らない |
| Positive metadata | 既存 full-layout join の明示 wide reader を維持。native Result 受理とは扱わない |
| DA / staged / public / funded source | 既知 wide を旧 tiled/flat 判定より前に拒否。funded は Missing(adapter) |
| decode / sampling / output | 既存 V1 wide 拒否を維持。wide 出力 attribution の native 完成とは扱わない |
| fraud court / FP / original binding | FP class / coverage を明示拒否。既存 V1 court の拒否を維持 |
| registration work / witness / Mesh | 登録 metadata 副作用を維持。legacy wide claim を audit_mesh off/on 共通で早期拒否 |
| 新 Result / role / funding / P / credit | 未採択。既存7 credit consumers の拒否を維持 |

deny-only row helper は既存 program の final scheme bytes だけを見る。raw `facts.tiled=false` と
旧 `logits_tiles()=1` を新しい family bit に読み替えない。4097 logits の wide row が本来2 tile
である回帰 source を含む。empty/unknown/synthetic row は従来の reader 検査へ渡す。
metadata view は元 layout の最大 context / geometry / job を認証しない。これらには既存の
full-layout join が必要であり、この一覧は repository 全体の網羅性の証明ではない。

## 試験 source と実行待ち

* Core dispatch: narrow4 dtype、inactive / court1 / unknown court、旧 error 順、Borsh skip、
  restore / sync、full fence substitution。
* Core reader: wide4097 の旧 raw facts を維持した拒否、FP class / coverage、unknown/synthetic。
* Core legacy claim: audit_mesh off/on、直接 apply の state root / exposure 無変更、own 拒否と
  merged skip。rider batch も最初の lead 書き込み前に全 class を preflight する。
* SDK: 実 tiny PALWTIR artifact / inventory root を用いた tag61/message、旧 narrow bytes parity、
  inactive / None / unknown / wrong canonical の拒否。
* Node: 実 ML-DSA bond 署名、通常 funding script / UTXO、mempool → mined carrier → accepted
  transaction DB → class/tir row/burn/exposure、replay、reorg rollback/reapply、sameDB restart。
* Node: 無効登録署名 / wrong signer / target と honest wide under court1 でも funding carrier は通常受理。
  独立した二つの funded carriers の両 UTXO を受理し、実 transaction 順の先頭登録だけを通す1/block cap。
* Node origin: 既存の隔離 genesis 例外を明示して full Params validation を呼び、実受理 carrier と
  aux141 の class / accepting block / DAA / Tir door / admission digest を照合。sameDB 復元でも一致確認。
  公開 T12 genesis / DAA18400 の arming-record 検証成功とは扱わない。

これらはすべて未実行 source。helper への object 直接投入は carrier UTXO inclusion の実績と
区別する。実 test binary / Params identity / raw log は Root の検証後に別途記録する。
source manifest と最終相互レビューの範囲は `validation/wide-native-registration-source/` を参照する。

## FP consumer の追補

最初の source freeze 後、統合担当の追加レビューで普通の FreePrompt は derived-work /
original-frame fence が無効だと既存 FP helper の拒否を通らない組合せが見つかった。
`eval=None` の class / Frozen 確認直後に、両 fence に依存しない known-wide 拒否を追加した。
off/off の旧経路では同じ fixture が claim を作る対照を置き、off/on matrix では新 guard と
既存の earlier G03 拒否を区別する。いずれも root / exposure / claim を変更しない回帰 source。

`eval=Some` は普通 FP より前に別分枝へ入る。既存の評価機能は延期・無効の条件を保ち、
その明示的 eval fixture が有効な場合だけ、既存 layout / Frozen / lifecycle 拒否の後、row
clone / 旧 GEN context / claim 書き込み前に known-wide を拒否する。旧 geometry / leaf count
だけでは wide を拒否しないこと、dormant 時の旧最初のエラーを維持する回帰 source を含む。
GEN の幅・court が未対応であるための拒否であり、評価機能を有効化する変更ではない。
明示的 full-layout wide SourceChecker は変更しない。詳細は
[fp-followup.json](validation/wide-native-registration-source/fp-followup.json)。追補も未実行。

## 7段階 plan の到達点

1. RFC43 / ADR211 と fixed base / 完了語彙を pin: source 記録済み。
2. 共通 dispatch / full mirror / narrow 互換性: source 接続、runtime 未確認。
3. SDK / native / rehearsal / fold / common registration: source 接続、runtime 未確認。
4. 上表 consumer の接続または拒否: source audit、相互レビュー。実行検証待ち。
5. private typed Params: validation を呼ぶ fixture source。validation PASS は未確認。
6. 実署名・原資・受理・rollback/restart: fixture source。binary と raw evidence 待ち。
7. 学習済み複数 HF artifact / 元最大文脈長 / 同一 Root binary の再現: 未実施。

モデルや program/context を短縮・clip・scale して成功に置き換えない。実 HF 最大位置の実行、
全 native claim/court reader、Panel/Node の資源実測、公開 activation は完了していない。
登録モデルの0.3固定、年次指定1.0、claim ごとの認証と経済条件は変更しない。
