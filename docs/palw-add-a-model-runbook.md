# 既存モデルを `.palwart` に変換してチェーンに追加する手順

> 対象: 手元に Qwen 系の checkpoint（HF safetensors か GGUF）があり、それを MISAKA の PALW
> クラスとして **変換 → 検証 → 登録 → 認証 → 着席** まで持っていきたい運用者。
> 2026-09-23 時点の `feat/testnet-12-regenesis` の build で実在するコマンドだけで書いてある。
> 開発者向けの下層（SDK の trait、lineage の追加）は [palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md)、
> 認証オブジェクトの詳細は [palw-certify-a-new-model.md](palw-certify-a-new-model.md)。

## 0. 先に知っておくこと（ここを読めば残りは作業）

1. **チェーンに載る「モデル」は class であり、class はこの build の catalog の 1 行である。**
   class id は **graph（形状 profile）の hash** であって、重みファイルの hash ではない。同じ
   checkpoint でも context 幅が違えば別の行・別の class（`…/graph-v7@512` と `…/graph-v7@2097152`
   は別物）。catalog は `palw-class ledger --network <net>` で見られる（§2）。
2. **対応している系統は 2 つ**（それぞれ変換ツールが違う）:
   | 系統 | catalog の行の例 | 入力 | 変換ツール |
   |---|---|---|---|
   | dense A16（`base0-dense-v1`） | `Qwen/Qwen2.5-1.5B/graph-v7@2097152`, `…/graph-v7@8192`, `…/graph-v7@2048`, `…/graph-v5@512`, `Qwen/Qwen2.5-Coder-1.5B-Instruct` | HF の `config.json` + `tokenizer.json` + `model.safetensors` | `qwen25-convert` |
   | hybrid mmap（`qwen36-mmap-v1`） | `Qwen3.6-35B-A3B/graph-v7@512`, `…/graph-v7@2097152`, `Qwen/Qwen3.5-2B/graph-v3`, `Qwen/Qwen3.8-27B/graph-v3`, `huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated/graph-v3` | `Q4_K_M` の GGUF | `qwen36-convert` |

   **catalog に無いアーキテクチャは、ファイルを置いただけでは class にならない。** 幾何の定数と
   catalog の行（ADR-0135 の言い方で「model は data」）、court が歩ける profile、決定論的な変換器と
   実行器、reachable kernel の裁定、conformance テストが要る（[palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md) §「新しい lineage」）。
   例外は ADR-0108 の **class manifest 経路**（`misaka model add --manifest <file>`）で、これは
   「build を変えずに、inline profile / canonical job / artifact root を書いた manifest を
   Full 深度で検証してから登録する」もの。
3. **4 つの合意が同時に成り立って初めて class は動く**: court が歩ける graph（= class id）、
   チェーンが pin する artifact の **root**、class が対価を受ける canonical job、graph 通りに
   実行する engine。手順の各ステップはこの 4 つを 1 つずつ確かめる。
4. **root は 2 種類あり、混ぜると事故になる**（testnet-11/12 の dense 行を止めた実事故）。
   ファイル全体の digest と、graph ごとの **operand-inventory root** は別物で、held 行
   （`graph-v7@…`）や graph-v6 行が登録するのは inventory root。人が root を手で書き写す場面を
   無くすために **sidecar `.palwmanifest`** がある（§3）。
5. **登録は permissionless だが、weight と lane は certification が買う**（ADR-0069/0075）。
   class の kernel 集合を drill 済み family が被覆していれば floor share で着席、無ければ
   **weightless**（登録はできるが block の重みが 0、`palw_uncertified_weightless` 武装網では収益 0）。
   この build が drill できる family は 5 つ: `base0`, `qwen36`, `a16`, `a16-v5`, `qwen36-v6`。
   held hybrid 行（`Qwen3.6-35B-A3B/graph-v7@…`）を被覆するのは **`qwen36-v6` だけ**
   （2026-09-23 追加。それ以前は weightless 以外に登録できなかった）。
   **ただし held hybrid 行は、登録・認証が通っても現在の build では attempt が完走しない**
   （2026-09-23 実測）。graph-v7 の held map は GDN の畳み込み窓を `(2·k_dim + v_dim)·heads`
   （12,288）で集めるが、engine の窓はそれと一致するのが key/value の head 数が等しいときだけで、
   Qwen3.6-35B-A3B は k 16 / v 32（engine の窓は 8,192）。最初の recurrence checkpoint（prefill の
   position 15）で `the prefill checkpoint at position 15: layer 0 holds a convolution window this
   geometry does not describe`（`ConvIsNotTheGeometrys`）になる。map 名は consensus の一部なので、
   直すには新しい map 版と class の再登録が要る。それまで held hybrid 行は block を作らない。
   testnet-12 の genesis から hybrid 行を外したのはこのため（genesis は floor + dense
   `graph-v7@8192` + dense `graph-v7@2097152`）。
6. **登録者は自分の class を自分では活かせない**（ADR-0145 §7）: 登録直後の状態 `Registered` は
   「登録者**以外**の operator の seat が ready になる」まで `Prefetching` にも進まない。着席
   （§6）を他人に頼めることが前提。

## 1. 用意するもの

* このリポジトリの release build（変換・登録・認証・座席は**同じ build** で揃える）:
  ```bash
  cargo build --release --bin qwen25-convert --bin qwen36-convert   # 変換
  cargo build --release --bin palw-class --bin palw-certify           # 検証・認証オブジェクト
  cargo build --release --bin kaspad --bin misaka                     # ノード・CLI（package は misaka-cli、binary は misaka）
  ```
* 変換元: HF safetensors 一式（dense）か、`Q4_K_M` GGUF（hybrid）。hybrid は Ollama の blob を
  そのまま渡せる（`Qwen3.6-35B-A3B-PALW-runtime` の README「固定する上流 artifact」）。
* ディスク: 出力 artifact は dense 1.5B で 1.8〜2.9 GB、hybrid 35B で 33〜39 GB。**context を
  広げると RoPE 表ぶん大きくなる**（Qwen3.6 で 512→2,097,152 は +2.1 GB）。
* 鍵と資金（登録・認証を行うホストにだけ要る）:
  * ML-DSA-87 の seed（`misaka key gen`）。`misaka mining setup` / `misaka verifier setup` を
    通しておくと `~/.misaka/mining.toml` に鍵・bond・fee 出力が揃い、`misaka model add` は
    それを使う。
  * **active な producer bond**。genesis card に無いホストは `kaspad --palw-register-bond
    --palw-producer-key <seed> --palw-producer-pay-address <addr> [--palw-bond-collateral <sompi>]`
    で 1 回だけ登録し、印字された `<txid>:<index>` を以後 `--palw-producer-bond` に渡す
    （既定の担保は「floor class の claim 1 件が要る額」＝最小値より上。少なすぎる bond は
    claim を 1 件も開けず永久に hold する）。
  * **fee 出力**（`--palw-fee-outpoint <txid>:<index>`）: bond 鍵の P2PKH に払う UTXO。lifecycle
    オブジェクト（登録、認証、possession proof）の carrier がここから払い、お釣りは同じアドレスに
    戻って rolling するので 1 回の funding で複数回持つ。genesis bond には 100 MSK の float が
    同梱されている。
* 座席（§6）を提供するホスト: artifact を **同じファイル**で持ち、replay できるだけのメモリ。
  artifact は mmap で host に 1 部（ADR-0136）だが、**K/V と replay の working set は class の
  context に比例する**（§7 の実測）。

## 2. 変換する（Step 1）

### dense（Qwen2.5 系）: `qwen25-convert`

```bash
# 入力ディレクトリに config.json / tokenizer.json / model.safetensors
qwen25-convert /path/to/Qwen2.5-1.5B-Instruct --a16 --n-ctx 2097152 --out qwen25-1.5b-a16-2m.palwart
```

* `--n-ctx` は **class 名の一部**（`…/graph-v7@2097152`）。artifact の header に `max_position`
  として書かれ、その幅の行としか pair しない。
* `--out` を付けて初めて runtime になる（無いと測定だけ）。`--layers N` は開発用の切り詰め。

### hybrid（Qwen3.6 / Qwen3-Coder / Qwen3.5 / Qwen3.8）: `qwen36-convert`

```bash
qwen36-convert --gguf /path/to/Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf \
               --out qwen36-35b-a3b-2m.palwart --context 2097152
# 手元に無ければ URL から（先頭 64 MiB の header を別に渡す）:
qwen36-convert --url https://…/model.gguf --header header.bin --out … --context 512
```

* `--context` 既定は 512。**hybrid 35B @2M の実測（ibm、2026-09-23）: 1,024 s、出力
  38,639,790,592 bytes（38.64 GB）**。
* 変換は決定論的: 同じ GGUF・同じ converter → 同じ root。root が pin と食い違うなら入力か
  converter の版が違う（README に GGUF の SHA-256 を書いておく理由）。

## 3. 何とペアになるかを、鍵も金も使わずに確かめる（Step 2）

```bash
palw-class ledger    --network testnet-12                       # この build の catalog と、genesis で登録済みの行
palw-class inspect   --network testnet-12 qwen36-35b-a3b-2m.palwart
palw-class preflight --network testnet-12 qwen36-35b-a3b-2m.palwart --model-id "Qwen3.6-35B-A3B/graph-v7@2097152"
```

* `inspect` は行ごとに `PAIRS … root …` か `no … — <理由>` を出す。**同じファイルが複数の行と
  pair することがある**。登録したい行を `--model-id` で名指しする。
  * dense は幅まで一致した行だけと pair する（実測: `--n-ctx 8192` の artifact の sidecar は
    `graph-v7@8192` の 1 行だけ、2M の artifact は `graph-v7@2097152` の 1 行だけ）。
  * **hybrid の pair 判定は context 幅を比べない（既知の穴）。** Qwen3.6 の 512 幅の artifact の
    sidecar には `Qwen3.6-35B-A3B`（v1）, `…/graph-v3`, `…/graph-v7@512` に加えて
    `…/graph-v7@2097152` の行も出る。RoPE 表は 512 位置分しか無いので、その組み合わせで
    登録しても attempt は動かない。**hybrid は登録する行の幅で変換し直し、その幅の行を名指しする。**
* `preflight` は admission gate そのもの（`verify_class_admission_v2`）を genesis の bundle に
  対して走らせ、拒否理由をチェーンと同じコードで返す。**ここで REFUSED ならチェーンでも
  REFUSED**（fee を払う前に分かる）。ただし genesis view なので、live chain が既に別 class を
  登録していれば `misaka model preflight <artifact>`（ノード経由、live の terms と同じ理由コード）で再確認する。
* よくある拒否:
  * `HeldMapNeedsItsFence` / `TokenLiftNeedsItsFence` / `KimiFamilyNeedsItsFence`: その graph が
    要る fence がそのネットで武装されていない（testnet-12 は全部 genesis から武装）。
  * `NotEndToEndCertified`: share > 0 で登録しようとしたが被覆 family が無い → weightless なら
    通る。§5 を先に。
  * 「known weights」: チェーンが既に同じ artifact root を持つ。**同じ重みを新しい id で登録し
    直すことはできない**（2026-08-28 に seat を 1 つ焼いた事故の再発防止）。

## 4. sidecar を作る（Step 3）

```bash
palw-class manifest --network testnet-12 qwen36-35b-a3b-2m.palwart
#   -> qwen36-35b-a3b-2m.palwart.palwmanifest（artifact digest + pair する行ごとの inventory root）
palw-class manifest --network testnet-12 --check qwen36-35b-a3b-2m.palwart   # 再導出して一致を確認、exit 1 で不一致
```

* sidecar は **ファイルの digest で束縛**される（dense はバイト digest、hybrid は mmap 全体の
  computed root）。別ファイルの隣に置いても `DigestMismatch` で信用されない。
* 配布するときは artifact と **必ず同梱**する。ノードは起動時に sidecar を読み、
  `--palw-verify-class-manifest` で再導出と突き合わせて食い違えば起動を拒否する。
* 事故の型: 「genesis card に root を手書き → 実は digest だった」。sidecar があれば card は
  `class_manifest_const_v1` で sidecar から root を読むだけになり、手書きの場面が消える。

## 5. 登録する（Step 4）

### 推奨: `misaka model add`（登録 → block lane 認証 → prompt lane 認証 → LIVE を再開可能に）

```bash
misaka --network testnet-12 model add                                   # 引数無し: catalog を一覧
misaka --network testnet-12 model add "Qwen3.6-35B-A3B/graph-v7@2097152" \
       --artifact /root/palw-class/qwen36-35b-a3b-2m.palwart --prompt-lane --yes
misaka --network testnet-12 model status "Qwen3.6-35B-A3B/graph-v7@2097152"   # 何が済んで、次に何をするか
```

* `~/.misaka/mining.toml` の鍵・bond・fee 出力を使い、**チェーンの live terms**
  （`getPalwRegistrationTerms`）で登録を組んで署名する（genesis の terms を使うと base class の
  retarget 後に拒否される）。
* 認証は「まず chain が既に持つ family で被覆できるか」を見て、できればその family に **bind**
  だけ、できなければこの build の drill で family を **filing** してから bind する。chunk
  （family オブジェクトは carrier 1 本に入らない）は index 順に投げ、group が適用されるまで
  待ってから bind する。
* 何も `--yes` 無しには払わない。途中で止めても **再実行すれば chain の状態から再開**する
  （journal ではなく class table と certified families を見る）。`--no-wait` は「今の pending を
  言って終了」。
* `--seat-ms-per-position <ms>`: fleet で一番遅い seat の実測 replay コスト。ADR-0082 D9 の
  seat-window 上限に使う。

### 低レベル（自動化・開発者向け）: `kaspad` の登録 flag

> **同じ bond で 2 つ目の `kaspad` を起動しない（2026-09-25 から slash される）。** node の duty は
> 起動 option によらず常に動く（`kaspad/src/palw_duties.rs`）。そのため、bond の鍵と outpoint を
> 渡した process は、登録用に起動したものでも、その bond の seat の仕事と execution lane の
> round block を担う。稼働中の node の横で同じ bond の登録用 process を動かすと、両方が同じ
> round permit に別々の block で署名し、chain は bond ごと slash する（`RoundPermitEquivocated`）。
> 登録用 process は class が chain に載っても終了しない。appdir が別なら、round の署名記録
> （`palw-round-last-signed`）も共有されない。

通常は上の `misaka model add` を使う。稼働中の node の RPC に登録を送るだけなので、2 つ目の process は
起動せず、seat の仕事も止まらない。低レベルの flag を使うときは、登録用の `kaspad` を
**その bond を動かす唯一の process** として起動する。次のどちらかにする。

1. **稼働中の node 自身に flag を足して再起動する**（同じ unit・同じ appdir）。
   * `--palw-register-class` がある間、panel は登録が chain に載るまで、各 tick で gossip を
     受け取るだけで seat の仕事（receipt・replay など）をしない（既存の挙動）。その間、この bond は
     座席に引かれても応答しないので、登録は短く済ませる。
   * ログに `[palw-panel] the class registration in tx … is on the chain` が出たら、**flag を
     外して再起動する**。flag を残したまま次に再起動すると、候補が `AllRegistered` で作れず、
     毎 tick 登録を試みて seat の仕事を飛ばし続ける（既存の挙動）。
2. **まだどの process も動かしていない bond で起動する。** その process が、その後もずっと
   その bond の node になる。登録後に止めると、その bond は座席に引かれたまま応答しない seat になる。

```bash
# 1 の例: 稼働中の node の unit に 1 行足して再起動する（別の process は起動しない）
kaspad --testnet --netsuffix=12 --utxoindex --appdir=<稼働中の node の appdir> \
  --palw-class-artifact=/root/palw-class/qwen36-35b-a3b-2m.palwart \
  --palw-register-class="Qwen3.6-35B-A3B/graph-v7@2097152" \
  --palw-producer-key=/etc/misaka/t12/bond.key --palw-producer-bond=<txid>:<index> \
  --palw-producer-pay-address=<addr> --palw-fee-outpoint=<txid>:<index>
```

ノードは artifact を読み、`--palw-register-class` の行と pair し、live terms で
`ClassRegistered` を 1 回だけ組んで fee 出力から carrier を出す（ログ `[palw-panel] class
registration carrier …`）。artifact の形状が複数の行に合うときは model id を必ず与える。
起動時には `PALW bond …: run it in exactly ONE process …` が WARN で出る。

### 認証を手でやる場合（`misaka model add` がやっていること）

```bash
# family（lane ごとに 1 回。chain に既にあれば FamilyAlreadyCertified で拒否される）
palw-certify drill --model-id "Qwen3.6-35B-A3B/graph-v7@2097152" --lane fp --out fam-fp.obj
#   -> fam-fp.obj.chunk0 .chunk1 … が書かれたら chunk を index 順に
misaka --network testnet-12 --rpc 127.0.0.1:<borsh-port> palw submit-object --key-file <seed> --object fam-fp.obj.chunk* --yes
# class を family に bind
palw-certify bind --model-id "Qwen3.6-35B-A3B/graph-v7@2097152" --lane fp --out cls-fp.obj
misaka --network testnet-12 --rpc 127.0.0.1:<borsh-port> palw submit-object --key-file <seed> --object cls-fp.obj --yes
palw-certify inspect --object cls-fp.obj     # 何を運ぶか、この build の court が grade するか
```

* **block（attempt）lane**: 登録時に pin 済み family（この build の 5 family）が class を被覆して
  いれば **登録そのものが floor share で着席させる**ので attempt lane の bind は不要
  （投げると `ClassAlreadyWeighted` で落ちる）。被覆が無い weightless 登録だけが後から
  `FamilyCertified` + `ClassLaneCertified(attempt)` で座る。
* **prompt（fp）lane**: chain 上の certified family（`FamilyCertified` で運ばれたもの）が要る。
  genesis の pin は chain の集合には数えられない（新しい identity の chain 集合は空から始まる、
  ADR-0075 D10）ので、**fp lane は必ず drill を filing してから bind** する。
* `misaka model certify <class-id> --yes` は「この build の drill で family を filing する」だけを
  行う CLI 形（activation ではなく fence でもない）。
* 拒否は carrier が落ちる形で現れ、fee は戻らず、理由は**ノードのログ**（`[palw-lifecycle]`）に
  だけ出る: `FamilyAlreadyCertified`, `NoCertifiedFamilyCovers`, `CertificationNeedsActiveClass`
  （class が Active でない）, `ClassAlreadyWeighted`。`submit-object` は投げる前にローカルで
  grade して同じ拒否を先に言う。

## 6. 座席を集めて lifecycle を歩かせる（Step 5）

登録した class が仕事を受けるには、**その artifact を持ち replay できる seat が
`required_ready_seats` だけ ready** にならなければならない（ADR-0135）。

```bash
misaka --network testnet-12 palw registry            # op 186: 行ごとの state・derived profile・ready seats・possession proofs
misaka --network testnet-12 model readiness <class-id>            # 各 seat の proof の期限・担保・非 ready の理由
misaka --network testnet-12 model registration <class-id|object|tx>  # 登録オブジェクトが constructed / submitted / accepted / included / folded のどこか
```

状態遷移（すべて chain が見える事実の関数）:

```
Registered ──(登録者以外の operator の seat が ready)──> Prefetching ──(ready ≥ required)──> Probation(probe 10 件 all pass)
  ──> ActiveLimited(3 epoch 安定) ──> Active          どこからでも Held（panel を引けない / 利用率超過）→ 自分の新規 claim だけ止まる
```

* `required_ready_seats` は既定 `max(seat_count 5 + spare 2, …)` = **7**（possession gate 下）。
  seat は **operator が別**でなければ数えられない。
* seat 側の起動（`misaka verifier setup` → `verifier start`、または直接）:
  ```bash
  kaspad --testnet --netsuffix=12 --palw-class-artifact=/path/to/same.palwart \
         --palw-producer-key=<seed> --palw-producer-bond=<txid>:<index> --palw-fee-outpoint=<txid>:<index> \
         --palw-host-memory-budget=<bytes> --palw-host-node-count=<n>     # または --palw-host-memory-share=<bytes>
  misaka --network testnet-12 palw panel join --class <class-id|QWEN36> --artifact /path/to/same.palwart --bond <txid>:<index> --yes
  ```
  seat の仕事は鍵と bond があれば常に動く（`--palw-panel` は 2026-09-25 から不要。渡しても何もせず WARN が 1 行出る）。
  この seat の bond は、この 1 つの process だけで動かす（上の「低レベル」節の注意）。
  possession proof（readiness）は seat が**自動で** span ごとに出す。ただし **replay 予算が無い seat は
  proof を出さない**（ADR-0136: 予算は host の share から導かれる）。1 host に複数 node を置くなら
  `--palw-host-memory-budget` ÷ `--palw-host-node-count`、役割が違う node が同居するなら
  `--palw-host-memory-share` で明示する。足りないと「a replay needs X GiB as full-seat」を
  ログに出して deferral し続ける。
* `misaka model status <model>` が「今どの段で、次のコマンドは何か」を 1 行で言う。

## 7. terms と経済（登録前に読む数字）

`palw registry` の各行には derived profile が出る: `verification_window_spans`,
`artifact_prefetch_spans`, `required_ready_seats`, `max_inflight_claims`, `registration_bond_sompi`
（= 1,000 MSK × window spans）。**2026-09-23 時点で `registration_bond_sompi` は表示のみで、読む
規則が無い**（未強制。将来 fence で強制され得る）。実際に縛るのは ready seats と
`collateral_ok`（seat の担保 ≥ 露出 × 倍率）。

**2026-09-25（Activation Pool の P4、ユーザー決定）: `registration_bond_sompi` は価格ではない。**
RPC では DEPRECATED（意味・wire 位置は据え置き、refund 0 のまま未強制）。代わりに
`recommendedPoolSompi`（非拘束の推奨 pool、`16 · A_MAX / α`、現行 terms で 2,400 MSK）を見る。
`misaka model add` と `misaka palw extension submit` は、登録が fold された後に別 carrier で
500 MSK（`10·A0/α`）をその class の Activation Pool に sponsor する（寄付・fold 後は返金なし。
`--sponsor <MSK>` で変更、`--no-sponsor` で無し）。Candidate 以外の class への top-up は全額 (b)
になり、次の formation（または再 formation）でしか払われない — `misaka palw model-pool` が警告する。

実測（testnet-12 の globals、`palw_derive_profile_v1`）:

| 行 | verification CCU | window | 表示 bond | required seats | seat 1 席の K/V（i16） |
|---|---|---|---|---|---|
| `Qwen/Qwen2.5-1.5B/graph-v7@8192`（genesis） | 1.39×10¹² | 3 spans | 3,000 MSK | 7 | ≈ 0.22 GiB（28 層 × 2 kv head × 128 × K/V 2 本 × 2 B × 8,192） |
| `Qwen/Qwen2.5-1.5B/graph-v7@2097152`（genesis） | 3.36×10¹⁵ | 2,799 | 2,799,000 MSK | 7 | ≈ 7–11 GiB |
| `Qwen3.6-35B-A3B/graph-v7@512`（登録候補、held map 修正待ち） | 1.6×10¹¹ | 2 spans | 2,000 MSK | 7 | 小 |
| `Qwen3.6-35B-A3B/graph-v7@2097152`（登録候補） | 3.50×10¹⁵ | 2,918 | 2,918,000 MSK | 7 | **≥ 43 GB**（10 attn 層 × 2 kv head × 256 × K/V 2 本 × 2 B（i16 換算）× 2M。i32 なら 2 倍。GDN の状態は別） |

つまり **2M の hybrid 行は 23 GiB のホスト 7 台では ready seat が 1 つも立たず、chain は
`Prefetching`（ready 0 < 7）と名指しして Held 相当のまま止める**。これは path の故障ではなく
registry の正しい答え。context を狭めた行（例 `@8192`、window 3 spans）なら同じ手順で立つ
（testnet-12 はその 8k 行を genesis に持つ）。

**seat の担保（testnet-12、option A）**: runtime は claim ごとに「escrow + weight」を bond に予約
する（escrow は block 1 の subsidy 444,562,014,000 sompi × worker carve 720‰ = 3,200.85 MSK）。
bond が同時に持てる claim 数は `担保 × 50 % ÷ (escrow + weight)` で決まり、担保に比例する。
genesis の seat は floor 64 本 + 各 model 行 4 本で 939,063.21 MSK。

market（任意、ADR-0087〜0090）: `misaka model market open <model> [--seed <MSK>]` で class の
founding line を seed する（最小 seed 100,000 MSK、分割払い可）。position の売買は
`misaka position list|quote|buy|sell`。登録・認証とは独立で、後からでよい。

## 8. 実例: Qwen3.6-35B-A3B @ 2,097,152（2026-09-23、ibm）

```
qwen36-convert --gguf Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf --out qwen36-35b-a3b-2m.palwart --context 2097152
  exit=0 elapsed=1024 s  -> 38,639,790,592 bytes, sha256 ecc6cc6a66c35ad3f67e696c3464325c5277d3f6577cbbe4e81efa4cfad4c400
catalog row  Qwen3.6-35B-A3B/graph-v7@2097152   class id b4b891afe49a59f5…
公開        https://huggingface.co/Misakachain/Qwen3.6-35B-A3B-PALW-runtime  palw-runtime/qwen36-35b-a3b-2m.palwart
            （commit bbc2f6c3; 512 版 palw-runtime/qwen36-35b-a3b.palwq36 と Xet 重複排除され新規送信 2.20 GB）
被覆 family  PALW-QWEN36-V6（両 lane）— これが無い build では weightless 登録しかできない
```

## 9. 困ったとき

| 症状 | 見るところ / 直し方 |
|---|---|
| `inspect` が全行 `no` | 理由を読む。層数・head・vocab が catalog の幾何と違う → 別の checkpoint か、catalog に無いモデル。幅だけ違う → `--n-ctx`/`--context` を行に合わせて再変換 |
| `manifest` が「pairs with no class」 | 上と同じ。`inspect` の理由がそのまま答え |
| `THIS NETWORK REGISTERED A DIFFERENT ROOT FOR THIS CLASS` | genesis が pin した root と手元の inventory root が違う。converter の版か入力が違う（README の SHA を照合）。root 形式（digest vs inventory）の取り違えなら card 側の欠陥 |
| 登録が `class registration is built and waiting: no fee UTXO resolves` | `--palw-fee-outpoint` が bond 鍵のアドレス宛てで未使用の UTXO か確認。`misaka wallet send` で自分の pay address に送って作る |
| carrier が落ちて fee だけ消えた | ノードの `[palw-lifecycle]` 行に理由。`submit-object` を先に走らせれば同じ拒否をローカルで言う |
| ready seats が増えない | 各 seat の `misaka model readiness`。予算不足（「a replay needs X GiB」）なら `--palw-host-memory-share`、operator が登録者と同じなら数えられない、proof が古い（`PALW_READINESS_LANDING_SPANS_V1` = 8 span） |
| OOM で seat/producer が落ちる | 1 host の node 数と share を宣言する（`--palw-host-memory-budget`/`--palw-host-node-count`）。2M 行は attempt 1 回で ≈ 11.6 GiB（dense）〜 43 GB（hybrid K/V）を要求する |
| `HeldMapNeedsItsFence` 等 | そのネットで fence が武装されていない。testnet-12 は全 fence が genesis から有効 |
| hybrid の attempt が prefill の position 15 で `ConvIsNotTheGeometrys` | held map の欠陥（§0 の 5）。登録や artifact の問題ではない。新しい map 版が出るまで held hybrid 行は動かない |

## 10. 参照

* ADR-0135（registry と lifecycle）、ADR-0136（mmap と host 予算）、ADR-0145 §7（`Registered` は
  登録者以外の seat が要る）、ADR-0075（認証は consensus object）、ADR-0069（weight は認証が買う）、
  ADR-0103/0119（held 行）、ADR-0108（manifest 経路）、ADR-0122（`misaka model add` と運用者 UX）、
  ADR-0087〜0090（market）。
* [palw-certify-a-new-model.md](palw-certify-a-new-model.md) — 5 family と手動の認証手順。
* [palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md) — 新しい checkpoint / 新しい lineage を
  SDK に載せる開発者手順。
* [testnet-12-regenesis-2026-09-23.md](testnet-12-regenesis-2026-09-23.md) — testnet-12 の genesis 行と fingerprint。
