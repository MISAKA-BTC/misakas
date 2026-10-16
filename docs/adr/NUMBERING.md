# Numbering — duplicate and missing numbers

**このファイルは記録だけです。番号の付け替え・ファイル名の変更はしていません(ユーザーの方針: ADR・RFC は renumber しない、rename しない)。どの文書にどの番号を残すかは、ユーザーが決めます。**

過去の衝突(ADR-0003/0008/0021/0022/0035/0039–0048/0045/0054/0055/0059/0060/0069/0077–0079/0102 など)と、そのとき採った付け替えの規則は、[HISTORY.md](HISTORY.md) の "Number hygiene — collisions, renumberings and dangling references" に残っています。下の表は、**いま同じ番号に 2 つのファイルがあり、その表に載っていないもの**です。

## 1. 同じ番号に 2 つのファイルがあるもの

| 番号 | ファイル A | ファイル B | わかっていること | 決定 |
| --- | --- | --- | --- | --- |
| ADR-0026 | [0026-current-palw-testnet11-architecture.md](0026-current-palw-testnet11-architecture.md) — 「Current PALW architecture on testnet-11」、Status: Accepted。git 上の初出 2026-10-07(`1ca871a8f`) | [0026-palw-v2-runtime-separated-verification.md](0026-palw-v2-runtime-separated-verification.md) — 「PALW v2 verification architecture — borrow Ambient's shape, strengthen the proof」、Status: Accepted (architecture)。git 上の初出 2026-08-15(`8a7adcbb89`) | [HISTORY.md](HISTORY.md) の Superseded 表と ADR-0021 の行が「0026」として指すのは B(`palw-v2-runtime-separated-verification`)。ADR-0038 は B を L1 machinery へ promote し、ADR-0053 は B の thesis を全面的に復元した。A は testnet-11 当時の現行構成の記録。コード・文書で `ADR-0026` と書かれた箇所が A と B のどちらを指すかは、本 reorg では調べていない(`*.rs` で 8 ファイルが `ADR-0026` を含む) | **ユーザーが決める** |
| ADR-0173 | [0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md](0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) — 「A seat holds a root, not a class」、Status: PROPOSED 2026-10-04(lane MU)。git 上の初出 2026-10-04(`967dfeb37`) | [0173-public-verifier-dispute-completeness-is-misaka-purpose.md](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) — 「MISAKA's purpose: public-verifier dispute completeness」、Status: Accepted as the project's design mandate, 2026-10-07。git 上の初出 2026-10-07(`1ca871a8f`) | 多くの文書(各 ADR・RFC の冒頭の「Mission alignment, 2026-10-07」、RFC README、`docs/adr/evidence/0173-*`)が「ADR-0173」と書くのは B(purpose)。A は seat/possession の ADR で、`adr-0162-0178-implementation-status-2026-10-15.md` では「0173 (seat root)」と「0173 (purpose)」と書き分けられている。`*.rs` で 31 ファイルが `ADR-0173` を含み、どちらを指すかは本 reorg では調べていない | **ユーザーが決める** |

## 2. `docs/spec/palw/` で `18-` で始まる章が 3 つ

| ファイル | 内容 |
| --- | --- |
| [../spec/palw/18-inference-surface.md](../spec/palw/18-inference-surface.md) | RFC-0001: the inference-surface rules |
| [../spec/palw/18-layer-sharded-panels.md](../spec/palw/18-layer-sharded-panels.md) | RFC-0006: layer-sharded panels |
| [../spec/palw/18-verification-certificates.md](../spec/palw/18-verification-certificates.md) | RFC-0007: verification vertices, the audit mesh, capped onboarding |

[../spec/README.md](../spec/README.md) の Numbering note (2026-10-03) は、この 3 つが並行して書かれて偶然同じ番号になったこと、DAA-5,300 の flag day を配備した後に **18(inference surface)、19(layer-sharded panels)、20(verification certificates)** へ付け替え、全ての「spec 18」参照も更新する予定であること、それまでは「spec 18」はその横にある RFC(RFC-0001、RFC-0006、RFC-0007)の章を指すことを述べている。**この reorg では付け替えていない。** コード・文書の「spec 18」参照は `*.rs` / `*.py` / `*.sh` / `*.toml` に 25 行ある(本 reorg 時点)。DAA-5,300 の flag day は `consensus/core/src/config/params.rs` の `PALW_T12_INT11_FLAG_DAY_DAA = Some(5_300)` として testnet-12 に設定されており、[../design/palw/t12-daa14000-release-plan-2026-10-15.md](../design/palw/t12-daa14000-release-plan-2026-10-15.md) によれば testnet-12 の DAA は既に 10,000 を超えているので、付け替えの前提条件は満たされているように見える。ただし付け替えるかどうか・いつかはユーザーが決める。

## 3. 参考: 同じ番号に companion ファイルがある RFC(重複ではなく補足と見られるもの)

| 番号 | ファイル | 備考 |
| --- | --- | --- |
| RFC-0010 | [../rfc/0010-permissionless-palw-panel-and-claim-completion.md](../rfc/0010-permissionless-palw-panel-and-claim-completion.md)、[../rfc/0010-dormant-implementation.md](../rfc/0010-dormant-implementation.md) | 後者の題名は「RFC-0010 dormant implementation and permissionlessness audit」。旧 RFC README の RFC-0010 の行は両方を指していた |
| RFC-0012 | [../rfc/0012-palw-only-consensus-and-native-evm-settlement.md](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md)、[../rfc/0012-dormant-implementation.md](../rfc/0012-dormant-implementation.md) | 後者の題名は「RFC-0012 dormant implementation」。旧 RFC README の RFC-0012 の行は前者だけを指していた(新しい [../rfc/README.md](../rfc/README.md) は両方を載せる) |
| RFC-0016 | [../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) | ファイルは 1 つだが、旧 RFC README の表に RFC-0016 の行が 2 つあった(新しい行と、旧 Draft の行)。新しい README は新しい行だけを載せ、旧表は [../rfc/HISTORY.md](../rfc/HISTORY.md) に残してある |

## 4. 番号は飛んでいるが、ファイルがないもの(ADR)

ADR の 0001–0180 のうち、ファイルがない番号は 0048、0104、0113、0156–0159、0164。判明している事情:

- 0048: live の系列では未使用([HISTORY.md](HISTORY.md) の Number hygiene)。
- 0104: ADR-0102 の項が、"close-cut ADR" がここへ来たときに取ると述べている番号。
- 0164: ファイルはなく、コードの "ADR-0164" コメントは ADR-0167 にあたる(`../design/palw/adr-0162-0178-implementation-status-2026-10-15.md`)。
- 0113、0156–0159: この reorg では経緯を確認できていない。**unknown — check HISTORY.md と git log。**

次の ADR の番号は 0181(ADR-0180 が現在の最大)。ただし、他の branch で先に使われている可能性があるので、ls ではなく [README.md](README.md) と [HISTORY.md](HISTORY.md) を見てください。
