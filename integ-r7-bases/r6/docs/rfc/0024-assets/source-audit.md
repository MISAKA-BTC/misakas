# 出典監査 — 添付 ZIP と提案の分離

作成日: 2026-10-19（Asia/Tokyo）。これは source の限定的な文書・コード読解記録であり、コンパイル、全体監査、実ノード E2E の報告ではない。

## 対象 snapshot

`misakas-pre(2).zip` の SHA-256 は `136f95326e9831f7778b9f8e73028c12b1f8f8ae00468b22770f8e54bec4c512`。ZIP は作業ディレクトリへ展開して読んだ。git の live branch や最新 commit を参照していない。

`.rs/.md/.toml/.py/.sh` の検索では `ADR-0191`、`b6e8a3d16`、`PALW_TIR_FRAME_COST_RECORDS_V1` は見つからなかった。これは当該識別子の snapshot 内の検索結果であり、他 branch の実装不存在を意味しない。検索結果は `repository-scan.json`。

<a id="u01"></a>

## U01 — 会話中のユーザー提示値（本 ZIP の測定結果とは別）

ユーザーは、32K/2M・tile=16、2M=131,072 tile/5往復、中心部分 1,464 DAA、帯域 125,000 B/DAA で 1,473/1,857（32K）、1,657/2,041（2M）、帯域 1/10 で 3,012/6,048 と 3,196/6,232 を提示した。左右は terminal 読込を含まない/含む値で、式からの導出・非実測との説明。最終 tile 約 135.6 KB、1 位置化後約 12 KB は見込み。既存目標 3,120 に対し、今回の依頼は 3,200 DAA。

これらを引き継ぐが、正式な ADR-0191 の式を再現・検証した値とは扱わない。

## 出典 ID

<a id="s01"></a>

### S01 — 原則: 外部 1 人、計算不正と DA/default の区別

**`docs/PRINCIPLES.md` L123–L141**

ファイル SHA-256: `a5eeb56f27d3dcabce0cbb5e4ffbc097af04b6eece5dd6ac07e991a361776ac6`

```text
  123 | ## 3. 不正発覚後の客観的裁定
  124 | 
  125 | 承認済み検査で不一致が発見された場合、その後の有罪認定をPanelの投票や主観的判断に委ねてはならない。
  126 | 
  127 | 対象は、すべての報酬対象class・kernel・verification planが対象とする計算、job、input、output、state、およびData Availabilityの違反である。producerとPanel全員が共謀する場合でも、次が成り立たなければならない。
  128 | 
  129 | - Panel外の1人の適格なpublic bonded verifierが、producerの秘密状態やPanelの協力なしに、公開・認証済みmaterialから独立に検査できる。
  130 | - その検証者が、不正箇所を有界に局所化できる。
  131 | - その検証者が、客観的な裁定まで進められる。
  132 | 
  133 | 裁定の結果は、次のとおり扱う。
  134 | 
  135 | - 計算不正が立証された場合、決定論的なexact courtにより、客観的な有罪判定、claimの無効化、および責任追及へ到達する。
  136 | - 義務付けられた証拠が開示されない場合、計算不正を推定して有罪とはしない。客観的なDA/defaultと、それに対応する責任を判定する。
  137 | - 有効な異議申立てが継続中の場合、定められた有界の手続きが完了するまで、不正claimを経済的Finalへ逃してはならない。
  138 | - 不正が立証されなかった場合、正当なclaimや検証者が恣意的に処罰されないよう、棄却・担保返還・費用配分を決定論的に処理する。
  139 | 
  140 | 不正の発見は確率的であっても、発見された具体的な不正の有罪判定は確率的であってはならない。
  141 | 
```

<a id="s02"></a>

### S02 — history dissection の原理・wire の値上限

**`consensus/core/src/palw_tir_dissect_v1.rs` L1–L46**

ファイル SHA-256: `14a0400f3f5b7ee7ac3deaba3b2431593e07d56deca932b2fe161fae82e6b383`

```text
    1 | //! **RFC-0002 Phase F, step F7: the generic history dissection of an IR class**
    2 | //! (`docs/design/palw/tir/phase-f-integration.md` §2.8; spec 04b §9.5).
    3 | //!
    4 | //! A committed tile whose cone reduces over the history — a `ReduceSum` or `ReduceMax` along an `H`
    5 | //! axis, a `MatMul` contracting `H` — costs `O(H)` to recompute, so past some context it cannot be
    6 | //! closed in one move. ADR-0082 dissected the one such site the legacy classes have (the fused
    7 | //! attention, three reductions: the row max, the exponent sum, the value sum); an IR program says
    8 | //! which reductions it has, so the dissection is over EVERY reduction over `H` in the disputed
    9 | //! tile's cone, `r_1 … r_n` (node order), each one's partials folding exactly — `Σ` for a sum or a
   10 | //! contraction, `max` for a maximum — because every integer sum and maximum is exact:
   11 | //!
   12 | //! 1. **The root claim** (the responder, when the ladder is terminal on a dissected leaf): for each
   13 | //!    `r_i`, the elements of it the tile's evaluation demands and its totals over `[0, H)`. It is
   14 | //!    admitted only if the tile evaluated with every `r_i` SUPPLIED from the claim reproduces the
   15 | //!    committed tile (the "finalize", spec 04b §9.5), and every total lies in `r_i`'s proven interval.
   16 | //! 2. **Rounds.** The disputed range of history tiles (`h_tile` positions each) is cut at pinned
   17 | //!    points into `k` children ([`crate::palw_attn_dissect::palw_attn_child_ranges_v1`]); the
   18 | //!    responder claims every `r_i`'s partials over each child, computed against the ROOT's totals
   19 | //!    wherever an `H`-local node consumes another reduction (the softmax's `exp(s − m*)`); the court
   20 | //!    checks the fold and the intervals; the challenger names a child.
   21 | //! 3. **The bottom** (a `TirDissection` close): one tile. The court evaluates every `r_i` over that
   22 | //!    tile's positions only, the others supplied from the root, and compares with the claim.
   23 | //!
   24 | //! This module holds the objects, the phase and the site; the court's two evaluations (the finalize
   25 | //! and the bottom) are [`crate::palw_tir_court_v1`]'s.
   26 | 
   27 | use borsh::{BorshDeserialize, BorshSerialize};
   28 | 
   29 | use crate::Hash64;
   30 | use crate::palw_attn_dissect::{PalwAttnDissectError, palw_attn_arity_is_legal_v1, palw_attn_child_ranges_v1, palw_kary_rounds_v1};
   31 | use crate::palw_bisect::{PalwBisectNoShowV1, PalwBisectPartyV1, PalwBisectTurnV1};
   32 | use crate::palw_tir_court_v1::PalwTirConeRefutationV1;
   33 | use crate::palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepSpaceV1};
   34 | use misaka_palw_tir::demand::{DemandContext, history_length_v1, reduces_over_h_v1};
   35 | use misaka_palw_tir::interval::Interval;
   36 | use misaka_palw_tir::program::{Block, TirProgramV1};
   37 | use misaka_palw_tir::{Prim, Ref};
   38 | 
   39 | /// Wire version of every object in this module.
   40 | pub const PALW_TIR_DISSECT_OBJECT_VERSION_V1: u16 = 1;
   41 | /// The most reductions over `H` one dissected cone may carry (a softmax attention has three).
   42 | pub const PALW_TIR_DISSECT_MAX_REDUCTIONS: usize = 16;
   43 | /// The most demanded elements of all reductions together one claim may carry.
   44 | pub const PALW_TIR_DISSECT_MAX_VALUES: usize = 4096;
   45 | 
   46 | /// How a reduction's partials fold.
```

<a id="s03"></a>

### S03 — exact fold と root の totals を供給する評価

**`consensus/core/src/palw_tir_dissect_v1.rs` L675–L707**

ファイル SHA-256: `14a0400f3f5b7ee7ac3deaba3b2431593e07d56deca932b2fe161fae82e6b383`

```text
  675 |         for child in &msg.children {
  676 |             if child.partials.len() != self.elements.len()
  677 |                 || child.partials.iter().zip(&self.elements).any(|(v, e)| v.len() != e.len())
  678 |             {
  679 |                 return Err(PalwTirDissectError::Shape("a child claims one value per demanded element"));
  680 |             }
  681 |             check_bounds(&self.bounds, child)?;
  682 |         }
  683 |         for (i, fold) in self.folds.iter().enumerate() {
  684 |             for e in 0..self.elements[i].len() {
  685 |                 let mut values = msg.children.iter().map(|c| c.partials[i][e]);
  686 |                 let folded = match fold {
  687 |                     PalwTirFoldV1::Sum => values.try_fold(0i128, |a, v| a.checked_add(v)),
  688 |                     PalwTirFoldV1::Max => values.max(),
  689 |                 };
  690 |                 let claimed = self.claim.partials[i][e];
  691 |                 match folded {
  692 |                     Some(f) if f == claimed => {}
  693 |                     other => {
  694 |                         return Err(PalwTirDissectError::DoesNotFold {
  695 |                             reduction: i,
  696 |                             element: e,
  697 |                             claimed,
  698 |                             folded: other.unwrap_or(i128::MAX),
  699 |                         });
  700 |                     }
  701 |                 }
  702 |             }
  703 |         }
  704 |         self.pending = msg.children.clone();
  705 |         self.last_deadline_daa = accepted_daa.saturating_add(w_round);
  706 |         self.turn = PalwBisectTurnV1::AwaitVerdict;
  707 |         Ok(())
```

**`consensus/core/src/palw_tir_court_v1.rs` L1967–L2008**

ファイル SHA-256: `9fb5297c7c23f697c09c72d37754c637ca5fb82be364fb6f5caa608b94127e94`

```text
 1967 |     site: &PalwTirDissectSiteV1,
 1968 |     binding: &PalwTirStepBindingV1,
 1969 |     phase: &PalwTirDissectPhaseV1,
 1970 |     range: (usize, usize),
 1971 |     store: &dyn PalwTirEvidenceStoreV1,
 1972 |     rules: &PalwTirCourtRulesV1,
 1973 |     mut budget: Option<&mut BuilderBudget>,
 1974 | ) -> Result<(PalwTirRangeClaimV1, Requests), PalwTirEvidenceErrorV1> {
 1975 |     let mut source = store_source(v, &binding.job_context, inventory, store, phase.leaf_index(), rules);
 1976 |     source.read_budget = budget.as_ref().and_then(|budget| budget.source_read_budget());
 1977 |     source.supplied_ctx = Some(site.ctx);
 1978 |     let mut partials = Vec::with_capacity(site.reductions.len());
 1979 |     for (i, node) in site.reductions.iter().enumerate() {
 1980 |         if let Some(budget) = budget.as_deref_mut() {
 1981 |             budget.targets(phase.elements()[i].len() as u64).map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))?;
 1982 |         }
 1983 |         source.supplied = supplied_values(&site.reductions, phase.elements(), phase.root(), Some(*node));
 1984 |         let supplied: Vec<u16> = site.reductions.iter().copied().filter(|r| r != node).collect();
 1985 |         let elements: Vec<usize> = phase.elements()[i].iter().map(|e| *e as usize).collect();
 1986 |         let request =
 1987 |             DemandRangeRequest { ctx: site.ctx, target: *node, elements: &elements, supplied: &supplied, range: Some(range) };
 1988 |         let limits = budget.as_ref().map_or(rules.limits, |budget| budget.remaining(&rules.limits));
 1989 |         let (values, work) = eval_demanded_range(&v.space.program, &v.space.info, &request, &mut source, &limits)
 1990 |             .map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))?;
 1991 |         if let Some(budget) = budget.as_deref_mut() {
 1992 |             budget.charge(work).map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))?;
 1993 |         }
 1994 |         partials.push(values);
 1995 |     }
 1996 |     Ok((PalwTirRangeClaimV1 { partials }, source.used))
 1997 | }
 1998 | 
 1999 | /// **Build the responder's round**: every child of the disputed range, its partials over the
 2000 | /// child's positions.
 2001 | pub fn build_tir_dissect_round_v1(
 2002 |     binding: &PalwTirStepBindingV1,
 2003 |     phase: &PalwTirDissectPhaseV1,
 2004 |     tile_positions: u32,
 2005 |     store: &dyn PalwTirEvidenceStoreV1,
 2006 |     rules: &PalwTirCourtRulesV1,
 2007 | ) -> Result<PalwTirDissectRoundV1, PalwTirEvidenceErrorV1> {
 2008 |     build_tir_dissect_round_with_budget(binding, phase, tile_positions, store, rules, None)
```

<a id="s04"></a>

### S04 — history operand の由来と first-divergence

**`consensus/core/src/palw_tir_court_v1.rs` L11–L47**

ファイル SHA-256: `9fb5297c7c23f697c09c72d37754c637ca5fb82be364fb6f5caa608b94127e94`

```text
   11 | //!
   12 | //! * **step leaves** of the executor's own step tree (one range-proved row, ascending leaf index):
   13 | //!   other commit points of the position, the previous occurrence's carry-outs, the committed row
   14 | //!   of every history position, history tiles, and `Fixed`-state checkpoints;
   15 | //! * **artifact leaves** of the class's TIR inventory (F3's rule), against its `artifact_root`;
   16 | //! * the **prompt ids** (the whole list against a flat digest, or one opening per tile against a
   17 | //!   Merkle root) and the **generated ids** (pinned through the class's logits scheme).
   18 | //!
   19 | //! **Where each value comes from** is fixed, so the operand set is a function of the disputed leaf:
   20 | //!
   21 | //! * a `Fixed` state at the start of position `p` is zero at `p = 0`, the checkpoint leaf of `p − 1`
   22 | //!   when `p ≡ 0 (mod C)`, and otherwise its writer's output at `p − 1`, replayed (design §2.6);
   23 | //! * a history row appended at `r`, read at `p`, comes from the history tile holding it when that
   24 | //!   tile ended before `p` (`⌊r / h_tile⌋ · h_tile + h_tile − 1 < p`), and otherwise from the row's
   25 | //!   own commit point at `r` (design §2.6);
   26 | //! * a checkpoint leaf holds the state after its position (the writer's output there — a leaf when
   27 | //!   the writer is committed — or, for an unwritten instance, the value carried to it); a history
   28 | //!   tile holds the committed rows it concatenates.
   29 | //!
   30 | //! By the step space's invariant (design D7) every such unit is a leaf that PRECEDES the disputed
   31 | //! one, so the court never reads a leaf the bisection has not already agreed on; a request for one
   32 | //! that does not precede it is refused, never answered (and, as every refusal of a source is, it
   33 | //! fails the evaluation `Missing`, spec 04b §9.4).
   34 | //!
   35 | //! **The canonical operand set is exactly what the evaluation asked for.** Every request is
   36 | //! recorded; a unit asked for and not carried, or carried and not asked for, is
   37 | //! `InputSetNotCanonical` — refused, nobody slashed — so a refutation of one leaf is one object,
   38 | //! whoever builds it. [`build_tir_cone_refutation_v1`] builds it by running the same evaluation over
   39 | //! a builder's own retained units ([`PalwTirEvidenceStoreV1`]); that is the API a node's evidence
   40 | //! builder calls (Phase F step F9).
   41 | //!
   42 | //! **PALW-TIR-33.** Every lane of the disputed leaf and of every carried step leaf must lie inside
   43 | //! the interval the class's program PROVES for it (spec 04b §7): a commit point's node interval, a
   44 | //! written state's writer interval (zero for an unwritten one), a history tile's row interval. A lane
   45 | //! outside is a value no execution can produce — the executor committed it, so the executor is
   46 | //! convicted ([`PalwStepFaultV1::TirValueOutsideProvenInterval`]), whichever leaf was disputed. After
   47 | //! this check nothing the evaluation reads can leave its interval, so, by the soundness of the range
```

**`consensus/core/src/palw_tir_court_v1.rs` L791–L816**

ファイル SHA-256: `9fb5297c7c23f697c09c72d37754c637ca5fb82be364fb6f5caa608b94127e94`

```text
  791 | 
  792 |     fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
  793 |         self.source_read()?;
  794 |         let h_tile = self.space.layout.h_tile;
  795 |         let tile_start = row_pos - row_pos % h_tile;
  796 |         let tile_end = tile_start + (h_tile - 1);
  797 |         if tile_end < pos {
  798 |             let Some(k) = self.space.hist_instance_index(state, layer) else {
  799 |                 return tir_err(TirErrorKind::Malformed, format!("history {state} layer {layer:?} has no tiles"));
  800 |             };
  801 |             let inst = self.space.hist_instances()[k];
  802 |             let sub = index as u64 / inst.tile_lanes as u64;
  803 |             let first_lane = sub * inst.tile_lanes as u64;
  804 |             let row_lanes = (inst.elements - first_lane).min(inst.tile_lanes as u64);
  805 |             let coord = self.reserved_coord(tile_end, (self.space.fixed_instances().len() + k) as u32, sub);
  806 |             let lane_index = (row_pos - tile_start) as u64 * row_lanes + (index as u64 - first_lane);
  807 |             self.step_lane(coord, inst.dtype, lane_index as usize)
  808 |         } else {
  809 |             let Some((ctx, node)) = hist_row_node_v1(&self.space.program, state, layer, row_pos) else {
  810 |                 return tir_err(TirErrorKind::Malformed, format!("history {state} layer {layer:?} appends nothing"));
  811 |             };
  812 |             self.node(ctx, node, index)
  813 |         }
  814 |     }
  815 | 
  816 |     fn token(&mut self, pos: u32) -> TirResult<u32> {
```

**`consensus/core/src/palw_tir_step_v1.rs` L10–L30**

ファイル SHA-256: `2686018d4b84e7b84ed06fd0e4c3978079b33e39bfebc14886400bf0f6a523f9`

```text
   10 | //! 1. **commit points** — every occurrence of the schedule (`pre`, each layer's block, `post`), and in
   11 | //!    each every node with `commit = true`, in node order, cut into tiles of the layout's `tile_len`
   12 | //!    over its output at `H = min(a + 1, W)`. `node_slot` is the node's unrolled index (spec 04b
   13 | //!    §3.3), so a slot names one node for everybody. `post` runs only where logits are consumed —
   14 | //!    `a ≥ P − 1` — which is exact because `post` writes no state (enforced below);
   15 | //! 2. **`Fixed` state checkpoints** — when `(a + 1) % C == 0`, each `Fixed` state instance (state
   16 | //!    order, then layer) after this position's write, in tiles of its `state_tiles` lanes, at the
   17 | //!    reserved slots past the program's last;
   18 | //! 3. **history tiles** — when `(a + 1) % h_tile == 0`, each `Hist` state instance's last `h_tile`
   19 | //!    rows, one leaf per sub-row of `state_tiles` lanes, at the next reserved slots.
   20 | //!
   21 | //! **The invariant this order buys** (design D7): every leaf is adjudicated from leaves that precede
   22 | //! it — a cone reads lower slots of its position and rows, tiles and checkpoints of earlier ones; a
   23 | //! checkpoint replays from the previous checkpoint over the positions between; a history tile
   24 | //! concatenates rows already committed. The bisection narrows to the FIRST divergent leaf, whose
   25 | //! predecessors all agree with the honest execution, so a forged checkpoint or history tile is
   26 | //! itself that leaf — there is no separate checkpoint leg for an IR class and no second court.
   27 | //!
   28 | //! **Lanes are 4 bytes** (PALW-TIR-5): `i8`/`i16`/`i32` values as little-endian `i32`, `idx` as
   29 | //! little-endian `u32`. A leaf is the legacy [`PalwStepTileLeafV1`], hashed with the IR class id
   30 | //! where a legacy leaf hashes its shape profile id.
```

<a id="s05"></a>

### S05 — step leaf の payload 全体を hash する規則

**`consensus/core/src/palw_step_leg.rs` L1246–L1279**

ファイル SHA-256: `1902704612524837848ccb86cfdcc0e3a5d065afedc9723578a0a471fce84fe7`

```text
 1246 | /// One committed node-output tile: exact canonical f32-LE bytes at pinned coordinates.
 1247 | #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
 1248 | pub struct PalwStepTileLeafV1 {
 1249 |     pub version: u16,
 1250 |     pub coord: PalwStepCoordinateV1,
 1251 |     pub value_count: u32,
 1252 |     /// `4 × value_count` little-endian f32 bytes.
 1253 |     pub values_le: Vec<u8>,
 1254 | }
 1255 | 
 1256 | /// **The step tile leaf, bound to the context its COORDINATE belongs to** (RFC-0001 §2.6 stage 2b): the prefix context for a
 1257 | /// prefill position below an inherited prefix, the job context for everything else — and for every version-2 context exactly
 1258 | /// [`step_tile_leaf_hash_v1`] over the job context, so no existing leaf moves. Every site that hashes a step tile from a context
 1259 | /// calls this and not the bare function.
 1260 | pub fn step_tile_leaf_hash_ctx_v1(ctx: &PalwJobContextV2, shape_profile_hash: &Hash64, leaf: &PalwStepTileLeafV1) -> Hash64 {
 1261 |     step_tile_leaf_hash_v1(&ctx.leaf_context_hash_v1(leaf.coord.call_index, leaf.coord.position), shape_profile_hash, leaf)
 1262 | }
 1263 | 
 1264 | pub fn step_tile_leaf_hash_v1(job_context_hash: &Hash64, shape_profile_hash: &Hash64, leaf: &PalwStepTileLeafV1) -> Hash64 {
 1265 |     let mut w = Writer::new();
 1266 |     w.u16(leaf.version);
 1267 |     w.hash64(job_context_hash);
 1268 |     w.hash64(shape_profile_hash);
 1269 |     w.u32(leaf.coord.call_index);
 1270 |     w.u32(leaf.coord.node_slot);
 1271 |     w.u32(leaf.coord.position);
 1272 |     w.u32(leaf.coord.tile_index);
 1273 |     w.u32(leaf.value_count);
 1274 |     w.bytes(&leaf.values_le);
 1275 |     w.keyed64(PALW_STEP_LEG_DOMAIN_TILE_LEAF)
 1276 | }
 1277 | 
 1278 | /// One committed KV aux chunk: the F16-LE cache rows of `position_count` consecutive
 1279 | /// positions for one (attention layer, kv head, K|V) — ADR-0030 §3's openability series.
```

<a id="s06"></a>

### S06 — original frame の料金 SUM と期限導出

**`consensus/core/src/palw_tir_public_allocation_v1.rs` L712–L752**

ファイル SHA-256: `f98103deec92feaa6c40dd2bb44b43d76d1fa63604845eaa40689027ea6ba5ff`

```text
  712 |     let mut sum = acquisition
  713 |         .envelope
  714 |         .checked_sum_v1(preparation_work.envelope)?
  715 |         .checked_sum_v1(owner_work.envelope)?
  716 |         .checked_sum_v1(preparation.envelope)?
  717 |         .checked_sum_v1(executor.envelope)?
  718 |         .checked_sum_v1(terminal.envelope)?
  719 |         .checked_sum_v1(controls)?
  720 |         .checked_sum_v1(owner_validation)?;
  721 |     if let Some(flat) = flat_trace {
  722 |         sum = sum.checked_sum_v1(flat)?;
  723 |     }
  724 |     let semantic_resource_plan_id = semantic.as_ref().map(|quote| quote.resource_plan_id);
  725 |     let maximum_semantic_history_records = semantic.as_ref().map_or(0, |quote| quote.maximum_records);
  726 |     let preparation_until_daa = add(geometry.accepted_daa, preparation.preparation_daa)?;
  727 |     let open = add(preparation_until_daa, rules.config.acquisition_daa)?;
  728 |     // Cold work and Ready are already before owner opening. They remain in the
  729 |     // monetary SUM, but do NOT allocate a second cold-preparation deadline after
  730 |     // every request. Per-owner WorkDebit/response inclusion stays after open.
  731 |     let pre_inclusion = add(preparation_work.envelope.inclusion_daa, preparation.envelope.inclusion_daa)?;
  732 |     let post_inclusion = sum.inclusion_daa.checked_sub(pre_inclusion).ok_or("native preparation inclusion is not inside SUM")?;
  733 |     let generic_end = add(open, post_inclusion.max(1))?;
  734 |     let stream = if let Some(quote) = &mut semantic {
  735 |         let shift = generic_end.checked_sub(geometry.accepted_daa).ok_or("native semantic offset underflow")?;
  736 |         rebase_new_semantic_plan(&mut quote.plan, shift)?;
  737 |         sum = sum.checked_sum_v1(quote.plan.terms.plan)?;
  738 |         quote.plan.terms.stream_until_daa
  739 |     } else {
  740 |         generic_end
  741 |     };
  742 |     let collect = add(stream, rules.config.collection_daa)?;
  743 |     let material = add(collect, rules.config.reorg_retention_daa)?;
  744 |     let terms = PalwRenderingClaimTermsV1 {
  745 |         accepted_daa: geometry.accepted_daa,
  746 |         open_until_daa: open,
  747 |         stream_until_daa: stream,
  748 |         collect_until_daa: collect,
  749 |         material_until_daa: material,
  750 |         bond_exit_after_daa: material,
  751 |         plan: sum,
  752 |     };
```

**`consensus/core/src/palw_tir_public_owned_v1.rs` L590–L605**

ファイル SHA-256: `6f4fedf76041b4e9179085ff96a575afd085a7506c9a47cb7fb880781f65dadb`

```text
  590 |     let upper = acquisition.maximum_custodians;
  591 |     let prep_moves = product(upper, maximum_slices_per_actor.checked_add(1).ok_or("TIR Ready moves overflow")?)?;
  592 |     let moves = sum(&[prep_moves, upper])?;
  593 |     let row = sum(&[4096, product(upper, 1024)?])?;
  594 |     let bytes = product(moves, sum(&[16_384, product(row, 8)?])?)?;
  595 |     let parser = product(bytes, 32)?;
  596 |     let margin = acquisition.config.prices.inclusion_margin_daa;
  597 |     // **The preparation door is one preparer's serial time**: each of its slices is one WorkDebit (`capture_debit_v1` refuses
  598 |     // a grant past `maximum_slices_per_actor`), then its Ready, one inclusion margin each. The preparers work at once, each on
  599 |     // its own lane; the door does not add every actor's carriers end to end (that sum is what the SUM's rent pays for).
  600 |     let preparation_daa = sum(&[
  601 |         acquisition.config.acquisition_daa,
  602 |         product(maximum_slices_per_actor.checked_add(1).ok_or("TIR Ready moves overflow")?, margin)?,
  603 |     ])?;
  604 |     let envelope = PalwRenderingResourcePlanV1 {
  605 |         prefix_tokens: 0,
```

<a id="s07"></a>

### S07 — G14 arming records は空・不正 record の拒否

**`consensus/core/src/palw_g14_gates_v1.rs` L247–L282**

ファイル SHA-256: `70e6918848b0b95c650a8e64b77a1c3181cd7cdf551f0a45872e377d391a958f`

```text
  247 | /// the review's digest. Pinned only by a reviewed change; the binary's list is [`PALW_G14_ARMING_RECORDS_V1`].
  248 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  249 | pub struct PalwG14ArmingRecordV1 {
  250 |     pub version: u16,
  251 |     pub fence: PalwG14GatedFenceV1,
  252 |     pub genesis: Hash64,
  253 |     pub activation_daa: u64,
  254 |     pub source_commit: [u8; 20],
  255 |     pub evidence: &'static [[u8; 32]],
  256 |     pub review: [u8; 32],
  257 | }
  258 | 
  259 | /// **The pinned records — EMPTY.** Every gated fence armed at a real height on a real network is refused.
  260 | pub const PALW_G14_ARMING_RECORDS_V1: &[PalwG14ArmingRecordV1] = &[];
  261 | 
  262 | /// Why a record does not authorize an arming.
  263 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  264 | pub enum PalwG14ArmingRefusalV1 {
  265 |     /// No pinned record names this fence, network and height.
  266 |     NoRecord,
  267 |     /// The record's version is not this binary's.
  268 |     Version,
  269 |     /// The record names a zero source commit.
  270 |     SourceCommit,
  271 |     /// The evidence list does not have one digest per requirement.
  272 |     EvidenceCount { expected: usize, got: usize },
  273 |     /// Evidence `i` is zero, repeated, or equal to the review.
  274 |     Evidence(usize),
  275 |     /// The review digest is zero.
  276 |     Review,
  277 | }
  278 | 
  279 | impl PalwG14ArmingRefusalV1 {
  280 |     pub fn reason(self) -> &'static str {
  281 |         match self {
  282 |             Self::NoRecord => "no pinned arm-readiness record names this fence, network and height",
```

<a id="s08"></a>

### S08 — custodian の WorkDebit と永続化

**`kaspad/src/palw_tir_public_custodian.rs` L1–L32**

ファイル SHA-256: `0c5b4cf573af8e84d7ab538658594afd34f8dd486b284e0a4bbe607b7bcb26db`

```text
    1 | //! **G14 R3/R8: the custodian preparation of an ORIGINAL TIR frame, node side** (node policy; a child module of
    2 | //! `palw_fraud_filer`, dormant below `palw_tir_public_conviction_v1` — there the accepted-state reader answers `Ok(false)`, the
    3 | //! book's live cases are dropped and nothing is queued, so a node below the fence behaves byte for byte as before).
    4 | //!
    5 | //! Two literal preparers use the ONE claim-wide preparation lane the producer prepaid at d0 (`Key::TirPreparation`):
    6 | //!
    7 | //! - **the producer** — a custodian from d0. Its pending duty is read off the accepted index
    8 | //!   (`staged_tir_preparation_duties_after_v1`); once its capture is prepared it files its own `PreparationReady`.
    9 | //! - **a drawn seat that signed `Valid` (late grant)** — it prepares AHEAD on the same paid lane while it is still only drawn
   10 | //!   (`capture_debit_v1` admits the producer and the drawn panel), and grants raw custody only once its capture is sealed, in
   11 | //!   ONE atomic object (`TirPublicGrantReadyV1`: its ordinary Valid credit + the raw custody grant + Ready). The grant therefore
   12 | //!   never carries a cold-preparation deadline it could miss: no liability exists until the prepared capture does.
   13 | //!
   14 | //! **One slice, one paid permit.** For each slice the backend quotes the work (`slice_quote_v1`: the cold reader's phases, then
   15 | //! the prepared writer's publication phase); the consensus work book prepares the EXACT debit on the accepted state; the signed
   16 | //! `WorkDebit` control rides the court queue (its own lane, [`PALW_TIR_PUBLIC_DEBIT_ROUND_V1`]); and only once the debit is
   17 | //! ACCEPTED does `confirm_v1` mint the permit that funds the slice (`tir_acquisition_driver`'s `prepare_claim_*_slice_v1`). A lost
   18 | //! carrier re-sends the same exact debit (`pending_action_v1`); a slice is never run on an unaccepted debit.
   19 | //!
   20 | //! **Restart, reorg and GC (G14 R8).** The sealed descriptor of every prepared capture is journaled
   21 | //! (`state_dir/palw-tir-public-custodian.v2`, written to a temporary file then renamed), so a restart re-files a lost Ready without
   22 | //! preparing again; each live sealed capture is re-authenticated once per process by a full reopen
   23 | //! (`reopen_tir_prepared_capture_v1`) under its quoted reopen RAM; and a sealed capture is retired only once its material horizon
   24 | //! is IRREVERSIBLY past (the pruning point's DAA), never on an accepted-but-reversible clock. Physical slices are charged to the
   25 | //! journal BEFORE they run, against the lane's finite allowance `maximum_slices_per_actor × (maximum_restarts + 1)`: neither a
   26 | //! restart nor a reorg that rolls the canonical counter back refills it. An unsealed preparation restarts from its first slice
   27 | //! on a fresh reader; the work book marks the first debit after a restart (`restart: true`).
   28 | //!
   29 | //! **Not covered here (recorded in docs/design/palw/integration/g14-objective-conviction-remaining-2026-10-15.md):** answering outside owners'
   30 | //! demands from the prepared capture (the Begin/Part/Finish responder), measured completion within the fixed preparation clock at
   31 | //! the largest profile, sweeping orphan `prepared-*` directories the journal never recorded, and re-sealing a corrupted capture.
   32 | #![allow(dead_code)] // the panel loop calls the tick; the journal helpers are exercised by the tests below.
```

<a id="s09"></a>

### S09 — 既存の annex・DA demand・first differing node

**`docs/design/palw/tir/evidence-transport-scope.md` L24–L60**

ファイル SHA-256: `dd16ea0b0a6c456143384cdac702e62d8e203c0f0489e1349a7d12a7c7fdaf3e`

```text
   24 | ## What a challenger actually needs
   25 | 
   26 | The one-move case names the FIRST leaf at which the accused parts from the challenger's own execution,
   27 | so every leaf before it is the challenger's own. `TirEvidenceV1::challenger` already builds a close
   28 | from the challenger's own leaves plus ONE accused leaf and the committed trace. Per door:
   29 | 
   30 | - cone close / F7 named leaf: the accused's leaf `L` (its preimage, at most one tile) and its opening under
   31 |   the step root (about 1.4 KB for a 2^21-leaf ladder);
   32 | - F7 bottom: the history operands of one `h_tile`, all before `L`, so the challenger's own; plus leaf `L`,
   33 |   which is already on chain in the accused's root claim (`PalwTirRootClaimV1::finalize`);
   34 | - logits and decode-token doors: one row's trace tile and its openings, plus the generated ids (the
   35 |   `TirEvent` disclosure's shape).
   36 | 
   37 | That is kilobytes per accusation, not the capture.
   38 | 
   39 | ## Options
   40 | 
   41 | - **A. Raise the cap or stream captures.** 40–625 MB per claim per seat. ADR-0084 measured 748 MB × 5 peers
   42 |   tearing down the p2p flows. Rejected.
   43 | - **B. A served IR annex (ADR-0084's shape, consensus-inert).** On request, the producer serves one small
   44 |   authenticated object per `(claim, leaf)`: leaf `L`'s preimage and opening, the row's trace tile and
   45 |   openings, and the ids. A seat pulls it once its replay parts from the claim, then builds its accusation
   46 |   over `TirEvidenceV1::challenger`. Node-only, no fence. But a lying producer can simply not serve it.
   47 | - **C. A DA-demanded step leaf (consensus).** A new IR DA unit `TirStepLeaf { index }`: the accused must
   48 |   disclose leaf `index` (preimage and opening) inside `W_disclose`, or it is defaulted, which is a
   49 |   conviction under the DA court's rules. This closes B's withholding hole. It needs a fence (a new DA
   50 |   unit variant, acceptance and fold) and the held-DA carrier rules.
   51 | - **D. Read the chain for F7.** The challenger's bottom takes leaf `L` from the accused's own root claim
   52 |   on chain (as the attention route reads its filings, `attn_root_filings_from_chain_v1`). Node-only.
   53 | 
   54 | ## Recommendation
   55 | 
   56 | **B as the fast path, C as the enforcement, and D for F7's bottom.** A seat pulls the annex first. If
   57 | the producer does not serve it before the accusation's due date, the seat files the `TirStepLeaf` demand
   58 | and accuses from the disclosure; the producer's silence is the default. Everything the seat files is
   59 | built over the existing `TirEvidenceV1::challenger` store, so no new court object is needed. The
   60 | one-move accusation and every close stay exactly as the chain adjudicates them today.
```

**`docs/design/palw/tir/evidence-transport-scope.md` L72–L102**

ファイル SHA-256: `dd16ea0b0a6c456143384cdac702e62d8e203c0f0489e1349a7d12a7c7fdaf3e`

```text
   72 |   levels below it, or the leaf nodes when nearer (≤ 1,024 hashes, 64 KiB) — and its opening; the chain folds
   73 |   the frontier by the tree's own rule (pairs, an odd last node promoted) and walks it to the committed step
   74 |   root. A session's units are fixed when it opens, so "eight nodes along a path" cannot be named up front;
   75 |   the frontier carries the same information for whichever path the seat then takes. The seat names the
   76 |   root, then the first frontier node its own tree disagrees with, then that leaf: ⌈h / 10⌉ node sessions
   77 |   and one leaf session — four up to a 2^30-leaf execution, inside one seat's budget of four. (It was eight
   78 |   levels, ≤ 256 hashes; real-size classes commit 2^24–2^29 leaves — Llama-3.1-70B at 2,048 positions and
   79 |   64-lane tiles 450 M — so the depth went to ten: a 1,024-hash frontier, 64 KiB, with its opening, binding
   80 |   and signature still one 100,000-byte carrier.)
   81 | - **At the class's ladder** (2026-09-29): every IR answer's binding — the release's `TirEvent` included —
   82 |   and every step demand's unit is checked at the claim's class's ladder past the fence (the court's step
   83 |   ladder the held regime rides, 2^40 on testnet-12), where the release checked them at 2^22 leaves: a claim
   84 |   of an execution past 2^22 could answer NO demand and defaulted, whatever it did. Admission refuses a class
   85 |   whose canonical job commits more than one seat reaches (`PALW_TIR_DA_SEAT_REACH_LEAVES_V1`, 2^30). Until
   86 |   the fence, declare-layout refuses a canonical job past 2^22 and a producer does not mine one.
   87 | - **The close from the disclosure alone.** The descent takes the FIRST differing node at every level, so
   88 |   every accused node wholly before the disputed leaf `L` is the seat's own; the nodes on `L`'s path follow
   89 |   from the disclosed leaf and its opening, and so do their right siblings. Every opening and run the cone of
   90 |   `L` reads names only such nodes, so the seat builds the cone close from its own execution plus `L`'s
   91 |   disclosure even when the accused garbled every leaf after `L` (it may not rebuild the accused's tree as
   92 |   "mine with one leaf replaced"). Tests: `palw_tir_da_step_fold` (one seat convicts a withholding liar that
   93 |   answers every demand; an inconsistent node answer is refused and the silence defaults the claim),
   94 |   `palw_tir_step_node` (a 2^22-leaf descent in four sessions).
   95 | 
   96 | ## Estimate
   97 | 
   98 | | part | who | size |
   99 | |---|---|---|
  100 | | B: annex object, serve/pull on the gossip lane, panel wiring, tests | tir-node | ~1 day |
  101 | | C: `TirStepLeaf` unit, acceptance + fold + fence (Phase F); node demand/answer (tir-node) | both | ~2–3 days, rides the next t12 fence |
  102 | | D: F7 bottom from the on-chain root claim | tir-node | ~0.5 day |
```

<a id="s10"></a>

### S10 — 再取得・literal slots・carrier・料金と owner 上限

**`consensus/core/src/palw_tir_public_owned_v1.rs` L325–L344**

ファイル SHA-256: `6f4fedf76041b4e9179085ff96a575afd085a7506c9a47cb7fb880781f65dadb`

```text
  325 |     // **G14 frame allocation fix (2026-10-16, the IR FP lane's gap pin): each lane counted once, and the claim-wide total is
  326 |     // the SUM of the lanes.** Before this fix the plan overflowed `u64` at testnet-12's Panel (custodian upper 6) for every
  327 |     // corpus class, and at the upper testnet-12 actually resolves for a registered TIR class (257: `palw_tir_shard_v1` is
  328 |     // armed, so a stratified Panel of up to 256 seats plus the producer) it overflowed by orders of magnitude:
  329 |     //   1. the per-actor work was multiplied by `slots × passes` (every actor's preparations) and then again by `slots` in
  330 |     //      `maximum` — a lane's quantity is `passes` (its own replay and its restarts), not every actor's;
  331 |     //   2. every lane was priced as the outside owner's (which reads every slot's answers, so it grows with the slots) and
  332 |     //      the total was that times the slots — a per-custodian PRODUCT that is a SUM: one owner lane plus one literal
  333 |     //      custodian lane (its own slot's answers) per custodian. The claim-wide preparation, whose preparers read no
  334 |     //      other slot's answers, holds custodian lanes only (`preparation_lanes_v1`);
  335 |     //   3. the acquisition's `maximum_stream_bytes` — a STORAGE/carriage envelope (eight raw copies, a row journal per move, the
  336 |     //      carrier framing) — was read as the bytes of answers to re-read, and multiplied by `passes` although the acquisition's
  337 |     //      turns already include every reacquisition (`maximum_turns = fresh × (restarts + 1)`). The answers' bytes are the raw
  338 |     //      bodies: every turn × every answering slot × one unit;
  339 |     //   4. the 16 KiB WorkDebit slices were counted over ALL IO — the replay's RAM traffic included — and then multiplied by
  340 |     //      `passes` again. A WorkDebit is one logical unit (a leaf, a tile row, a 1024-ID chunk, a replay position: the walk
  341 |     //      below) or one 16 KiB slice of DURABLE bytes (the answers read, the prepared generation published each pass);
  342 |     //      RAM traffic is debited in the IO budget, not sliced (the node's readers and writers debit exactly so).
  343 |     // Then the full-frame validation per accepting action (`original_controls`: frame ∝ custodians, actions ∝ the lanes) is
  344 |     // quadratic in the custodian upper, as its own comment states, instead of cubic.
```

**`consensus/core/src/palw_tir_public_owned_v1.rs` L1137–L1187**

ファイル SHA-256: `6f4fedf76041b4e9179085ff96a575afd085a7506c9a47cb7fb880781f65dadb`

```text
 1137 |             atom = atom.max(request_bound(source, request)?.0);
 1138 |         }
 1139 |         // Leaf-by-leaf canonical operands fit without assuming a backend meter's
 1140 |         // operational target limit. Trace allowance covers every tile and its
 1141 |         // interior localization, not a caller-chosen subset or u8 tile ceiling.
 1142 |         let fresh = exact(
 1143 |             u128::from(n)
 1144 |                 + u128::from(source.step_descent_units_v1())
 1145 |                 + if source.supports_tiled_trace_v1() { u128::from(d) * (u128::from(tiles) * 3 + 2) + 1 } else { 0 },
 1146 |         )?;
 1147 |         let reacquire = exact(u128::from(fresh) * u128::from(config.prices.reorg_replay_moves))?;
 1148 |         let turns = fresh.checked_add(reacquire).ok_or("TIR acquisition turn budget overflow")?;
 1149 |         let slots = maximum_custodians.checked_add(1).ok_or("TIR literal slots overflow")?;
 1150 |         let attempts = product(turns, slots)?;
 1151 |         let moves = sum(&[product(attempts, atom.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) + 3)?, product(turns, 2)?, 1])?;
 1152 |         let raw = product(attempts, atom)?;
 1153 |         let row = sum(&[product(slots, 1536)?, 16_384])?;
 1154 |         let carrier = product(moves, PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64 + 16_384)?;
 1155 |         // Small mutable metadata only; body chunks are separate immutable rows.
 1156 |         // Eight raw copies cover the immutable rooted row, delta210 old/new
 1157 |         // framing, retained journal and bounded encode/reorg scratch. This is
 1158 |         // not a bound on unrelated global BTreeMap clone/root amplification.
 1159 |         let raw_storage = product(raw, 8)?;
 1160 |         let disk = sum(&[
 1161 |             raw_storage,
 1162 |             product(moves, 1024)?,
 1163 |             product(attempts, 1024 + 64 * 64)?,
 1164 |             product(product(moves, row)?, 2)?,
 1165 |             crate::palw_prefix_history_v1::PALW_STAGED_CACHE_LEASE_STORAGE_BYTES_V1,
 1166 |         ])?;
 1167 |         let work = sum(&[product(sum(&[raw_storage, carrier, product(moves, row)?])?, 32)?, product(moves, 65_536)?])?;
 1168 |         let maximum_unit_bytes = atom.checked_mul(n.min(256)).ok_or("TIR run body upper bound overflow")?;
 1169 |         let envelope = PalwRenderingResourcePlanV1 {
 1170 |             prefix_tokens: 0,
 1171 |             vocab: source.vocab_v1(),
 1172 |             maximum_stream_bytes: sum(&[disk, carrier])?,
 1173 |             maximum_stream_leaves: attempts,
 1174 |             maximum_eog_bytes: 0,
 1175 |             maximum_moves: moves,
 1176 |             maximum_parser_steps: work,
 1177 |             maximum_blake_blocks: work.div_ceil(128),
 1178 |             inclusion_daa: sum(&[product(moves, config.prices.inclusion_margin_daa)?, product(turns, config.acquisition_daa)?])?,
 1179 |             rent_sompi: sum(&[
 1180 |                 product(sum(&[disk, carrier])?, config.prices.rent_per_stream_byte)?,
 1181 |                 product(row, config.prices.rent_per_row_byte)?,
 1182 |                 product(work, config.prices.rent_per_parser_step)?,
 1183 |             ])?,
 1184 |             deposit_sompi: product(moves, config.prices.deposit_per_move)?,
 1185 |             row_bytes: row,
 1186 |             resident_ram_bytes: sum(&[product(row, 8)?, product(maximum_unit_bytes, 16)?, 262_144])?,
 1187 |         };
```

**`consensus/core/src/palw_tir_public_claim_v1.rs` L128–L148**

ファイル SHA-256: `4d53e8e159484c8b50c48434e4b07c7162bc27b3db91ed761e47f3c35dcd8266`

```text
  128 |         self.custody.contract_v1()
  129 |     }
  130 |     pub fn original_custody_v1(&self) -> &PalwTirPublicOriginalCustodyV1 {
  131 |         &self.custody
  132 |     }
  133 |     /// **G14 R1/R7 owner capacity (Lead default 2026-10-15, PENDING USER REVIEW):** at most this many outside owner acquisitions
  134 |     /// per ORIGINAL frame over its life. Excess owners are refused at Open, before any custodian duty exists.
  135 |     pub const MAXIMUM_OWNERS_V1: u64 = PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1;
  136 |     /// The custodian load this frame can impose, derived from the plan's per-owner acquisition capacity and the owner cap: every
  137 |     /// liable custodian answers at most `owners × maximum_turns` demands of at most `maximum_unit_bytes` each. Nodes provision
  138 |     /// for this bound; the fold never lets a further owner raise it.
  139 |     pub fn custodian_load_bound_v1(&self) -> PalwTirPublicCustodianLoadV1 {
  140 |         let owners = PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1;
  141 |         let turns = owners.saturating_mul(self.plan.acquisition.maximum_turns);
  142 |         PalwTirPublicCustodianLoadV1 {
  143 |             owners,
  144 |             demands_per_custodian: turns,
  145 |             answer_bytes_per_custodian: turns.saturating_mul(self.plan.acquisition.maximum_unit_bytes),
  146 |         }
  147 |     }
  148 |     /// **G14 R4: what one public conviction filing burns** — the frame's own native terminal envelope rent (the price of one
```

**`consensus/core/src/palw_tir_public_claim_v1.rs` L273–L279**

ファイル SHA-256: `4d53e8e159484c8b50c48434e4b07c7162bc27b3db91ed761e47f3c35dcd8266`

```text
  273 | /// **K_owner — outside owners per ORIGINAL TIR frame** (Lead default 2026-10-15, pending user review). Each owner pays its own
  274 | /// rent and deposit (never the producer's prepaid pool); the cap bounds what the frame's liable custodians must answer. Known
  275 | /// residual: a colluder can pre-empt the slots by paying K owners' rent and deposit; slot recycling for idle owners is a
  276 | /// follow-up decision (recorded in the remaining-work document).
  277 | pub const PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1: u64 = 8;
  278 | 
  279 | /// The bound on what one liable custodian of a frame can be asked to answer (see [`PalwTirPublicClaimV1::custodian_load_bound_v1`]).
```

<a id="s11"></a>

### S11 — 設計凍結・パラメータと新 RFC/ADR の手続き

**`docs/governance/design-freeze.md` L1–L40**

ファイル SHA-256: `aaf370f3d7b68df12574318139ba0d42bd1299a9e1630288b3466574bdde3527`

```text
    1 | # 設計凍結と変更手続き
    2 | 
    3 | **Decision: Accepted — 今回のユーザー指示を記録。整理日 2026-10-17。**  
    4 | [PRINCIPLES §11](../PRINCIPLES.md#design-freeze) / [ADR-0181](../adr/0181-design-freeze-and-controlled-change.md) / [RFC-0021](../rfc/0021-design-freeze-and-documentation-governance.md)。
    5 | 
    6 | > **設計の変数・パラメーター以外は原則凍結し、通常は変更禁止とする。ただし、重大な問題が確認された場合には、新規 RFC と新規 ADR の両方により、レビューと明示的な採択を経て変更できる。**
    7 | 
    8 | ## 凍結対象と継続できる作業
    9 | 
   10 | 採用済みの原則、合意・状態遷移、責任分界、モデル制度、認証・裁定、予算会計、データ・wire の意味、不変条件を通常変更しません。凍結は完成・検証・有効化を意味せず、未採択 Draft 全文を採用したり旧案を復活させたりしません。
   11 | 
   12 | 既存設計への実装、意味不変の不具合修正・性能改善、試験・監査・文書整理は継続できます。「修正」「最適化」と呼んでも、挙動・保証・責任を変える場合は設計変更として扱います。
   13 | 
   14 | ## パラメーター調整
   15 | 
   16 | 明示された調整変数は、採用済みの構造と不変条件を維持した範囲で調整できます。名前・単位・対象・旧新値・調整範囲・導出・測定・影響・試験・承認者・適用条件を [台帳](parameter-register.md) に記録します。未承認値を本番値に読み替えず、範囲が未定なら推測で埋めません。既存の固定値を再調整する場合も設計責任者の明示的な再承認を要します。
   17 | 
   18 | 数値・フラグの変更でも、認証数や待機をゼロにして検証を外す、二階層・超線形配分・登録総枠を実質廃止する、責任保持を短絡する、Model ID / root / wire の意味を変える場合は通常調整ではありません。重大問題時の設計変更手続きへ移します。
   19 | 
   20 | ## 重大な問題
   21 | 
   22 | 合意安全性の破綻、継続的停止・回復不能、資金・担保・報酬の重大な損失や二重計上、認証・裁定が成立しない再現可能な重大欠陥などです。影響・根拠・再現方法、既存設計内の修正やパラメーター調整では解決できない理由を示します。
   23 | 
   24 | 「別方式が好み」「流行」「実装しやすい」「文書を短くしたい」は単独では例外理由になりません。懸念の報告・調査は可能ですが、それだけで凍結を解除しません。
   25 | 
   26 | ## 例外変更の手順
   27 | 
   28 | 1. 問題・深刻度・対象 network / release / commit / 仕様節と、再現または検証可能な証拠を記録する。
   29 | 2. **新規 RFC** に、問題、代替案、最小変更、影響、不変条件、互換性・移行、試験、失敗時対応を書く。
   30 | 3. **新規 ADR** に、RFC のレビュー、採用理由、不採用案、承認者、残余リスク、旧 ADR / RFC の置換節を書く。相互参照し、両方の明示的採択が揃うまで現行設計を維持する。
   31 | 4. Spec、PRINCIPLES の該当節、台帳、状態・置換関係を整合させる。PRINCIPLES 自体の改定も設計責任者（現行のユーザー承認主体）の明示的承認を要する。
   32 | 5. 実装・試験・独立レビュー・移行・リリースを確認し、必要な **有効化を別途承認**する。文書やコードの merge だけで fence・高さ・wire・fingerprint・稼働設定を変えない。
   33 | 
   34 | 緊急の停止・ロールバックは既存の承認済み運用手順内で行い、恒久的設計変更の抜け道にしません。承認権限の移譲は今回行いません。
   35 | 
   36 | ## 適用時期と履歴
   37 | 
   38 | 明示的に指示された [画像・動画・モデル改善の延期](post-mainnet-scope.md) は、リリース対象と有効化の管理であり、技術方式の差し替えではありません。重大欠陥の存在を仮定した決定ではありません。後日の再開でも凍結された設計を無断で変更できません。
   39 | 
   40 | 過去の「再審しない」「恒久」「変更不可」は通常の変更禁止として維持し、重大問題時の例外手続きは本規程に統一します。既存の禁止方式や技術的決定を今回撤回しません。番号・日付の大きい文書が自動的に上位になるわけではなく、PRINCIPLES と明示的な採択・置換関係に従います。コードとの違いは不整合として記録します。
```

<a id="s12"></a>

### S12 — original collection door・Final hold と未解決責任

**`consensus/core/src/palw_tir_public_fold_v1.rs` L889–L936**

ファイル SHA-256: `9092c26113c4636ee8ea7bbcc3550f1648c0134e7cbeccd59b723d42c20531ef`

```text
  889 |     }
  890 |     Err(refused("TIR public output conviction never reached the original authenticated EOF"))
  891 | }
  892 | 
  893 | /// **G14 R4/R6: one public bonded verifier convicts an ORIGINAL frame from authenticated public material alone.**
  894 | ///
  895 | /// 1. The filer is any non-liable Active above-floor bond ([`conviction_filer_v1`]); the window is the frame's own original
  896 | ///    collection door `[accepted, collect_until)` — fixed at d0, never extended — inside which the claim cannot reach `Final`
  897 | ///    (`palw_claim_final_floor_v1` floors it at `collect_until`), so no Final weight, receipt spend, escrow, Gate C credit or
  898 | ///    coinbase leg can escape before this door closes.
  899 | /// 2. The evidence is adjudicated natively ([`crate::palw_tir_objective_v1`]): the registered program filled from state, the
  900 | ///    original class/artifact/execution/trace/job pinned from the claim, the court rules and the step ladder the native ones,
  901 | ///    the proof body bounded by the producer-prepaid terminal witness ceiling. `PredicateHolds` / `OutputHolds` refuse the object
  902 | ///    (nothing changes; it is neither guilt nor a certificate of honesty). Errors (`Need`, missing material, oversize, EOF short)
  903 | ///    refuse it too — none of them is read as a finding.
  904 | /// 3. The sanction reuses the established economic funnels, never a new tariff: every credited Valid custodian's S4 (its native
  905 | ///    lock plus the S3/S4 action tier, keyed `palw_false_valid_offence_id_v2(seat, claim)`, so once only across this route, the
  906 | ///    kind-3 route and the DA route), then the producer through `convict_by_court_verdict_v1` (void a live claim `CourtFraud`,
  907 | ///    or reverse a `Final` one — which revokes its RFC-0018 Gate C weight and frontier/settlement credit by the one predicate —
  908 | ///    and record `CourtConviction` once, the filer's reporter reward, by-claim round-right forfeiture and ADR-0160's intent-class
  909 | ///    funnel). An execution-proving fault also forfeits every minted right by ROOT, so a sibling claim sharing the convicted
  910 | ///    execution (and an EXEC root's dependent slices, through `void_claim`) loses its standing too.
  911 | /// 4. Every liable bond must still hold its original exit hold (the frame's `staged_holds`), so its collateral is locked when the
  912 | ///    conviction opens and the debit is actually collectible (`conviction_collected_v1`); a liable bond without it refuses the
  913 | ///    filing instead of recording a fictitious collection.
  914 | fn apply_conviction_v1(
  915 |     b: &mut TransitionBuilder<'_>,
  916 |     ctx: &PalwBlockContextV2,
  917 |     frame: &Frame,
  918 |     filer: PalwBondKeyV2,
  919 |     evidence: &crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
  920 | ) -> Result<(), PalwStateV2Error> {
  921 |     use crate::palw_tir_objective_v1::{
  922 |         PalwTirObjectiveOutcomeV1 as Arith, PalwTirOutputFaultV1 as OutFault, PalwTirOutputOutcomeV1 as Out,
  923 |     };
  924 |     use crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1 as E;
  925 |     let claim_id = frame.plan_v1().original_claim;
  926 |     let terms = frame.plan_v1().terms;
  927 |     if ctx.daa_score < terms.accepted_daa || ctx.daa_score >= terms.collect_until_daa {
  928 |         return Err(refused("TIR public conviction is outside the frame's original collection door"));
  929 |     }
  930 |     conviction_filer_v1(b, frame, filer)?;
  931 |     let claim = b.state.claim(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
  932 |     let producer = frame.custody_v1().pledge_v1().producer;
  933 |     if claim.bond != producer || claim.execution_root != frame.original_v1().execution_root {
  934 |         return Err(refused("TIR public conviction target changes the original claim's producer or execution"));
  935 |     }
  936 |     if b.state.consumed_offences.contains_key(&palw_court_conviction_offence_id_v1(&producer.0, &claim_id))
```

**`docs/rfc/0023-unresolved-claim-liability-and-independent-progress.md` L25–L38**

ファイル SHA-256: `b7733389cb0462db4d3ce99def2c95d819c78178d61be0d3daaf6daf02449ab3`

```text
   25 | ## 3. 段階 1 — 現行 frontier を保ち、足止め時間に上限を置く(次回 fence)
   26 | 
   27 | 1. **claim ごとの未解決期限。** `未解決でいられる期限 ≤ 受理 DAA + L_class`。`L_class` は検証・資料取得・court・必要な収録待ちを含む。受理時に court 予約容量・期限内の処理順・再抽選と追加告発による延長の上限と整合しなければ、その claim を報酬対象として受理しない(既存の bounded admission・detection policy・retention の規則を使う)。新しい告発で責任時計を最初からやり直さない。
   28 | 2. **期限到達の扱い。** 期限到達は不正の証明ではない。fraud slash は証明された不正と明確な応答義務違反(適格 default)にだけ行う。期限で credit を与えない場合も、係争中の担保・証拠は解放しない。
   29 | 3. **測定。** 次を別々に記録する:B の Final、Gate C 認証、報酬保留の解除、支払い出力の作成、その取引の `settled = true`、native の safe/finalized。「出力ができた」と「確定した」をまとめない。
   30 | 4. **継続攻撃。** 1 件ごとの期限が有限でも、別の古い claim が次々と足止めを引き継ぐ場合の定常遅れを測る(資料応答の遅延、正当な court、虚偽告発、再抽選、容量飽和、遅延 claim の継続投入を原因別に)。
   31 | 5. **期限の異なる claim。** A と B の責任期間が異なる場合の退役 floor(退役時の floor 更新は `oldest_unresolved` を再確認しない)、late conviction による後退、その後の pruning・IBD 復元を 1 組の試験にする。
   32 | 
   33 | 段階 1 が保証するのは「未解決 claim 由来の遅れの上限」だけで、認証済みの仕事が生まれない期間やネットワークの停止まで frontier の前進を保証しない。
   34 | 
   35 | ## 4. 段階 2 — 未解決 claim を局所管理する(別の休眠 fence)
   36 | 
   37 | 共通 DAG は維持し、次の 3 つを分けて管理する。
   38 | 
```

<a id="s13"></a>

### S13 — canonical 16 分岐の recurrence

**`consensus/core/src/palw_attn_dissect.rs` L214–L262**

ファイル SHA-256: `3a8ddd74ee29c73bca44008a25655cebd99ed8979d832c74747f39aa6023955a`

```text
  214 | pub fn palw_attn_child_ranges_v1(first: u64, count: u64, arity: u8) -> Result<Vec<(u64, u64)>, PalwAttnDissectError> {
  215 |     if !palw_attn_arity_is_legal_v1(arity) {
  216 |         return Err(PalwAttnDissectError::ArityOutOfRange { got: arity });
  217 |     }
  218 |     if count <= 1 {
  219 |         return Ok(Vec::new());
  220 |     }
  221 |     let width = count.div_ceil(arity as u64);
  222 |     let mut out = Vec::with_capacity(arity as usize);
  223 |     let mut start = 0u64;
  224 |     while start < count {
  225 |         let len = width.min(count - start);
  226 |         out.push((first + start, len));
  227 |         start += len;
  228 |     }
  229 |     Ok(out)
  230 | }
  231 | 
  232 | /// **How many rounds a k-ary search over `space` items takes**: the number of times the pinned
  233 | /// cut ([`palw_attn_child_ranges_v1`]) must be applied before one item remains. Computed by the
  234 | /// cut's own recurrence rather than by a logarithm, so the two cannot disagree; `None` for an
  235 | /// illegal arity. `space ≤ 1` is zero rounds.
  236 | pub fn palw_kary_rounds_v1(space: u64, arity: u8) -> Option<u32> {
  237 |     if !palw_attn_arity_is_legal_v1(arity) {
  238 |         return None;
  239 |     }
  240 |     let mut count = space;
  241 |     let mut rounds = 0u32;
  242 |     while count > 1 {
  243 |         count = count.div_ceil(arity as u64);
  244 |         rounds += 1;
  245 |     }
  246 |     Some(rounds)
  247 | }
  248 | 
  249 | /// The rounds a dissection over `history_positions` positions takes at `tile` positions a tile.
  250 | pub fn palw_attn_dissection_rounds_v1(history_positions: u64, tile: u32, arity: u8) -> Option<u32> {
  251 |     if tile == 0 {
  252 |         return None;
  253 |     }
  254 |     palw_kary_rounds_v1(history_positions.div_ceil(tile as u64), arity)
  255 | }
  256 | 
  257 | /// **What one round's disclosure weighs**, counted the way `arithmetic_close_bytes_v2` counts:
  258 | /// `arity` children of `(4 + 8 + 8 × lanes)` payload bytes plus the object's version and length
  259 | /// prefixes. The per-move number the court's window arithmetic and the close ceiling both read.
  260 | pub fn palw_attn_dissect_move_bytes_v1(arity: u8, lane_count: usize) -> u64 {
  261 |     let per_child = 4u64 + 8 + 8 * lane_count as u64;
  262 |     // version (2) + vec length prefix (4) + per child: the fields plus the v_acc length prefix (4).
```

<a id="s14"></a>

### S14 — 既存 public source request と strict bounded decode

**`consensus/core/src/palw_tir_public_source_v1.rs` L1–L77**

ファイル SHA-256: `56844daf1592284692d2127c7fb9a84d4386806e4ab5d8a7ec5202204b060eb3`

```text
    1 | //! Original-domain, bounded TIR public acquisition primitives. This module does
    2 | //! not admit a court, impose a custody obligation, or allocate a clock/budget.
    3 | //! Those are native paid-sidecar responsibilities. Flat trace EOF and Float32
    4 | //! trace acquisition are deliberately not supplied by the tiled trace route.
    5 | use crate::palw_step_leg::{PalwStepOpeningV1, PalwStepRangeOpeningV1, PalwStepTileLeafV1};
    6 | use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
    7 | use borsh::{BorshDeserialize, BorshSerialize};
    8 | use std::io::{self, Read};
    9 | 
   10 | pub const PALW_TIR_PUBLIC_FRONTIER_HASHES_V1: usize = 1024;
   11 | pub const PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1: u32 = 1024;
   12 | pub const PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1: u32 = crate::palw_tir_court_v1::PALW_TIR_STEP_RUN_MAX_LEAVES_V1;
   13 | const PATH: usize = 64;
   14 | const RUN: usize = PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1 as usize;
   15 | const LEAF_BYTES: usize = crate::palw_step::PALW_STEP_MAX_TILE_LEN as usize * 4;
   16 | const TILE_LANES: usize = crate::palw_step_refute::PALW_LOGITS_TILE_LANES;
   17 | const DEPTH: u8 = crate::palw_tir_court_v1::PALW_TIR_STEP_NODE_DEPTH_V1;
   18 | 
   19 | #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
   20 | pub enum PalwTirPublicRequestV1 {
   21 |     StepNode {
   22 |         level: u8,
   23 |         index: u64,
   24 |     },
   25 |     StepLeaf {
   26 |         index: u64,
   27 |     },
   28 |     StepRun {
   29 |         first: u64,
   30 |         count: u32,
   31 |     },
   32 |     /// Interior rows-tree node only. RowNode(0)'s whole vocabulary is never used.
   33 |     RowsNode {
   34 |         level: u8,
   35 |         index: u64,
   36 |     },
   37 |     TileNode {
   38 |         row: u32,
   39 |         level: u8,
   40 |         index: u64,
   41 |     },
   42 |     TraceTile {
   43 |         row: u32,
   44 |         tile: u32,
   45 |     },
   46 |     /// Canonical 1024-ID chunks, except the final partial chunk.
   47 |     Ids {
   48 |         offset: u32,
   49 |     },
   50 | }
   51 | 
   52 | /// Signed demand identity, not an admission certificate. The paid native fold
   53 | /// must authenticate these clocks/owner and purchase the complete response span.
   54 | #[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
   55 | pub struct PalwTirPublicDemandScopeV1 {
   56 |     pub claim: Hash64,
   57 |     pub owner: PalwBondKeyV2,
   58 |     pub reserved_daa: u64,
   59 |     pub bundle_id: Hash64,
   60 |     pub turn: u64,
   61 |     pub opened_daa: u64,
   62 |     pub deadline_daa: u64,
   63 | }
   64 | 
   65 | #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
   66 | pub enum PalwTirPublicResponseV1 {
   67 |     StepNode { frontier: Vec<Hash64>, siblings: Vec<Hash64> },
   68 |     StepLeaf { preimage: PalwStepTileLeafV1, opening: PalwStepOpeningV1 },
   69 |     StepRun { preimages: Vec<PalwStepTileLeafV1>, range: PalwStepRangeOpeningV1 },
   70 |     RowsNode { frontier: Vec<Hash64>, siblings: Vec<Hash64> },
   71 |     TileNode { row_root: Hash64, row_opening: PalwStepOpeningV1, frontier: Vec<Hash64>, siblings: Vec<Hash64> },
   72 |     TraceTile { row_root: Hash64, row_opening: PalwStepOpeningV1, lanes: Vec<i32>, opening: PalwStepOpeningV1 },
   73 |     Ids { rows_root: Hash64, ids: Vec<u32> },
   74 | }
   75 | fn invalid(message: &str) -> io::Error {
   76 |     io::Error::new(io::ErrorKind::InvalidData, message)
   77 | }
```

**`consensus/core/src/palw_tir_public_source_v1.rs` L79–L107**

ファイル SHA-256: `56844daf1592284692d2127c7fb9a84d4386806e4ab5d8a7ec5202204b060eb3`

```text
   79 |     let length = u32::deserialize_reader(r)? as usize;
   80 |     if length > maximum {
   81 |         return Err(invalid("TIR public vector exceeds its bound before allocation"));
   82 |     }
   83 |     let mut values = Vec::new();
   84 |     values.try_reserve_exact(length).map_err(|_| invalid("TIR public allocation failed"))?;
   85 |     for _ in 0..length {
   86 |         values.push(T::deserialize_reader(r)?);
   87 |     }
   88 |     Ok(values)
   89 | }
   90 | fn opening<R: Read>(r: &mut R) -> io::Result<PalwStepOpeningV1> {
   91 |     Ok(PalwStepOpeningV1 {
   92 |         leaf_index: BorshDeserialize::deserialize_reader(r)?,
   93 |         leaf_hash: BorshDeserialize::deserialize_reader(r)?,
   94 |         siblings: vector(r, PATH)?,
   95 |     })
   96 | }
   97 | fn leaf<R: Read>(r: &mut R, maximum_bytes: usize) -> io::Result<PalwStepTileLeafV1> {
   98 |     Ok(PalwStepTileLeafV1 {
   99 |         version: BorshDeserialize::deserialize_reader(r)?,
  100 |         coord: BorshDeserialize::deserialize_reader(r)?,
  101 |         value_count: BorshDeserialize::deserialize_reader(r)?,
  102 |         values_le: crate::palw_public_binding_v1::read_bounded_bytes_v1(r, maximum_bytes)?,
  103 |     })
  104 | }
  105 | impl BorshDeserialize for PalwTirPublicResponseV1 {
  106 |     fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
  107 |         Self::decode_with_leaf_bound_v1(r, LEAF_BYTES)
```

## 外部参考（規範的根拠ではない）

参照日: 2026-10-19。モデル計算に新しい validity proof を導入する根拠として用いていない。他 protocol の値を MISAKA の door 見積もりへ移していない。

- OP Stack, Fault Dispute Game specification: https://specs.optimism.io/fault-proof/stage-one/fault-dispute-game.html — single-step transition と事前の preimage 提供が別の要件であることを確認するための一般的参考。

- Arbitrum docs, Overview of BoLD: https://docs.arbitrum.io/how-arbitrum-works/bold/gentle-introduction — 1 人の正直な参加者と有界時間の説明を比較用に参照。MISAKA に同じ保証が成立した証拠ではない。

## 実施・未実施

実施: 添付 ZIP の展開、関連 source と規範文書の読解、限定した識別子検索、source hash 記録、RFC/ADR 案作成、同梱補助スクリプトの自己試験。

未実施: Rust build、最新 branch との merge、A/B/C の実装、出荷 binary の帯域・span 計測、node E2E、正式 arming record の生成・pin、fence 変更、main 更新、t12 配備。
