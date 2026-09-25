# testnet-12 R-core+ drill kit（公開後に実行する）

作成: 2026-09-25（branch `rcore/release-prep`、統合線 `rcore/int-3` @ `a0af3c92` 基準）。**何も実行していない。**
Mac 上で `drill-host.sh self-test` と host guard の空打ち（拒否されること）だけを確かめた。

## 0. 位置づけ

- **drill は公開の後に行う**（運用者の決定 2026-09-24。ADR-0152 §8.3 item 3「short drill は公開の gate ではない」、§8.4 の
  「公開後に回す drill と計測」）。この kit は `T12_LAUNCHED=yes` が無いと何も始めない。
- 中身は ADR §8.3 item 3 の **D-1〜D-10** と、§8.4 で後回しにした drill（SEAT-0 の混在版 drill、8k の実 weight timing drill、
  deadline 設計の M5 / M5b / M9 / M12、O-13 の計数）。step の実体は P2-12 の
  `scripts/misaka-palw-t12-rcore-drill.sh`（`steps` で一覧、番号は配列の位置）と analyzer
  `scripts/misaka-palw-t12-rcore-analyze.py`（O-1 / O-2 の 3 つの report）。
- この kit が足すのは fleet 側の 3 点だけ（`lib.sh`）:
  1. **host guard**: drill script 自身の `check-host` より厳しい。deploy kit の unit（`misaka-t12-node*` / `misaka-t12-seat*` /
     `misaka-t12f-*` / `misaka-t12p-*`、active でも enabled でも）、drop-in `zz-t12-regenesis.conf`、`/root/t12-rel/*/launch/b*.sh`、
     appdir `/root/.t12r-b*`、salt の無い testnet-12 kaspad、deploy kit と testnet 既定のポート（26311〜26354、26210、27210、
     28210、8545）の **どれか一つでもあれば拒否**。macOS も拒否。最後に drill script の `check-host` も通す。
  2. **公開後の gate**: `T12_LAUNCHED=yes`、かつ deploy kit の `fleet.env` が公開した release（`EXPECT_GENESIS`・`EXPECT_FP`・
     `KASPAD_SHA256`・`MISAKA_SHA256`）を固定していること。
  3. **出荷した binary だけ**（メモリ「drill は出荷するバイナリで走らせる」）: kaspad と misaka は `fleet.env` の sha256 と一致する
     ものしか置かない・使わない。どちらも `--palw-drill-genesis-salt` を知っていること。

旧 kit（drill 専用 commit `005e5c5f` に salt を焼き込んだ pre-drill、C0〜C16）は `legacy-005e5c5f/` に記録として残した。
`legacy-005e5c5f/lib.sh` は `bin/REV` が `005e5c5f…` でなければ送金を拒否する作りなので、R-core+ の binary では動かない。

## 1. drill を drill にするもの（ADR §8.2、P2-12、T53）

- **salt**（64 hex、`drill-host.sh run new-salt` で引く。全 drill host で同じ値を帯域外で渡す。all-zero は拒否）が genesis を動かす。
  genesis の outpoint・network domain・handshake の identity が drill のものになり、公開 t12 と drill は互いを拒否する。
- **鍵は drill 専用**: kaspad 自身が書く keyring（`run keyring` → `$WORK_DIR/keyring/manifest.json`）だけを使う。card 鍵、公開 t12 の
  premine txid を名指す `--palw-producer-bond` / `--palw-fee-outpoint` / `--stake-bond` は kaspad が起動時に拒否する。
- **EVM は salt で分離されない**（chain id は全 network で 1 つ）。manifest の `evm` のアカウントだけで署名する。
- 発見は無い: `--nodnsseed` と明示の `PEERS`。appdir は `WORK_DIR` の下で、kaspad が drill marker を書き、公開 node との共用を拒否する。
- 公開 kit 側は逆向きに、salt と他の `--palw-drill-*`・`KASPAD_*` 環境・drill marker を持つ node を exit 78 で拒否し、drill が動いて
  いるホストでは `switch` 自体を拒否する（`../t12-deploy-kit/lib.sh`）。

## 2. ホスト（運用者が用意する — PLAN R4）

公開後は ibm（169.58.39.220）・.113（169.58.232.113）・5.104（5.104.81.23）のすべてが公開 t12 node を動かすので、**この 3 台では
drill できない**（guard が拒否する）。95.111.236.186 は seeder ホストで、drill script の `check-host` が拒否する（unit file が salt 無しの
testnet-12 を名指す）うえ、11 GiB・egress 制限。ADR §8.3 item 3 は **4 台以上の fleet ホスト**を求める。8k の ready は 7 seat（別 operator）
が要るので、manifest の 8 seat を「1 ホストに複数 seat（`node <seat>` を seat ごとに、ポートは P2P_BASE+seat など）」か「7 台以上に 1 seat」
で配る。メモリは deploy kit PLAN §2 の算術（8k の仕事 1 本 3,493 MiB、producer はその上に attempt 3,456 MiB）で見積もる。

## 3. 手順（公開後）

```bash
# 0. Mac: 公開した release の fleet.env（deploy kit）と、この kit・scripts/ を drill host へ置く（運用者。リモート操作は【確認】）
#    drill host 上の配置例: /root/t12-drill/kit/{contrib/t12-drill-kit,contrib/t12-deploy-kit,scripts}
export T12_LAUNCHED=yes DRILL_ROOT=/root/t12-drill
./drill-host.sh guard                                   # 読み取りのみ。拒否されたらそのホストでは drill しない
./drill-host.sh stage-bins /root/t12-rel/incoming/<REV>  # 出荷した kaspad / misaka を sha 照合して $DRILL_ROOT/bin へ
SALT=$(./drill-host.sh run new-salt)                     # 1 回だけ引き、他の drill host へ帯域外で渡す
SALT=$SALT ./drill-host.sh drill-genesis                 # keyring を書き、drill genesis を検査し、DRILL_GENESES 行を出す
#    → Mac の deploy kit fleet.env に DRILL_GENESES+=" <hash>" を足す（公開 kit がこの genesis を拒否する）
SALT=$SALT PEERS="<他の drill node ip:port …>" ./drill-host.sh run node <seat>   # seat ごと
SALT=$SALT ./drill-host.sh run steps                    # D-1…D-10、market、maturity、report
SALT=$SALT SEAT=<seat> ./drill-host.sh run step <k>     # 証拠は action の後に書かれたログだけ（cursor 以後）
SALT=$SALT ./drill-host.sh run snapshot                 # cron で数分ごと（analyzer の入力）
SALT=$SALT ./drill-host.sh run analyze
```

- D-1(a) は `PUBLIC_PEER=<公開 t12 node の ip:port>` を渡すと、drill probe が公開 node に handshake して genesis で拒否されることを見る
  （公開 node への接続が 1 本発生する。運用者の判断で）。D-1(b) は `OLD_KASPAD_BIN=<R-core+ 以前の t12 build>`（例: 旧 live の
  `561d5b61`）。
- step の結果は PASS / FAIL / INCOMPLETE / MANUAL。MANUAL は `run attest <k> '<証拠>'` で記録する。`report` は全 step が PASS か
  ATTESTED で、analyzer が 0 を返すまで合格にしない。

## 4. このキットで確かめたこと（2026-09-25、Mac）

- `bash -n`（lib.sh・drill-host.sh・drill script）。shellcheck は未導入。
- `drill-host.sh self-test`: drill script の step 機構の self-test と analyzer の `--self-test` が通る。
- guard の空打ち: macOS で拒否、`T12_LAUNCHED` 無しで拒否、`fleet.env` が placeholder のままなら拒否、Linux を装って（`uname` を
  関数で差し替え）公開ポート 26313 を塞ぐと拒否、塞がなければ drill script の `check-host` まで通る。
