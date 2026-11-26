# MOE3/BMR1最終設計の検証記録

2026-11-26（Asia/Tokyo）。対象は最終50/20/20/5/5、全モデル統一、小数POSITION、用途別モデル廃止、Court49/51。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 1. 報酬参照

[check_moe3.py](check_moe3.py) で固定小数、F/P各5%・A/J各20%・D残差50%、一回購入B=D+J、LPのfloor(qJ/B)、各保存則、MISAKA役務予約、配分端数、overflow、quote、抽象原子性・undo・一回移行を検査する。

**26件の参照unit testは全件PASS（失敗0、error0）**。最新結果は [math-check.json](math-check.json)、式と限界は [MATH.md](MATH.md)。全探索7,920組、seed固定random600組、複数モデルの結合按分、先行P/F予約が市場dustで消えないことを含む。

## 2. モデル廃止・Court参照

[check_retirement.py](check_retirement.py) で、用途lotの分離、兼任6000/4000、全Panel/Gate C保護、他model非干渉、資金範囲重複、用途変更、current reserve burn、未発行取消、未確定POSITION retire、既確定報酬保護、原子性・undoを検査する。

Courtは累積実回収Sの49%、ΔQ専用escrow、別の受取plan資格解決、期限burnを検査する。1 atomを100回回収した場合と100 atoms一回の49/51一致、不正な受取人でも回収自体は維持、被告の受給拒否、collection再送、scope・上限・plan変更、BFTとCourtの順序別一回消費を含む。

**20件の参照unit testは全件PASS（失敗0、error0）**。結果は [retirement-check.json](retirement-check.json)、式と限界は [RETIREMENT_MATH.md](RETIREMENT_MATH.md)。真の有罪・quorum・署名・実UTXOは入力済みであるという抽象前提であり、これらを本コードで検証してはいない。

## 3. 独立設計レビュー

次の指摘を最終正本へ反映した。

- 最後の50/20/20/5/5を全て同じCへ一度だけ適用する。古い80%案、BFT10%、検証15%を現行規範へ残さない。
- LPはfloor(qJ/B)。端数込みの実購入予算比を使い、q/4又は常時2q/7にしない。
- 複数モデルはJ/Aを結合し、D_i=2J_i+E_i。ゼロ配分modelは除算前にno-opとする。
- 購入原資の50%は換金額の保証ではない。二因子積u256・多因子比較u512等を要求する。
- 正当な検証報酬とBFT役務資金を市場原資から分離し、モデル廃止・dustで消さない。
- BFTは用途で検証bondを保護し、検証者の善悪を判別しない。個人の責任はCourtへ残す。
- BFT100%burnとCourt49/51は別原資。履歴LP累計・virtual reserve・未発行取消を実回収MISAKAへ混ぜない。
- Court受給者不在でもslashを免除しない。49%を有限専用escrowへ置き、増分だけを配る。未受給分期限burnをinitial51%burnと区別する。
- 旧確定PAUSEDの保存と旧未決操作のdrainを分け、登録者不在の共通BurnResume経路を設ける。

## 4. 文書と元資料

入力ZIPの編集前から文書checkerはFAILであり、欠落doc参照1,384件、見出し欠落2件、組込baseline既知43件、新規扱い1,343件があった。元ZIPの欠落であり、今回の改訂により生じたものではない。

最終検査では元の10,719ファイルを全て保持し、既存65ファイルの変更前bytesを65個のbefore snapshotとSHA-256照合した。Rust・旧policy・過去evidence等の保護対象6,563ファイルは全て入力と一致する。文書failureの入力比は追加0・除去0で、入力と同一の問題集合だった。詳細は [総合検証JSON](revision-validation.json) に記録する。全リンク正常とは主張せず、既存baselineを変更して問題を隠さない。

[入力時文書検査](docs-check-input.json)／[最終文書検査](docs-check-final.json)／[既存変更とSHA-256](changed-existing-files.json)。全既存ファイルを保持し、編集した既存文書はbefore/に入力bytesを保存する。Rust、旧policy、過去evidence/audit/revision、既存baselineのbytesを保全する。

**文書工具の16件unit testも全件PASS**。参照46件と合わせて62件で失敗・errorは0。索引はRFC0068/0069、ADR0236/0237、最新のモデル・報酬・制裁の優先関係へ更新する。

## 5. 検査対象外

新Rust/native、実BFT投票・署名、Courtの正否、実UTXO回収、役務の実資格・資金、価格bps、実DAG reorg/永続化/IBD、RPC/ABI/wallet/Web、公開有効化は未実施。参照のundoは実chain reorg試験ではない。

旧allowanceの認証持越しや全stateの移行を、参照の一回scaleだけで完成したとしない。実質所有者/Sybil、将来価格、換金額、MEV、全状態の活性又は共謀不採算性も保証していない。
