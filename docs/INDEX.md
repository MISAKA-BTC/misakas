# How MISAKA's design documentation is organised

MISAKA's design documentation has four types: **Spec**, **Design**, **RFC** and **ADR**. This page
says what each type is for, when to write which one, how a change moves through them, and where the
templates are. Two pages keep their existing roles. [`README.md`](README.md) lists the operator
documents, and [`architecture/overview.md`](architecture/overview.md) is the one-page map of the protocol.

## 概要(日本語)

- **なぜ分けるのか。** `docs/adr/` は 153 ファイル・約 3.5 MB あり、索引(`adr/README.md`)だけで
  145 KB ある。大きい ADR は 50〜80 KB あり、1 本の中に決定・規範ルール・理由・計測・監査記録・実装メモが
  混ざっている。そのため「今のルールは何か」を 1 か所で読めない。
- **4 種類に分ける。**
  - **Spec(`spec/`)**: 今、各ネットワークで何がルールか(MUST/SHOULD)。fence とその高さも書く。
    各節に出典の ADR と実装コード(ファイル・関数)を書く。歴史や理由は書かない(理由は 1 文まで)。
  - **Design(`design/`)**: なぜそのルールなのか。理由、退けた案、脅威モデル、計測、トレードオフ。
  - **RFC(`rfc/`)**: 議論中の提案。RFC-0001 は `rcore/fp-sampler` にあり、番号だけここで予約する。
  - **ADR(`adr/`)**: 短い決定記録(Context / Decision / Consequences / Links、1〜2 ページ)。
    番号とファイル名は変えないので、既存の参照は切れない。長い本文は Spec と Design へ移し、
    元の場所にはリンクを残す。置き換えられた ADR は先頭にそう書く。
- **流れ。** RFC → 承認 → ADR に決定を記録 → **同じ変更で** Spec を更新 → 理由は Design に残す。
- **PALW** は ADR-0144(使うつもりだった推論に払う:P1〜P7、§3 検証する/しないもの、§4 先に実行・後で精算)を
  骨格にして、`spec/palw/` に書き直す。
- **守ること。** 内容は消さずに移す(移さない段落は、理由を付けて「廃止」として一覧に載せる)。
  ADR とコードが食い違ったら、Spec にはコードの動作を書き、食い違いを一覧に記録する。
- **進め方。** Phase 1 はこのページ、テンプレート、[ADR 棚卸し](adr/INVENTORY.md)、
  [PALW spec の章立て](spec/palw/00-index.md)。Phase 2 は PALW の本文、Design、ADR の圧縮。
  Phase 3 は EVM・DNS-BFT・bridge・network。

## 1. The four types

| Type | Folder | Answers | Normative | Changes | Size |
| --- | --- | --- | --- | --- | --- |
| **Spec** | `spec/<domain>/` | What are the rules on each network today? | Yes (RFC 2119 keywords) | In the same change as the code or fence that changes the rule | As long as the rules need, with no history |
| **Design** | `design/<domain>/` | Why are the rules this way? | No | When the reasoning or a measurement changes. New sections are dated | One topic per file |
| **RFC** | `rfc/` | What do we propose to change? | No (proposal) | While under discussion. Frozen once decided | A proposal |
| **ADR** | `adr/` | What was decided, when, and what follows from it? | No (points at the Spec) | Never rewritten after acceptance. Only status banners and links are added | 1–2 pages |

The domains are `palw`, `evm`, `dns-bft`, `bridge`, `network` (base layer: post-quantum transactions
and identity, P2P isolation, IBD and pruning, the params identity) and `wallet` (RPC/SDK types, keys
and operator tooling).

**Supporting material** is not one of the four types, and it keeps its own folders:

| Material | Where | Rule |
| --- | --- | --- |
| Audit reports | `audit/` | Dated and never edited after publication. A finding that changes a rule goes through the path in §3 |
| Measurements and raw data | `evidence/` | Dated. Cited by Design documents |
| Runbooks and operator guides | `docs/*.md`, `wiki/` | Say how to operate a node, never what the rule is. They link to the Spec |
| Network history | `history/` | Flag days, fingerprints and rollouts of networks that have been retired |
| Protocol map | `architecture/overview.md` | One page. Each topic links to the Spec chapter that governs it |

## 2. Which one do I write?

| You are… | Write |
| --- | --- |
| proposing a rule, fence, parameter or wire-format change that needs discussion | an **RFC**. When it is accepted, follow the next row |
| changing a consensus rule, a fence, a network parameter or a wire format | an **ADR** (the decision) and a **Spec** edit (the rule), in the same change as the code. Add the reasoning to **Design** |
| fixing code that does not do what the Spec says | no ADR. If the Spec was wrong and the code was right, fix the Spec and record the divergence |
| arming a fence on a network, or choosing a height | a **Spec** edit (network parameters chapter). Add a short **ADR** only if you chose between alternatives (for example DAA 750 over 500) |
| reporting a measurement, benchmark or drill | a dated section in **Design**, with the commit and network. Raw data goes in `evidence/` |
| auditing | a report in `audit/`. A finding that changes a rule follows the second row |
| asking a question that decides nothing yet | an **RFC** |
| telling operators what to do | a runbook. Never the Spec |

## 3. Lifecycle

```
RFC-NNNN  Draft → Discussion ──accepted──► ADR-NNNN (Accepted) ──same change──► Spec updated
   │                                          │                                   (+ code, + fence)
   └── Rejected / Withdrawn (RFC kept)        └──► Design: reasoning, alternatives, measurements

Later:  ADR-MMMM "Supersedes ADR-NNNN (in whole | in part: D3)"
        → banner at the top of ADR-NNNN
        → Spec rewritten to the new rule. The old rule survives only as an activation row:
          "before DAA h: rule A; from DAA h: rule B"
```

| Type | Status values |
| --- | --- |
| RFC | Draft · Discussion · Accepted (→ ADR-NNNN) · Rejected · Withdrawn. A part can be frozen on its own, as RFC-0001 §A is |
| ADR | Proposed · Accepted · Superseded by ADR-MMMM (in whole, or in part with the clause named) · Withdrawn · Constitution (ADR-0144) |
| Spec rule | an **Activation** row per network: *not active* · *from DAA h (`Params::field`)* · *dormant (`None`)* · *refused by validation* |
| Design | no status. A section whose reasoning was overtaken is marked in place and kept |

### 3.1 Spec rules

- Use RFC 2119 keywords: MUST, MUST NOT, SHOULD, MAY.
- Give each rule an ID of the form `PALW-<chapter>-<n>`, for example `PALW-LC-12`. Assign the ID when
  the rule text is written. Never reuse it: a withdrawn rule keeps its ID with the text *Withdrawn
  (ADR-NNNN)*.
- Every section ends with three lines:
  - **Activation**: one row per network.
  - **Sources**: ADR numbers and clauses.
  - **Code**: paths and key functions.
- No history, and no rationale beyond one sentence. Link to Design for the reasoning.
- **The code is the truth for what the rule is.** If an ADR and the code disagree, the Spec describes
  the code, and the disagreement is listed in the domain's `divergences.md` until an ADR or a fix
  resolves it.
- Cover mainnet and the live testnet (today testnet-12). Cover retired networks and devnet only
  where they differ, and mark them as such.

### 3.2 Design documents

- One topic per file. Each file answers: the problem, the design in one paragraph (with links to the
  ADRs), the alternatives rejected, the threat model, measurements (dated, with commit and network),
  trade-offs and residual risk, and open questions.
- Reasoning that has been overtaken stays in the file under **History**, marked with the ADR that
  overtook it.

### 3.3 RFCs

- Take numbers from [`rfc/README.md`](rfc/README.md). **RFC-0001 is reserved.** It lives on
  `rcore/fp-sampler` and lands with that branch.
- An RFC carries the normative text it would add to the Spec, and its activation plan (fence name,
  networks, heights).

### 3.4 ADRs

- Use [`templates/adr.md`](templates/adr.md): Context, Decision, Consequences, Links. Aim for one to
  two pages.
- Take the next number from [`adr/README.md`](adr/README.md), not from `ls`. Numbers have
  collided before, and the index records each collision.
- **Existing ADRs keep their numbers and filenames.** When a long body is moved out, the ADR keeps
  its decision and gains a banner that says where each part went (see §4). A superseded ADR gets a
  banner at the top that names the ADR superseding it. Its body stays as the record.
- The per-file plan is in [`adr/INVENTORY.md`](adr/INVENTORY.md).

## 4. Moving content without losing it

The restructure has one rule: **every paragraph that leaves an ADR lands in a Spec, Design or audit
document, or is listed as obsolete with the reason.** To slim an ADR:

1. Move its normative clauses into the Spec chapter as rules, with the ADR in **Sources**.
2. Move its body **verbatim** into `design/<domain>/archive/NNNN-<slug>.md` (only relative link
   targets are rewritten so they still resolve). The archive holds the rationale, rejected
   alternatives, measurements, review logs and security amendments exactly as written. The topic's
   Design document (`design/<domain>/<topic>.md`) summarises the reasoning and links each archive
   file.
3. Leave the ADR as Context / Decision / Consequences / Links. At the top, add a banner:

   ```
   > **Body moved (YYYY-MM-DD).** Normative rules → spec/palw/08-verification.md §8.3;
   > the full text as written → design/palw/archive/NNNN-<slug>.md; reasoning summarised in
   > design/palw/verification.md.
   ```

4. Superseded ADRs are not slimmed. They get this banner at the top:

   ```
   > **Superseded by ADR-MMMM (in whole | in part: D2, D5), YYYY-MM-DD.** Kept as a record; not
   > normative. The current rule is spec/palw/NN-….md §N.M.
   ```

5. `STATUS-AUDIT-*` files move to `audit/`. A short stub stays at the old path with a link, so
   existing links keep working.

## 5. Where things live

```
docs/
  INDEX.md                  this page
  README.md                 map of operator documents
  architecture/overview.md  one-page protocol map
  spec/                     normative: spec/README.md, spec/palw/00-index.md, …
  design/                   reasoning: design/README.md, design/palw/…
  rfc/                      proposals: rfc/README.md (numbers)
  adr/                      decision records: adr/README.md (index, supersede map, numbers),
                            adr/INVENTORY.md (restructure plan, one row per file)
  audit/                    audit reports
  evidence/                 measurements and raw data
  templates/                spec.md · design.md · rfc.md · adr.md
```

Documents at the top of `docs/` that are design material, such as `misaka-evm-design-v0.4.md` and
`misaka-palw-slash-protocol-design-v0.1.md`, stay where they are until their domain's phase
classifies them.

## 6. Status of the restructure

| Phase | Scope | State |
| --- | --- | --- |
| 1 | This page, the templates, the ADR inventory, the PALW spec chapter skeletons | done (2026-09-27) |
| 2 | Write the PALW spec chapters from the ADRs and the code, list the divergences, move the rationale into `design/palw/`, slim the PALW ADRs, move `STATUS-AUDIT-*` into `audit/` | done (2026-09-27); ADR-0160 waits for int-6 |
| 3 | The same for EVM, DNS-BFT, bridge, network and wallet | after Phase 2 |
