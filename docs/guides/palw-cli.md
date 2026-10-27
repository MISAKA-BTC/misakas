# PALW CLI ガイド — `misaka palw` で claim・court・bond・fence を読み、object を作って出す

> **位置づけ:** 利用者・運用者向けの手引き。合意規則・fence・パラメーターは変更しない。
> 読み取りは node のローカル RPC(ops 225–228)だけを使い、どの規則もその結果を読まない。
> 設計側の対応表は [PALW user interactions survey](../design/palw/tooling/palw-user-interactions.md)。

## 0. 前提

- 実行ファイルは `misaka`(crate `misaka-cli`)。
- 接続先は `--network`(既定 `testnet-12`)と `--rpc`(wRPC Borsh)で決まる。
  - `--rpc` を省略したときは `~/.misaka/<network>/endpoints.json` を使う。
- `--json`(または `--output json`)を付けると、機械可読の文書を出力する。
  - 文書には `version`(node 側の版)と `schema: misaka.palw.user-view.v1` が入る。
  - 知らない版の文書は読まずに拒否する。
- **古い node への対応:**
  - ops 225–228 を持たない node は、その op を受け取ると WebSocket ごと接続を切る。
  - そのとき CLI は exit 36(`COMPONENT_DOWN`)を返し、代わりに使える古い読み取り(`misaka palw claim`、`misaka bond status` など)を表示する。
- **署名と送信:**
  - 署名は `--key-file`(ML-DSA-87 の hex 32 byte seed)で行う。
  - 送信は `misaka palw submit-object` だけが行う。
  - 既定は dry-run で、実際に送信するのは `--submit`(または `--yes`)を付けたときだけ。

## 1. 読む

### 1.1 全体の状態 — `misaka palw status`

```sh
misaka palw status
misaka palw status --bond <txid>:<index>
```

表示する内容は次のとおり。

- いま armed の fence の数
- 全面有効化(full activation)の高さ
  - 未採択なら `not adopted (dormant)` と表示する。
- 合意が型として「未採択」で持っている値の一覧(§3)
- `--bond` を付けたときは、その bond の状態

### 1.2 fence — `misaka palw fences`(op 227 `getPalwFenceSchedule`)

```sh
misaka palw fences                               # 全 fence: 高さ、armed か、属する release list
misaka palw fences --armed-only                  # いま armed のものだけ
misaka palw fences --list fullActivation --json  # 全面有効化リストの 36 本
```

- `armed` は、node の virtual DAA において `ForkActivation::is_active` が真かどうか。fold 側も同じ述語で判定する。
- `lists[].adoptedDaa` が `null` の list は、高さが採択されていない(`not adopted`)。
  - 例: 全面有効化の目標 16,000 は `targetDaa` として表示するが、採択されるまで予定には入らない。

### 1.3 claim — `misaka palw claim-detail <CLAIM_ID>`(op 225)

```sh
misaka palw claim-detail 3fa1…(128 hex)
```

| 行 | 意味 |
|---|---|
| `phase … ends by itself at DAA N` | 現在の phase と、その phase が自動で終わる DAA(`palw_claim_phase_deadline_v1`)。court や告発があると保留される |
| `certification (Gate C)` | `not-governed` / `pending` / `certified` / `revoked` などの認証状態。**Final と SecurityCertified は別の状態**で、合意 credit を生むのは認証済み claim だけ |
| `claim-local settlement` | RFC-0023 の claim ごとの settlement。fence が休眠中は `-` |
| `accusation pending` | 告発が係属しているか。係属中は、不正以外の理由による終了(期限切れ・引き直し)が止まる |
| `receipt pause` | Gate C が管轄する claim の受領窓の一時停止(aux 145 の行) |
| `court clocks` | `notice until`(受理 + 3,120)、`hard`(fold が読む hard deadline)、`extended hard`(受理 + 10,000)、`per-turn`(1 手ごとの期限)、`window_court` |
| `DA session` / `court session` | 開いている data availability(DA)の session と court の session。court session には、ladder の手番と手番の期限、backstop を表示する |

clock v2 が管轄しない claim では、`notice until` と `extended hard` が `-` になる。
v2 fence より前に受理された claim には、旧式(`trace_retention_daa − W_disclose`)がそのまま使われる。

### 1.4 court — `misaka palw court-state --claim <C> | --session <S>`(op 226)

```sh
misaka palw court-state --claim <claim>      # claim 上のすべての court session
misaka palw court-state --session <session>  # 1 つの session
```

`--claim` と `--session` はどちらか一方だけを指定する。両方の指定も、どちらも無い指定もエラーになる。

### 1.5 bond — `misaka palw bond-detail <txid>:<index>`(op 228)

表示する内容は次のとおり。

- 担保(slash 後の正味)と累計 slash 額
- 状態(`active` / `retiring`)
- 予約中の exposure(claim の分と登録の分)
- 生きている slashable lock(`bond retire` が待たされる原因)と、その解放 DAA
- 現役の claim(executor として、seat として)
- 保持している dispute reservation
- 訴追中の court session

### 1.6 object ファイルを見る — `misaka palw object-show --object <F> [--at-daa D]`

node には接続しない。表示する内容は次のとおり。

- tag と variant
- 所有 fence
- carrier に載せられるか(`may_ride`)
- `--network` の規則で、`--at-daa`(既定は遠い将来)にその種類が読まれるか(`kindInForce`)

## 2. 作る → 見る → 署名する → 出す

### 2.1 dispute reservation(tag 154)/ release(tag 155)

```sh
# 1. 作る(署名済み object をファイルに書く。送信はしない)
misaka palw build dispute-reserve --claim <claim> --bond <txid>:<index> --key-file ~/.misaka/bond.key --out r.borsh
# 2. 見る
misaka palw object-show --object r.borsh
# 3. 出す(既定は dry-run。--submit で送信)
misaka palw submit-object --object r.borsh --key-file ~/.misaka/bond.key
misaka palw submit-object --object r.borsh --key-file ~/.misaka/bond.key --submit
```

**root の取得:** claim の execution root と trace root は node(`getPalwLegacyDispute`、op 204)から読む。
`--execution-root` と `--trace-root` は、2 つ揃えたときだけ指定できる。

**作る前の検査:**

- **鍵の一致:** bond に登録された鍵と `--key-file` の鍵が一致しなければ拒否する。別の鍵で署名した object は chain に拒否され、手数料が無駄になる。
- **fence:** `palw_legacy_public_filer_v1` が node の DAA で armed でなければ、exit 10 で拒否する。
  - drill で後の高さ向けに作るときだけ `--allow-dormant` を使う。

**署名:**

- 合意側の builder `palw_dispute_reserved_object_v1` / `palw_dispute_released_object_v1` を使う。
- 合意側の context `misaka-palw/legacy-dispute/object/v1` で、chain 固有の domain に対して署名する。
- 状態に依存する判定(deposit、上限、期限)は、その object を運ぶ block で fold が行う。

### 2.2 値に依存する builder — CLI では作らない

```sh
misaka palw build funded-original --claim <claim>   # exit 10: funded original schedule が未採択(typed-missing)
misaka palw build court-ingress   --claim <claim>   # exit 10: 値は採択済み(V4 目安)、CLI の builder は未実装 — SDK を使う
misaka palw build material-intake --claim <claim>   # exit 10: 値は採択済み(V5 目安)、CLI の builder は未実装 — SDK を使う
misaka palw build d2-demand       --class <class>   # exit 10: 値は採択済み、CLI の builder は未実装 — SDK を使う
misaka palw build position-close  --claim <claim>   # exit 10: 値は採択済み、CLI の builder は未実装 — SDK を使う
```

- 2026-10-27 時点で typed-missing(未採択)なのは funded original schedule だけ。chain はその object をすべて拒否する。CLI は値を推測せず、値の名前と、合意がその値を読む関数を表示して終了する。
- 採択済み: court service profile(V1)、court paid-ingress(V4)、material intake(V5)、D2 disclosure fee、D2 readiness draw cap、TIR position witness budget、TIR lookahead frontier bound。いずれも目安の値で、根拠と見直し条件は台帳にある。
- 採択済みの値を使う builder は、CLI に実装されるまで exit 10 で SDK を案内する(UX-B が追加中)。

## 3. 未採択の値 — `misaka palw not-adopted`

```sh
misaka palw not-adopted
misaka palw not-adopted --json
```

- 各行は、fold が読むのと同じ accessor を、この build の合意コードで評価した結果。
- 値が採択されると、その行は自動で `adopted` に変わる(2026-10-27: funded original 以外はすべて `adopted`)。
  - その変更と同時に、合意コアの試験 `no_shipped_build_adopts_a_typed_missing_value` が落ちる。
  - これにより、この手引きと survey の見直しが強制される。

## 4. 終了コード

| code | 意味 |
|---|---|
| 0 | 成功 |
| 1 | 一般エラー(不正な id、対象が見つからない、知らない文書版) |
| 4 | 接続できない |
| 10 | 安全のため拒否(未採択の値、休眠中の fence) |
| 36 | node が ops 225–228 より古い(op で接続が切れた) |

## 5. RPC を直接使う

wRPC(Borsh / JSON)と gRPC で同じ 4 つの op を呼べる。
いずれも応答は `{available, found, nowDaa, viewJson}` の形をとる。

| op | method | request |
|---|---|---|
| 225 | `getPalwClaimDetail` | `{claimHex}` |
| 226 | `getPalwCourtState` | `{claimHex}` または `{sessionHex}` |
| 227 | `getPalwFenceSchedule` | `{}` |
| 228 | `getPalwBondDetail` | `{bond: "txid:index"}` |

gRPC の field 番号は 1264 から 1271。
