# RFC-0043 — 学習済みLLMの広いQ24 logit表現

> A0への取り込み注記（2026-11-01）: 本文の実装・検証実績は元checkoutでの私設実験を指す。A0の実装統合・再検証・有効化は別で、[統合境界](../design/palw/positive-court/WIDE-Q24-INTEGRATION.md)を参照。

- Status: Accepted for implementation / 2026-10-31 ユーザーが両案を明示採択。正式番号を台帳・全local refの履歴と照合して割り当てた。
- Owner: 今回の登録実験。承認者: ユーザー（2026-10-31、両案を明示採択）。独立レビューは未完了。
- Implementation / Verification / Activation: 登録・明示的なv2 APIの私設実装と実checkpoint検証は完了。公開網は未有効化、公開運用入口の接続・独立レビューは未完了。
- 対応ADR: [ADR-0211](../adr/0211-wide-q24-llm-output.md)。上位規程: [PRINCIPLES §11](../PRINCIPLES.md#design-freeze)。

## 重大問題と証拠

学習済みEleutherAI/pythia-70m、revision `a39f36b100fe8a5377810d56c3f4789b9c53ac42` の通常英文入力で、logitが約1082自然対数単位になる。公式Transformers 4.57.6 / PyTorch 2.15.0の同じcheckpointでも再現した。未学習モデルの異常値や変換器だけの計算誤りではない。[実験](../evidence/models/trained-llm-cohort-2026-10-31.md)。

現行LOGITS_Q24_V1はi32×2^-24で約±128しか表現できず、変換時は120で拒否する。これは最大文脈長2048を短縮しても解消しない。120の閾値だけを1082へ上げると後段のi32が飽和する。

## 提案する最小変更

既存i32 Q24 profile・既存class ID・既存scheme/tag・過去のopening bytesはそのまま保持する。新しい明示的なi64 Q24出力profileを追加し、自然対数単位×2^24を維持する。値のクリップ、温度変更、中心化や無言の再スケーリングは行わない。TIRの既存i64演算を利用するが、出力profileとtrace/openingの識別は既存i32系列と分離する。

compiler option・runtime pack・program dtype・class identityに新profileを明示する。単に変換器のlogits nodeだけをi64へ変え、旧i32のreaderへ流す実装は禁止する。

## 実装対象と受入条件

1. lowererの出力dtype・範囲検査・streamed/resident materialisation、CLIとruntime pack/rebuildで同じprofileをpinする。既定i32の既存golden bytesは不変。
2. `tir/exec/src/node/run.rs`、`cellstep.rs`、`cellproc.rs`、capture codec、evidence/annexのi32 logit行を監査する。新profileは幅を保持する別versionとし、旧consumerへの無検査castを拒否する。
3. `consensus/core/src/palw_step_refute.rs` と `palw_tir_court_v1.rs` のflat/tiled decode disclosure、sampling、argmax、temperatureの演算とrootを新profile用に接続する。単位は同じでもlane幅・hash preimageを黙って変えない。
4. admission/courtは拡大した資料byte数・work・scratchを導出し、既存上限で判定する。上限免除・court省略はしない。
5. claimごとの抽選認証・公開告発・court・bond責任は全モデルで維持する。モデル登録は0.3の既存規則のまま。モデル単位の審査・tier変更は追加しない。
6. Pythiaの実checkpointで、最大context2048、3実装一致、独立HF比較、登録・再生、large-logitのdecode/courtと誤開示の拒否を確認する。旧i32の履歴再生・不正裁定も回帰確認する。

## 互換性と有効化

旧wire ID・schemeの意味を再利用しない。新profileの採択と公開presetへの有効化は別の承認とする。現行公開presetへ自動追加しない。過去claim・責任を変更せず、未対応consumerは明示的に拒否する。

## 採択記録

当初は未承認として提案した。下記2026-10-31の回答により本RFCと対応ADRの両方を採択。旧i32契約は保持し、新profileの実装を進める。

## 2026-10-31 採択記録

ユーザー回答「両案を採択し、実装を進める」により、対応するRFC案とADR案を実装対象として採択した。公開networkの有効化承認ではない。旧i32契約・ID・履歴を維持し、新profileを旧readerへ流さず、明示的なv2 APIと回帰試験を接続する。公開運用入口の接続・独立レビューは有効化前に別途確認する。

## 実装する識別と私設規則

- 出力は signed i64、自然対数単位×2^24。I64を全commit pointへ一般開放せず、postの最終logits nodeだけに許可する。carry/stateは旧規則を維持。
- 新schemeはkey `misaka-palw/tiled-logits/scheme/v2` のBLAKE2b-512、u64 little-endian長を前置した単一part `tiled-logits-i64-q24-v2` から導出する。digestは `ad7dc4fbb0a83019c4f27ad54f44395d7cd9700f886e9d6ca8515ee3360679eaec36ef8765fdac4fc72091cb41cb22fa890fc827833b60ab3d060d94f30625cb`。
- tile/root hashは `/v2` の別domain。wide step leafはversion2で8 bytes/lane、旧leafはversion1・4 bytes/laneを維持。新capture magicは `PALWTRC2`。cell IPCはHelloでversion2を要求し、logitsのみi64。旧wire version1のreader/encoderを変更しない。
- samplingの単位はQ24のまま、比較keyをi128のQ48にする。旧i32値域では同じseed/temperatureの選択結果と一致させる。
- private ruleset名は `private-wide-q24`。既存IR fenceのcourt_version2で識別し、params/schedule fingerprintにも入る。公開presetのcourt_version1・有効化高さ・IDを変更しない。private native試験は明示的な `MISAKA_PRIVATE_WIDE_Q24=1` のときだけversion2をarmする。
- courtの資料量はwide profileについて8-byte laneの保守的上界で導出する。旧4-byte価格は不変。拡大したprofileを旧consumerで縮めず、未接続の公開claim/FP/V4/device入口は拒否する。認証・決済へ迂回させない。

実装と公開claim経路の有効化は別である。既存node backendの旧capture/annex/APIをnew profileへ読み替えず、明示的なv2実行・capture・cell・evidence/court APIで私設検証する。公開有効化の前に全運用入口の接続と独立レビューを完了する。

本RFCの追加規則はspec 04bの旧i32契約を置換せず、ここで識別した新schemeに限る補足仕様である。旧NF・旧IPC・旧trace・旧courtのwire bytesは維持する。私設実装と公開運用入口の接続・独立レビュー・有効化は別の状態として記録する。
