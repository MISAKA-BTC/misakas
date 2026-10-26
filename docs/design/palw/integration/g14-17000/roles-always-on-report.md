# lane ROLES — G14 の役割を duty として常時 on(2026-10-27)

branch `claude/g14-roles`(基点は `claude/g14-panel1-17000` cd54005fb)。規則(09-25):protocol duty は起動 option ではない。producing(新しい work の投稿)だけはノードの選択とする。

| 役割 | 変更前 | 変更後 | fence 条件 |
|---|---|---|---|
| kernel-route 公開検証者 (`palw_kernel_verifier`) | `--palw-kernel-model` が空なら何もしない。kit はこの flag を渡さない → fleet では検証が行われない | flag で指定したファイルに加え、`<appdir>/<net>/palw-kernel-models/*.palwkm` を自動で保持 model とする(`palw_kernel_models_with_default_dir_v1`)。kit は任意の `KERNEL_MODEL` を渡せる | `palw_probabilistic_constraints_v1` または Panel=0 の fence が成立した後にだけ model を開く(既存の armed 判定、変更なし)。fee outpoint と bond も必要 |
| Panel=1 kernel seat / slice roles (`palw_kernel_roles`、producer 以外) | flag による gate はない(armed + fee outpoint + key)。ただし model を検証者の book から取るため、model がないと空回りする | 上記の model 自動読込で動く | 既存の armed 判定 |
| kernel producer (`--palw-kernel-producer`) | opt-in | **変更なし**(producing はノードの選択) | — |
| GDN step filer | `--palw-gdn-step-filer` による opt-in、既定 off | 既定 on。緊急停止は `--palw-disable-gdn-step-filer`。旧 flag は受理するが何もしない(ALWAYS-ON DUTY) | `palw_memory_routing_court_v2`(tick 内の既存判定) |
| 有料 court ingress(held/outsider filer) | `--palw-court-paid-ingress` による opt-in、既定 off | 既定 on。緊急停止は `--palw-disable-court-paid-ingress` | fee schedule の採択(typed-missing の間は休眠)かつ `palw_court_outsider_lane_v1` |
| Gate C 抽選検証者 / PanelDa 開示運搬 / FpSelection TFP1 provider fetch / Panel seat | コードを読んだ限り flag による gate はない(`palw_fp_seat.rs`、`p2_7_disclosure_policy.rs`、`palw_selected_event_responder.rs`、`palw_public_prefix.rs`、`seat_progress.rs` が参照するのは `config.params`・`retention_dir`・`fee_outpoint` だけ) | 変更なし(すでに duty) | 各 fence(params) |

fence 未満の挙動:2 種の filer は tick 冒頭の fence 判定で idle になる(log のみ)。kernel 検証者は fence 前に model を開かない。起動時に増えるのは default directory の一覧取得 1 回だけ(ファイルは読まない)。drill の extra seat では、従来どおり両 filer を off にする(`daemon.rs`)。

## kit
- `lib.sh`:`KERNEL_MODEL`(カンマ区切り)があれば `--palw-kernel-model=` を追加する。`fleet.env.example` には空の `KERNEL_MODEL=` を置いた。
- PALWKM01 は .palwart や class artifact からは導出できない(別形式の `MapParams`)。推奨は、登録 class の PALWKM01 を default directory に置く方法(ARGS が変わらない)。手順は `contrib/t12-deploy-kit/PLAN.md` の §15 追記にある。

## 資源(要確認)
- GDN step filer は 1 tick あたり発見 16、claim 1、tile probe 2 の上限がある。material session は held GDN filer と共有する。
- **kernel 検証者の再実行と GDN step filer の recompute は、`--palw-host-memory-share` の memory ledger にも replay slot にも予約しない**(コードを読んだ結果、どちらにも ledger/slot の参照がない)。このため BASE-0 audit や replay slot と取り合うことはないが、ledger の外でメモリを使う。小型の kernel model なら影響は小さいと見込むが、未測定。

## 試験・waiver
- 実施したのは `cargo check -p kaspad` だけ。
- **WAIVER-R1**:arg parse の単体試験は追加も実行もしていない(既定 on / disable flag)。
- **WAIVER-R2**:fence 17,000 を跨ぐ drill は行っていない(ユーザー指示で drill を skip)。
- **WAIVER-R3**:kernel 検証者と GDN step filer が ledger の外で使うメモリ量は未測定。
- **WAIVER-R4**:PALWKM01 ファイルを fleet に配置していない(配備は別途承認が要る)。配置しない限り、kernel 検証 duty は空回りする。
