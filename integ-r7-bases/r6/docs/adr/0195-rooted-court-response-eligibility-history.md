# ADR-0195 — native court 応答の前提受理時刻を保持する

- Status: **Proposed / Review Required** (2026-10-21)。本番 schema・責任規則・数値・有効化の採択ではない。
- Companion: [RFC-0028](../rfc/0028-rooted-court-response-eligibility-history.md)。
- Scope: RFC-0026 F02/F03 と、ADR-0192 A/A の block-start eligibility を実 state から検査するための保持情報。
- Candidate source: `codex/rfc14-g14-integration-candidate` / `11a9b2b0e`。

## 問題を示す実装上の事実

`PalwTirPublicPendingV1` は demand の `opened_daa` / `respond_until_daa` を保持する。一方、`PalwTirPublicSlotV1` は body hash / total / accepted offset / head を保持し、各 Begin/Part の受理 DAA を保持しない。`PalwTirPublicChunkRecordV1` にも受理 DAA はない。Begin/Part の `debit` は move counter を進めるが、last-progress field は各 chunk の受理時刻の履歴ではない。

`ServiceSupply` は first accepting block と資源集計を保持し、`ServiceReservations::assess` は packet の割当 DAA に supply があったかを見るだけである。これは資源会計であり、その block の開始時点で当該応答段階が eligible だったことを証明しない。既存コードも personal default の権限とはしていない。

同じ demand/body を早い時刻と遅い時刻に Begin/Part した二つの native fixture について、現時点の owner row と duty view は同じになるが、ある割当 DAA より前に EOF を受理済みだったかは異なる。この反例を native dispatcher で実行する試験を追加し、成功を確認した。これは current retained TIR metadata の情報欠落を示すものであり、node 全履歴まで同一、または全 journal からも復元不可能とは主張しない。

## 比較する案

| 案 | 根拠 | 費用・未完 |
| --- | --- | --- |
| A — 推奨: native 段階が実際に変わったときの rooted marker | actual native accepting context と duty ID を同時に journal へ入れる。block-start eligibility の最早 DAA を再計算する | 新しい固定長 row の codec/heap/scanner/index/retention/atomicity と費用が必要。marker 自体は default を許可しない |
| B — first supply ごとの eligible packet 集合 | first accepting block の開始 state から実際に eligible な packet ID を集計して root 化 | complete template / packet mapping が先に必要。全 packet の走査・集合保持・branch refinement の費用を扱う必要がある。資源集計だけでは代用不可 |
| C — accepted block journals を問い合わせ時に再生 | 過去の native Begin/Part の受理 block/DAA を復元する | 必要 journal の pruning/IBD/再起動後の保持、再生・検索費用、読み取り範囲と上限が未定。local cache は根拠にしない |

A を隔離候補で具体化することを推奨する。既存の service v2、X3 container、source query と native duty の ID の意味を変えず、別の versioned marker namespace を末尾に加える。詳細案は RFC-0028。実装・試験は承認済みの隔離候補内に限定し、公開規則の採択・pre 統合・武装・配備は別判断のままとする。

## 責任の判定は別問題

最後の stage が eligible でなかったことだけで、当事者の無責任を確定してはならない。自分が前段を遅らせたことで後段の機会を失った場合には、前段の機会と全応答の義務を含む causal graph を検討する必要がある。逆に割当 DAA に空きがあっただけでも guilt にしない。合法な最遅応答を含む全分岐、実供給、費用、配送・収録前提と終了理由を採択するまでは、現行の service default 拒否を維持する。

新 marker は original-admission の全分岐予約、owner 非依存の公開 ingress、資格を持つ signer の生存性、物理供給を代替しない。後から現れた stage だけを予約しても、original acceptance での complete admission を完了したことにはならない。

## 互換性と採択条件

旧 key 0..12 / row 0..18 の順序・意味・payload を保存する。旧 marker 不在 snapshot に現在時刻を入れて「昔から eligible」と backfill しない。typed native transaction、carriage、journal、undo/replay、snapshot load と marker retention を同じ build で検査する。t12 identity や cap を repin しない。未知の帯域・費用はゼロにせず、marker の追加 work/IO/storage も profile に含める。

採択済み設計を変更する場合は [凍結手続き](../governance/design-freeze.md)に従い、新 RFC/ADR のレビューと明示的採択を要する。この Proposed 文書はその採択ではない。

## 検証済みの範囲

反例 1 件は、fresh core executable の native 回帰 28 件にも含まれる。同じ lock 下の既存 identity/G14/activation/release executables の再実行 26 件と合わせ、重複を除く選択結果は **54 passed / 0 failed / 0 ignored**。この反例 checkpoint では production Rust は未変更、marker runtime は未実装。コマンド、raw log、SHA-256 と重複の扱いは [試験記録](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#retained-stage-timing-counterexample)を参照。

`g14/door-design` の local ref `2a6ea1a4ff934635978144a84f8ec96351d2bdb8` の capacity scheduler と owned response formats も読み取り確認した。予約/待ち行列会計や WorkDebit receipt の accepted DAA は、Begin/Part 自体の受理時刻を与えない。third branch の統合・schema 採択を行ったという意味ではない。

## 隔離候補の実装状況

`e82921ee1` 後の候補で A 案の履歴 row、native prerequisite transition、journal、保持・復元の実装を追加し、選択した検証を実行した。公開採択ではない。最新の対象と残件は [RFC-0028 §8](../rfc/0028-rooted-court-response-eligibility-history.md#8-isolated-runtime-candidate-2026-10-21) を参照。今回の選択結果は **64 passed / 0 failed / 0 ignored**、non-test consensus check と生成器再生成一致も成功。前節の54件や targeted 2件を加算しない。全義務 graph・original admission・実 supply・物理校正・node 全体は未完で、実装済みと全体検証済みを混同しない。
