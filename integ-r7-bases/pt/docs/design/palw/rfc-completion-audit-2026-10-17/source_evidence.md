# RFC完成監査：原文ソース抜粋

対象: misakas-pre(1).zip / SHA-256 c13be519c006ac707d2c3f2ae1dcb468b50695844121f1e4c6e0321b92e9f22e

行番号はZIPを展開した原本の1始まり。ソースコメント・過去の状況記録と今回の実行結果を混同しない。

<a id="s01"></a>

## S01 — RFC01: 受入条件・実装済みprefix・残る最適化・seed提案

### `docs/rfc/0001-palw-inference-surface-gaps.md:370–377`

```text
370: ## 6. テスト・ドリル計画
371: 
372: 1. **単体**:§2.1〜§2.3 の key 関数、丸め、境界値、identity 値での V3 一致。
373: 2. **produce ⇔ replay の bit 一致**:seed × 温度 × penalty × bias × stop の組み合わせを、floor・8k class で掃く。
374: 3. **反証**:規則を無視した token を commit した claim が、2 tile(+ 履歴)で反証され、正しい claim は反証されない。
375: 4. **サービング**:prefix-resume == fresh、batch 内 == 単独、キャッシュの LRU とメモリ上限、並行接続の上限。
376: 5. **ドリル**:salt 付きドリルで decode rules v2 の fence を低い高さで跨ぎ、seed・温度・repeat_penalty 付きの FP claim が bind → licence → Final まで進むこと、全ノードの状態が一致すること。
377: 6. **公開チェーン**:flag day 後、fence 越えの FP claim の Final と、反証 0 件を監視する。
```

### `docs/rfc/0001-palw-inference-surface-gaps.md:408–429`

```text
408: ### 7.3 追記(2026-10-03 夜、lane U 第 3 弾)— 段階 2b と運用
409: 
410: - **段階 2b(FP job version 12、dormant fence `palw_fp_prefix_inherit`)**:prefix 位置の step leaf を job 非依存の prefix context(version-3 job context、`job_nullifier` = k、`assignment_id` = prefix state root、`step_tile_leaf_hash_ctx_v1`)で hash する。同じ prefix を共有する job の継承 leaf は byte 単位で一致し、再計算した root と等しく、継承範囲の嘘は先に同じ leaf を commit した claim で反証できる。V2・V11 の context と leaf は 1 bit も変わらない。fence は `palw_fp_prefix_state` を前提に持つ。
411: - **admission(必須の安全規則、acceptance で名指し拒否)**:version 12 の claim は、leaf が継承に安全と証明された class にだけ載る。条件は「held でない」「IR・生成 class でない」「fused-attention の court window を持たない」「shape profile が公開済みで整数 lane(`Int32`)」「KV aux series を持たない(`kv_chunk_calls == 0`)」。それ以外(fused attention、KV aux、未証明の kind)は walk が `an inherited-prefix claim (FP job version 12) over a class whose leaves are not inheritance-safe …` で skip する(`palw_fp_prefix_inherit_class_safe_v1`、`PalwChainStateV2::class_prefix_inherit_safe_v1`)。checkpoint の leaf は継承せず job ごとの context で hash する(整数 floor 上で seat の interval replay が Valid になることを e2e で確認)。
412: - **token table の自動読み込み**:seat は class artifact の隣の `<artifact>.palwtokens`(`palw-a16-fp-worker --emit-token-table` の出力)を起動時に登録する。table は自分の bytes から導いた root で登録され、制約付き claim は job が commit した `table_root` で table を選ぶので、root の合わない sidecar はその claim には使われない(seat は `Unverifiable` で棄権)。`--palw-token-table` は上書き・追加として残る。
413: - **drill 用 producer**:`misaka-palw-rfc1-drill-claim`(floor class 上で constraint / constraint2 / prefix / inherit の claim を実走して outbox に書く)+ `scripts/drills/misaka-palw-rfc1-drill-claims.sh`(rail で署名・submit)。adapter 一覧の producer は composite class が要るため floor 版はない。
414: - **残(最適化・凍結後)**:executor の計算削減、seat 側の prefix-leaf cache。
415: 
416: ## Mission alignment amendment — 2026-10-07
417: 
418: §Aと§2のdecode、artifact、KV reuse、adapter、multimodal拡張は、それぞれ公開の入力・state境界・version・outputと裁定証拠を拘束する。ローカルChatの非claim機能や将来profileのsource fidelityは、公開訴追可能性の代わりにならない。新しい報酬profileの受入にRFC14の外部public-bond試験を追加する。
419: 
420: 本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。
421: 
422: ## 改定 — 2026-10-12（W1・W2・X4・N4：提案）
423: 
424: **状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 詳細は[RFC/ADRレビューの整理（2026-10-12）](../design/palw/rfc-adr-review-resolution-2026-10-12.md)。
425: 
426: - **W1:** Gatewayの整数seed変換を `le64(n+1)` に統一し、オーバーフローとなる入力を拒否する。gateway側の表示・入力変換規則であり、直ちにコンセンサス分岐が起こると断定しない。
427: - **W2:** FP V4自体のtext規則は変更しない。TextStream pipelineには追加規則を適用する。
428: - **X4:** FP Job V6は「実装済み・dormant。ただしcourtの一部に未完了事項あり」と記録する。
429: - **N4:** 通常の非報酬AI推論はプライバシーを維持して利用できる。公開訴追に必要な証拠を公開できない経路は、新規の報酬・weight対象から外す。
```

### `misaka/palw/gateway/src/surface.rs:120–138`

```text
120:     #[serde(default)]
121:     pub require_committed_format: bool,
122: }
123: 
124: /// OpenAI's `seed` is an integer; this lane's (ADR-0082 Decision 11) is 64 hex characters. An
125: /// integer is read as its decimal text so that it reaches `sampling_from_request`'s own rule and
126: /// is refused THERE, by that rule's sentence, rather than by a serde type error that names no
127: /// field.
128: fn seed_as_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
129:     match Option::<Value>::deserialize(deserializer)? {
130:         None | Some(Value::Null) => Ok(None),
131:         Some(Value::String(s)) => Ok(Some(s)),
132:         Some(Value::Number(n)) => Ok(Some(n.to_string())),
133:         Some(other) => Err(serde::de::Error::custom(format!("seed must be 64 hex characters, got {}", kind_of(&other)))),
134:     }
135: }
136: 
137: /// The chat completion request — every field OpenAI's surface names, and this lane's own.
138: #[derive(Deserialize, Default)]
```

### `misaka/palw/gateway/src/main.rs:1168–1194`

```text
1168: fn sampling_from_request(chat: &ChatRequest, facts: &chain::ChainFacts) -> Result<([u8; 32], u32), String> {
1169:     use kaspa_consensus_core::palw_decode_select_v2::{PALW_DECODE_SEED_GREEDY, PALW_DECODE_T_ONE, PALW_DECODE_TEMPERATURE_GREEDY};
1170:     let temperature_q = match chat.temperature {
1171:         None => PALW_DECODE_TEMPERATURE_GREEDY,
1172:         Some(t) if t.is_nan() || !(0.0..=MAX_TEMPERATURE).contains(&t) => {
1173:             return Err(format!("temperature must be a number in 0..={MAX_TEMPERATURE:.3}; got {t}"));
1174:         }
1175:         Some(t) => (t * PALW_DECODE_T_ONE as f64).round() as u32,
1176:     };
1177:     let sampling_seed = match chat.seed.as_deref() {
1178:         None | Some("") => PALW_DECODE_SEED_GREEDY,
1179:         Some(hex) => {
1180:             let mut out = [0u8; 32];
1181:             if hex.len() != 64 || faster_hex::hex_decode(hex.as_bytes(), &mut out).is_err() {
1182:                 return Err("seed must be 64 hex characters".to_string());
1183:             }
1184:             out
1185:         }
1186:     };
1187:     if !facts.fp_decode_rules_armed && (temperature_q != PALW_DECODE_TEMPERATURE_GREEDY || sampling_seed != PALW_DECODE_SEED_GREEDY) {
1188:         return Err("this network has not armed ADR-0082 Decision 11's sampler (Params::palw_fp_decode_rules) — a job with a \
1189:              temperature or a seed would be refused by the transition as SamplingNotArmed, after the inference had \
1190:              already been paid for. Omit `temperature` and `seed`, or send temperature 0."
1191:             .to_string());
1192:     }
1193:     Ok((sampling_seed, temperature_q))
1194: }
```

<a id="s02"></a>

## S02 — PALWART3 container rootと登録/court rootの未接続

### `consensus/core/src/palw_genesis8_catalog_v1.rs:8–25`

```text
8: //! **Scope: text generation only** (user policy 2026-10-15) — token ids in, next-token logits out. Vision towers and projectors
9: //! (mmproj), image/audio/video embedders and multi-token-prediction heads are not computed, not covered, not admitted as claims and
10: //! not rewarded: [`palw_genesis8_admits_input_v1`] refuses every input but token ids by name, and the artifact refuses such
11: //! components by name (`misaka_palw_tir_artifact::palwart_v3::check_text_scope_v3`). Each ships as ONE `PALWART3` artifact, and the catalog's `artifact_root` is that
12: //! artifact's root (`misaka_palw_tir_artifact::palwart_v3::palwart_root_v3`), which commits the legacy identity of its weight section
13: //! (the A16 `artifact_digest`, the TIR inventory root, or the PALWQ361 file digest) as an inner root.
14: //!
15: //! * **Dormant.** No preset reads this module. testnet-12, its preset and its DAA 14,000 release scope are untouched; the list is
16: //!   for the NEXT network's genesis (edition 0 is part of a network's identity, RFC-0019 §7.2).
17: //! * **Roots are not invented.** The class ids, artifact roots and plans are unknown until each conversion has run and the class is
18: //!   declared; [`palw_genesis8_edition0_v1`] builds edition 0 only from roots a caller supplies, and refuses a missing, zero, unknown
19: //!   or duplicated entry — there are no placeholder hashes that could be armed by mistake.
20: //! * **Membership is economic only** (PRINCIPLES §9): a listed class gets the 1.0 tier; every claim still needs its own Gate C
21: //!   (`SecurityCertified`) and court. #10 is listed as the user chose but is not end-to-end adjudicable, so its claims cannot be
22: //!   certified until it is: listing it grants nothing a claim has not earned.
23: //! * **Open integration requirement** (recorded, not implemented here): registration on the next network must bind the `PALWART3`
24: //!   root as the class's artifact root (today a TIR class binds its inventory root directly), with the court opening the inner
25: //!   inventory leaves under the inner root the container root commits.
```

### `misaka/palw/sdk/src/tir_layout.rs:819–842`

```text
819:     parent: Option<&Path>,
820:     profile: Option<misaka_palw_tir_artifact::metadata_v16::MetadataParseProfileV16>,
821: ) -> Result<TirDeclaredLayoutV1, String> {
822:     use misaka_palw_tir_artifact::metadata_v16::{parse_metadata_v16, parse_recorded_metadata_v16, record_metadata_profile_v16};
823:     let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(input)?;
824:     let container = artifact.container();
825:     let program = &tir_program_with_scheme_v1(&container.program, choice.logits_scheme)?;
826:     let program_bytes = program.encode();
827:     let (inventory_root, leaf_count) = artifact.inventory_root()?;
828:     let meta0 = match profile {
829:         Some(p) => parse_metadata_v16(&container.header.meta, p)?,
830:         None => match serde_json::from_str::<serde_json::Value>(&container.header.meta) {
831:             Ok(v) if v.get("metadata_parsing").is_some() => parse_recorded_metadata_v16(&container.header.meta)?,
832:             Ok(v) => v,
833:             Err(_) => serde_json::json!({}),
834:         },
835:     };
836:     let composite = composite_binding(container, program, &meta0, parent)?;
837:     let artifact_root = composite.as_ref().map_or(inventory_root, |c| c.r.artifact_root());
838:     let carriable = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court);
839:     // The gate a layout is judged by: the registration's, or the composite's.
840:     let judge = |class: &PalwTirClassV1| -> Result<(), String> {
841:         match &composite {
842:             None => tir_class_admission_offline_v1(params, bundle, class, artifact_root),
```

### `misaka/palw/sdk/src/tir_registration.rs:129–159`

```text
129: /// **Build the IR registration** for `entry` under the chain's `terms`: the network's own pricing
130: /// (the base class's slash value and initial target), weightless, at the canonical job admission
131: /// v10 requires (`palw_tir_job_context_v1` at `palw_tir_attempt_canonical_v1`) — gated before it is
132: /// returned. Call once with an empty signature to learn the message to sign
133: /// ([`tir_registration_message_v1`]), then again with the signature.
134: #[allow(clippy::too_many_arguments)]
135: pub fn build_tir_registration_v1(
136:     params: &Params,
137:     bundle: &PalwConsensusParamsV2,
138:     entry: &PalwTirClassEntryV1,
139:     terms: &PalwRegistrationTermsV2,
140:     activation_daa: u64,
141:     registrant_bond: PalwBondKeyV2,
142:     signature: Vec<u8>,
143:     daa: u64,
144: ) -> Result<PalwConsensusObjectV2, String> {
145:     let object = palw_tir_post_genesis_registration_v1(
146:         entry.class.as_ref().clone(),
147:         entry.canonical_context(),
148:         entry.artifact_root,
149:         0,
150:         terms.initial_target,
151:         terms.slash_value_per_pwu,
152:         activation_daa,
153:         registrant_bond,
154:         signature,
155:         bundle.court.max_step_leaf_count(),
156:     )
157:     .map_err(|e| format!("{}: {} ({e})", entry.model_id, e.code()))?;
158:     tir_registration_preflight_v1(params, bundle, &object, daa, &terms.chain_certified_families)
159:         .map_err(|e| format!("{}: the admission gate refuses the registration: {e}", entry.model_id))?;
```

### `consensus/core/src/palw_gdn_court_v2.rs:91–115`

```text
91: pub fn palw_gdn_court_verdict_v2(
92:     accusation: &PalwGdnCourtAccusationV2,
93:     claim: &PalwOneMoveClaimV2,
94:     ladder: u64,
95: ) -> Result<PalwShardCourtVerdictV1, PalwGdnCourtErrorV2> {
96:     let refused = |message| PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(message));
97:     accusation.validate_shape(ladder).map_err(refused)?;
98:     if accusation.execution_root != claim.execution_root {
99:         return Err(refused("GDN accusation is not the actual native claim's execution"));
100:     }
101:     // ProvenOperands clones only the opened operand records. Reserve their
102:     // actual typed ownership before cloning; membership hashing is streaming.
103:     let mut q = TirNativeRequestsV1::new();
104:     q.exact_v1::<PalwArtifactOperandV1>(accusation.artifact_openings.len() as u64)?;
105:     for opening in &accusation.artifact_openings {
106:         let heap = opening.operand.native_heap_v1("GDN court authenticated artifact operand clone");
107:         q.add_v1(heap.bytes().ok_or(heap)?)?;
108:     }
109:     let _operands = reserve_native_requests_v1(q.bytes_v1())?;
110:     let proven = PalwProvenOperandsV1::from_openings_v1(&accusation.artifact_openings, claim.artifact_root)
111:         .map_err(|_| refused("GDN artifact operands do not belong to the registered class artifact"))?;
112:     match check_gdn_step_refutation_v2(&accusation.refutation, claim.execution_root, claim.class_id, &proven, ladder) {
113:         Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
114:         Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound)) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
115:         Err(other) => Err(other),
```

<a id="s03"></a>

## S03 — TIR FPの登録semantics注入API

### `misaka/palw/tir/exec/src/node/backend.rs:398–444`

```text
398:     /// Serve the class `artifact` declares under `artifact_root` (the inventory root the chain
399:     /// registered — derived from the artifact by the caller, never read from a sidecar here), at the
400:     /// canonical job `canonical` (its `shape_profile_id` must be the class id).
401:     pub fn new(
402:         model_id: String,
403:         artifact: Arc<TirArtifactV1>,
404:         artifact_root: Hash64,
405:         canonical: PalwJobContextV2,
406:         prompt_ids_form: PalwPromptIdsFormV1,
407:         ladder: u64,
408:     ) -> Result<Self, String> {
409:         let class = artifact.class()?;
410:         let space = PalwTirStepSpaceV1::new(&class).map_err(|e| e.to_string())?;
411:         let class_id = class.class_id(&artifact_root);
412:         let facts = PalwTirJobFactsV1::of(&class, &space.program, class_id);
413:         let network_form = prompt_ids_form;
414:         let prompt_ids_form = facts.prompt_ids_form(network_form);
415:         if canonical.shape_profile_id != class_id {
416:             return Err("the canonical job names another class".into());
417:         }
418:         space.leaf_count_capped(&canonical, ladder).map_err(|e| format!("the canonical job does not count: {e}"))?;
419:         Ok(Self {
420:             model_id,
421:             artifact,
422:             class,
423:             space,
424:             artifact_root,
425:             class_id,
426:             canonical,
427:             facts,
428:             network_form,
429:             prompt_ids_form,
430:             ladder,
431:             attempt_rules: PalwAttemptRulesV1::CoreV1,
432:             fold_hashes: std::sync::Mutex::new(None),
433:             dense_capture_bytes: TIR_DENSE_CAPTURE_BYTES_V1,
434:             fused: tir_fused_kernels_default_v1(),
435:             memory_reserver: None,
436:             network_id: Vec::new(),
437:             fp_class: kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 {
438:                 model_profile_id: Hash64::default(),
439:                 runtime_manifest_hash: Hash64::default(),
440:                 runtime_class_id: Hash64::default(),
441:                 shape_profile_id: class_id,
442:                 cu_ruleset_id: Hash64::default(),
443:             },
444:         })
```

### `misaka/palw/tir/exec/src/node/backend.rs:454–466`

```text
454:     /// **RFC-0002 Phase H: the network this backend's free-prompt contexts are built under** (the lineage passes the node's).
455:     pub fn with_network_id_v1(mut self, network_id: &[u8]) -> Self {
456:         self.network_id = network_id.to_vec();
457:         self
458:     }
459: 
460:     /// **RFC-0002 Phase H: the runtime ids a free-prompt context carries** — the class's registered semantics
461:     /// (`PalwClassSemanticsV1`: model profile, runtime manifest and class, CU ruleset), which the frame matches the header's
462:     /// context against. `shape_profile_id` stays this class's id whatever is passed.
463:     pub fn with_fp_class_facts_v1(mut self, facts: kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3) -> Self {
464:         self.fp_class = kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 { shape_profile_id: self.class_id, ..facts };
465:         self
466:     }
```

<a id="s04"></a>

## S04 — Generative job bodyのAudio/Video欠落

### `consensus/core/src/palw_gen_job_v1.rs:132–167`

```text
132:     pub pooling: u8,
133:     /// MUST be one of the class's offered output widths.
134:     pub dims: u32,
135:     /// `EmbeddingI32 = 3`.
136:     pub output: u8,
137: }
138: 
139: /// **The body** — one variant per built profile. Audio and video are not built; their tags are not
140: /// bytes this enum decodes.
141: #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
142: pub enum PalwGenBodyV1 {
143:     Image(PalwGenImageBodyV1),
144:     Embedding(PalwGenEmbeddingBodyV1),
145:     /// **A task head's job** (`crate::palw_task_heads_v1`, Borsh variant 2, appended): bytes the int-12 build cannot decode, read as
146:     /// undecodable below `palw_task_heads_v1`.
147:     Head(crate::palw_task_heads_v1::PalwGenHeadBodyV1),
148: }
149: 
150: impl PalwGenBodyV1 {
151:     /// The profile this body is a job of.
152:     pub fn profile(&self) -> PalwGenProfileV1 {
153:         match self {
154:             Self::Image(_) => PalwGenProfileV1::Image,
155:             Self::Embedding(_) => PalwGenProfileV1::Embedding,
156:             Self::Head(_) => PalwGenProfileV1::Head,
157:         }
158:     }
159: }
160: 
161: /// **A generative job** (RFC-0003 §I.0).
162: #[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
163: pub struct PalwGenJobV1 {
164:     /// [`PALW_GEN_JOB_VERSION_V1`].
165:     pub version: u16,
166:     pub envelope: PalwJobEnvelopeV1,
167:     /// R's key (§I.1): all zeros for a class that draws no randomness.
```

### `consensus/core/src/palw_gen_step_v1.rs:20–35`

```text
20: //! The enumeration is structural — the programs, the layouts and the job's trip counts — so a court
21: //! derives any leaf's index from its coordinate without the run.
22: 
23: use std::collections::BTreeMap;
24: 
25: /// **The most step leaves a node ever enumerates for one job** (2^22, about 400 MiB of leaves and index).
26: /// [`PalwGenStepSpaceV1::new`] checks the closed-form count against it BEFORE enumerating, and no profile's
27: /// `max_job_step_leaves` may exceed it (`PalwGenProfileCeilingsV1::within_format_caps`), so an admitted class's
28: /// widest job always fits. Audit 2026-10-04 G-1: without the pre-check a court binding over a 2^30-leaf job made
29: /// every validating node allocate ~100 GB on the same block.
30: pub const PALW_GEN_MAX_ENUMERATED_STEP_LEAVES_V1: u64 = 1 << 22;
31: 
32: use crate::Hash64;
33: use crate::palw_gen_class_v1::PalwGenClassV1;
34: use crate::palw_tir_class_v1::PalwTirLayoutV1;
35: use misaka_palw_tir::demand::{DemandContext, history_length_v1};
```

### `docs/rfc/0003-palw-generative-model-classes.md:2214–2282`

```text
2214: ## II.5 Audio — generation and understanding
2215: 
2216: **Job fields.** For understanding: `AudioInputRefV1 { input_root, sample_rate, channels, frames }`,
2217: PCM `i16` at the class's declared rate. Decoding and resampling stay outside consensus. For
2218: generation: text ids (and optionally a reference clip as an input), `clip_index`, the steps or token
2219: budget from the class's offers, and `output: PcmI16`.
2220: 
2221: **Canonical output.** PCM `i16` LE interleaved, with header (sample rate, channels, frames). The last
2222: stage clamps to `[−32768, 32767]`.
2223: 
2224: **Mapping onto the TIR scan.**
2225: 
2226: - **Understanding.** A feature stage in integer TIR:
2227:   - framing (`Gather` with a pinned index table);
2228:   - the window and DFT as `MatMul`s with pinned tables;
2229:   - power (`Mul`/`Add`) and the mel filterbank (`MatMul`);
2230:   - the logarithm (`IntLn`) and normalisation.
2231: 
2232:   Then an encoder over frames (a `Fixed` axis) and, for speech-to-text, a text decoder that is an LM
2233:   program reading the encoder rows through an `External` input. Cross-attention is over a `Fixed`
2234:   axis.
2235: - **Generation.** Either an LM over codec tokens, selected by RFC-0001 §A's rule with domain 0,
2236:   followed by a codec-decoder stage; or latent diffusion or flow over a `[C, T]` latent
2237:   (`AUDIO_*_NOISE_V1`, the image profile in one dimension), followed by a decoder or vocoder stage.
2238:   Transposed convolutions are zero insertion (`Concat`) plus convolution. Periodic activations are
2239:   range reduction plus a table.
2240: 
2241: **What the IR lacks.** Nothing beyond Part I. A codec LM with several codebooks per position emits
2242: several logits rows per position, which needs a rows-per-position field in the FP logits scheme, a
2243: later FP job version. That is an FP-side extension, not an IR one.
2244: 
2245: **Court feasibility.** One-dimensional convolution cones are small. Long activations (for example
2246: 30 s × 44.1 kHz × 64 channels ≈ 8.5·10^7 elements) fit the `2^28` cap. Attention over thousands of
2247: frames fits up to about 15,600 per head, beyond which committed partials apply.
2248: 
2249: ## II.6 Video generation
2250: 
2251: **Job fields.** The image body plus `frames`, which MUST equal the class's. fps is class metadata,
2252: carried in the output header. The item index is `clip_index`. One class per (resolution, frame count).
2253: 
2254: **Canonical output.** `u8` THWC with header (`T`, `H`, `W`, fps).
2255: 
2256: **Mapping onto the TIR scan.** It generalises §II.1:
2257: 
2258: - the latent gains a time axis, `[C, F, H, W]` (`VIDEO_INIT_NOISE_V1`);
2259: - the denoise stage is the same scan (one step = one position), with multi-axis RoPE over
2260:   `(frame, y, x)` from static tables;
2261: - attention runs over all spatio-temporal tokens inside a step.
2262: 
2263: A frame-causal 3-D VAE decoder maps naturally onto a **scan over latent frame chunks**: its causal
2264: convolution caches become `Fixed` states, and its `Rows` output is the pixel frames per position.
2265: 
2266: **What the IR lacks.**
2267: 
2268: - Dissection over the token axis (§II.1.5.6) is **required**, because one head's K and V exceed the
2269:   close ceiling at a few tens of thousands of tokens.
2270: - Per-position leaf ceilings (`2^22`) are exceeded by a single step at production sizes.
2271: - The commitment volume decides feasibility.
2272: 
2273: **Court feasibility, with an example geometry** (`P_tok = 1.4·10^10`, `d = 5,120`, `L_blk = 40`,
2274: `N_tot ≈ 33,000`, `S = 50`, `c = 2`):
2275: 
2276: - about 9·10^16 MACs per clip, about 13 minutes on a 120 TMAC/s GPU;
2277: - about 2.7·10^13 bytes of lanes to hash;
2278: - about 34 MB of K and V per head.
2279: 
2280: Production-scale video is therefore not practical under per-step full commitment. It needs dissection
2281: **and** either packed lanes or lazy commitment (*Alternatives*). Small clips (≤ 1B, ≤ 16 frames at
2282: 256²) are feasible for testnet once dissection exists.
```

<a id="s05"></a>

## S05 — GDN court本体とpublic DA filerの残件

### `consensus/core/src/palw_gdn_court_v2.rs:28–47`

```text
28: 
29: #[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
30: pub struct PalwGdnCourtAccusationV2 {
31:     pub version: u16,
32:     pub claim: Hash64,
33:     pub execution_root: Hash64,
34:     pub trace_root: Hash64,
35:     pub executor_bond: PalwBondKeyV2,
36:     pub accuser_bond: PalwBondKeyV2,
37:     pub leaf_index: u64,
38:     pub refutation: PalwGdnStepRefutationV2,
39:     pub artifact_openings: Vec<PalwArtifactOpeningV1>,
40:     pub signature: Vec<u8>,
41: }
42: 
43: impl PalwGdnCourtAccusationV2 {
44:     /// Only framing/identity checks; no arithmetic or reserved workspace is
45:     /// consumed by the String-only outer admission walk. The typed fold always
46:     /// runs the full terminal and preserves local resource refusal separately.
47:     pub fn validate_shape(&self, ladder: u64) -> Result<(), &'static str> {
```

### `consensus/core/src/palw_gdn_court_v2.rs:91–115`

```text
91: pub fn palw_gdn_court_verdict_v2(
92:     accusation: &PalwGdnCourtAccusationV2,
93:     claim: &PalwOneMoveClaimV2,
94:     ladder: u64,
95: ) -> Result<PalwShardCourtVerdictV1, PalwGdnCourtErrorV2> {
96:     let refused = |message| PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(message));
97:     accusation.validate_shape(ladder).map_err(refused)?;
98:     if accusation.execution_root != claim.execution_root {
99:         return Err(refused("GDN accusation is not the actual native claim's execution"));
100:     }
101:     // ProvenOperands clones only the opened operand records. Reserve their
102:     // actual typed ownership before cloning; membership hashing is streaming.
103:     let mut q = TirNativeRequestsV1::new();
104:     q.exact_v1::<PalwArtifactOperandV1>(accusation.artifact_openings.len() as u64)?;
105:     for opening in &accusation.artifact_openings {
106:         let heap = opening.operand.native_heap_v1("GDN court authenticated artifact operand clone");
107:         q.add_v1(heap.bytes().ok_or(heap)?)?;
108:     }
109:     let _operands = reserve_native_requests_v1(q.bytes_v1())?;
110:     let proven = PalwProvenOperandsV1::from_openings_v1(&accusation.artifact_openings, claim.artifact_root)
111:         .map_err(|_| refused("GDN artifact operands do not belong to the registered class artifact"))?;
112:     match check_gdn_step_refutation_v2(&accusation.refutation, claim.execution_root, claim.class_id, &proven, ladder) {
113:         Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
114:         Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound)) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
115:         Err(other) => Err(other),
```

### `docs/design/palw/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md:104–125`

```text
104:     - checkpoint transition の検査(`memory_v2/transition.rs:356`)。anchored step 検査だけでは、偽造 checkpoint を無罪にしてしまうため。
105:     - convolution-window の検査(`:569`)。
106:     - capture からの evidence 組立(`capture.rs:135`、`transition.rs:655`)。
107:   - composed hybrid map は新しい map id とした。歴史 root と codec は不変(`palw_state_chunk_map.rs:1218`)。base0 backend が capture と restore を行う(`legs.rs:672`、`fp_capture.rs:1912`)。
108:   - court までの経路: tag-244 fold(`palw_memory_routing_fold_v2.rs:20`)→ block 受入(`palw_validate.rs:2504`)→ `adjudicate_court_close_v5`(`palw_court_v2.rs:1419`)。
109:   - Codex 担当分(`memory_v2` の anchored step court、tag 243 `GdnStepAccusedV2`、fence 配線)は `1d622f430`。欠けていた `capture.rs`、fork-id probe arm、tag 243 の heartbeat/kaspad arm、mirror は lane が補った。
110:   - 告発者側の evidence(`026f76877`): `misaka/palw/base0/src/gdn_evidence_v2.rs` が base0 の保持データから step・transition・conv window の evidence を作る。最初に偽造された checkpoint も探す。告発者は anchor state を自分で再計算する。committed leaf と一致しなければ ForgedCheckpoint とし、transition court へ回す。`Qwen36Backend::gdn_step_evidence_v2`/`gdn_transition_evidence_v2` が包む。
111:   - held hybrid の偽造 checkpoint(`9aee75d66`): held class は checkpoint の chunk を保持・提供しない。そこで、transition terminal が読む `StateChunk` 単位だけを、既存の held DA court で要求する。
112:     - 要求単位の導出は `gdn_transition_demands_v2`/`gdn_conv_window_demands_v2`。
113:     - producer は `MaterialDisclosedV2` で答え、DA court が hash で検査する。沈黙は既存の DA-7 default で有罪になる。
114:     - 回答から accused witness を組み、同じ conviction に至る。新しい tag・object は無い。
115:     - 告発者自身の再計算が checkpoint と一致しない場合に限り、要求を出せる。
116:   - genesis model の範囲(`ee5202938`、`scripts/genesis/genesis8_models.json`):
117:     - id 8(Qwen3.6-35B-A3B MoE、graph-v8 held v5): この court 経路で端から端まで扱える。layer ごとの router は 1 つなので binding は不要。
118:     - id 2・3・6(Qwen3.5/3.8、TIR program): GDN state と conv は TIR の state 関係に lowering され、TIR の native court が裁く。費用の実測は UNKNOWN(TIR lane の担当)。
119:   - **残り:**
120:     - kaspad で DA session を開き回答を集める filer の自動化は未作成。library 経路は完成している。
121:     - DA-8 の session 上限は未測定(1 transition は 1〜2 単位)。
122:     - checkpoint map を持つ solo GDN v3 は登録できない。fence の無い `validate_shape` の呼出が 158 箇所あり、fence 付きにするには compile 検証が要る。map を持たない solo v3 は現在も登録でき、legacy step court が genesis replay で裁く。
123:     - convolution-window 検査は Q36 `SsmConv` 層だけを扱う。
124: - [x] 新しい anchor evidence は、データ所属と checkpoint/step の認証専用とする。 — **implemented (GDN lane), unverified.**
125:   - evidence kind 0x53/0x54 を追加した。chunk geometry は承認済み profile から取り、challenger の値は使わない(`capture.rs:15`)。
```

<a id="s06"></a>

## S06 — SelectedApproval adapterとPanelLicensed eventの分岐

### `consensus/core/src/palw_kernel_route_v1.rs:794–840`

```text
794: /// The beacon event of a selected-approved Final (RFC07 adapter): the kernel's `WorkFinalEventV1`, read through the ONE kernel
795: /// settlement predicate (RFC-0015 §9, GAP-B3) — a conviction withdraws the fact (`claim_final = false`), a default withdraws its
796: /// availability (`da_satisfied = false`) — and attributed from authenticated ledger rows (producer, and the job's permanent payer, so
797: /// one payer cannot fill the distinct-consumer quorum through several producers). No weight or reward: kernel claims reserve
798: /// B = F = 0; the event is a beacon fact only.
799: fn selected_approval_event_v1(
800:     ledger: &KernelLedgerV1,
801:     receipt: &misaka_palw_kernel::opv::FinalReceiptV1,
802:     settlement: crate::palw_kernel_settlement_v1::PalwKernelSettlementV1,
803: ) -> Result<Vec<u8>, String> {
804:     use crate::palw_kernel_settlement_v1::PalwKernelSettlementV1 as S;
805:     use misaka_palw_kernel::opv::WorkFinalContextV1;
806:     let work = ledger.claims.get(&receipt.claim).map(|r| (r.class_binding_id, r.job_id));
807:     // The earliest acceptance of the work identity: the earliest commitment of any claim of the same job (a convicted or defaulted
808:     // earlier holder counts).
809:     let earliest = ledger
810:         .claims
811:         .values()
812:         .filter(|row| Some((row.class_binding_id, row.job_id)) == work)
813:         .map(|row| row.committed_daa)
814:         .min()
815:         .unwrap_or(receipt.accepted_daa);
816:     let ctx = WorkFinalContextV1 {
817:         accepted_position: earliest,
818:         settlement_position: receipt.final_daa,
819:         occurrence_index: 0,
820:         // Selected checkers are drawn independently of any Panel.
821:         validity_independent: true,
822:         depends_on_profiles: Vec::new(),
823:         panel: None,
824:     };
825:     let mut event = receipt.to_work_final_event(&ctx)?;
826:     match settlement {
827:         S::Convicted => event.claim_final = false,
828:         S::Defaulted => event.da_satisfied = false,
829:         S::FinalRevocable { .. } | S::Settled { .. } => {}
830:         S::Pending => event.claim_final = false,
831:     }
832:     let producer = ledger.claims.get(&receipt.claim).map(|r| r.producer).unwrap_or([0u8; 64]);
833:     let consumer = work
834:         .and_then(|(_, job)| ledger.job_poster(&job))
835:         .map_or(misaka_palw_challenge::RootV1::Absent, misaka_palw_challenge::RootV1::Present);
836:     let attributed = misaka_palw_challenge::AttributedWorkV1 {
837:         event,
838:         attribution: misaka_palw_challenge::SourceAttributionV1 { producer_id: producer, consumer_id: consumer },
839:     };
840:     borsh::to_vec(&attributed).map_err(|e| e.to_string())
```

### `consensus/core/src/palw_kernel_route_v1.rs:843–877`

```text
843: impl PalwKernelRouteStateV1 {
844:     /// Every historical Final in canonical order. A lifecycle timeout is readable,
845:     /// but cannot authorize beacon influence or substitute for selected approval.
846:     pub fn finals_read_v1(&self) -> Result<Vec<KernelFinalReadV1>, String> {
847:         use misaka_palw_kernel::mode::VerificationModeV1;
848:         let ledger = self.ledger_native_projection_v9()?;
849:         let mut out = Vec::new();
850:         for receipt in ledger.final_receipts() {
851:             let settlement = crate::palw_kernel_settlement_v1::palw_kernel_row_settlement_v1(&ledger, &receipt.claim, ledger.daa)
852:                 .unwrap_or(crate::palw_kernel_settlement_v1::PalwKernelSettlementV1::Pending);
853:             // A historical OPV Final states only that its dispute window expired.
854:             // Neither zero direct payout nor a matching local replay establishes
855:             // the selected-validator approval required for entropy/consensus credit.
856:             // Keep the receipt and settlement available for payment/history readers;
857:             // do not manufacture PanelIndependent identity from lifecycle state.
858:             //
859:             // **RFC07 selected-approval adapter.** The one authenticated source of a beacon event is a paid claim whose canonical
860:             // paid Final the native acceptance/restore checks admitted with the ASSIGNED positive quorum — randomly selected
861:             // checkers, a bound source and completed rounds (`palw_exec_v2_selected_approval_v1`, the same gate EXEC credit
862:             // uses). That approval rests on the selection rule and its explicit honest-fraction premise, never on a timeout, a
863:             // zero payout, a matching replay or an operator identity. A Panel-licensed Final still exports nothing: the route
864:             // does not know the licensing Panel's seed and epoch.
865:             let (final_path, approved) = match (receipt.mode, ledger.paid_claim_consumers_v2.contains_key(&receipt.claim)) {
866:                 (_, true) => (
867:                     "PaidClaimV2",
868:                     crate::palw_exec_v2_verify::palw_exec_v2_selected_approval_v1(self, &Hash64::from_bytes(receipt.claim)),
869:                 ),
870:                 (VerificationModeV1::OptimisticPublicVerification, false) => ("OptimisticUnapproved", false),
871:                 (VerificationModeV1::PanelLicensed, false) => ("PanelLicensed", false),
872:             };
873:             let event = if approved { Some(selected_approval_event_v1(&ledger, &receipt, settlement)?) } else { None };
874:             let statement = receipt.assurance.statement();
875:             out.push(KernelFinalReadV1 { receipt, final_path, event, statement, settlement: settlement.label() });
876:         }
877:         Ok(out)
```

### `docs/design/palw/remaining-work-2026-10-16.md:13–21`

```text
13: ## 1. RFC07 の認証済み selected-approval adapter(今回実装)
14: 
15: - `PalwKernelRouteStateV1::finals_read_v1` は、`PaidClaimV2` Final のうち、`palw_exec_v2_selected_approval_v1` が認めたものに限って beacon の `AttributedWorkV1` を出す。
16:   - 認める条件:canonical paid Final が、割り当てられた正の quorum(抽選された checker)・bound source・完了 round で native 受理・restore 検査を通っていること。EXEC credit と同じ gate である。
17:   - optimistic の timeout、Panel-licensed、未承認の paid claim は、従来どおり `None`。
18: - 残り:
19:   - 旧 OPV-only の fixture に依存する conformance / OPV bootstrap の試験、約 45 本と core の `rfc0010_production_fold::paid_panel_sources…`。抽選 checker の承認を持つ paid claim の fixture へ移す(迂回で通さない)。
20:   - Panel-licensed Final の event(route が Panel の seed・epoch を知らない)。
21:   - m・k の測定(Gate C と共通)。
```

<a id="s07"></a>

## S07 — Paid trapの非staged限定とinteractive拒否

### `consensus/core/src/palw_paid_claim_trap_v1.rs:1–16`

```text
1: //! **RFC-0007 §4.1 T6 lazy verifier: planted faults in paid claims and the receipt signers' liability** (`palw_paid_claim_trap_v1`;
2: //! dormant — `None` on every preset and under the RFC06–08 measured-value gate; kernel route aux 142). User decision 2026-10-15 §8
3: //! ("推奨で進めて良い"): the recommended (a)+(b) of the remaining-work note §4.1, with every paid claim carrying a commitment so a
4: //! planted fault is indistinguishable to its checkers until it is revealed.
5: //!
6: //! ```text
7: //! (a) commitments      past the fence EVERY non-staged paid V2 claim carries exactly one producer commitment, posted in the
8: //!                      paid-claim object family (`PaidClaimObjectV2::TrapCommitment`, appended; no new outer tag) strictly
9: //!                      before the claim's beacon seal window opens: H(base claim ‖ fault leaf ‖ tiles ‖ salt) — a planted fault
10: //!                      (a trap) or the null fault (tiles 0: an ordinary claim). Same object, same size, same poster, same
11: //!                      deposit, same lifetime: its presence says nothing. No seat is drawn (the paid lock) without it. Its own
12: //!                      rules, not the audit mesh's: no one-open-per-setter limit, no mesh lottery, and it lives to the claim's
13: //!                      own reveal-window end.
14: //! (b) trap validity    whether a revealed fault COUNTS as a trap is decided by the paid claim's beacon seed — fixed after the
15: //!                      commitment (it was posted before the seal window) — and the commitment's secret salt, at the measured
16: //!                      trap rate: the setter cannot choose which drawn checkers face a counted trap, and nobody but the setter
```

### `consensus/core/src/palw_paid_claim_trap_v1.rs:284–309`

```text
284: }
285: 
286: /// **The paid-trap reveal rule**, pure (every check names itself; the first that fails refuses): a fault opening (a null opening,
287: /// tiles 0, needs no reveal); it opens the claim's commitment; the setter is the paid claim's producer; a non-interactive claim; the
288: /// commitment landed STRICTLY before the seat lock; the fault counts as a trap; revealed after the receipt window and inside the
289: /// reveal window; and the court convicted the claim (the fault's evidence is the court's own deterministic check, never a proof
290: /// carried by the reveal).
291: pub fn palw_paid_trap_reveal_admits_v1(i: &PalwPaidTrapRevealInputV1) -> Result<(), &'static str> {
292:     if i.tiles == 0 || i.tiles > crate::palw_mesh_v1::PALW_TRAP_MAX_TILES_V1 || i.fault_leaf >= i.tiles {
293:         return Err("paid trap: not a fault opening (a null commitment needs no reveal) or the planted tile is out of range");
294:     }
295:     let Some((committer, committed_daa)) = i.trap else {
296:         return Err("paid trap: the reveal opens no commitment of this claim (claim, tile, tile count or salt differ)");
297:     };
298:     if committer != i.setter || i.setter != i.producer {
299:         return Err("paid trap: only the paid claim's producer, who committed, can have planted its fault");
300:     }
301:     if i.interactive {
302:         return Err("paid trap: a trap on an interactive (staged) paid claim is not supported");
303:     }
304:     let Some(lock_daa) = i.lock_daa else { return Err("paid trap: the paid claim has no seat lock") };
305:     if committed_daa >= lock_daa {
306:         return Err("paid trap: the commitment landed at or after the paid claim's seat lock");
307:     }
308:     if !i.counts {
309:         return Err("paid trap: the claim's beacon seed did not select this fault as a trap");
```

<a id="s08"></a>

## S08 — Native liabilityの未定義を受理時に拒否

### `consensus/core/src/palw_settlement_liability_fold_v1.rs:454–463`

```text
454: 
455: impl TransitionBuilder<'_> {
456:     /// **Admission of a claim type whose liability terms have no authenticated source** (no signed hard deadline, no material
457:     /// TTL — the native funded, native text, native job and native mining claims): refused past the fence, by name; a no-op below.
458:     pub(super) fn refuse_unsourced_admission_v1(&self, now: u64, why: &'static str) -> Result<(), PalwStateV2Error> {
459:         if self.liability_fence_from_v1(now).is_some() {
460:             return Err(liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(why)));
461:         }
462:         Ok(())
463:     }
```

### `consensus/core/src/palw_bond_budget_fold_v1.rs:2498–2524`

```text
2498: 
2499: impl TransitionBuilder<'_> {
2500:     pub(super) fn plan_funded_execution_budget_v13(
2501:         &self,
2502:         ctx: &PalwBlockContextV2,
2503:         producer: PalwBondKeyV2,
2504:         class: &misaka_palw_kernel::flat_execution11::CheckedNativeExecutionClassV11,
2505:         job: &misaka_palw_kernel::flat_execution11::ExecutionJobRecordV11,
2506:     ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
2507:         use misaka_palw_kernel::flat_execution11::ExecutionClassContextV11;
2508:         // RFC-0008 RFC08-2 (dormant `palw_settlement_once_v1`): no signed hard deadline or material TTL to source for this type.
2509:         self.refuse_unsourced_admission_v1(ctx.daa_score, "a native funded execution claim carries no signed hard deadline")?;
2510:         if !class.can_admit()
2511:             || class.native_admission_epoch() != Some((ctx.block.as_bytes(), ctx.daa_score))
2512:             || job.class_id != class.record().id()
2513:             || job.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&producer)
2514:             || job.admitted_daa != ctx.daa_score
2515:             || job.reward == 0
2516:             || job.escrow_original_debit != job.reward
2517:             || job.escrow_debit_receipt == [0; 64]
2518:         {
2519:             return Err(PalwStateV2Error::BondBudgetRefused(
2520:                 "funded work budget lacks exact native original admission/producer/money".into(),
2521:             ));
2522:         }
2523:         // RFC-0015 §8.3: native funded work is Panel-free; unbudgeted only where no OPV policy requires a budget.
2524:         let Some(mirror) = self.bond_budget_mirror_at(ctx.daa_score) else { return self.unbudgeted_panel_free_v1() };
```

### `consensus/core/src/palw_bond_budget_fold_v1.rs:2560–2580`

```text
2560:     /// principal nonce and paid monetary receipt before this budget is planned.
2561:     pub(super) fn plan_native_text_budget_v29(
2562:         &self,
2563:         ctx: &PalwBlockContextV2,
2564:         producer: PalwBondKeyV2,
2565:         maximum: &crate::palw_native_text_stop_v29::CheckedNativeTextMaximumV29,
2566:         funding: &crate::palw_native_text_stop_v29::NativeTextFundingV29,
2567:         admission: &crate::palw_native_text_stop_v29::CheckedNativeTextAdmissionV29,
2568:     ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
2569:         let bad = |why: &str| PalwStateV2Error::BondBudgetRefused(why.into());
2570:         // RFC-0008 RFC08-2 (dormant `palw_settlement_once_v1`): no signed hard deadline or material TTL to source for this type.
2571:         self.refuse_unsourced_admission_v1(ctx.daa_score, "a native text claim carries no signed hard deadline")?;
2572:         admission.check_current(ctx.block.as_bytes(), ctx.daa_score, maximum, funding).map_err(|why| bad(&why))?;
2573:         let m = maximum.record();
2574:         let r = &funding.request;
2575:         let extras = self.extras.kernel_route.as_ref().ok_or_else(|| bad("text29 budget lacks original consumer environment"))?;
2576:         let final27 = extras.native_final.as_ref().ok_or_else(|| bad("text29 budget lacks trusted Final27 policy"))?;
2577:         let original = extras.funded_execution.as_ref().ok_or_else(|| bad("text29 budget lacks original global21 producer terms"))?;
2578:         let bond = self.state.bonds.get(&producer).ok_or_else(|| bad("text29 original producer bond absent"))?;
2579:         if !matches!(bond.status, PalwBondStatusV2::Active)
2580:             || r.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&producer)
```

### `consensus/core/src/palw_bond_budget_fold_v1.rs:2630–2647`

```text
2630:         producer: PalwBondKeyV2,
2631:         class: &crate::palw_native_execution_v39::CheckedNativeClassV39,
2632:         maximum: &misaka_palw_kernel::flat_job39::CheckedJobMaximumV39,
2633:         quota: &misaka_palw_kernel::flat_quota11::owning39::CheckedQuotaV39,
2634:         admission: &crate::palw_native_job_v39::CheckedNativeFundingV39,
2635:     ) -> Result<Option<PalwBudgetPlanV1>, PalwStateV2Error> {
2636:         let bad = |why: &str| PalwStateV2Error::BondBudgetRefused(why.into());
2637:         // RFC-0008 RFC08-2 (dormant `palw_settlement_once_v1`): no signed hard deadline or material TTL to source for this type.
2638:         self.refuse_unsourced_admission_v1(ctx.daa_score, "a native job claim carries no signed hard deadline")?;
2639:         let extras = self.extras.kernel_route.as_ref().ok_or_else(|| bad("native39 budget lacks original native environment"))?;
2640:         let owning = extras
2641:             .native_execution39
2642:             .as_ref()
2643:             .filter(|f| f.activation != crate::config::params::ForkActivation::never() && f.activation.is_active(ctx.daa_score))
2644:             .ok_or_else(|| bad("native39 budget lacks active owning policy"))?;
2645:         let final27 = extras
2646:             .native_final
2647:             .as_ref()
```

### `consensus/core/src/palw_bond_budget_fold_v1.rs:2732–2751`

```text
2732:         ctx: &PalwBlockContextV2,
2733:         producer: PalwBondKeyV2,
2734:         model: Hash64,
2735:         quanta: u64,
2736:         weight_per_quantum: u128,
2737:         exposure_sompi: u128,
2738:     ) -> Result<PalwBudgetPlanV1, PalwStateV2Error> {
2739:         let bad = |why: &str| PalwStateV2Error::BondBudgetRefused(why.into());
2740:         // RFC-0008 RFC08-2 (dormant `palw_settlement_once_v1`): no signed hard deadline or material TTL to source for this type.
2741:         self.refuse_unsourced_admission_v1(ctx.daa_score, "a native mining claim carries no signed hard deadline")?;
2742:         if self.bond_budget_mirror_at(ctx.daa_score).is_none() {
2743:             return Err(bad("native39 mining requires the ADR-0176 bond budget in force"));
2744:         }
2745:         let carve = self.params.worker_carve_at(ctx.subsidy, self.extras.escrow_carve);
2746:         let block_units = PALW_BUDGET_BLOCK_UNIT_V1.checked_mul(quanta).ok_or_else(|| bad("native39 block units overflow"))?;
2747:         let reward_sompi = carve.checked_mul(quanta).ok_or_else(|| bad("native39 reward ask overflow"))?;
2748:         let final_weight = weight_per_quantum.checked_mul(quanta as u128).ok_or_else(|| bad("native39 weight ask overflow"))?;
2749:         let liability_sompi = self.bond_formula_native_mining_liability_v39(ctx, reward_sompi, final_weight, block_units, exposure_sompi);
2750:         self.plan_bond_budget_v1(
2751:             ctx,
```

<a id="s09"></a>

## S09 — 各feeのUnsourcedとzero-amount終端

### `consensus/core/src/palw_monetary_material_fold_v1.rs:782–811`

```text
782:                 // RFC-0008 once-only settlement (dormant): one provider's prepaid fee of one consent is paid once across every
783:                 // consumer; a fee another consumer settled is marked queued here without moving value again.
784:                 let once = {
785:                     use crate::palw_settlement_once_v1::{
786:                         PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
787:                     };
788:                     builder.settle_once_liable_v1(
789:                         ctx.daa_score,
790:                         PalwSettlementObligationV1::with_party(
791:                             PalwSettlementLegV1::MaterialProviderFee,
792:                             provider.fee_context.as_bytes(),
793:                             crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&provider.provider),
794:                             0,
795:                         ),
796:                         PalwSettlementConsumerV1::MonetaryMaterial,
797:                         provider.prepaid_fee_sompi,
798:                         PalwSettlementDebitV1::Held { amount: provider.prepaid_fee_sompi },
799:                         // RFC08-2: the provider stands behind an event-relative covenant (service until every right terminated and
800:                         // the last payment + W), which has no finite liability horizon or signed hard deadline an at-once fee could
801:                         // be checked against — refused past the fence, never defaulted.
802:                         super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
803:                             "a material provider's fee: its covenant's liability horizon is event-relative (no finite horizon or hard deadline)",
804:                         ),
805:                     )?
806:                 };
807:                 if !once.pays() {
808:                     provider.fee_queued = true;
809:                     continue;
810:                 }
811:                 builder.settle_source_income_v1(
```

### `consensus/core/src/palw_kernel_route_fold_v1.rs:950–974`

```text
950:                 );
951:                 already_settled.insert(ordinal + 2);
952:             } else {
953:                 // RFC08-2: outside a paid V2 claim past the trap fence a drawn verifier stands behind no claim of its own for its
954:                 // attestation, so its fee has no liability terms to source — refused past the fence, never defaulted.
955:                 let once = builder.settle_once_liable_v1(
956:                     now,
957:                     obligation,
958:                     consumer,
959:                     fee,
960:                     PalwSettlementDebitV1::Escrow { poster, amount: fee },
961:                     super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
962:                         "RFC07-5: a drawn verifier's own liability horizon for its attestation is undefined",
963:                     ),
964:                 )?;
965:                 if !once.pays() {
966:                     if strict {
967:                         return Err(refused(match once {
968:                             super::palw_settlement_once_fold_v1::PalwSettleOnceV1::Refused(why) => why.name(),
969:                             _ => "the verifier's check fee for this slot was already settled by another consumer",
970:                         }));
971:                     }
972:                     already_settled.extend([ordinal + 1, ordinal + 2]);
973:                     continue;
974:                 }
```

### `consensus/core/src/palw_kernel_route_fold_v1.rs:1062–1089`

```text
1062:                     // **RFC-0008 once-only settlement (RFC08-1; dormant `palw_settlement_once_v1`)**: the promise `(context,
1063:                     // producer)` is paid once across every consumer and batch, not only within this batch (`fee_paid`). Every fee
1064:                     // instruction is strict, so a repeat or a refusal refuses the batch. RFC08-2: the sponsor-signed terms carry a
1065:                     // hold and a final-by DAA but a beacon source has no claim reservation, X3 deadline or material TTL — no
1066:                     // authentic source for the predicate's terms, so past the fence the payout is refused, never defaulted.
1067:                     let once = builder.settle_once_liable_v1(
1068:                         now,
1069:                         crate::palw_settlement_once_v1::PalwSettlementObligationV1::with_party(
1070:                             crate::palw_settlement_once_v1::PalwSettlementLegV1::SourceFeePromise,
1071:                             context,
1072:                             s.bond,
1073:                             0,
1074:                         ),
1075:                         match (exec_root.is_some(), strict) {
1076:                             (true, _) => crate::palw_settlement_once_v1::PalwSettlementConsumerV1::ExecKernelFee,
1077:                             (false, true) => crate::palw_settlement_once_v1::PalwSettlementConsumerV1::KernelObject,
1078:                             (false, false) => crate::palw_settlement_once_v1::PalwSettlementConsumerV1::KernelTick,
1079:                         },
1080:                         s.amount,
1081:                         crate::palw_settlement_once_v1::PalwSettlementDebitV1::Prepaid { sponsor: funding.terms.sponsor, amount: s.amount },
1082:                         super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
1083:                             "a beacon source's fee: no claim reservation, X3 deadline or material TTL backs the source",
1084:                         ),
1085:                     )?;
1086:                     match once {
1087:                         // Withheld by the predicate (a policy outcome, not an inconsistency): nothing is debited or paid; the
1088:                         // obligation is closed at zero.
1089:                         super::palw_settlement_once_fold_v1::PalwSettleOnceV1::Refused(_) => continue,
```

### `consensus/core/src/palw_settlement_liability_fold_v1.rs:349–379`

```text
349:     /// **Settle once, under the combined predicate** (RFC08-1 + RFC08-2). Below the fence: `Dormant`, nothing read. A repeat:
350:     /// `AlreadySettled` (the predicate is not re-run for a closed obligation). Otherwise the predicate runs BEFORE any value moves:
351:     /// refused, the obligation is closed with a zero-amount marker (the journal records the refusal; nothing is paid, now or by
352:     /// another consumer) and `Refused` is returned; admitted, the marker is written as by
353:     /// [`TransitionBuilder::settle_once_v1`] and `First` is returned.
354:     pub(super) fn settle_once_liable_v1(
355:         &mut self,
356:         now: u64,
357:         obligation: PalwSettlementObligationV1,
358:         consumer: PalwSettlementConsumerV1,
359:         amount: u64,
360:         debit: PalwSettlementDebitV1,
361:         leg: PalwLegLiabilityV1,
362:     ) -> Result<PalwSettleOnceV1, PalwStateV2Error> {
363:         if !self.params.settlement_once_active_at(now) {
364:             return Ok(PalwSettleOnceV1::Dormant);
365:         }
366:         // RFC-0006: the marker lookup's owned 64-byte key, admitted before it exists.
367:         if self.native_writers_tracking_v1() {
368:             let mut q = super::native_writers::NativeWriteQuoteV1::new("settlement-once marker lookup key");
369:             q.exact::<u8>(64)?;
370:             self.admit_native_write_v1(q)?;
371:         }
372:         if let Some(row) = super::palw_settlement_once_fold_v1::settled_row_v1(&self.state, &obligation)? {
373:             return Ok(PalwSettleOnceV1::AlreadySettled(row));
374:         }
375:         if let Some(refusal) = self.leg_liability_refusal_v1(now, &leg, amount) {
376:             self.settle_once_v1(now, obligation, consumer, 0, debit)?;
377:             return Ok(PalwSettleOnceV1::Refused(refusal));
378:         }
379:         self.settle_once_v1(now, obligation, consumer, amount, debit)
```

<a id="s10"></a>

## S10 — EXEC native統合試験のadmission seam

### `consensus/src/pipeline/virtual_processor/processor.rs:283–313`

```text
283:         .lock()
284:         .unwrap()
285:         .iter()
286:         .filter(|(_, from)| *from <= daa_score)
287:         .map(|(root, _)| *root)
288:         .collect();
289:     roots.sort();
290:     roots
291: }
292: 
293: /// **RFC-0008 v2 (X8R), TEST ONLY: the classes whose REAL attempts skip the model-registry, seating and per-bond-share gates** — the
294: /// stand-in, in the composed EXEC-slice × G14 run (`g14_kernel_route_e2e::conformance::exec_slices`), for the admission of the session's
295: /// REAL root. **Re-scoped by the Lead's GAP-81 decision (2026-10-10):** a V2 REAL claim of an onboarded class earns only on the LEGACY
296: /// channel, whose old rules need Panel seat readiness (possession proofs) that this harness cannot produce — so this stands for that
297: /// channel's admission; no reward gate can replace it (the gate grants a V2 root nothing). Named and shaped like
298: /// [`kernel_route_test_attest_artifact_v1`]: height-gated and
299: /// append-only, so every node of one test, replaying the same blocks, waives the same gates at the same block; empty in every build
300: /// that is not a test (`PalwKernelRouteExtrasV1::test_admitted_classes`).
301: #[cfg(test)]
302: static EXEC_V2_TEST_ADMITTED_CLASSES_V1: std::sync::Mutex<Vec<(kaspa_hashes::Hash64, u64)>> = std::sync::Mutex::new(Vec::new());
303: 
304: #[cfg(test)]
305: pub(crate) fn exec_v2_test_admit_class_v1(class_id: kaspa_hashes::Hash64, from_daa: u64) {
306:     let mut list = EXEC_V2_TEST_ADMITTED_CLASSES_V1.lock().unwrap();
307:     if !list.iter().any(|(class, _)| *class == class_id) {
308:         list.push((class_id, from_daa));
309:     }
310: }
311: 
312: #[cfg(test)]
313: fn exec_v2_test_admitted_classes_v1(daa_score: u64) -> Vec<kaspa_hashes::Hash64> {
```

<a id="s11"></a>

## S11 — Remote minerがV1証拠経路を使い、Rule EでSTOP

### `misaka/palw/remote/src/miner.rs:510–570`

```text
510: /// header (`l3_root_at_header_v1`), and the class is at most `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED`, the reason on the L2 line.
511: fn attested_mode_v1(
512:     nodes: &[&dyn RemoteNode],
513:     trust: &MinerTrustV1,
514:     fc: &RemoteForkChoiceV1,
515:     peers: &[PeerViewV1],
516:     views: &[VerifiedChainV1],
517:     template: &Header,
518:     job: &NodeJobFactsV1,
519: ) -> Result<RemoteModeV1, MinerHalt> {
520:     let v = trust.verification.as_ref().expect("called with a verification config");
521:     let candidates = candidate_chains_v1(peers)?;
522:     let mut openings: Vec<PalwForkChoiceOpeningV1> = Vec::new();
523:     let wanted = openings_wanted_v1(&candidates);
524:     for node in nodes {
525:         for chunk in wanted.chunks(PALW_FORK_CHOICE_MAX_BLOCKS_PER_REQUEST_V1) {
526:             // A node that serves nothing is no evidence of anything: the openings another node served may still verify.
527:             if let Ok(served) = node.fork_choice_openings(chunk) {
528:                 openings.extend(served);
529:             }
530:         }
531:     }
532:     let tips: Vec<Hash64> = candidates.iter().map(|c| c.tip_hash()).collect();
533:     let attestations = (fc.attestations)(&tips).unwrap_or_default();
534:     // A valid, fresh attestation of a block no view shows is the hidden-tip signal — a STOP — even though no node served its opening
535:     // (no node that shows the block exists here to serve one). It is checked before the attestations without an opening are dropped.
536:     let now_daa = candidates.iter().map(|c| c.tip.daa_score).max().unwrap_or(0);
537:     for att in &attestations {
538:         if !candidates.iter().any(|c| c.contains(&att.block))
539:             && verify_attestation_v1(att, &fc.issuers, &v.ruleset, now_daa, &fc.limits, &fc.verify_signature).is_ok()
540:         {
541:             return Err(L2StopV1::AttestedBlockHidden(att.block).into());
542:         }
543:     }
544:     // Each attestation with an opening of its block: the one that hashes to the attested root if any node served it, else any (refused
545:     // by name inside the verdict). An attestation of a shown block that no node opens leaves that block unweighed.
546:     let evidence: Vec<ForkChoiceEvidenceV1> = attestations
547:         .into_iter()
548:         .filter_map(|attestation| {
549:             let mut of_block = openings.iter().filter(|o| o.leaf.block == attestation.block);
550:             let first = of_block.clone().next().copied();
551:             let opening = of_block.find(|o| o.committed_root() == attestation.committed_root).copied().or(first)?;
552:             Some(ForkChoiceEvidenceV1 { attestation, opening })
553:         })
554:         .collect();
555:     let input = L2InputV1 {
556:         views: peers,
557:         evidence: &evidence,
558:         chain_openings: &openings,
559:         trusted: &fc.issuers,
560:         ruleset: &v.ruleset,
561:         rules: &fc.rules,
562:         limits: &fc.limits,
563:     };
564:     let verdict = verify_fork_choice_v1(&input, &fc.verify_signature)?;
565:     let node = nodes.first().ok_or(VerifyErrorV1::Empty)?;
566:     let (bond, class) = match &verdict {
567:         L2VerdictV1::Established { chosen, .. } => {
568:             check_template_v1(template, chosen)?;
569:             let tip = chosen.tip_hash();
570:             let (bh, bonds) = node.state_proof(tip, "bonds").map_err(VerifyErrorV1::Proof)?;
```

### `misaka/palw/remote/src/l2.rs:305–322`

```text
305:     Some(PalwForkChoiceOpeningV1 { leaf: PalwForkChoiceLeafV1::decode(leaf)?, inner_root: inner_root.parse::<Hash64>().ok()? })
306: }
307: 
308: /// **RFC-0018 GAP-R4 — a header's committed opening in ANY form**, checked in the grammar the client's own ruleset (`params`)
309: /// selects at `point`: flat, the v1 leaf, the rule-E leaf v2, or the rule-E leaf v3 (rule E v2 capital and Gate C certification
310: /// state). The same function the node's v39 anchor verifier calls (`palw_verify_committed_leaf_v2`), so node and client accept exactly
311: /// the same openings; past rule E it is what a client reads instead of [`opening_from_wire_v1`], whose v1 leaf a rule-E header does
312: /// not commit.
313: pub fn verify_committed_opening_v2(
314:     params: &Params,
315:     committed_root: &Hash64,
316:     point: &PalwForkChoicePointV1,
317:     leaf: &[u8],
318:     inner_root: &Hash64,
319: ) -> Result<kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::PalwCommittedLeafViewV2, String> {
320:     kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::palw_verify_committed_leaf_v2(params, committed_root, point, leaf, inner_root)
321:         .map_err(|e| format!("the committed opening does not verify in the client's form: {e:?}"))
322: }
```

### `misaka/palw/remote/src/l2.rs:933–949`

```text
933:                     return Err(L2StopV1::Sealed { tip: x.chain.tip_hash(), depth, finality: input.rules.finality_depth });
934:                 }
935:             }
936:         }
937:     }
938: 
939:     // A comparator the v1 leaf cannot feed decides here: the openings do not carry its inputs.
940:     if daas.iter().any(|d| ForkChoiceRulesV1::active(input.rules.rule_e, *d)) {
941:         return Err(L2StopV1::LeafV1Insufficient(
942:             "ADR-0178 rule E reads inputs the v1 leaf does not carry; its leaf (v2) ships under its fence",
943:         ));
944:     }
945: 
946:     // The unique robust winner, by the node's own functions.
947:     let winners: Vec<usize> = (0..walked.len())
948:         .filter(|&i| {
949:             (0..walked.len())
```

<a id="s12"></a>

## S12 — Panel production historyとreceipt clock

### `consensus/core/src/palw_panel_v3_fold_v1.rs:133–164`

```text
133: /// The history a proof is judged against: the branch up to a block's selected parent.
134: pub struct ChainPanelBeaconHistoryV1<'a> {
135:     state: &'a PalwChainStateV2,
136:     base_class: Hash64,
137:     engine: Option<&'a PermissionlessPanelStateV1>,
138:     source: &'a PalwPanelV3BeaconSourceV1,
139:     tip_position: u64,
140: }
141: 
142: impl<'a> ChainPanelBeaconHistoryV1<'a> {
143:     pub fn new(
144:         state: &'a PalwChainStateV2,
145:         base_class: Hash64,
146:         engine: Option<&'a PermissionlessPanelStateV1>,
147:         source: &'a PalwPanelV3BeaconSourceV1,
148:         tip_position: u64,
149:     ) -> Self {
150:         Self { state, base_class, engine, source, tip_position }
151:     }
152: 
153:     /// Selected-approval-authorized route events. An optimistic Final alone
154:     /// exports none. Invalid route rows cannot derive a beacon and remain a
155:     /// non-fraud `BeaconUnavailable` outcome.
156:     fn route_works(&self) -> Vec<AttributedWorkV1> {
157:         self.state.kernel_route.as_ref().and_then(|route| route.beacon_events_v1().ok()).unwrap_or_default()
158:     }
159: }
160: 
161: /// A V2 settlement event as an attributed work: the V2 lattice records no consumer, and its producer is the claim's bond; every
162: /// such event is `PanelLicensed`, which the contract refuses for a Panel draw whatever its attribution.
163: fn attributed_v2(event: WorkFinalEventV1) -> AttributedWorkV1 {
164:     AttributedWorkV1 { event, attribution: SourceAttributionV1 { producer_id: [0u8; 64], consumer_id: RootV1::Absent } }
```

### `consensus/core/src/palw_panel_v3_fold_v1.rs:504–544`

```text
504:     /// **The receipt clock never runs against a pending accusation — structural, G14.** A Panel's receipt window
505:     /// (and the redraw and `PanelUnavailable` expiry it ends in) must not pre-empt a DA default a public bond has already
506:     /// demanded: a colluding producer and Panel could otherwise let the window lapse, void the claim uncharged and close the
507:     /// session neutrally (a void closes every session without conviction). So the engine's clock is PAUSED while ANY session
508:     /// on the claim is open — seat or non-seat; V2 itself pauses only for seat sessions (V3S-08), which is the narrower rule
509:     /// this strengthens — and so is it while a court session is open; a session's close re-bases the window to start no
510:     /// earlier than that close. The pause is bounded:
511:     /// a claim takes at most three non-seat sessions open at once and sixteen over its life, each at most the disclose window.
512:     /// V2's own phase anchor (which a seat session's pause credit moves) and the old court's `DefaultDisputed` are read too.
513:     ///
514:     /// The pause is [`PalwChainStateV2::palw_accusation_pending_v1`] — the one G14 predicate every V3 non-fraud end reads (a DA
515:     /// session, a court session, `DefaultDisputed`).
516:     fn receipt_clock(&self, claim: &Hash64) -> Option<ReceiptClockV1> {
517:         if self.builder.state.palw_accusation_pending_v1(claim) {
518:             return Some(ReceiptClockV1::Paused);
519:         }
520:         let record = self.builder.state.da_claims.get(claim);
521:         match self.builder.state.claims.get(claim).map(|claim| &claim.phase) {
522:             Some(PalwClaimPhaseV2::PanelBound { bound_daa }) => Some(ReceiptClockV1::Running {
523:                 bound_daa: (*bound_daa).max(record.and_then(|record| record.last_closed_daa).unwrap_or(0)),
524:             }),
525:             _ => None,
526:         }
527:     }
528: 
529:     fn consumer_service_limits(&self, id: &Hash64) -> Option<(u64, u32)> {
530:         let claim = self.builder.state.claims.get(id)?;
531:         self.builder.params.panel_lifetime_v2_at(claim.accepted_daa)?;
532:         match crate::palw_panel_lifetime_v2::panel_lifetime_claim_v2(&self.builder.state, self.builder.params, id, claim) {
533:             Ok(Some(row))
534:                 if row.total_paused_daa <= row.terms.maximum_pause_daa
535:                     && self
536:                         .engine
537:                         .claim(id)
538:                         .is_some_and(|record| record.claim.immutable_fields.as_bytes() == row.terms.immutable_fields) =>
539:             {
540:                 Some((row.terms.service_until_daa, row.terms.maximum_retries))
541:             }
542:             // Missing/corrupt authority is no permission to draw another window.
543:             _ => Some((0, 0)),
544:         }
```

<a id="s13"></a>

## S13 — RFC11 full-task/full-contextの分母と未完了分類

### `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md:684–719`

```text
684: ## 18. Coverage target restated — 2026-10-08 (user decision)
685: 
686: The 2026-10-08 census (`docs/design/palw/tir/hf-census-report-2026-10-08.md`) measured 3,117,871 public repositories:
687: 
688: - shape-ready (full task, declared context, shape depth): **16.65%** of `D_all` (one-sided 95% lower bound 15.74%);
689: - software-closable failures: 34.84%;
690: - external failures under the current classification: 48.51% (missing weights 18.93%, gated 1.71%, adapter base missing 7.58%, no model task or config 20.28%).
691: 
692: So even with every software failure closed, `D_all` tops out at 51.49% under today's snapshot and buckets. The 90% bar of §0/§7/§10 over `D_all` is unreachable *by construction*. That says nothing about how much of the actual model population MISAKA can serve. It is a property of what the denominator counts.
693: 
694: **Primary target (replaces `registered_full_task / D_all ≥ 0.90` as the acceptance bar):** at least 90%, as a one-sided 95% lower bound, of the public Hugging Face repositories that form a **complete model** are registered on chain as their complete advertised task and advertised context, without model-specific consensus changes.
695: 
696: `D_complete` is the set of public repositories at a pinned snapshot whose weights and the information needed to build the model can be obtained. That means weights present or resolvable, a base resolvable for adapters, a task and config that are declared or deterministically derivable, and access permitted.
697: 
698: **Denominator rules (anti-gaming):**
699: 
700: - **Architectures and tasks MISAKA does not support stay in `D_complete`** and count as failures. This applies to any architecture or task (Qwen, Llama, GLM, DeepSeek, vision, audio, classification, …). Excluding a repository because MISAKA cannot handle it is forbidden; otherwise 90% could be reached by choosing easy models.
701: - **External failures are not permanent.** A repository moves into `D_complete` when its gap is resolved by a generic, auditable rule. Examples:
702:   - weights it references in another repository, resolved at a pinned revision;
703:   - a LoRA/adapter whose base is identified and pinned;
704:   - a task fixed safely from its weights and metadata;
705:   - a gated model whose access and redistribution rights are actually held.
706: 
707:   Each reclassification rule is published with the census, and its effect is reported separately.
708: 
709: **Always publish three numbers,** with their lower bounds, never only the flattering one:
710: 
711: 1. coverage over **all public repositories** (`D_all`);
712: 2. **full-task, full-advertised-context on-chain registration** over `D_complete`;
713: 3. the share that reached **mining and a funded `Final` reward**.
714: 
715: **Evidence standard.** Frontend or shape-ready coverage never counts as registration. A sampled success is a real checkpoint → `.palwart` → G14 verification (public prosecution complete for its route) → devnet/chain registration. Alternatively, it is independently reproduced equivalence to an already registered class (§11.2). Text-only, short-context, synthetic-beacon, unit-test and dormant registrations are reported in their own columns. This section supersedes the acceptance bar of §0, §7, §10 and §14's "Broad coverage" row. `D_all` stays the first published number.
716: 
717: ## Mission alignment amendment — 2026-10-07
718: 
719: §13、§15、§16のVerificationPlan/kernel admission、9B-8k/long-context/全taskのcoverageに、fresh non-seat public-bond prosecutionの実測を追加する。kernel-only certificateや正直なFinalだけを成功と数えない。非公開weights/input/state、FOLD prefixやfused preimageが必要なprofileは、公開取得または承認済み公開証明経路が完結するまで新しい報酬対象にしない。HF coverage、ε_check、source fidelityとこのgateは別の指標である。
```

### `misaka/palw/sdk/src/census/onboarding.rs:1–30`

```text
1: //! **Every frontend refusal has a machine-readable onboarding class** (RFC-0011 §16.2 outcomes; `misaka-palw-challenge`'s
2: //! `OnboardingFailureV1`; the model onboarding matrix of 2026-10-08): *who has to change something* for the model to move on.
3: //!
4: //! ```text
5: //! FRONTEND_REQUIRED          the importer / lowerer / adapter data cannot read or lower it although the IR could express it
6: //! KERNEL_EXTENSION_REQUIRED  a semantic primitive, relation, memory rule or court is missing from every shipped kernel
7: //! KERNEL_NOT_ACTIVE          the kernel exists and the fence that activates it is not in force
8: //! LAYOUT_REQUIRED            no exact layout is pinned (pack → preflight → registration)
9: //! RESOURCE_REFUSED           a mandatory node / DA / court / census bound is over its ceiling (never answered by raising a cap)
10: //! PROFILE_REQUIRED           the declared task has no canonical job profile (an RFC-level, consensus decision; recorded as a
11: //!                            KERNEL_EXTENSION_REQUIRED in the lifecycle, because the court path of the job is what is missing)
12: //! EXTERNAL_BLOCKER           the source material is missing, gated, unpinned or malformed (nothing in this tree can supply it)
13: //! NOT_RUN                    a depth or data the run did not read: **not a verdict**, never a gap of any kind
14: //! UNMAPPED                   a code this table does not know. Never a pass; a test keeps the published codes out of it.
15: //! ```
16: //!
17: //! The rule that matters most is the last-but-one: **a gate that was not run because the bytes were not read is not a semantic
18: //! gap.** `NOT_RUN_NEEDS_TENSOR_DATA` (a GGUF's `rope_freqs.weight`, 128–256 bytes the header census does not fetch),
19: //! `NOT_RUN_NEEDS_WEIGHTS`, `NOT_RUN_DEPTH_HEADERS`, `NOT_RUN_AFTER_<GATE>` and the rest of the `NOT_RUN_*` family are
20: //! [`GapClassV1::NotRun`], and [`GapClassV1::is_semantic_gap`] is `false` for every class but `KERNEL_EXTENSION_REQUIRED` and
21: //! `PROFILE_REQUIRED`. A `TENSOR_MISSING` is the binder's (`FRONTEND_REQUIRED`), never the kernel's.
22: //!
23: //! Nothing here reads a model name. The only table consulted besides the code vocabulary is the feature registry
24: //! (`misaka_palw_tir_lower::model::feature_info`): a feature whose protocol requirement is a [`Requirement::Capability`] is a kernel
25: //! gap, one whose requirement is `None` (every `Missing` and `Specified` feature of this build) is the lowerer's.
26: 
27: use super::codes::{self, Gate};
28: use crate::preflight::Blocker;
29: use misaka_palw_tir_lower::model::{Requirement, feature_info};
30: use serde::{Deserialize, Serialize};
```

### `misaka/palw/sdk/src/census/onboarding.rs:90–143`

```text
90:     /// The model's *semantics* need something the chain does not have (a relation, a court path, a job profile). Only these two
91:     /// classes say so; a frontend, layout, resource, external or not-run outcome never does.
92:     pub const fn is_semantic_gap(self) -> bool {
93:         matches!(self, GapClassV1::KernelExtensionRequired | GapClassV1::ProfileRequired)
94:     }
95: 
96:     /// A verdict about the model, as opposed to a depth or data this run did not read.
97:     pub const fn is_verdict(self) -> bool {
98:         !matches!(self, GapClassV1::NotRun | GapClassV1::Unmapped)
99:     }
100: 
101:     /// The class of a `misaka_palw_kernel::outcome::RegistrationOutcomeV1::code()` — the same split as its
102:     /// [`coverage_bucket`](misaka_palw_kernel::outcome::RegistrationOutcomeV1::coverage_bucket) (`ELIGIBLE_AT` is no gap: `None`).
103:     pub fn of_registration_outcome(code: &str) -> Option<GapClassV1> {
104:         Some(match code {
105:             "ELIGIBLE_AT" => return None,
106:             "FRONTEND_REQUIRED" | "PLAN_FORGED" => GapClassV1::FrontendRequired,
107:             "KERNEL_EXTENSION_REQUIRED" => GapClassV1::KernelExtensionRequired,
108:             "KERNEL_NOT_ACTIVE" => GapClassV1::KernelNotActive,
109:             // The bucket `resource_or_lifecycle_gap` of §16.4: bounds, capacity, and a plan that omits a relation or a budget.
110:             "BOUNDS_EXCEEDED" | "READINESS_OR_CAPACITY" | "INCOMPLETE_COVERAGE" => GapClassV1::ResourceRefused,
111:             "EXTERNAL_BLOCKER" => GapClassV1::ExternalBlocker,
112:             _ => GapClassV1::Unmapped,
113:         })
114:     }
115: }
116: 
117: /// `NOT_RUN_*` — a depth, bytes or a chain this run did not have. Never a verdict.
118: pub fn is_not_run_code(code: &str) -> bool {
119:     code.starts_with("NOT_RUN")
120: }
121: 
122: /// The class of a feature a refusal names (`ARCH_NEEDS_FEATURE(<arg>)`, the census's `FEATURE_C`): the registry's protocol
123: /// requirement when the argument is a feature id, else (`an adapter for \`X\``, a description) the lowerer's.
124: fn class_of_feature(arg: Option<&str>, general_primitive_named: bool) -> GapClassV1 {
125:     if let Some(info) = arg.and_then(feature_info) {
126:         return match info.protocol {
127:             Requirement::Capability { .. } => GapClassV1::KernelExtensionRequired,
128:             Requirement::None => GapClassV1::FrontendRequired,
129:         };
130:     }
131:     // Not in the registry: a gap that names a *general primitive that would close it* is a protocol gap (the frontend's own
132:     // refusals never do — "a data adapter can map a missing key or tensor name, never a missing computation").
133:     if general_primitive_named { GapClassV1::KernelExtensionRequired } else { GapClassV1::FrontendRequired }
134: }
135: 
136: /// **The class of one census code** (the gate it was found at, its `blocking` code, its argument and evidence).
137: ///
138: /// Total: a code outside the vocabulary is [`GapClassV1::Unmapped`], never a guess.
139: pub fn classify_census_code_v1(gate: Gate, code: &str, arg: Option<&str>, evidence: &[String]) -> GapClassV1 {
140:     use GapClassV1::*;
141:     if is_not_run_code(code) {
142:         return NotRun;
143:     }
```

<a id="s14"></a>

## S14 — RFC12受入driverに不足するRPC/failpoint/refund CLI

### `scripts/drills/misaka-rfc12-acceptance.py:55–78`

```text
55: EVIDENCE_KIND = 'rfc12-acceptance-evidence'
56: EVIDENCE_VERSION = 1
57: PRIMARY = ('A', 'B', 'JOIN')
58: # The checkboxes this driver owns; checkbox 8 (RFC-0008 costs) belongs to misaka-rfc08-cost-measure.py.
59: BOXES = (1, 2, 3, 4, 5, 6, 7, 9)
60: SOURCE_OF = {'B': 'A', 'JOIN': 'A'}
61: MARKER = 'rfc12-private-probe-v1.json'
62: # Reads this harness needs that no node serves in this tree. Each is reported in the evidence;
63: # predicates that depend on one are BLOCKED, never inferred.
64: # The paid-claim read (op 247, getPalwPaidClaim) is served since 2026-10-15; a node built before it still makes its
65: # predicates BLOCKED (rfc12_acceptance_observe.paid_claim_value).
66: # RFC-0012 §4.4 (2026-10-16): the per-coinbase DNS share is classified from coinbase outputs and the bonds' own
67: # StakeBond transactions (rfc12_retirement_reconciliation), and --private-probe-retirement-daa gives pre-fence DNS
68: # history, so those two gaps are closed. The reserve balance itself still has no read.
69: NODE_GAPS = [
70:     dict(read='DNS security reserve balance per block (reserve_balance_store)', proposed=None,
71:          consumers=['reserve reconciliation across the retirement boundary (checkbox 7); drip outputs are classified']),
72:     dict(read='in-process crash failpoints between import WAL barriers', proposed=None,
73:          consumers=['exact crash points; this harness kills at observed phases/files only (checkbox 3)']),
74:     dict(read='a first-party generator of the paid-work request from a live authenticated snapshot', proposed=None,
75:          consumers=['usefulWork.requestCommand stays an operator input']),
76:     dict(read='virtual UTXO point read by outpoint (deposit lock / synthetic withdrawal outpoint)', proposed=None,
77:          consumers=['bridge exactly-once is judged by effects at dedicated addresses instead']),
78: ]
```

### `scripts/drills/misaka-rfc12-acceptance.py:980–1004`

```text
980:                   'the refund lock reaches its timeout')
981:         late = complete.cli_prefix(drill, cli, salt) + ['evm', 'claim', '--outpoint=' + refund_lock['outpoint']]
982:         self.judge(5, 'expired-lock-claim-refused', J.refused, self.expect_refusal('REFUND_LOCK_LATE_CLAIM', late),
983:                    'a claim at/after the refund timeout')
984:         refund = bridge['refund']
985:         if refund['commands']:
986:             self.fixtures('REFUND', refund['commands'])
987:             low, high = refund['amountSompi'] - refund['maximumRefundFeeSompi'], refund['amountSompi']
988: 
989:             def refunded():
990:                 rows = self.destination_set(self.live(PRIMARY), 'refund-after', refund['destinationAddress'])
991:                 for row in rows:
992:                     J.exactly_one_new_utxo(self.bridge['destinationsBefore']['refund'], row['value']['entries'], low, high)
993:                 return rows
994:             rows = self.wait(refunded, 'the refund materializes once on every process')
995:             self.rows.extend(rows)
996:             for row in rows:
997:                 self.judge(5, 'refund-exactly-once/%s' % P.ROLE_NAMES[row['role']], J.exactly_one_new_utxo,
998:                            self.bridge['destinationsBefore']['refund'], row['value']['entries'], low, high)
999:         else:
1000:             self.block(5, 'refund-exactly-once', 'no refund command: the CLI has no deposit-refund subcommand and the plan supplies none')
1001:         evm = self.wait(lambda: self.evm_set(self.live(PRIMARY), 'refund-evm', [refund['evmAddress']]), 'refund EVM account read')
1002:         self.rows.extend(evm)
1003:         self.judge(5, 'refunded-lock-never-credited', J.backing, self.bridge['beforeWei'][refund['evmAddress']],
1004:                    evm[0]['value']['balances'][refund['evmAddress']], 0, 0, 0)
```

<a id="s15"></a>

## S15 — Rule E rootのnodeへの接続は実装済み

### `consensus/src/pipeline/virtual_processor/processor/palw.rs:8042–8049`

```text
8042:     /// **RFC-0009 L2: the root a header commits for `state`** — the ONE spelling the chain walk, the template, the proof server, the
8043:     /// pruning-point import and the native-settlement walk share (`palw_fork_choice_commitment_v1`). Below the fence (every shipped
8044:     /// network) it is `state.state_root()`.
8045:     pub(crate) fn palw_committed_root_v1(&self, state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2) -> Hash64 {
8046:         // RFC-0018 GAP-R4: the form the state's own point selects — flat, leaf v1, rule-E leaf v2, or (rule E with rule E v2 / Gate C)
8047:         // leaf v3 — under the node's own ruleset. Equal to `palw_committed_state_root_v1` wherever rule E is not in force (every
8048:         // shipped network: the commitment fence itself is `None` there, so this is `state.state_root()`).
8049:         kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::palw_committed_state_root_v2(state, &self.native_redemption_params_v33)
```

<a id="s16"></a>

## S16 — Genesis long-context fidelity / artifact残件

### `docs/design/palw/genesis8-status.md:177–206`

```text
177: The pipeline keeps a 30 GB free-disk floor (`--min-free-gb`) for the validation agent that shares the disk.
178: 
179: RAM (unmeasured estimates, §4.1 table): calibration runs layer by layer, one occurrence's params resident; at 32K one
180: full-sequence carry (#6 640 MiB), one attention layer's history (#6 256 MiB), per-query scores only. The 9B 8k run measured 21.4 GB
181: RSS with an mmapped checkpoint (mapped pages count). Measure with `--ram-accounting`.
182: 
183: ## 5. Open items
184: 
185: * **#8's fit at 32K is not measurable on this Mac:** no tool compares the PALWQ361 engine with the f32 reference at windows along a
186:   long sequence (`qwen36-run --reference` scores the converter's 8-token prompt), the f32 reference over 32K positions of the
187:   35B-A3B MoE is another full read of the GGUF and hours of compute, and the integer engine needs the ~37 GB artifact on local
188:   disk against 32 GB of RAM. Recorded as not measured in the run; the measuring pass's log carries the reference's next-token
189:   check. Needs a ≥ 64 GB host and a windowed comparator for the legacy engine (not written).
190: * **#8's calibration** is the legacy converter's own 8-token prompt with the analytic horizon correction of the GDN state exponents
191:   to 32,768 (`qwen36_calibrate::horizon_growth_bits`), not a 32K sequence: the freeze-v1 §5.2 rule binds the TIR route, and the
192:   PALWQ361 route has no 32K evidence until the fit above is measured.
193: * **The fit of #5, #6 and #10** is not measured on this host (§4.1); measure it on a host with the source on disk and RAM for the
194:   integer program before the index is relied on.
195: * **#1 at 32K**: the A16 route has no long-context check (qwen25-convert's FAITHFUL report covers its 57-token prompt).
196: * **Registration binds the PALWART3 root**: today a TIR class binds its inventory root; the next network's registration must bind the
197:   container root and the court must open the inner inventory leaves under the inner root it commits. Consensus work, not started.
198: * **K2-TIR activation**: the kernels are implemented, not active; arming them is a fenced decision outside this lane.
199: * **#3** awaits the user's decision; **#6**'s calibration tokenizer is the Qwen3.8 tokenizer from
200:   `huihui-ai/Huihui-Qwen3.8-27B-abliterated@739e3c5b`, checked id by id against the GGUF's own vocabulary before use.
201: * **Past refusal of Huihui-Qwen3.5-9B** (RFC-0011 P1): the fixture stays (`misaka/palw/tir/lower/tests/fixtures/vlm-generic/
202:   huihui-qwen3.5-9b`, `misaka/palw/sdk/tests/coverage_p1_huihui_refusals.rs`). Causes: (a) the conversion fit refusal
203:   (HF_FIT_OUT_OF_TOLERANCE) — fixed by `gdn_core_wide`, which every genesis GDN conversion sets; (b) refusals 1–3 (court clock,
204:   close sizing, logits tile) — resolved under the rules in force, pinned by those tests; (c) refusal 4 (`HELD_CLASS_UNANSWERABLE`) —
205:   a property of the legacy held route, not of the TIR route the 9B uses; (d) `SEAT_MEMORY_SHORT` — a mine/readiness estimate against
206:   the fixed t12 seat tiers (3.5 / 8 GiB), not a conversion refusal: RFC-0006 layer-sharded seats or capable seats address it.
```

### `docs/design/palw/remaining-work-2026-10-16.md:51–59`

```text
51: 全部完了 → 一括検証 → 実ノードでの検証・測定(σ/k、Q/B/R/F、m/k、court 予算、frame の cost unit)→ 出荷 binary で fence を跨ぐ drill → ユーザー承認 → 配備。
52: fence の高さは、条件が揃ってから決める。
53: 
54: ## 6. genesis モデル(RFC-0020 / PALWART3)
55: 
56: - #2 Huihui-Qwen3.5-2B の 32K 変換は、fit の長文チェック(integer 20k/32k 付近)で停止した。再実行は fit から(`genesis10_pipeline.py run --model 2`、変換と record は済み)。
57: - #8 は Q8_0 GGUF からの 32K streaming 変換(`--stream`)が未実行。再量子化であり、品質ガードは 64GB 以上の機械が要る。
58: - #6(27B bf16 GGUF streaming、15〜22 h)、#1 の 32K 再変換、#3/#4/#5/#10 が未着手。
59: - RFC02 の変換 artifact は、main checkout の `target/` 削除で失われた。evidence の sha256 で再生成・照合する。
```

<a id="s17"></a>

## S17 — RFC13既存tool evidenceのスコープ

### `docs/rfc/evidence/0013-onboarding-tool-validation.json:1–198`

```text
1: {
2:   "schema": "misaka.onboarding-tool-evidence.v1",
3:   "environment_date": "2026-10-06",
4:   "source_base_commit": "808baa6b9adcb029e64fffe51fcd84027f82e7c0",
5:   "source_status_at_conformance_capture": "uncommitted tool and RFC changes; not a deployed release",
6:   "source": {
7:     "repo": "mradermacher/Llama-3.2-3B-Instruct-uncensored-GGUF",
8:     "revision": "186911b5658f85bfe68684b83646ba2e89169a0c",
9:     "file": "Llama-3.2-3B-Instruct-uncensored.Q4_K_M.gguf",
10:     "bytes": 2241004096,
11:     "sha256": "1a01383341be9bf04a904e168089401c3f0533b3af4089d3536d8b36e497ccd6"
12:   },
13:   "class": {
14:     "class_id": "b4a2ba784ae3d2680b7770a01fac46bbcde51ff401248e5b44c55fdc24f65f4c8f585edad1ce442b7d4e524159d318a0e8415f1a1a4f9a5ed27a2969fa67c97b",
15:     "inventory_root": "d5a4ef2d6c0ed8a97055b89e178ac38df81771d2e643d95fd5210f497be67be8f857b4c50555196cb8aac52163e51ab2162da495b163d089078c52672e52e2a5",
16:     "layout_digest": "bfd7733e655d96d2e8808078540250b8ddb397692dfa68172f5b382cf444f67eb605ec53216c8c507cdfba13715f3806239c877757be6e951dec72fd64bd6b99",
17:     "declared_file_digest": "2f76528daa676ee6e906c91ff9a018574162d75d5c85471280e6b6ba39aa44ca1cd1b0b4daf9d157bd35708a1d1aa1c4db5404c2d6a29447e23f3aae12703da9",
18:     "declared_file_bytes": 5079273984,
19:     "max_context": 512,
20:     "checkpoint_interval": 65536,
21:     "h_tile": 32,
22:     "commit_tiles": [
23:       64,
24:       64,
25:       64,
26:       64,
27:       64,
28:       64,
29:       64,
30:       64,
31:       64,
32:       64,
33:       64,
34:       64,
35:       64,
36:       64,
37:       64,
38:       512
39:     ],
40:     "state_tiles": [
41:       64,
42:       64
43:     ]
44:   },
45:   "bind_class": {
46:     "status": "PASS",
47:     "new_pack_digest": "e2cb3f0610210f44ff75d943dfd8145151dc128e3133d6e893d376025012ac9b",
48:     "old_pack_unchanged": true,
49:     "class_artifact_unchanged": true,
50:     "admission_or_certification_implied": false
51:   },
52:   "live_preflight": {
53:     "tipDaa": 5775,
54:     "available": true,
55:     "admissible": true,
56:     "processorVerdict": "ADMISSION_OK",
57:     "check": "TIR_ADMISSION_V10",
58:     "registration_submitted": false,
59:     "node_binary_schedule_parity_with_local_tool": "NOT_ESTABLISHED"
60:   },
61:   "tests": {
62:     "runtime_pack": {
63:       "passed": 9,
64:       "failed": 0
65:     },
66:     "ref2_lib": {
67:       "passed": 12,
68:       "failed": 0
69:     },
70:     "ref2_differential": {
71:       "passed": 8,
72:       "failed": 0
73:     },
74:     "ref2_golden": {
75:       "passed": 3,
76:       "failed": 0
77:     },
78:     "calibration_filtered_lib": {
79:       "passed": 5,
80:       "failed": 0
81:     },
82:     "fidelity_binary_check": "PASS",
83:     "existing_rfc11_coverage_audit_check": "PASS; coverage remains UNPROVEN",
84:     "existing_rfc11_coverage_audit_self_test": "PASS",
85:     "streamed_position_progress_and_extended_identity_mutations": "PASS; final runtime_pack rerun: 9 passed, 0 failed"
86:   },
87:   "final_tool_build": {
88:     "status": "PASS",
89:     "profile": "release",
90:     "binary": "palw-class",
91:     "sha256": "a70e96a4fc9e627ab3faeb52218dff117eb237267ae8dfcfa437c3b5aa04a41f",
92:     "progress_and_extended_report_included": true,
93:     "real_artifact_rerun_after_logging_changes": false,
94:     "deployed": false
95:   },
96:   "real_artifact_conformance": {
97:     "status": "PASS",
98:     "tool_sha256": "5fea575e4389e6912b315397dcd60be95a32ed73816f2771ec174903173ff5ef",
99:     "profile": "release",
100:     "vectors": 2,
101:     "positions": 8,
102:     "requested_implementations": [
103:       "reference",
104:       "typed-backend",
105:       "independent"
106:     ],
107:     "size_skip_override": false,
108:     "full_source_fidelity_implied": false,
109:     "pack_fully_verified": false,
110:     "elapsed_seconds": 1228.9,
111:     "user_seconds": 1147.77,
112:     "system_seconds": 72.4,
113:     "maximum_resident_set_bytes": 12828917760,
114:     "peak_memory_footprint_bytes": 13707344008,
115:     "swaps": 0,
116:     "exit_code": 0,
117:     "run_precedes_position_progress_logging_and_peak_tensor_reporting_changes": true,
118:     "report": {
119:       "checks": [
120:         {
121:           "check": "manifest",
122:           "detail": "Llama-3.2-3B-Instruct Uncensored Q4_K_M (e2cb3f0610210f44), schema misaka.palw.runtime-pack.v1",
123:           "status": "PASS"
124:         },
125:         {
126:           "check": "sidecars",
127:           "detail": "2 file(s) match their hashes",
128:           "status": "PASS"
129:         },
130:         {
131:           "check": "adapter",
132:           "detail": "built-in `llama` 4be1f4c6294c049a",
133:           "status": "PASS"
134:         },
135:         {
136:           "check": "descriptors",
137:           "detail": "3 pinned, all equal",
138:           "status": "PASS"
139:         },
140:         {
141:           "check": "converter",
142:           "detail": "built by palw-tir-convert 1.1.0 (misaka.palw.lowering.v1); this build 1.1.0 (misaka.palw.lowering.v1) — the same converter; math libm-v1: the same bytes on every platform",
143:           "status": "PASS"
144:         },
145:         {
146:           "check": "source",
147:           "detail": "1 file(s) match their SHA-256",
148:           "status": "PASS"
149:         },
150:         {
151:           "check": "frontend",
152:           "detail": "level B, spec a3952e897fa73f5e, 9 feature uses, scope: text-generation: token ids → next-token logits; nothing left out",
153:           "status": "PASS"
154:         },
155:         {
156:           "check": "artifact",
157:           "detail": "the artifact declared for testnet-12 (class b4a2ba784ae3d268): file 2f76528daa676ee6 · inventory root d5a4ef2d6c0ed8a9",
158:           "status": "PASS"
159:         },
160:         {
161:           "check": "declared",
162:           "detail": "testnet-12: exact class b4a2ba784ae3d268 (context 512, interval 65536); identity verification, not live admission",
163:           "status": "PASS"
164:         },
165:         {
166:           "check": "conformance",
167:           "detail": "streamed: 2 vector(s), 8 positions, equal on reference, typed-backend, independent (the reference held at most 6012.0 MiB at once)",
168:           "status": "PASS"
169:         },
170:         {
171:           "check": "hf-reference",
172:           "detail": "the pack carries no Hugging Face reference",
173:           "status": "SKIPPED"
174:         }
175:       ],
176:       "ok": true,
177:       "pack": "e2cb3f0610210f44ff75d943dfd8145151dc128e3133d6e893d376025012ac9b",
178:       "verified": false
179:     }
180:   },
181:   "registration_at_conformance_capture": "NOT_SUBMITTED",
182:   "certification_mineability": "NOT_ESTABLISHED",
183:   "panel_started": false,
184:   "consensus_rules_changed": false,
185:   "main_pushed_at_conformance_capture": false,
186:   "publication_fingerprint_check": {
187:     "date": "2026-10-06",
188:     "origin_main_at_check": "d57f32cd28a19063386befca17c0c5c5af48321f",
189:     "consensus_core_tree_on_main_and_source_base": "8801f9bd7fdb763a2bb1b6b9fca443c41bced9bc",
190:     "t12_repin_values": "PASS; no pins rewritten",
191:     "shipped_presets_have_pinned_fingerprints": "PASS",
192:     "params_fingerprint": "5ee7fd8ee019968cf52929b844cf9ddfb1aad500842a89cf04bced8ba4edefb6",
193:     "identity": "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
194:     "schedule": "1678e07359f6727e96224041450a3b1d2aadcf8acd6bb6db0c277ff4d401c9b9",
195:     "live_doctor_node": "PASS; same fingerprint and activation schedule, synced at DAA 5797"
196:   },
197:   "coverage_claim": "UNPROVEN"
198: }
```

<a id="s18"></a>

## S18 — Legacy FP固定4区間のdraw

### `consensus/core/src/palw_fp_interval_v1.rs:1–82`

```text
1: //! **The seat's interval draw** (ADR-0077 Decision 8).
2: //!
3: //! A free-prompt seat no longer fetches the whole capture and hashes it. It draws `k` checkpoint
4: //! intervals from the claim's beacon and its own seat index — unpredictable when the commitment
5: //! was fixed (the beacon does not exist yet, ADR-0044 F4/F5), different per seat — asks the
6: //! executor for each interval's opening, replays it with the class's own kernels and compares
7: //! every row exactly. This module is the draw alone: a pure function of chain facts, pinned by a
8: //! golden vector, so that "which intervals did seat 3 have to check" is a question anyone can
9: //! answer from the chain and nobody can answer before the beacon lands.
10: //!
11: //! The draw is NOT a consensus object: what a seat checked before signing is the seat's own duty
12: //! (the receipt object and its signature are unchanged). It is pinned anyway, because a seat that
13: //! could pick its own intervals could pick the ones an executor knows it will pick.
14: 
15: use kaspa_hashes::Hash64;
16: 
17: fn keyed(domain: &[u8]) -> blake2b_simd::State {
18:     blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
19: }
20: 
21: fn finish(state: blake2b_simd::State) -> Hash64 {
22:     Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
23: }
24: 
25: /// `k`: how many intervals a seat opens per claim. Four, as the sampled-leaf arm it replaces
26: /// drew; a skipped inference is wrong at every interval and any sample catches it, a single
27: /// corrupted row is the court's question and the court is cheap.
28: pub const PALW_FP_SEAT_INTERVAL_SAMPLES_V1: u32 = 4;
29: 
30: pub const PALW_FP_INTERVAL_DRAW_DOMAIN_V1: &[u8] = b"misaka-palw/fp-v3/seat-interval-draw/v1";
31: 
32: /// The intervals seat `seat_index` of the panel bound under `beacon_block` must open for
33: /// `claim_id`: `k` DISTINCT indices in `0..interval_count`, in draw order. When the claim has no
34: /// more than `k` intervals, every interval, in order — a short job is checked whole.
35: ///
36: /// Draw `i` is `H(domain ‖ network_domain ‖ beacon_block ‖ claim_id ‖ seat_index ‖ i)` reduced
37: /// modulo `interval_count`; a repeat is skipped and the next draw taken, so the result never
38: /// depends on anything but those five inputs.
39: pub fn palw_fp_interval_draw_v1(
40:     network_domain: &Hash64,
41:     beacon_block: &Hash64,
42:     claim_id: &Hash64,
43:     seat_index: u8,
44:     k: u32,
45:     interval_count: u32,
46: ) -> Vec<u32> {
47:     if interval_count == 0 || k == 0 {
48:         return Vec::new();
49:     }
50:     if interval_count <= k {
51:         return (0..interval_count).collect();
52:     }
53:     let mut out: Vec<u32> = Vec::with_capacity(k as usize);
54:     let mut draw: u64 = 0;
55:     while out.len() < k as usize {
56:         let mut state = keyed(PALW_FP_INTERVAL_DRAW_DOMAIN_V1);
57:         state.update(network_domain.as_byte_slice());
58:         state.update(beacon_block.as_byte_slice());
59:         state.update(claim_id.as_byte_slice());
60:         state.update(&[seat_index]);
61:         state.update(&draw.to_le_bytes());
62:         let digest = finish(state);
63:         let word = u64::from_le_bytes(digest.as_byte_slice()[..8].try_into().expect("8 bytes"));
64:         let index = (word % interval_count as u64) as u32;
65:         if !out.contains(&index) {
66:             out.push(index);
67:         }
68:         draw += 1;
69:     }
70:     out
71: }
72: 
73: #[cfg(test)]
74: mod tests {
75:     use super::*;
76: 
77:     fn h(v: u64) -> Hash64 {
78:         Hash64::from_u64_word(v)
79:     }
80: 
81:     #[test]
82:     fn a_short_job_is_checked_whole_and_a_long_one_is_sampled_distinctly() {
```

### `kaspad/src/palw_fp_seat.rs:391–428`

```text
391: pub fn palw_fp_seat_verify_openings_v1(
392:     backend: &dyn PalwExecutionBackendV1,
393:     roots: PalwClaimRootsV1,
394:     prompt_token_ids: &[u32],
395:     work_leaves: u64,
396:     draw: &PalwFpSeatDrawV1,
397:     openings: &dyn Fn(u32) -> Vec<Vec<u8>>,
398:     opening_ceiling: usize,
399:     mut on_rejection: impl FnMut(u32, PalwFpOpeningRejectionV1),
400: ) -> PalwFpSeatOutcomeV1 {
401:     let mut unanswered = Vec::new();
402:     for index in &draw.intervals {
403:         let mut answered = false;
404:         for candidate in openings(*index) {
405:             if candidate.len() > opening_ceiling {
406:                 on_rejection(*index, PalwFpOpeningRejectionV1::Oversized);
407:                 continue;
408:             }
409:             match backend.verify_fp_interval_opening(&candidate, roots, *index, prompt_token_ids, work_leaves) {
410:                 PalwFpIntervalVerdictV1::Valid => {
411:                     answered = true;
412:                     break;
413:                 }
414:                 PalwFpIntervalVerdictV1::Fault { leaf_index } => {
415:                     return PalwFpSeatOutcomeV1::Fault { interval_index: *index, leaf_index };
416:                 }
417:                 PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count } => {
418:                     return PalwFpSeatOutcomeV1::FaultInRange { interval_index: *index, first_leaf_index, leaf_count };
419:                 }
420:                 PalwFpIntervalVerdictV1::Mismatch => on_rejection(*index, PalwFpOpeningRejectionV1::Mismatch),
421:                 PalwFpIntervalVerdictV1::Unverifiable => on_rejection(*index, PalwFpOpeningRejectionV1::Unverifiable),
422:             }
423:         }
424:         if !answered {
425:             unanswered.push(*index);
426:         }
427:     }
428:     if unanswered.is_empty() { PalwFpSeatOutcomeV1::Certified } else { PalwFpSeatOutcomeV1::NotVerified { unanswered } }
```

<a id="s19"></a>

## S19 — Court共有上限とその残件テスト

### `consensus/core/src/palw_state_v2/objects.rs:1999–2012`

```text
1999:             // as wide as it is, and this makes the promise it implies — every session reaches it
2000:             // inside its own rung — true by construction. It is a pure state fact, so the fold,
2001:             // the acceptance rehearsal and the sync walk all answer it the same way.
2002:             let open_sessions = builder.state.court_sessions.len() as u64;
2003:             let max_sessions = palw_max_concurrent_court_sessions_v1(builder.params.turn_deadline_daa());
2004:             if open_sessions >= max_sessions {
2005:                 return Err(PalwStateV2Error::CourtSessionsAtCapacity {
2006:                     open: open_sessions,
2007:                     max: max_sessions,
2008:                     turn_deadline_daa: builder.params.turn_deadline_daa(),
2009:                     per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,
2010:                 });
2011:             }
2012:             let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();
```

### `consensus/core/src/palw_state_v2/da_sessions.rs:746–762`

```text
746:         .map_err(|e| refused(e.to_string()))?;
747:     }
748:     // **X3 (REQ-X3-4, `palw_claim_hard_deadline_v1`)**: a court pursuit on a claim accepted past the fence starts only at or before
749:     // the claim's open cutoff and only if `start + B_remaining <= hard`. `Ok` below the fence.
750:     crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(builder.params, claim, ctx.daa_score)
751:         .map_err(|why| refused(why.to_string()))?;
752:     let open_sessions = builder.state.court_sessions.len() as u64;
753:     let max_sessions = palw_max_concurrent_court_sessions_v1(builder.params.turn_deadline_daa());
754:     if open_sessions >= max_sessions {
755:         return Err(PalwStateV2Error::CourtSessionsAtCapacity {
756:             open: open_sessions,
757:             max: max_sessions,
758:             turn_deadline_daa: builder.params.turn_deadline_daa(),
759:             per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,
760:         });
761:     }
762:     let challenger_collateral =
```

### `consensus/core/tests/g14_close_legacy.rs:531–566`

```text
531: }
532: 
533: /// **G-07's residual, pinned (NOT closed):** the network's court-session cap (`palw_max_concurrent_court_sessions_v1`: the rung window
534: /// times the one court move a block admits) is shared. A coalition of reservers fills it with decoy dissections on a claim of its own;
535: /// the outsider's dissection on the lying claim is then refused for capacity — while its reservation still holds that claim's `Final`.
536: /// A per-bond budget or a partition does not stop a multi-bond coalition, and lifting the cap breaks the slot guarantee that keeps an
537: /// honest mover from defaulting; closing it needs a measured per-block court budget or a priced slot (audit appendix C).
538: #[test]
539: fn g07_residual_the_network_court_cap_is_shared() {
540:     let probe = bundle(&palw_t12_launch_params_v1()).state;
541:     let cap = palw_max_concurrent_court_sessions_v1(probe.turn_deadline_daa());
542:     eprintln!("[G-07] testnet-12 court-session cap: {cap} (turn_deadline_daa {})", probe.turn_deadline_daa());
543:     let (mut c, row, seats) = held_chain(true, cap);
544:     // The lying claim: reserved by the outsider and held past its floor (no court is open, so no rung clock runs).
545:     let (target, licensed_daa) = licensed(&mut c, &row, &seats, 3);
546:     c.step(&[reservation(&c.s, target, bond_key(OUTSIDER))], Rule::T12).expect("the outsider reserves the lying claim");
547:     c.skip_to(licensed_daa + c.sp.window_challenge_at(licensed_daa) + 5);
548:     assert!(matches!(c.s.claim(&target).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "held: no Final, no reward");
549:     // The coalition's own claim; one decoy opener, then reservers stacking dissections on it up to the network cap (inside one rung
550:     // window: the passive responder's sessions would otherwise default — a live coalition plays both sides and keeps them open).
551:     let (decoy, _) = licensed(&mut c, &row, &seats, 2);
552:     let sybils: Vec<PalwBondKeyV2> = (0..cap - 1).map(|n| bond_key(SYBIL_BASE + n)).collect();
553:     let reservations: Vec<PalwConsensusObjectV2> = sybils.iter().map(|sybil| reservation(&c.s, decoy, *sybil)).collect();
554:     c.step(&reservations, Rule::T12).expect("the Sybils reserve the decoy claim");
555:     open_by(&mut c, &row, decoy, bond_key(BYSTANDER), 1).expect("the first decoy");
556:     let openings: Vec<PalwConsensusObjectV2> = sybils.iter().map(|sybil| accusation_by(&c, &row, decoy, *sybil, 1)).collect();
557:     c.step(&openings, Rule::Opening).expect("every Sybil reserver's decoy dissection");
558:     assert_eq!(c.s.court_sessions_iter().count() as u64, cap, "the network's court sessions are at the cap");
559:     let full = open_by(&mut c, &row, target, bond_key(OUTSIDER), 1).expect_err("the cap is full");
560:     assert!(matches!(full, PalwStateV2Error::CourtSessionsAtCapacity { .. }), "{full:?}");
561:     // The reservation still holds the lying claim — the residual is a delay bounded by its hard deadline, not a payout.
562:     assert!(matches!(c.s.claim(&target).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
563:     assert!(c.s.legacy_dispute_reservation_v1(&target, &bond_key(OUTSIDER)).is_some());
564: }
565: 
566: // ---- G-13 -----------------------------------------------------------------------------------------------------------------------
```

<a id="s20"></a>

## S20 — TIR K_owner=8、idle回収、全active時拒否

### `consensus/core/src/palw_tir_public_fold_v1.rs:618–642`

```text
618: fn open_v1(
619:     b: &mut TransitionBuilder<'_>,
620:     ctx: &PalwBlockContextV2,
621:     frame: &Frame,
622:     owner: PalwBondKeyV2,
623: ) -> Result<(), PalwStateV2Error> {
624:     independent(b, frame, owner)?;
625:     // **K_owner (Lead default, pending user review): refuse an excess owner before ANY custodian duty is created.** The frame's
626:     // liable custodians answer at most `custodian_load_bound_v1` across the owners holding slots at once; each owner below pays
627:     // only its own rent and deposit (`owner_terms_v1`), never the producer's prepaid preparation pool.
628:     //
629:     // **G-T1 (Lead default R1, idle-slot reclamation; the cap stays K_owner = 8): an idle owner never locks an honest verifier
630:     // out.** At the cap, this Open reclaims the slot of the holder idle longest (no move for one native acquisition span since its
631:     // last progress, `owner_idle_at_v1`): that row is retired (its open demand closed, its turns spent — it can never demand again),
632:     // and its deposit is forfeited (burned through `slash_bond`, credited to nobody, so reclaiming pays no one). Only when every
633:     // holder is still making progress is the Open refused. Atomic with the Open (the caller's checkpoint).
634:     let claim = frame.plan_v1().original_claim;
635:     if b.state.staged_tir_owner_count_v1(&claim) >= Frame::MAXIMUM_OWNERS_V1 {
636:         let Some((idle_key, idle_owner)) = b.state.staged_tir_idle_owner_v1(&claim, ctx.daa_score) else {
637:             return Err(refused("TIR original frame already carries its K_owner outside owners, all still active; custodian load is bounded"));
638:         };
639:         reclaim_owner_slot_v1(b, ctx, idle_key, idle_owner)?;
640:     }
641:     let c = owner_contract(frame, owner, ctx.daa_score)?;
642:     let key = owner_key(frame, owner, ctx.daa_score)?;
```

<a id="s21"></a>

## S21 — Native owner上限とv39回収

### `consensus/core/src/palw_native_text_fold_v29.rs:1148–1172`

```text
1148:         assembly_attempts: 0,
1149:         court_attempts: 0,
1150:         discovery: wire.discovery,
1151:         original_ticket: ticket,
1152:         conversion_attempts: 0,
1153:         retired: false,
1154:         spent: false,
1155:         committed: paid.admission.checked_add(&paid.guarantee).map_err(refused)?,
1156:         consumed: paid.admission.clone(),
1157:         remaining: paid.guarantee.clone(),
1158:     };
1159:     let mut owner_ids: Vec<Digest> = read(r, EXECUTION_SEAL_ACTOR_TABLE_V11, 0, &id, 4 + 4 * 64).unwrap_or_default();
1160:     if owner_ids.len() >= 4 || owner_ids.contains(&own.id()) {
1161:         return Err(refused("text29 original lifetime owner catalogue exhausted"));
1162:     }
1163:     owner_ids.push(own.id());
1164:     let status = paid_status(r, id)?;
1165:     let epoch = TrustedEpochV11::from_chain(&p.quota, ctx.block.as_bytes(), ctx.daa_score).map_err(refused)?;
1166:     let shard = quota_shard(builder, p, &epoch, actor, id)?;
1167:     let token = CheckedNativePagedAdmissionV29::from_current_native(status.clone(), own.clone(), &q.paged, &p.quota, &epoch)
1168:         .map_err(refused)?;
1169:     let mut projection = NativePagedQuotaProjectionV29::from_native_rows(
1170:         &p.quota,
1171:         &q.paged,
1172:         &shard.global_row(),
```

### `consensus/core/src/palw_flat_execution_court_fold_v11.rs:85–120`

```text
85:     let mut native = NativePaidSealAnchorV11 {
86:         job_id: id,
87:         funded_job_root: book.job().root(),
88:         book_root: book.root(),
89:         claim_id: claim.id(),
90:         vector_id: claim.vector,
91:         seal_actor: actor,
92:         seal_operator_id: operator,
93:         producer_operator_id: h.producer_operator,
94:         proof_root,
95:         posted_daa: ctx.daa_score,
96:         matures_daa: mature,
97:         horizon_daa: horizon,
98:         original_paid_seal_root: [0; 64],
99:         original_debit: fee,
100:         original_debit_receipt: debit_receipt,
101:         shared_live_seals: global,
102:         shared_actor_live_seals: actor_count as u32,
103:     };
104:     native.original_paid_seal_root = native_paid_seal_record_root_v11(&native);
105:     let checked = check_native_paid_seal_v11(&class, &book, &epoch, &native).map_err(refused)?;
106:     let mut state = pursuit(r, &id)?;
107:     if state.retained_seal_keys.len() + state.retained_discovery_keys.len() >= 4
108:         || state.retained_seal_keys.contains(&protected_key.root())
109:     {
110:         return Err(refused("native shared retained job tickets exhausted"));
111:     }
112:     if r.aux.contains_key(&(EXECUTION_SEAL_TABLE_V11, key(&protected_key.root()))) {
113:         return Err(refused("native own seal duplicate"));
114:     }
115:     shard.admit_protected_seal(&p.quota, &epoch, &checked, &plan).map_err(refused)?;
116:     let trace_plan = trace_plan(builder, p, id)?;
117:     let checked_trace = check_trace_seal_v17(&class, &book, &epoch, &trace_plan, &native).map_err(refused)?;
118:     let mut trace =
119:         trace_shard_from_rows(p, &trace_plan, &shard.global_row(), &shard.actor_row(), shard.subject_row().unwrap(), None)?;
120:     trace.admit(&p.quota, &epoch, &trace_plan, &checked_trace).map_err(refused)?;
```

### `consensus/core/src/palw_native_protected_fold_v39.rs:590–606`

```text
590:     g.protected_seal_count -= 1;
591:     a.retained_protected_seals -= 1;
592:     s.retained_protected_seals -= 1;
593:     o.retired = true;
594:     o.consumed = o.committed.clone();
595:     o.remaining = QuotaQuantitiesV11::default();
596:     for i in 0..o.count {
597:         builder.write_kernel_row(TABLE, k(38, &(id, i)), None)?;
598:     }
599:     let slot = st.owners.iter().position(|v| *v == id).unwrap();
600:     st.owners[slot] = [0; 64];
601:     // G14 lane closure G-N1: an owner that ran out its window without a conviction frees its prosecution slot — `admitted` counts the
602:     // live owners, so four idle owners can no longer hold an honest outside verifier off the claim until Final. Each new owner pays its
603:     // own tariff again; the expired owner's archive row (kind 39) stays.
604:     st.admitted = st.admitted.checked_sub(1).ok_or_else(|| bad("native39 expiry original owner count absent"))?;
605:     put_rows(builder, &g, &a, &s)?;
606:     write_status(builder, &st)?;
```

<a id="s22"></a>

## S22 — G14最新appendix: frame費用/時計/本番harness

### `docs/design/palw/g14-completeness-proof-2026-10-16.md:978–1050`

```text
978: 3. Gate C の待機の観測。Gate C はこの binary では武装できない。
979: 
980: ### 8.5 DAA 14,000 で frame とこの lane を武装するのに残るもの
981: 
982: - 8.2 の allocation GAP を frame lane が修正すること。
983: - T-N1。
984: - `palw_legacy_public_filer_v1` の武装可能化(G-18。frame の前提である staged court の前提)。
985: - G-10 の記録 V1〜V7。この lane の evidence(上の fold 試験と node 試験、T-N1)を加える。
986: - drill D-05、D-06、D-07、D-18。genesis 後に登録した IR class で、出荷 binary を使い、fence をまたいで行う。
987: - 未測定の値(UNKNOWN): 最大 genesis profile での generic backend の FP 実行時間と bind window の比較、§7.5 の 2〜6。
988: - node が class semantics の runtime id を chain から読むこと(`with_fp_class_facts_v1` は未接続)。
989: 
990: ## 9. 追補: frame allocation の u64 溢れの修正(2026-10-16、branch `integ/g14-tir-alloc`)
991: 
992: §8.2 の GAP(frame の native allocation が testnet-12 の Panel 規模で u64 を溢れる)を直した。この branch は fence を武装しない。
993: 
994: ### 9.1 testnet-12 の custodian 上限は 6 ではなく 257
995: 
996: testnet-12 は `palw_tir_shard_v1` を武装している(`PALW_T12_INT11_FENCES_V1`、DAA 5,300)。
997: そのため、登録された TIR class について `staged_registered_tir_maximum_custodians_v1` が返す上限は
998: max(seat 数, `MAX_STRATIFIED_SEATS_V1` = 256) + 1 = **257** である。V2 Panel の 5 seat と producer を足した 6 ではない。
999: §8.2 の「上限 6」はこの点で過小だった。shard licensing(×1024、上限 5,121)は testnet-12 では休眠している。
1000: 
1001: ### 9.2 原因(`derive_tir_public_work_allocation_v1`)
1002: 
1003: 1. **custodian ごとの積が和であるべき所で積になっていた。**
1004:    全 lane を外部 owner の lane(全 slot の answer を読むので slot 数に比例する)で値付けし、claim 全体をその slot 倍にしていた。
1005:    正しくは、owner の lane 1 本と custodian ごとの lane(自分の slot の answer だけ)の和である。
1006:    claim 全体の cold preparation も owner の allocation を複製していた。preparer は他の slot の answer を読まない。
1007: 2. lane の work に「全 actor の preparation 数」(`slots × passes`)を掛け、`maximum` でさらに slot 数を掛けていた。
1008: 3. acquisition の `maximum_stream_bytes` を answer の byte として読んでいた。これは保管・運搬の envelope(生 copy 8 個、move ごとの row journal)である。
1009:    しかも turns に再取得が含まれているのに、さらに `passes` を掛けていた。
1010: 4. 16 KiB の WorkDebit slice を、replay の RAM traffic を含む全 IO で数え、`passes` を二重に掛けていた。
1011:    node が WorkDebit を切る単位は、論理 unit(leaf、tile 行、1024 ID、replay 位置)と、durable byte の 16 KiB(読んだ answer、各 pass で公開する prepared generation)である。
1012: 
1013: `original_controls` は accepting action ごとに frame 全体を検証し、その費用を frame(∝ 上限)× action 数で数える。
1014: action 数が上限の 2 乗に比例していたので、SUM は上限の 3 乗で増えていた。修正後は、`original_controls` の注記どおり 2 乗である。
1015: 
1016: ### 9.3 修正
1017: 
1018: - `PalwTirPublicWorkAllocationV1` に 2 つの field を足した(generated の inventory 2 行を更新した)。
1019:   - `per_custodian`: literal custodian の lane。
1020:   - `prepared_bytes`: 1 pass の prepared generation の byte 数。`tir_public_prepared_bytes_v1` で求め、executor plan と共有する。
1021: - `maximum` は lane の和である(`per_actor + per_custodian × maximum_custodians`、`lanes_sum_v1`)。
1022:   owner の fold は、signer が owner なら `per_actor`、custodian なら `per_custodian` で上限を判定する。
1023:   したがって、共謀した custodian が owner の lane を使うことはできない(既存試験を強化した)。
1024: - claim 全体の preparation は `preparation_lanes_v1` を使う。全 preparer が custodian の lane を持つ。
1025:   preparation contract は `per_actor == per_custodian` と `maximum == lanes_sum` を検査する。
1026: - preparation の slice 数 = 論理 walk × passes + prepared generation の 16 KiB slice × passes × 2(書いて再度開く)。
1027:   preparation の締切は、preparer 1 人の直列時間(slice ごとに 1 inclusion margin、最後に Ready)である。
1028:   `capture_debit_v1` は `maximum_slices_per_actor` を超える grant を拒否する。
1029: - saturating 演算と u128 の切り詰めは使っていない。checked 算術のままで、項の数え方だけを直した。
1030: 
1031: ### 9.4 cell(`tir_frame_allocation_holds_at_testnet12s_custodian_upper`、gap pin を反転)
1032: 
1033: 上限は、出荷 bundle に対する fold 自身の resolver から取る(DAA 14,000 で 257 と assert)。条件は network の staged config と ladder 2^22。
1034: corpus の 11 class(step leaf 52〜222)はすべて、上限 1〜6、33、257 で `Complete` になった。主な値(SUM、`accepted_daa` からの差):
1035: 
1036: | class | n | 上限 | parser | rent(sompi) | preparation_until | collect_until |
1037: |---|---|---|---|---|---|---|
1038: | moe-top2-shared | 52 | 6 | 1.92e15 | 1.99e15 | +9.07e5 | +6.33e7 |
1039: | moe-top2-shared | 52 | 257 | 2.36e18 | 2.44e18 | +9.07e5 | +1.70e9 |
1040: | dense-gqa-2layer | 118 | 6 | 4.04e15 | 4.19e15 | +2.03e6 | +1.31e8 |
1041: | dense-gqa-2layer | 118 | 257 | 4.98e18 | 5.16e18 | +2.03e6 | +3.56e9 |
1042: | gdn-k2-v4-grouped | 222 | 6 | 7.39e15 | 7.65e15 | +3.80e6 | +2.39e8 |
1043: | gdn-k2-v4-grouped | 222 | 257 | 9.14e18 | 9.47e18 | +3.80e6 | +6.50e9 |
1044: 
1045: - 上限 257 での最大は parser 9.14e18 で、u64 の上限 1.84e19 に対して約 2 倍の余裕しかない。
1046:   支配項は、accepting action ごとの全 frame 検証(`owner_validation` と、controls の preparation 検証)である。
1047: - 上限 1,025 と 5,121(shard licensing、testnet-12 では休眠)は依然 `native TIR allocation exceeds u64` になる(probe で確認した。assert はしていない)。
1048: - 実 model(leaf 数が桁違いに多い)は上限 257 で溢れる見込みである。owner の lane は turns に比例し、turns は leaf 数と tile 数で増えるため。未測定。
1049: 
1050: ### 9.5 新しい発見(設計判断が要る。未修正)
```

### `docs/design/palw/g14-completeness-proof-2026-10-16.md:1051–1080`

```text
1051: 
1052: **frame は構築できるが、network_v1 の価格と時計では払えず、Final に届かない。**
1053: 
1054: - **rent。** `rent_per_parser_step` は 1 sompi なので、rent はほぼ parser step 数である。
1055:   - 原 FP carrier が払う最低 rent(`palw_tir_public_minimum_rent_v1`)は、上限 6 で 2e15〜8e15 sompi(2e7〜8e7 MSK)、上限 257 で 2.4e18〜9.5e18 sompi(2.4e10〜9.5e10 MSK)。
1056:   - 外部 owner の Open の rent(`owner_terms_v1`)は、corpus dense で上限 6 のとき約 2.2e15 sompi(2.2e7 MSK)。上限 257 では約 2.7e18 sompi。
1057:   - G14 の「外部の 1 人が conviction できる」は、この費用を払えることが前提である。
1058: - **時計。**
1059:   - preparation_until は +0.9e6〜3.8e6 DAA。外部 owner の Open は `reserved_daa ≥ preparation_until_daa` を要する。
1060:   - collect_until(Final の下限)は、上限 6 で +6e7〜2.4e8 DAA、上限 257 で +1.7e9〜6.5e9 DAA。
1061:   - 原因は、WorkDebit と move を 1 件ずつ `inclusion_margin_daa` = 4 で直列に積む時計のモデルである(R7、未測定)。
1062: - **preparer の負担。** 正直な preparer は、1 frame あたり 2.3e5〜9.5e5 件の WorkDebit を出す必要がある。
1063:   prepared generation(最大 tile 長 65,536 での leaf 形式の上限で、2.8e7〜1.2e8 byte)を 16 KiB に分け、65 pass 分、書いて再度開くため。
1064: - **帰結。**
1065:   - DAA 14,000 で `palw_tir_public_conviction_v1` を武装しても、IR class の FP claim は frame の rent を払えない。
1066:   - 払えたとしても、Final と決済に届かない。
1067:   - T-N1 も、network_v1 の時計では node E2E で走らせられない。Open までに 0.9e6 DAA 以上を採掘する必要がある。
1068: - **判断が要る点。**
1069:   - 価格(parser step 単価、frame 検証を action ごとに 32 回 × 32 step/byte と数える係数)。
1070:   - 時計(debit ごとに直列の inclusion margin を積むか)。
1071:   - restart の回数(`reorg_replay_moves` = 64)。
1072:   - いずれも未測定であり、この branch は値を作っていない。
1073: 
1074: ### 9.6 実行した試験(この branch)
1075: 
1076: - `cargo test -p misaka-palw-tir-exec --features node --test free_prompt`: `5 passed; 0 failed`。
1077:   gap pin を反転した `tir_frame_allocation_holds_at_testnet12s_custodian_upper` を含む。honest run は上限 6 で完全な frame を作る。
1078: - `cargo test -p kaspa-consensus-core --lib -- palw_tir_public tir_fp_lane shipped_presets_have_pinned_fingerprints tir_preparation_is_claim_wide aux_registry_tests palw_native_resource`:
1079:   `126 passed; 2 failed`。
1080:   - 失敗 1: `flat_original_binding_does_not_acquire_tiled_trace_obligations`。§7.4 と §8.3 で既に失敗と記録されたもの。
```

### `docs/design/palw/remaining-work-2026-10-16.md:23–40`

```text
23: ## 2. G14 の lane — 保存済みで未完了の作業(WIP branch、未 build・未試験)
24: 
25: | branch | head | 内容 | 残り |
26: |---|---|---|---|
27: | `integ/g14-tir-pricing` | `61ab3a9a8` | frame の課金・期限を測る harness の途中 | 32×32=1,024 step の課金が要るかの実測。debit ごとの inclusion margin と `reorg_replay_moves` の積み上げの見直し。実測にもとづく cost model と期限。T-N1(試験専用 config)。正直な IR FP claim が Final・支払いまで届く node 試験 |
28: | `integ/g14-court-capacity` | `5b64cac9c` | court 容量(`palw_court_capacity_v1.rs` / fold)、Panel 担保の試験の途中 | 処理予算の実測 harness、占有時間の課金、期限付きスケジューラ。42 枠飽和+継続的な埋め直し攻撃の試験。Panel の Active/Resolved 分離(`palw_final_lock_full_collateral`、R_new の式)。late conviction、資本効率の比較 |
29: | `integ/g14-drill` | `b8858af58` | 共謀 drill の driver の途中(`scripts/drills/g14_collusion_drill.py`、`g14_drill_factory.rs`) | D-01〜D-18、court/Panel の 7 条件、正常系 P-1〜P-5、RFC-0019 R-7/R-8 の実装と実行 |
30: 
31: ## 3. G14 の未解決 gap(監査の ID)
32: 
33: - **G-07** court の 42 枠:連合が全枠を占有できる。fused leaf 経路は、飢餓耐性を実証するまで (b) で「G14 経路あり」と数えない(ユーザー方針)。
34: - **G-13 の効率**:Panel 担保の Active/Resolved 分離が未実装。Final+1,000 の無条件解放へは戻さない。
35: - **TIR frame の価格・期限**:t12 の現行価格では、賃料が 2e7 MSK 以上、告発を開けるのが 0.9e6 DAA 後、Final が 6e7 DAA 後で、実用にならない。実モデルの leaf 数では u64 を超える可能性もある(未測定)。
36: - **native**:G-N1(v11/v29 の告発枠)、G-N2(v39 mining の Final 後の告発。veto の扱いは RFC-0018 U-6 のユーザー判断)、G-K2(EXEC の試験 seam GAP-81)。
37: - **G-K3 / F17** 検証者の誘因:正直な検証者が費用を回収できることの証明。自作自演は finder 49% 上限で損になる。
38: - **G-11** rule E、**G-12** bond 式 v2、**G-04** Gate C の武装:いずれも測定値と判断が待ち。
39: - **X3** の候補値は 5,520 DAA。未測定の項(B_cold_fetch など)が残る。
40: - T-N1 の本番経路 harness:IR class の登録 → `Active`、TIR family の認証、layout 公開、`TirPublicGrantReadyV1`、外部 owner/filer の service。
```

<a id="s23"></a>

## S23 — Native mining Final後のchain告発がない

### `consensus/core/src/palw_security_certification_fold_v1.rs:488–503`

```text
488:         if row.outcome.is_none() {
489:             let facts = if native {
490:                 // Native courts settle inside the component jobs before their escrow Final; the Master Final row's presence is
491:                 // the standing fact, and no chain accusation can name a native mining claim.
492:                 PalwCertificationClaimFactsV1 { still_final: native_live, accusation_pending: false }
493:             } else {
494:                 PalwCertificationClaimFactsV1 {
495:                     still_final: claim.as_ref().is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. })),
496:                     // G14 R6: an ORIGINAL TIR frame's conviction door is a pending prosecution for Gate C until it closes — the
497:                     // decision waits (never a deadline failure), so no SecurityCertified credit precedes the last possible verdict.
498:                     accusation_pending: builder.state.palw_accusation_pending_v1(&claim_id)
499:                         || builder.state.palw_tir_public_prosecution_open_v1(&claim_id, now),
500:                 }
501:             };
502:             if let Some(outcome) = palw_certification_verdict_v1(&policy, &row, facts, now) {
503:                 row.outcome = Some((outcome, now));
```

<a id="s24"></a>

## S24 — Kernel signer liability/EXEC covering signerの実装済み修正

### `consensus/core/src/palw_g14_lane_closure_fold_v1.rs:24–91`

```text
24: /// its FREE collateral (collateral less V2's committed ledger, which already holds its other locks) cannot back the lock — the receipt
25: /// is then not counted (a `Valid` no collateral stands behind is worth nothing, the 2026-09-23 audit's rule for the Panel route). A
26: /// seat already locked on the claim keeps its one lock.
27: pub(super) fn lock_kernel_signer_v1(
28:     builder: &mut TransitionBuilder<'_>,
29:     ctx: &PalwBlockContextV2,
30:     seat: &PalwBondKeyV2,
31:     claim: &Digest,
32:     claim_reward: u64,
33:     quorum: u8,
34: ) -> Result<bool, PalwStateV2Error> {
35:     let key = palw_kernel_signer_lock_key_v1(&palw_kernel_bond_id_v1(seat), claim);
36:     let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(false) };
37:     if route.aux.contains_key(&(PALW_G14_LANE_CLOSURE_TABLE_V1, key.clone())) {
38:         return Ok(true);
39:     }
40:     let amount = palw_kernel_signer_lock_amount_v1(claim_reward, quorum);
41:     let Some(record) = builder.state.bonds.get(seat) else { return Ok(false) };
42:     if !matches!(record.status, PalwBondStatusV2::Active) {
43:         return Ok(false);
44:     }
45:     let free = u128::from(record.collateral).saturating_sub(builder.committed_at(seat, ctx.daa_score));
46:     if free < u128::from(amount) {
47:         return Ok(false);
48:     }
49:     let row = PalwKernelSignerLockRowV1 {
50:         version: PALW_G14_LANE_CLOSURE_ROW_VERSION_V1,
51:         claim: *claim,
52:         seat: *seat,
53:         amount,
54:         locked_daa: ctx.daa_score,
55:     };
56:     builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, Some(borsh::to_vec(&row).expect("a lock row serializes")))?;
57:     Ok(true)
58: }
59: 
60: /// Charge every lock of `claim`: the row goes first (it leaves V2's committed ledger), then the seat's real bond is slashed by the
61: /// lock (clamped to what the bond still holds — another lane may have drained it; the conviction itself never depends on it). The
62: /// slash is burned (no payout: a signer's liability is a court charge on its own collateral, as RFC-0007 T6's). Once: the row is the
63: /// marker.
64: fn charge_signer_locks_of_claim_v1(builder: &mut TransitionBuilder<'_>, claim: &Digest) -> Result<(), PalwStateV2Error> {
65:     let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
66:     let locks: Vec<(Vec<u8>, PalwKernelSignerLockRowV1)> =
67:         signer_locks_v1(route).into_iter().filter(|(_, row)| row.claim == *claim).collect();
68:     for (key, row) in locks {
69:         builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
70:         if builder.state.bonds.contains_key(&row.seat) {
71:             builder.slash_bond(row.seat, u128::from(row.amount))?;
72:         }
73:     }
74:     Ok(())
75: }
76: 
77: /// **G-K1 at a conviction** (after every kernel settlement batch — an object's arm or the closing tick): each `Convicted` claim's
78: /// signer locks are charged, pre- or post-Final alike. A no-op on a batch with no conviction or a claim no seat covered.
79: pub(super) fn charge_kernel_signers_on_conviction_v1(
80:     builder: &mut TransitionBuilder<'_>,
81:     events: &[LedgerEventV1],
82: ) -> Result<(), PalwStateV2Error> {
83:     for event in events {
84:         if let LedgerEventV1::Convicted { claim, .. } = event {
85:             charge_signer_locks_of_claim_v1(builder, claim)?;
86:         }
87:     }
88:     Ok(())
89: }
90: 
91: /// **G-K4: a response to a segmented claim's demand stood** — the first block that carried one is recorded (once), so pruning never
```

### `consensus/core/src/palw_kernel_route_fold_v1.rs:1233–1246`

```text
1233:     }
1234:     // RFC-0007 §4.1 T6 (dormant `palw_paid_claim_trap_v1`): a paid V2 claim's conviction in this batch is recorded once and its
1235:     // Clean receipt signers are charged their liability. A no-op below the fence.
1236:     super::palw_paid_claim_trap_fold_v1::on_paid_claim_convictions_v1(builder, events, now)?;
1237:     // G14 lane closure G-K1: every conviction in this batch charges the claim's receipt signers' liability locks.
1238:     super::palw_g14_lane_closure_fold_v1::charge_kernel_signers_on_conviction_v1(builder, events)?;
1239:     Ok(())
1240: }
1241: 
1242: /// **The interim seat draw of a freshly committed claim** (module doc). Distinct operators, none the producer's.
1243: fn draw_seats(
1244:     builder: &TransitionBuilder<'_>,
1245:     claim: &misaka_palw_kernel::hash::Digest,
1246:     producer: &PalwBondKeyV2,
```

### `consensus/core/src/palw_g14_lane_closure_fold_v1.rs:170–190`

```text
170: 
171: /// **G-X1: the REAL root's own licensed piece is proven false** — every V2 seat that signed `Valid` on the REAL claim (it holds a
172: /// slashable lock on it) is charged S4 on that lock through the covering-signer funnel (lock first; once per seat and claim across
173: /// every route: the false-Valid offence key). The producer is not charged a second time here: its prefix claim's kernel reservation
174: /// is the one charge for the one lie (I8). A later executor's slice is not something the REAL claim's V2 seats signed — they licensed
175: /// the claim before any slice existed — so its falsity charges that executor's kernel claim and that claim's own receipt signers
176: /// (G-K1), never these seats.
177: pub(super) fn charge_exec_root_signers_on_proven_false_v1(
178:     builder: &mut TransitionBuilder<'_>,
179:     root_id: Hash64,
180:     now_daa: u64,
181: ) -> Result<(), PalwStateV2Error> {
182:     let covering: Vec<PalwBondKeyV2> =
183:         builder.state.slashable_locks.keys().filter(|(_, claim)| *claim == root_id).map(|(seat, _)| *seat).collect();
184:     if covering.is_empty() {
185:         return Ok(());
186:     }
187:     let conv = builder.open_conviction_v1(&covering, now_daa);
188:     let g = builder.claim_g_v1(&root_id).map(|gains| gains.g()).unwrap_or(0);
189:     builder.charge_covering_signers_v1(&conv, root_id, &covering, g)
190: }
```

<a id="s25"></a>

## S25 — Gate C報酬のhorizonまでの保留は実装済み

### `consensus/core/src/palw_security_certification_fold_v1.rs:321–359`

```text
321:     // **G-15 (`palw_reward_liability_horizon_v1`)**: past the fence the held reward is NOT named at certification — it waits in the row
322:     // until the claim's liability horizon (`final + window_court`) has passed and is named by the sweep only if the claim is still
323:     // certified then (the deferred release in the sweep, which alone names a governed reward and zeroes it, so it is named once even
324:     // when certification lands past the horizon). The weight credit above is unchanged. Below the fence: named now.
325:     if crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, claim).is_some() {
326:         return Ok(());
327:     }
328:     name_certified_reward_v1(builder, now, claim_id, claim, row)
329: }
330: 
331: /// **G-15: a deferred certified reward at its claim's retirement** — the claim's liability window closes at the EARLIER of
332: /// `final + window_court` (the sweep's release) and its retirement (a retired claim can no longer be convicted). Called by the deadline
333: /// sweep just before a terminal claim retires: a `Final`, governed, still-certified claim whose held reward has not been named is named
334: /// here, once (the row's `held_reward` is zeroed). A no-op below the fence, for a voided claim and for a reward already named.
335: pub(super) fn release_deferred_certified_reward_at_retirement_v1(
336:     builder: &mut TransitionBuilder<'_>,
337:     now: u64,
338:     claim_id: &Hash64,
339:     claim: &PalwClaimStateV2,
340: ) -> Result<(), PalwStateV2Error> {
341:     if crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, claim).is_none() {
342:         return Ok(());
343:     }
344:     let key = certification_claim_key_v1(&palw_certification_id_v1(claim_id));
345:     let Some(mut row) = row_of::<PalwClaimCertificationRowV1>(&builder.state, &key) else { return Ok(()) };
346:     if row.held_reward == 0
347:         || !row.outcome.is_some_and(|(outcome, _)| outcome.certifies())
348:         || !crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
349:             claim_id,
350:             claim,
351:             &crate::palw_rfc0018_v1::PalwGateCViewV1::of(builder.params, &builder.state),
352:         )
353:     {
354:         return Ok(());
355:     }
356:     name_certified_reward_v1(builder, now, claim_id, claim, &row)?;
357:     row.held_reward = 0;
358:     write_row(builder, key, Some(&row))
359: }
```

<a id="s26"></a>

## S26 — RFC15の明示スコープ・実装状態

### `docs/design/palw/rfc-0015-implementation-status-2026-10-15.md:1–40`

```text
1: # RFC-0015 (Panel=0) — implementation status, 2026-10-15
2: 
3: Branch `integ/rfc02-rfc06-12-g14` (RFC02 + RFC06/07/08/12 + G14 integration). **Implemented, dormant, unverified.** Nothing was
4: built, checked, tested or run for this record (user rule: code first, validation later); no line below is a PASS. Panel=0 stays
5: **deferred** (RFC-0015 amendment 2026-10-12, ruling N5): `palw_panel_free_v1` is `None` on every preset and validation refuses
6: every armed height outside an isolated calibration genesis, now through an authenticated activation gate record whose pin list
7: is empty. Live testnet-12 behaviour is unchanged (no preset value, no params / schedule id, no wire tag moves).
8: 
9: Earlier record (kernel reference level, 2026-10-08): [rfc-0015-panel-free-record.md](rfc-0015-panel-free-record.md). Values
10: that need the user: [rfc-0006-0012-user-decisions-2026-10-15.md §RFC15](rfc-0006-0012-user-decisions-2026-10-15.md).
11: 
12: Status words: **implemented** = code exists on this branch (dormant, unbuilt in this round, unverified); **test source only** =
13: a test exists as source, not run here; **evidence-only** = needs measurement, review or a real network outside the repository;
14: **user decision** = a value or ruling; **GAP** = repository-implementable and not done.
15: 
16: Where the OPV lifecycle lives: the kernel route (`misaka-palw-kernel`, folded into `PalwChainStateV2` by
17: `palw_kernel_route_fold_v1`). The V2 Attempt route (`palw_state_v2`'s `Provisional → PanelBound → ReceiptLicensed → Final`) is
18: **not** given a Panel-free path: V2 claims keep Panel rules (§11.2), and V2 REAL work earns new rewards only through the
19: RFC-0008 slice to the kernel route (X8R, GAP-62 / GAP-81).
20: 
21: ## 1. This round's changes (2026-10-15)
22: 
23: | Change | Where |
24: | --- | --- |
25: | **§13.3 `ACTIVATION_ALLOWED` as code**: `PalwPanelFreeGateRecordV1` — 12 conjuncts (the ten of §13.3, PESG 2026-10-10, the N5 lift), each PASS with a unique non-zero evidence digest and independent review digest; bound to the genesis hash, the fence height, the fence's terms digest, the `palw_bond_budget_v1` and `palw_model_bond_allocation_v1` policy digests; requires `palw_probabilistic_constraints_v1`, `palw_bond_budget_v1`, `palw_model_bond_allocation_v1`, `palw_court_model_scope_v1`, `palw_fork_choice_rule_e_v1` armed at or below; its digest must be in `PALW_PANEL_FREE_GATE_PINNED_RECORDS_V1` (empty) | `consensus/core/src/palw_panel_free_gate_v1.rs:39` (pins), `:49` (conjuncts), `:281` (`verify_v1`) |
26: | The fence's value carries the record (`activation_gate`, `None` everywhere; hashed Some-only) and the refusal runs it | `consensus/core/src/palw_panel_free_v1.rs:66`, `:178` |
27: | **§8.3 at the OPV door**: where a gate record is present, an OPV claim with no `palw_bond_budget_v1` plan is refused (budget refusal path) | `consensus/core/src/palw_kernel_route_fold_v1.rs:1417`; `PalwKernelOpvExtrasV1::budget_required` (`palw_kernel_route_v1.rs`), set in `consensus/src/pipeline/virtual_processor/processor/palw.rs` |
28: | Public kernel verifier (the shared filer of the kernel route) also runs where `palw_panel_free_v1` is in force | `kaspad/src/palw_kernel_verifier.rs:212` |
29: | §13.1 negative gate tests | `consensus/core/tests/rfc0015_activation_gate.rs` (test source only) |
30: | §13.2 node E2E additions: old Panel claim across the fence, §8.3 budget on the OPV door (`d + W`, Q=1, no B/F, liability collateral kept past `d + W`), unbudgeted OPV claim refused under a gate record, direct proof over another bond's open demand, concurrent claims of one bond, a self-prosecuted lie (fake bounty loop) | `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/rfc15_panel_free.rs` (test source only) |
31: 
32: Allocations: **none** — no object tag, kernel inner kind, aux table, RPC op, proto field, p2p version, funded action, carriage tail
33: or delta. The record is a value of an existing fence; its hash domains are `misaka-palw/panel-free/{activation-gate-record,terms,
34: allocation-policy}/v1` and `palw_panel_free_v1/activation-gate-record/v1` (identity hasher).
35: 
36: ## 1b. Lead defaults and gap closure (2026-10-15, second round; lead defaults pending user review)
37: 
38: | Change | Where |
39: | --- | --- |
40: | Gate also requires `palw_staged_public_court_v1` (G14 public prosecution) and `palw_verifier_pay_v1` (monitoring economics); `palw_staged_tir_route_v1` / `palw_staged_float_constraint_v1` only when the record's bound rewarded profile set (`PalwPanelFreeRewardedProfilesV1`) includes TIR / Float32 classes | `consensus/core/src/palw_panel_free_gate_v1.rs` |
```

### `docs/design/palw/rfc-0015-implementation-status-2026-10-15.md:115–134`

```text
115: | largest admitted class / context / layout | `canonical.rs` `g14_canonical_a_routing_and_history_class_is_refused_by_the_node_carrier_fit…` (refusal, by design) | **evidence-only** (finding 2 of the 10-08 record: held / long-context classes do not fit the carriers) |
116: | work-slice and native settlement | shared predicate unit tests (`palw_kernel_settlement_v1.rs`); slice outcome behaviour-preserving refactor | implemented; node E2E source `conformance/exec_slices/opv_settlement.rs` (pending → final-revocable → settled; conviction and default revoke) — test source only |
117: | upgrade across old claims | new `rfc15_e2e_a_panel_licensed_claim_admitted_below_the_fence_finishes_under_panel_rules`; `g14_opv_registration_is_dropped_without_the_fence…_and_the_legacy_route_coexists`; `k2_opv.rs` `panel_licensed_classes_behave_exactly_as_before…` | existing + test source only |
118: 
119: ## 5. Compile risks of this round (unbuilt)
120: 
121: * `PalwPanelFreeFenceV1` gained a field: every struct literal was updated (`palw_panel_free_v1.rs`, `palw_native_calibration_v34.rs`);
122:   a literal elsewhere would fail to compile (grep found none).
123: * `PalwKernelOpvExtrasV1` gained `budget_required`: literals updated in the processor, `consensus/core/tests/rfc0010_production_fold.rs`
124:   (3) and `consensus/core/src/palw_state_v2/tests/reward_gate_genesis.rs` (1).
125: * `palw_panel_free_gate_v1.rs` uses `blake2b_simd` (already a consensus-core dependency) and Borsh derives on `Hash64` (existing).
126: * The new E2E module uses the parent suite's private helpers through `use super::*` (as `canonical.rs` does) and reads the budget's
127:   rows by `origin == KernelRoute` (no visibility change). The budget-armed OPV world (`rfc15_config`) is new: the kernel claim's
128:   `require_kernel_monetary_material_scope_v1` / `hold_kernel_income_v1` under an armed budget has not been exercised on an OPV world.
129: 
130: ## 6. What is still not done
131: 
132: * Every **evidence-only** row (§1 G14 on real nodes for every rewarded profile, measurements, independent reviews, PESG, the
133:   adversarial network run) and every **user decision** (values, the pin, the N5 lift).
134: * Code gaps: none known in the repository for RFC-0015's §12 rows (all remaining items are evidence-only or user decisions). Everything is unbuilt and unrun.
```

### `consensus/core/src/palw_kernel_settlement_v1.rs:1–135`

```text
1: //! **RFC-0015 §9 / §11.3: the one settlement predicate of a kernel-route claim** — what every reader of a kernel claim's `Final`
2: //! (an OPV claim's above all) may conclude: the EXEC work-slice verification (RFC-0008 v2, `palw_exec_v2_claim_outcome_v1`), the
3: //! public claim read (RPC op `getPalwKernelClaim`, the web view), and any bridge / native-settlement consumer.
4: //!
5: //! ```text
6: //! convicted (before or after Final)                          → Convicted   (void; a slice is ProvenFalse)
7: //! Unavailable / TimedOut, or Final with the reservation gone
8: //!   while still liable, or an OPV post-Final default         → Defaulted   (never safe)
9: //! Final, liability horizon not passed                        → FinalRevocable { final_daa, liability_until }
10: //!                                                               (a slice verifies, its leg stays held; NOT safe for an
11: //!                                                                irreversible release, NOT shown as settled)
12: //! Final, past the liability horizon, unconvicted             → Settled { final_daa, settled_from_daa = liability_until + 1 }
13: //! anything else (Committed, Challengeable, ProofGrace, ...)  → Pending
14: //! ```
15: //!
16: //! **Why one predicate.** An OPV `Final` means only that the fixed challenge window closed with no accepted dispute; a valid proof
17: //! within the liability horizon still convicts it (and the beacon withdraws its fact, `FinalStandingV1::ConvictedAfterFinal`). A
18: //! reader that treated `Final` as irreversible would release value the court can still void. The fraud-expiry of every reader is the
19: //! kernel claim's own `liability_until` — the same horizon the EXEC reward / native-settlement maturity already folds in
20: //! (`palw_exec_v2_rewards_v1::horizon`) — so no reader can become "safe" earlier than the route can stop convicting.
21: //!
22: //! **Weight.** Kernel-route claims reserve no reward block and no Final weight in the ADR-0176 budget (`B = F = 0`): an OPV `Final`
23: //! adds no consensus weight and no native-settlement work of its own; work-slice credit reaches settlement only through the V2 root it
24: //! verifies, whose maturity already includes this horizon. Nothing here writes state.
25: 
26: use misaka_palw_kernel::lifecycle::ClaimStateV1;
27: 
28: /// The settlement standing of one kernel claim at one DAA (module doc).
29: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
30: pub enum PalwKernelSettlementV1 {
31:     Pending,
32:     FinalRevocable { final_daa: u64, liability_until: u64 },
33:     Settled { final_daa: u64, settled_from_daa: u64 },
34:     Convicted,
35:     Defaulted,
36: }
37: 
38: impl PalwKernelSettlementV1 {
39:     /// The stable label readers display (`pending`, `final-revocable`, `settled`, `convicted`, `defaulted`).
40:     pub const fn label(self) -> &'static str {
41:         match self {
42:             Self::Pending => "pending",
43:             Self::FinalRevocable { .. } => "final-revocable",
44:             Self::Settled { .. } => "settled",
45:             Self::Convicted => "convicted",
46:             Self::Defaulted => "defaulted",
47:         }
48:     }
49: 
50:     /// Whether an irreversible release (a bridge, an external settlement, a "safe" display) may rely on the claim.
51:     pub const fn is_safe(self) -> bool {
52:         matches!(self, Self::Settled { .. })
53:     }
54: 
55:     /// The first DAA at which the claim is (or would be, absent a conviction or default) settled; `None` before `Final`.
56:     pub const fn settled_from_daa(self) -> Option<u64> {
57:         match self {
58:             Self::Settled { settled_from_daa, .. } => Some(settled_from_daa),
59:             Self::FinalRevocable { liability_until, .. } => Some(liability_until.saturating_add(1)),
60:             _ => None,
61:         }
62:     }
63: }
64: 
65: /// **The predicate** over a kernel claim row's facts at `daa`. `forfeited_after_final` is the OPV row's post-Final default flag
66: /// (`OpvClaimRowV1::forfeited_after_final`; `false` for a Panel-licensed claim, whose row has none). A `Final` with no recorded
67: /// horizon is never settled.
68: pub fn palw_kernel_claim_settlement_v1(
69:     state: &ClaimStateV1,
70:     convicted: bool,
71:     reserved: u64,
72:     liability_until: Option<u64>,
73:     forfeited_after_final: bool,
74:     daa: u64,
75: ) -> PalwKernelSettlementV1 {
76:     if convicted {
77:         return PalwKernelSettlementV1::Convicted;
78:     }
79:     match *state {
80:         ClaimStateV1::Final { final_daa } => {
81:             if forfeited_after_final {
82:                 return PalwKernelSettlementV1::Defaulted;
83:             }
84:             let until = liability_until.unwrap_or(u64::MAX);
85:             if daa <= until && reserved == 0 {
86:                 // The reservation is gone while the claim is still liable: a post-Final default (the slice's `Defaulted`).
87:                 PalwKernelSettlementV1::Defaulted
88:             } else if daa > until {
89:                 PalwKernelSettlementV1::Settled { final_daa, settled_from_daa: until.saturating_add(1) }
90:             } else {
91:                 PalwKernelSettlementV1::FinalRevocable { final_daa, liability_until: until }
92:             }
93:         }
94:         ClaimStateV1::Convicted { .. } => PalwKernelSettlementV1::Convicted,
95:         ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. } => PalwKernelSettlementV1::Defaulted,
96:         _ => PalwKernelSettlementV1::Pending,
97:     }
98: }
99: 
100: /// The same predicate over a ledger row (and the ledger's OPV row, if any).
101: pub fn palw_kernel_row_settlement_v1(
102:     ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
103:     claim: &misaka_palw_kernel::hash::Digest,
104:     daa: u64,
105: ) -> Option<PalwKernelSettlementV1> {
106:     let row = ledger.claims.get(claim)?;
107:     let forfeited = ledger.opv.claims.get(claim).is_some_and(|o| o.forfeited_after_final);
108:     Some(palw_kernel_claim_settlement_v1(&row.life.state, row.convicted, row.reserved, row.liability_until, forfeited, daa))
109: }
110: 
111: #[cfg(test)]
112: mod tests {
113:     use super::*;
114:     use PalwKernelSettlementV1 as S;
115: 
116:     fn at(state: ClaimStateV1, convicted: bool, reserved: u64, until: Option<u64>, forfeited: bool, daa: u64) -> S {
117:         palw_kernel_claim_settlement_v1(&state, convicted, reserved, until, forfeited, daa)
118:     }
119: 
120:     #[test]
121:     fn an_opv_final_is_revocable_until_its_liability_horizon_then_settled() {
122:         let fin = ClaimStateV1::Final { final_daa: 50 };
123:         assert_eq!(at(fin.clone(), false, 10, Some(90), false, 50), S::FinalRevocable { final_daa: 50, liability_until: 90 });
124:         assert!(!at(fin.clone(), false, 10, Some(90), false, 90).is_safe(), "the last liable DAA is not safe");
125:         assert_eq!(at(fin.clone(), false, 0, Some(90), false, 91), S::Settled { final_daa: 50, settled_from_daa: 91 });
126:         assert!(at(fin.clone(), false, 0, Some(90), false, 91).is_safe());
127:         assert_eq!(at(fin.clone(), false, 10, None, false, u64::MAX), S::FinalRevocable { final_daa: 50, liability_until: u64::MAX });
128:     }
129: 
130:     #[test]
131:     fn convictions_and_defaults_are_never_safe_whenever_they_land() {
132:         let fin = ClaimStateV1::Final { final_daa: 50 };
133:         assert_eq!(at(fin.clone(), true, 0, Some(90), false, 60), S::Convicted, "a conviction after Final voids it");
134:         assert_eq!(at(fin.clone(), true, 0, Some(90), false, 1_000), S::Convicted, "and stays void past the horizon");
135:         assert_eq!(at(fin.clone(), false, 0, Some(90), false, 60), S::Defaulted, "the reservation gone while liable");
```

<a id="s27"></a>

## S27 — RFC16の実foldはattestationではなく一律seat値

### `consensus/core/src/palw_rfc0016_fold_v1.rs:283–315`

```text
283:     /// **The water-filled capacity read** of `class_id` (scaled like the demand terms, `× PALW_PANEL_DEMAND_SCALE_V1`): every ready
284:     /// verifier's per-span replay split over the classes it is ready for, by demand; its demand with `extra` pooled in; and the number
285:     /// of ready verifiers holding it. O(bonds × classes) readiness reads — bounded by the registry's tables; dormant.
286:     pub(super) fn rfc0016_capacity_read_v1(
287:         &self,
288:         class_id: &Hash64,
289:         fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
290:         now_daa: u64,
291:         extra: Option<(Hash64, PalwGatedClaimV1)>,
292:     ) -> (u128, u128, u32) {
293:         let g = &fold.globals;
294:         let seat = palw_panel_shipped_per_span_v1(1, g).saturating_mul(crate::palw_work_target_v1::PALW_PANEL_DEMAND_SCALE_V1);
295:         let base = self.params.base_class_id();
296:         let classes: Vec<Hash64> = self.state.model_lifecycles.keys().filter(|c| **c != base).copied().collect();
297:         let mut verifiers: Vec<PalwVerifierSupplyV1> = Vec::new();
298:         let mut ready_here: u32 = 0;
299:         for (bond_key, bond) in self.state.bonds.iter() {
300:             let ready: BTreeSet<Hash64> =
301:                 classes.iter().filter(|c| self.model_registry_seat_is_ready(bond_key, bond, c, now_daa, fold)).copied().collect();
302:             if ready.contains(class_id) {
303:                 ready_here = ready_here.saturating_add(1);
304:             }
305:             if !ready.is_empty() {
306:                 verifiers.push(PalwVerifierSupplyV1 { per_span: seat, classes: ready });
307:             }
308:         }
309:         let terms = self.with_inflight_index(|index| {
310:             let owed = palw_panel_owed_v1(self.state, self.params, index, self.own_attempt_class, extra);
311:             palw_panel_terms_v1(self.state, g.seat_count as u128, &owed)
312:         });
313:         let capacity = palw_waterfill_capacity_v1(&verifiers, &terms);
314:         (capacity.get(class_id).copied().unwrap_or(0), terms.get(class_id).copied().unwrap_or(0), ready_here)
315:     }
```

<a id="s28"></a>

## S28 — RFC16資源制約がsupplyで落ちる

### `consensus/core/src/palw_rfc0016_v1/capacity.rs:22–62`

```text
22: /// One ready verifier as the capacity account sees it: its per-span replay budget and the classes it is ready for (deduplicated).
23: #[derive(Clone, Debug, PartialEq, Eq)]
24: pub struct PalwVerifierSupplyV1 {
25:     pub per_span: u128,
26:     pub classes: BTreeSet<Hash64>,
27: }
28: 
29: /// **Per-verifier water-filling.** For each verifier, its budget is split over ITS classes in proportion to their `demand` (equally,
30: /// floored, where none of them has demand). Returns each class's capacity per span. Every verifier's allocations sum to at most its
31: /// budget, so `Σ_class capacity ≤ Σ_verifier budget` — no verifier is counted twice across classes or models.
32: pub fn palw_waterfill_capacity_v1(verifiers: &[PalwVerifierSupplyV1], demand: &BTreeMap<Hash64, u128>) -> BTreeMap<Hash64, u128> {
33:     let mut capacity: BTreeMap<Hash64, u128> = BTreeMap::new();
34:     for v in verifiers {
35:         if v.classes.is_empty() || v.per_span == 0 {
36:             continue;
37:         }
38:         let total: u128 = v.classes.iter().map(|c| demand.get(c).copied().unwrap_or(0)).fold(0u128, u128::saturating_add);
39:         for class in &v.classes {
40:             let share = if total == 0 {
41:                 v.per_span / v.classes.len() as u128
42:             } else {
43:                 let d = demand.get(class).copied().unwrap_or(0);
44:                 // ⌊per_span · d / total⌋ without overflow: split into whole and remainder parts.
45:                 let whole = (v.per_span / total).saturating_mul(d);
46:                 let part = (v.per_span % total).checked_mul(d).map(|x| x / total).unwrap_or_else(|| {
47:                     // Fall back to the coarser product (still ≤ the exact share).
48:                     ((v.per_span % total) / total.max(1)).saturating_mul(d)
49:                 });
50:                 whole.saturating_add(part).min(v.per_span)
51:             };
52:             let slot = capacity.entry(*class).or_insert(0);
53:             *slot = slot.saturating_add(share);
54:         }
55:     }
56:     capacity
57: }
58: 
59: /// **Does the class's demand (with the candidate claim) fit its water-filled capacity?** Capacity zero never admits.
60: pub fn palw_capacity_fits_v1(capacity_per_span: u128, demand_per_span: u128) -> bool {
61:     capacity_per_span > 0 && demand_per_span <= capacity_per_span
62: }
```

### `consensus/core/src/palw_rfc0016_v1/capacity.rs:238–337`

```text
238: /// evidence root names what an independent reviewer re-runs.
239: #[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
240: pub struct PalwCapacityAttestationV1 {
241:     pub model: Hash64,
242:     pub verifier: PalwBondKeyV2,
243:     pub capability: PalwCapacityResourcesV1,
244:     /// The measurement window the capability was measured over, `[from, to)` in DAA.
245:     pub window_from_daa: u64,
246:     pub window_to_daa: u64,
247:     /// The DAA after which the attestation counts nothing.
248:     pub expires_daa: u64,
249:     /// The measurement evidence's root (never zero).
250:     pub evidence_root: Hash64,
251: }
252: 
253: /// **V8: the message the verifier signs** — every field, domain-separated; changing any field changes the message.
254: pub fn palw_capacity_attestation_message_v1(attestation: &PalwCapacityAttestationV1) -> Hash64 {
255:     let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_RFC0016_CAPACITY_ATTESTATION_DOMAIN_V1).to_state();
256:     state.update(&borsh::to_vec(attestation).expect("an attestation serializes"));
257:     let mut out = [0u8; 64];
258:     out.copy_from_slice(state.finalize().as_bytes());
259:     Hash64::from_bytes(out)
260: }
261: 
262: /// Why an attestation counts nothing.
263: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
264: pub enum PalwCapacityAttestationRefusalV1 {
265:     EmptyWindow,
266:     /// The measurement window ends in the future, or the expiry is not after it.
267:     WindowNotClosed,
268:     Expired,
269:     NoEvidence,
270:     ZeroCapability,
271: }
272: 
273: /// **V8: the attestation's own checks at `now_daa`** (the signature is the carrier's, checked against
274: /// [`palw_capacity_attestation_message_v1`]): a closed, non-empty measurement window, an expiry after it and not passed, a non-zero
275: /// evidence root and a non-zero capability.
276: pub fn palw_capacity_attestation_check_v1(
277:     attestation: &PalwCapacityAttestationV1,
278:     now_daa: u64,
279: ) -> Result<(), PalwCapacityAttestationRefusalV1> {
280:     if attestation.window_to_daa <= attestation.window_from_daa {
281:         return Err(PalwCapacityAttestationRefusalV1::EmptyWindow);
282:     }
283:     if attestation.window_to_daa > now_daa || attestation.expires_daa <= attestation.window_to_daa {
284:         return Err(PalwCapacityAttestationRefusalV1::WindowNotClosed);
285:     }
286:     if now_daa >= attestation.expires_daa {
287:         return Err(PalwCapacityAttestationRefusalV1::Expired);
288:     }
289:     if attestation.evidence_root == Hash64::default() {
290:         return Err(PalwCapacityAttestationRefusalV1::NoEvidence);
291:     }
292:     if attestation.capability.compute_ccu_per_span == 0 {
293:         return Err(PalwCapacityAttestationRefusalV1::ZeroCapability);
294:     }
295:     Ok(())
296: }
297: 
298: /// **V8: claims per span one verifier's resources carry for a model's load** — the minimum over compute, DA and court (each a rate),
299: /// zero when the claim's working set does not fit its memory. Never an amplification: no resource is multiplied.
300: pub fn palw_capacity_claims_per_span_v1(capability: &PalwCapacityResourcesV1, load: &PalwClaimLoadV1) -> u128 {
301:     if load.memory_bytes > capability.memory_bytes || load.compute_ccu == 0 {
302:         return 0;
303:     }
304:     let mut claims = capability.compute_ccu_per_span / load.compute_ccu;
305:     if load.da_bytes > 0 {
306:         claims = claims.min((capability.da_bytes_per_span / load.da_bytes) as u128);
307:     }
308:     if load.court_permille > 0 {
309:         claims = claims.min((capability.court_permille_per_span / load.court_permille) as u128);
310:     }
311:     claims
312: }
313: 
314: /// **V8: the verifier supply the water-filling reads, from attestations** — ONE budget per verifier however many models it attests
315: /// for (its largest valid compute, never the sum: the same hardware attested to N models is still one machine), its classes the
316: /// models whose load fits its resources jointly. Expired, unproven or non-fitting attestations count nothing. The water-filling then
317: /// splits that one budget across the models ([`palw_waterfill_capacity_v1`]) — no verifier counted twice.
318: pub fn palw_capacity_supply_from_attestations_v1(
319:     attestations: &[PalwCapacityAttestationV1],
320:     now_daa: u64,
321:     load_of: impl Fn(&Hash64) -> Option<PalwClaimLoadV1>,
322: ) -> Vec<PalwVerifierSupplyV1> {
323:     let mut by_verifier: BTreeMap<PalwBondKeyV2, (u128, BTreeSet<Hash64>)> = BTreeMap::new();
324:     for a in attestations {
325:         if palw_capacity_attestation_check_v1(a, now_daa).is_err() {
326:             continue;
327:         }
328:         let Some(load) = load_of(&a.model) else { continue };
329:         if palw_capacity_claims_per_span_v1(&a.capability, &load) == 0 {
330:             continue;
331:         }
332:         let entry = by_verifier.entry(a.verifier).or_default();
333:         entry.0 = entry.0.max(a.capability.compute_ccu_per_span);
334:         entry.1.insert(a.model);
335:     }
336:     by_verifier.into_values().map(|(per_span, classes)| PalwVerifierSupplyV1 { per_span, classes }).collect()
337: }
```

### `consensus/core/src/palw_rfc0016_v1/capacity.rs:473–510`

```text
473:     /// V8: compute, memory, DA and court load are evaluated together (the scarcest binds; memory is a fit gate).
474:     #[test]
475:     fn v8_resources_are_evaluated_jointly() {
476:         let cap = PalwCapacityResourcesV1 {
477:             compute_ccu_per_span: 1_000,
478:             memory_bytes: 100,
479:             da_bytes_per_span: 50,
480:             court_permille_per_span: 3_000,
481:         };
482:         let load = PalwClaimLoadV1 { compute_ccu: 10, memory_bytes: 100, da_bytes: 10, court_permille: 1_000 };
483:         assert_eq!(palw_capacity_claims_per_span_v1(&cap, &load), 3, "court binds: min(100, 5, 3)");
484:         assert_eq!(palw_capacity_claims_per_span_v1(&cap, &PalwClaimLoadV1 { memory_bytes: 101, ..load }), 0, "does not fit");
485:         assert_eq!(PALW_RFC0016_SIGMA_CAP_MAX_PERMILLE_V1, 1_000);
486:     }
487: 
488:     /// V8: one verifier attesting the same hardware to several models is ONE budget, split by the water-filling — never summed.
489:     #[test]
490:     fn v8_a_verifier_is_never_double_counted_across_models() {
491:         let atts = [attestation(1, 1, 100), attestation(2, 1, 100), attestation(3, 1, 100), attestation(1, 2, 40)];
492:         let load = PalwClaimLoadV1 { compute_ccu: 1, memory_bytes: 1, da_bytes: 1, court_permille: 1 };
493:         let supply = palw_capacity_supply_from_attestations_v1(&atts, 500, |_| Some(load));
494:         assert_eq!(supply.len(), 2, "two verifiers");
495:         let total_budget: u128 = supply.iter().map(|s| s.per_span).sum();
496:         assert_eq!(total_budget, 140, "100 once (not 300) + 40");
497:         let demand: BTreeMap<Hash64, u128> = [(h(1), 10), (h(2), 10), (h(3), 10)].into_iter().collect();
498:         let capacity = palw_waterfill_capacity_v1(&supply, &demand);
499:         assert!(capacity.values().sum::<u128>() <= total_budget);
500:         // An expired attestation counts nothing.
501:         assert!(palw_capacity_supply_from_attestations_v1(&atts, 1_000, |_| Some(load)).is_empty());
502:     }
503: 
504:     /// V7 (user decision 2026-10-15): the queue's fair share follows bond capital (3:1 capital → 3:1 share of `L`).
505:     #[test]
506:     fn v7_queue_shares_are_capital_proportional() {
507:         let mut v = PalwRfc0016ValuesV1 { queue_len_per_class: 8, ..Default::default() };
508:         assert_eq!(palw_queue_bond_share_cap_v1(3_000, 4_000, &v), 6);
509:         assert_eq!(palw_queue_bond_share_cap_v1(1_000, 4_000, &v), 2);
510:         v.queue_len_per_class = 4;
```

<a id="s29"></a>

## S29 — RFC16 production値が未測定placeholder

### `consensus/core/src/palw_rfc0016_v1/values.rs:79–103`

```text
79: /// **The binary's RFC-0016 values — PLACEHOLDER (unmeasured).** `k = 1` (tilt disabled); every other term zero. Validation refuses
80: /// either fence armed while this set has no pinned measured record.
81: pub const PALW_RFC0016_VALUES_V1: PalwRfc0016ValuesV1 = PalwRfc0016ValuesV1 {
82:     sigma_permille: 0,
83:     tilt_k_permille: 1_000,
84:     reissue_checkpoint_permille: 0,
85:     pace_margin_permille: 0,
86:     entrant_reserve_permille: 0,
87:     entrant_max_per_model_permille: 0,
88:     entrant_min_capital_sompi: 0,
89:     entrant_age_daa: 0,
90:     capacity_safety_permille: 0,
91:     c7_measured_concurrency: 0,
92:     queue_len_per_class: 0,
93:     queue_ttl_daa: 0,
94:     quote_margin_h: 0,
95:     verifier_share_base_permille: 0,
96:     verifier_slope_gamma_permille: 0,
97:     verifier_util_target_permille: 0,
98:     opv_checks_per_claim_permille: 0,
99:     opv_checks_per_verifier_epoch: 0,
100: };
101: 
102: // ---- User decisions 2026-10-15 (RFC-0016 §14.1): CALIBRATION STARTING POINTS — never production values; production approval is
103: // ---- withheld and the measured-value gate below still refuses every non-calibration network. ----------------------------------
```

### `consensus/core/src/palw_rfc0016_v1/values.rs:184–204`

```text
184: /// **A measured-value record**: the values the three-stage derivation selected, with the digests of the evidence (harness commit,
185: /// params, hardware, network conditions, raw data, derivation) and of its independent review. Pinned only by a reviewed change.
186: #[derive(Clone, Copy, Debug, PartialEq, Eq)]
187: pub struct PalwRfc0016MeasuredRecordV1 {
188:     pub values: PalwRfc0016ValuesV1,
189:     pub evidence_digest: [u8; 32],
190:     pub review_digest: [u8; 32],
191:     /// The harness and scenario set the record came from (`tools/rfc0016/…`, RFC-0016 §8.6).
192:     pub harness: &'static str,
193: }
194: 
195: /// **The pinned measured records — EMPTY.** No value is measured; arming either fence is refused.
196: pub const PALW_RFC0016_MEASURED_RECORDS_V1: &[PalwRfc0016MeasuredRecordV1] = &[];
197: 
198: /// **Calibration builds only: the values a σ/k measurement run arms** — `MISAKA_RFC0016_CALIBRATION_VALUES` at BUILD time, 18
199: /// comma-separated integers in the field order of [`PalwRfc0016ValuesV1`]. Read only on an ISOLATED calibration network
200: /// (`Params::has_isolated_native_calibration_v34`); a production binary is built without it, and the measured-value gate still
201: /// governs every other network. `None` where unset or malformed.
202: pub fn palw_rfc0016_calibration_values_v1() -> Option<PalwRfc0016ValuesV1> {
203:     palw_rfc0016_parse_values_v1(option_env!("MISAKA_RFC0016_CALIBRATION_VALUES")?)
204: }
```

<a id="s30"></a>

## S30 — RFC17現行tierと旧審査の無効化・係数適用

### `docs/rfc/0017-model-eligibility-examination-and-renewal.md:1–42`

```text
1: # RFC-0017 — モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)
2: 
3: > **統治する前提 — [PRINCIPLES §9](../PRINCIPLES.md)(凍結前提、2026-10-15 ユーザー決定):** モデルの二階層と、モデル単位の検証の不採用。本 RFC は §9 に従い、§9 を再審しない。規則は [ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) が定める。
4: >
5: > **モデル単位の検証を行わない理由(ユーザー):** モデルに対して検証を行うと、その検証において何を信頼するか(誰が・何を根拠に「正しく共有・運用されている」と判断するか)という問題が新たに発生し、それは解決しない。チェーンは、モデルの公開状況・保持者数・取得可能性・共有状況を審査しない。
6: 
7: ## 現行仕様 — ユーザー判断 2026-10-15 第 4 回・第 5 回
8: 
9: 本 RFC は、モデル資格の審査の RFC から、**モデル階層の RFC** に改めた。判断記録は [ユーザー判断](../design/palw/user-decisions-2026-10-15-adr-rfc16-rfc17.md) §7b・§7c。
10: 
11: ユーザー判断(原文、第 5 回):「genesis/fork モデルは 1 倍で有効、後から追加されるモデルは 0.3 倍で、そこから検証などはなく 0.3 倍のまま固定。つまりモデルに対しての検証がなくなった。」
12: 
13: 1. **1.0 の階層。** `ApprovedModels(h)` に含まれるモデルである。`ApprovedModels(h)` は、genesis のモデルと、高さ `h` までに有効化された年次フォーク(ソフト / ハード)が指定した「モデル ID + 固定 artifact root」の集合である。
14:    * 年次フォークは追加するだけで、既存モデルを上書きしない(ADR-0175)。
15:    * 指定は政策的決定であり、公開性・独立検証可能性の技術的証明ではない。
16: 2. **0.3 の階層。** 後から登録したモデルは、登録した時点から審査なしで 0.3 に固定する。
17:    * 昇格・更新・失効・試行手数料・試行上限はない。
18:    * 1.0 になる経路は年次フォークでの指定だけである。
19:    * registry の `Active` などの状態は、運用状態として階層とは別に残る。
20: 3. **モデル単位の検証・審査は、すべてのモデルについて廃止した。** 下記「廃止した審査」の段階(Registered / Probation / Eligible / Lapsed)、抽選審査、更新、失効、試行手数料、試行上限、公平な待ち行列は適用しない。
21: 4. **係数は一度だけ掛ける。**
22:    * `A_m = c_m · S̄_m^1.5`(ADR-0177)。その `P_m` を RFC-0016 V+ の物理ブロック権利と coinbase 配分の両方が使う。
23:    * 受理ブロックの報酬に重ねて掛けない。
24:    * 登録 tier 全体の発行上限 `κ` により、多数登録で 0.3 の枠を増やせない。
25:    * RFC-0016 の entrant reserve と旧 Probation 予算は審査の廃止で無意味になる。`ε = 0` とし、`κ` だけを上限とする。
26: 5. **claim 単位の検証は、すべての階層で維持する**(ユーザー判断、原文:「これは残す。そうじゃないと claim の検証ができない」)。
27:    * claim ごとの認証は [RFC-0018](0018-dag-parallel-multi-model-consensus.md) の Gate C(`SecurityCertified`)にある。これは、抽選検証者の m-of-k 承認と G14 公開告発 window による、前提に基づく認証である。court と確率的検出も維持する。
28:    * 未認証の claim は、genesis・年次指定・登録のどの階層でも、fork choice・決済深度・Rule E 参加・settled anchor・weight を進めない。
29:    * 旧 U-2 の Probation bar は対象が存在しないため削除した。
30: 
31: **実装(休眠、未 build、未試験):**
32: 
33: * fence は `palw_model_tier_v1` で、module は `consensus/core/src/palw_model_tier_v1.rs`(ADR-0179 §実装)。
34: * fence `palw_model_eligibility_v1` は、tier fence が同じ高さ以下にあるときだけ arm できる。この fence は RFC-0018 の認証 attestation が通る inner 74/75 の A-2 gate として残る。
35: * tier fence の下では、審査の code はすべて到達しない。全ての資格 read は「対象外」になり、inner 74/75 の審査 object は拒否され、sweep は何もしない。
36: * 審査の code は、休眠のまま履歴として残す。審査は一度も live になっていない。
37: * 試験の対応は ADR-0179 §試験 source にある。
38: 
39: ---
40: 
41: ## 廃止した審査(2026-10-15 第 5 回で全廃、履歴として残す)
42: 
```

### `consensus/core/src/palw_model_eligibility_fold_v1.rs:34–42`

```text
34: /// **Round 5 (user decision 2026-10-15): model-level review is ABOLISHED for every model.** Past `palw_model_tier_v1` (ADR-0179) this
35: /// whole review — the stages Registered → Probation → Eligible → Lapsed, the exam (inner 74/75), the draws, the attempt fee, the attempt
36: /// caps and the fair queue, renewal and lapse, the probation grant and pin — is unreachable: every read answers "not governed" (the
37: /// old rule), exam objects are refused and the sweep does nothing. A registered model is the tier system's fixed 0.3 from its
38: /// registration; per-claim safety is RFC-0018 Gate C's alone. (Validation arms `palw_model_eligibility_v1` only with the tier fence at
39: /// or below it, so the review never runs on any valid network; below the tier fence the code stays the dormant RFC-0017 rule.)
40: pub(super) fn eligibility_review_retired_v1(params: &PalwStateParamsV2, daa: u64) -> bool {
41:     params.model_tier_v1().is_some_and(|mirror| mirror.active_at(daa))
42: }
```

### `consensus/core/src/palw_model_tier_v1.rs:32–72`

```text
32: //! carries that chain and nothing parallel to it; the tier API below (`palw_model_tier_v1`, `_at_v1`, `_of_model_v1`,
33: //! `palw_model_tier_of_v1`, the coefficient, the weight and the cap) is unchanged for its callers.
34: 
35: use crate::Hash64;
36: use crate::config::params::{ForkActivation, Params};
37: use crate::palw_annual_catalog_v1::PalwAnnualCatalogChainV1;
38: use crate::palw_bond_budget_v1::PalwAllocationWeightV1;
39: use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
40: use crate::palw_state_v2::{PalwChainStateV2, PalwStateParamsV2};
41: 
42: /// **The approved tier's coefficient** (1.0).
43: pub const PALW_MODEL_TIER_APPROVED_PERMILLE_V1: u32 = 1_000;
44: /// **The registered tier's coefficient** (0.3) — the user's design value; permanent for every on-chain registered model.
45: pub const PALW_MODEL_TIER_REGISTERED_PERMILLE_DESIGN_V1: u32 = 300;
46: /// **The registered tier's aggregate cap κ**, permille of the network's total allocation (block rights and coinbase) — a DESIGN
47: /// value, unapproved: however many models register, the whole 0.3 tier (its shared new-model pool included) never takes more.
48: pub const PALW_MODEL_TIER_REGISTERED_CAP_PERMILLE_DESIGN_V1: u32 = 300;
49: 
50: /// The fixed-point scale of the registered-tier cap factor.
51: const PALW_MODEL_TIER_CAP_FACTOR_SCALE_V1: u64 = 1_000_000;
52: 
53: /// **`Params::palw_model_tier_v1`'s value**: the tier system's height, the annual catalog chain (RFC-0019: `ApprovedModels(h)` IS the
54: /// catalog active at `h` — edition 0 the genesis catalog, each later edition one annual fork's additions) and the two design values.
55: #[derive(Clone, Debug, PartialEq, Eq)]
56: pub struct PalwModelTierFenceV1 {
57:     pub activation: ForkActivation,
58:     /// **RFC-0019's catalog chain** — the ONE storage of `ApprovedModels` (no parallel list). Edition 0 names the genesis models
59:     /// explicitly (classes the genesis assembly registered are approved as a chain fact without being listed); every later edition
60:     /// activates at its own height, at or after the fence, and only adds.
61:     pub catalogs: PalwAnnualCatalogChainV1,
62:     /// The registered tier's coefficient `c`, permille (design 300).
63:     pub registered_permille: u32,
64:     /// The registered tier's aggregate cap `κ`, permille (design value).
65:     pub registered_cap_permille: u32,
66: }
67: 
68: impl PalwModelTierFenceV1 {
69:     /// **NOT A PROPOSAL**: the fence at `activation` with the design values and an empty genesis catalog (the fork-id probe, fixtures).
70:     pub fn at(activation: ForkActivation) -> Self {
71:         Self {
72:             activation,
```

### `consensus/core/src/palw_bond_budget_v1/allocation_v3.rs:288–320`

```text
288:             .filter(|(k, row)| !Self::is_allocation_aux_row_v1(row) && !averages.contains_key(*k))
289:             .map(|(k, _)| *k)
290:             .collect();
291:         for model in stale {
292:             put(&mut self.models, PALW_BUDGET_TABLE_MODELS_V1, &model, None, j);
293:         }
294:         // ADR-0179: `A_m = c_m · S̄_m^α`, once; then the registered tier's aggregate cap.
295:         let mut sum_a = PalwAllocationWeightV1::ZERO;
296:         let mut registered_sum = PalwAllocationWeightV1::ZERO;
297:         let mut rows: Vec<(Hash64, u128, PalwAllocationWeightV1, bool)> = Vec::with_capacity(averages.len());
298:         for (model, average) in averages {
299:             let (registered, c_permille) = tier(&model);
300:             let weight = crate::palw_model_tier_v1::palw_model_tier_weight_v1(policy.curve.weight(average), c_permille);
301:             sum_a = sum_a.saturating_add(weight);
302:             if registered {
303:                 registered_sum = registered_sum.saturating_add(weight);
304:             }
305:             rows.push((model, average, weight, registered));
306:         }
307:         let factor = registered_cap_permille
308:             .map(|cap| crate::palw_model_tier_v1::palw_model_tier_cap_factor_ppm_v1(sum_a, registered_sum, cap))
309:             .unwrap_or(1_000_000);
310:         for (model, average, weight, registered) in rows {
311:             let weight = if registered { crate::palw_model_tier_v1::palw_model_tier_scale_v1(weight, factor) } else { weight };
312:             put(
313:                 &mut self.models,
314:                 PALW_BUDGET_TABLE_MODELS_V1,
315:                 &model,
316:                 Some(PalwModelBudgetRowV1 { epoch: index, capital: average, weight, reserved_sompi: 0 }),
317:                 j,
318:             );
319:         }
320:         let mut header = self.header.clone();
```

### `consensus/core/src/palw_rfc0016_fold_v1.rs:130–146`

```text
130:             }
131:         }
132:     }
133:     // V3 (user decision 2026-10-15): `S_entry` is the network's ELIGIBLE minimum producer bond — never below the bond floor — and
134:     // an entrant's capital is its allocation capital, which counts only eligible capital (RFC-0017 GAP-E1: Registered / Lapsed 0;
135:     // ADR-0177 C5: verifier capital only with a defined liability).
136:     let mut values = *values;
137:     values.entrant_min_capital_sompi = values.entrant_min_capital_sompi.max(params.min_collateral_sompi());
138:     // ADR-0179 / round 5 (user decisions 2026-10-15): past `palw_model_tier_v1` there is no model review — no Probation and no
139:     // promotion, hence no entrant — so the entrant reserve is MOOT and is not deducted (ε = 0): the registered tier's aggregate cap κ in
140:     // the allocation weights is the one tier-level bound.
141:     if params.model_tier_v1().is_some_and(|mirror| mirror.active_at(start)) {
142:         values.entrant_reserve_permille = 0;
143:     }
144:     palw_rights_issue_v1(epoch_index, start, length, n_exp, &values, weights.as_ref(), &entrants, |m| {
145:         eligibility.block_rights_eligible_v1(m, start)
146:     })
```

<a id="s31"></a>

## S31 — FP receipt admissionはFinalのみ、Gate Cはweightだけ

### `consensus/core/src/palw_fp_admission_v3.rs:175–195`

```text
175:     receipt_maturity_daa: u64,
176:     receipt_use_window_daa: u64,
177:     beacon: &PalwBeaconFactV3,
178:     spend: &ReceiptSpendFacts,
179:     pricing: Option<&crate::palw_state_v2::PalwFpPricingV1>,
180: ) -> Result<&'a crate::palw_state_v2::PalwClaimStateV2, PalwFpAdmissionV3Error> {
181:     // 1. The claim: exists, free-prompt, certified.
182:     let claim = state.claim(&spend.claim_id).ok_or(PalwFpAdmissionV3Error::ClaimMissing(spend.claim_id))?;
183:     let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else {
184:         return Err(PalwFpAdmissionV3Error::NotFreePrompt(spend.claim_id));
185:     };
186:     let PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
187:         return Err(PalwFpAdmissionV3Error::NotCertified(spend.claim_id));
188:     };
189: 
190:     // 2. The quantum: exists, unspent on this chain.
191:     if spend.quantum_index >= *quanta {
192:         return Err(PalwFpAdmissionV3Error::QuantumOutOfRange { claim: spend.claim_id, index: spend.quantum_index, quanta: *quanta });
193:     }
194:     if spent.contains(&spend.quantum_index) {
195:         return Err(PalwFpAdmissionV3Error::QuantumAlreadySpent { claim: spend.claim_id, index: spend.quantum_index });
```

### `consensus/core/src/palw_state_v2.rs:19466–19499`

```text
19466: fn apply_receipt_spend(
19467:     builder: &mut TransitionBuilder<'_>,
19468:     ctx: &PalwBlockContextV2,
19469:     spend: &PalwReceiptSpendUnsignedV3,
19470: ) -> Result<(), PalwStateV2Error> {
19471:     // RFC-0002 §II.14: a `PNR9` native receipt block reaches this shared site as fold-envelope version 39 (the processor admits
19472:     // it only past `palw_native_mining_v39`); it never reads a free-prompt claim.
19473:     if spend.version == crate::palw_native_receipt_v39::PALW_NATIVE_RECEIPT_FOLD_VERSION_V39 {
19474:         return palw_native_mining_fold_v39::apply_native_receipt_block_v39(builder, ctx, spend);
19475:     }
19476:     let claim_id = spend.claim_id;
19477:     let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
19478:     let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else {
19479:         return Err(PalwStateV2Error::NotFreePromptClaim(claim_id));
19480:     };
19481:     if !matches!(claim.phase, PalwClaimPhaseV2::Final { .. }) {
19482:         return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "ReceiptSpend" });
19483:     }
19484:     // **2026-09-24 DoS audit #5, the forfeiture half.** A free-prompt `Final` licenses receipt
19485:     // blocks, each paid the unescrowed worker base at the coinbase — the audit measured one W₀ of
19486:     // fabricated compute at ~3,200 MSK of such blocks against a seat lock of ~7 MSK. ADR-0151
19487:     // forfeits a convicted execution's rights at every stage of the EXECUTION lane (snapshot,
19488:     // mint, and since #7 the minted schedule), but this lane checked only the phase, and a
19489:     // `PanelFalseValid` conviction never moved it: spends went on after the conviction exactly as
19490:     // before it (`dos_repro_1`). Past the fence the conviction's root closes the lane too.
19491:     //
19492:     // By ROOT, as ADR-0151 forfeits: a sibling claim that carries the same execution certified
19493:     // the same convicted work. The named claim itself no longer reaches here past the fence — the
19494:     // conviction voids it (`reverse_convicted_final`, #8) and the phase check above refuses it —
19495:     // so this arm is what reaches the copies. Below the fence nothing is read, so a dormant
19496:     // network's spends fold exactly as they did.
19497:     if builder.extras.audit_2026_09_23_active && builder.state.palw_execution_root_is_forfeited_v1(&claim.execution_root) {
19498:         return Err(PalwStateV2Error::ReceiptRightsForfeited { claim: claim_id, execution_root: claim.execution_root });
19499:     }
```

### `consensus/core/src/palw_state_v2.rs:19574–19586`

```text
19574:     // **Lane BUDGET (ADR-0176 D3, design §2.6): the spend draws on its commitment's reservation** — one block and this block's carve,
19575:     // strict (the coinbase pays the worker share whole, so a short reservation refuses the spend), and the quantum's weight, clipped:
19576:     // `safe_weight` takes the grant. The receipt census below still counts the work done (it measures production, never reward).
19577:     let last_quantum = spent.len() as u64 + 1 >= *quanta as u64;
19578:     let credited = builder.bond_budget_receipt_spend_v1(ctx, &claim_id, per_quantum, last_quantum)?;
19579:     // **RFC-0018 Gate C's receipt-spend site**: a spend of an uncertified free-prompt claim weighs nothing (the identity below its
19580:     // fence, keyed on the claim's acceptance DAA, and on every shipped network). The block still spends its quantum.
19581:     let credited = crate::palw_rfc0018_v1::palw_gate_c_weight_v1(builder.params, &builder.state, &claim_id, &claim, credited);
19582:     builder.state.safe_weight = builder.state.safe_weight.checked_add(credited).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
19583:     let mut updated = claim.clone();
19584:     let PalwClaimSourceV2::FreePrompt { spent: ledger, .. } = &mut updated.source else { unreachable!("matched above") };
19585:     ledger.insert(spend.quantum_index);
19586:     builder.write_claim(claim_id, Some(updated))?;
```

### `consensus/src/pipeline/virtual_processor/processor/palw.rs:10042–10077`

```text
10042:         let pre_pow_hash = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
10043:         if let Some(v4) = &v4 {
10044:             kaspa_consensus_core::palw_receipt_v4::check_palw_receipt_spend_admission_full_v5(
10045:                 state,
10046:                 point,
10047:                 network_domain,
10048:                 pre_pow_hash,
10049:                 header.timestamp,
10050:                 header.nonce,
10051:                 freeprompt.receipt_maturity_daa(),
10052:                 freeprompt.receipt_use_window_daa(),
10053:                 &beacon,
10054:                 v4,
10055:                 |key, message, sig, context| kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false),
10056:                 self.palw_fp_pricing().as_ref(),
10057:             )
10058:             .map_err(|e| e.to_string())?;
10059:             let _ = state_params;
10060:             return Ok(Some(envelope));
10061:         }
10062:         kaspa_consensus_core::palw_fp_admission_v3::check_palw_receipt_spend_admission_full_v4(
10063:             state,
10064:             point,
10065:             network_domain,
10066:             pre_pow_hash,
10067:             header.timestamp,
10068:             header.nonce,
10069:             freeprompt.receipt_maturity_daa(),
10070:             freeprompt.receipt_use_window_daa(),
10071:             &beacon,
10072:             &envelope,
10073:             |key, message, sig, context| kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false),
10074:             // ADR-0148: a compute-era claim draws against the lane's pooled target.
10075:             self.palw_fp_pricing().as_ref(),
10076:         )
10077:         .map_err(|e| e.to_string())?;
```

### `consensus/src/pipeline/virtual_processor/processor/palw.rs:4288–4308`

```text
4288:                 }
4289:                 Ok(None) => match self.palw_v2_check_receipt_spend(&header, state, state_params, point) {
4290:                     Ok(Some(envelope)) => {
4291:                         // B-5: at most one sibling per (claim, quantum) is entitled; the fold folds
4292:                         // the quantum's weight once, so paying a second spend of it mints reward the
4293:                         // chain never counted. Only past the audit fence (below it, unchanged).
4294:                         if audit_active && !seen_here_quanta.insert((envelope.spend.claim_id, envelope.spend.quantum_index)) {
4295:                             debug!(
4296:                                 "merged block {blue} spends a certified quantum ({}, {}) already paid in this mergeset",
4297:                                 envelope.spend.claim_id, envelope.spend.quantum_index
4298:                             );
4299:                             unentitled.insert(*blue);
4300:                         }
4301:                     }
4302:                     Ok(None) => {
4303:                         debug!("merged block {blue} carries no work this chain accepted");
4304:                         unentitled.insert(*blue);
4305:                     }
4306:                     Err(why) => {
4307:                         debug!("merged block {blue} carries a receipt spend this chain refuses: {why}");
4308:                         unentitled.insert(*blue);
```

### `consensus/src/processes/coinbase.rs:227–232`

```text
227:         // **RFC-0009 (`palw_receipt_spend_v4`): the merged V4 receipt blocks and how each is paid.** A block in this map has its
228:         // subsidy-derived worker reward split — `subsidy_part − ⌊subsidy_part × bps / 10 000⌋` to `miner_script` (the executor bond's
229:         // registered payout), the rest (the builder's fee and every fee-derived share of its own block) to the block's own script — as two
230:         // outputs, in that order. The two sum to exactly what the one output would have been, so no issuance, panel leg, reserve or maturity
231:         // moves. Empty where the fence is dormant, which leaves this function byte-identical.
232:         palw_receipt_v4_payouts: &BlockHashMap<kaspa_consensus_core::palw_receipt_v4::PalwReceiptV4Payout>,
```

### `consensus/core/src/palw_state_v2.rs:14806–14813`

```text
14806:         // A receipt spend is NOT skipped when this block's own objects convicted its execution
14807:         // (`WrongPhase` from #8's reversal, `ReceiptRightsForfeited` from #5): the processor
14808:         // admitted it against the parent state, and the fold refuses it here, so the block is
14809:         // disqualified on every node alike. Deliberately — unlike an attempt, a spend escrows
14810:         // nothing: the coinbase pays a chain block's worker share whatever its work turned out to be
14811:         // (the selected-parent exemption in the processor's `palw_v2_unentitled_blues`), so skipping would
14812:         // pay one receipt block for a convicted execution, which is what #5 closed.
14813:         PalwBlockWorkV3::ReceiptSpend(spend) => apply_receipt_spend(&mut builder, ctx, spend)?,
```

<a id="s32"></a>

## S32 — Held issuance releaseはproducerの単一payload

### `consensus/core/src/palw_security_certification_fold_v1.rs:362–405`

```text
362: fn name_certified_reward_v1(
363:     builder: &mut TransitionBuilder<'_>,
364:     now: u64,
365:     claim_id: &Hash64,
366:     claim: &PalwClaimStateV2,
367:     row: &PalwClaimCertificationRowV1,
368: ) -> Result<(), PalwStateV2Error> {
369:     // RFC-0008 once-only settlement (dormant): the held issuance reward is named once across every consumer. This guards the
370:     // PAYMENT only; the weight credit above is Gate C's own (SecurityCertified) and is neither added nor read here.
371:     let releases = row.held_reward > 0 && {
372:         use crate::palw_settlement_once_v1::{
373:             PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
374:         };
375:         // RFC08-2: the issuance reward drawn at the claim's Final and held for certification — no new R draw; named at once now.
376:         let final_daa = match claim.phase {
377:             PalwClaimPhaseV2::Final { final_daa } => Some(final_daa),
378:             _ => None,
379:         };
380:         builder
381:             .settle_once_liable_v1(
382:                 now,
383:                 PalwSettlementObligationV1::of(PalwSettlementLegV1::CertificationRelease, claim_id.as_bytes()),
384:                 PalwSettlementConsumerV1::SecurityCertification,
385:                 row.held_reward,
386:                 PalwSettlementDebitV1::Held { amount: row.held_reward },
387:                 super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::V2Claim {
388:                     claim: *claim_id,
389:                     final_daa,
390:                     horizon: None,
391:                     draw: 0,
392:                     paid_now: row.held_reward,
393:                     exported_before: None,
394:                     vest_until: None,
395:                 },
396:             )?
397:             .pays()
398:     };
399:     if releases {
400:         let key = palw_certification_payout_key_v1(claim_id);
401:         let held = builder.state.pending_payouts.get(&key).map(|p| p.amount).unwrap_or(0);
402:         let amount = held.checked_add(row.held_reward).ok_or(PalwStateV2Error::Overflow("certified reward"))?;
403:         builder.write_payout(key, Some(PalwPayoutV2 { payload: Hash64::from_bytes(row.payout_payload), amount }))?;
404:     }
405:     Ok(())
```

### `docs/design/palw/rfc-0018-implementation-status-2026-10-15.md:33–45`

```text
33: | leaf v2 / gRPC / RPCへの配線 | 実装 | record `certification`、`palw_rule_e_certification_summary_v1`、`rpc.proto`(settlement 13/14、claim row 39)、`convert/message.rs`、claim row wire v6 | `rfc18_a_client_reads_gate_c_from_the_window` |
34: | siblingの順序非依存(上限・seed anchor) | 実装(休眠、sibling fence) | mergeset内はcanonical順、seed anchorはspan内最小priority(`record_round_seed_anchor_of`)。selected parentのsiblingはGHOSTDAGの決定的規則で先に採用(RFC §4.1 I-S6) | E2E `rfc0018_dag_siblings.rs`(到着順全6通りでstate root・seed anchor一致) |
35: 
36: ## 3. 完了できなかった点と理由
37: 
38: * **build・試験の実行:** 本laneの規則(cargo build/check/test禁止)により未実施。validation agentが実行する。
39: * **anchor:** GAP-A1/A2は解消。認識は採用の1 chain block後になる。
40: * **GAP-C3:** 認証前のfree-prompt spendのcoinbase carveは止めていない(weightは止める)。processorのproducer pre-checkまで変える必要がある。
41: * **GAP-C4:** 保留した発行由来報酬は認証時にproducerへ1 legで払う。Panel share・buyback・vestingへの分配は未設計(U-2)。
42: * **GAP-R4:** 2026-10-15に解消(rule-E leaf v3: `palw_fork_choice_rule_e_leaf_v3.rs`、node `palw_committed_root_v1` → `palw_committed_state_root_v2`、v39 anchor・L2 client・carriage RPCの共通検証、native roots chain。試験 `palw_fork_choice_rule_e_leaf_v3/tests.rs`)。
43: * **selected parentのsibling:** GHOSTDAGがselected parentに選んだsiblingは自分のblockで先に採用される。決定的(blue work、block hash)で到着順に依存しないが、mergeset内の他siblingと同じ土俵では比べない。
44: * **DAG幅のreader:** live経路(header処理、GHOSTDAG、difficulty、pruning)に触れるため実装しない(RFC §8.4)。
45: * **DNS-BFTの物理除去:** 別lane(`integ/rfc12-dns-removal`)。本branchには取り込まない。
```

<a id="s33"></a>

## S33 — RFC19 R7/R8未実装とdriver choices

### `docs/rfc/0019-annual-model-upgrade.md:310–330`

```text
310: | D-1 | 再起動・IBD・reorg で同じ読み | `the_catalog_status_is_a_pure_function_of_params_and_tip_across_restart_and_reorg`、`the_tiered_roll_is_the_same_after_reorg_ibd_and_restart`、driver `restart` |
311: | D-2 | カタログのモデルがいなくても BASE-0 / heartbeat が進む、掲載はブロックを gate しない | `a_catalog_naming_absent_models_never_gates_blocks_or_liveness`、driver `liveness` |
312: | E-1 | lifecycle の名前、出荷できる状態、ノードは SCHEDULED / ACTIVE だけ | `the_lifecycle_names_are_operational_and_only_a_released_edition_ships` |
313: | E-2 | RPC wire v4 の往復、CLI 表示 | `rpc/core/src/model/tests.rs`(Mock に annualCatalog)、`palw_registry.rs` `rfc0019_the_registry_renders_the_annual_catalog` |
314: | R-1 | 混在バイナリ(切替前): 同じブロック・state・fork choice | `scripts/drills/rfc0019_rolling_upgrade_drill.py rolling` |
315: | R-2 | 切替直後に全更新ノードが同じ root と階層を読む | driver `switch`(interop 記録を書く) |
316: | R-3 | 未更新ノードは同期成功を偽らない、更新集合は旧ノードの block を採らない | driver `stale` |
317: | R-4 | DAG sibling / RED の境界保持、claim は一度だけ | driver `boundary` |
318: | R-5 | 再起動・IBD・partition / rejoin・pruning の一貫性 | driver `restart` |
319: | R-6 | カタログのモデルの miner なしで heartbeat / BASE-0 が切替を越える | driver `liveness` |
320: | R-7 | **新規ローカル LLM の ID / root は追加のみ。** 旧 genesis / edition 0 と前 release の接頭辞は不変、新 `class_id` は未使用で root は固定。階層 fence が H 前から有効な年次追加試験では、新登録モデルは H 前 300‰、H 後 1000‰、旧 genesis は前後 1000‰。既存 ID / root の再利用・上書き・消失は FAIL。更新ノードで edition / root / tier / state が一致する | **追加予定:** 年次 edition 1 devnet の `model-add` drill。`getPalwModelRegistry`、manifest `--previous`、`palw-class inspect`、R-1〜R-5 の実ノード記録。現行 driver には未実装 |
321: | R-8 | **実ローカル LLM 推論。** 旧 genesis モデルと追加モデルを別の固定 ID / artifact で起動し、H の前後で実際の text prompt を送る。`/health` の `class_id`、`/v1/models`、artifact root 検査と推論応答 / commitment を対応付ける。H 後も旧モデルは利用可能、新モデルは新 ID で回答し、他 ID への暗黙の fallback はしない。報酬対象を主張する場合は実 claim の受理・Gate C 認証も別途確認する。未起動・artifact 不在・ID 不一致・空回答・未観測なら PASS にしない | **追加予定:** `local-llm` E2E drill(実 gateway / worker + RPC / claim 記録)。現行 driver には未実装 |
322: | M-1 | manifest: 署名・checksum・再現・指紋・カタログ・追加のみ・未署名は失敗 | `misaka-release-manifest.py self-test`(実行済み)と、使い捨て鍵による write → payload → sign → attach → verify と改竄 2 種の失敗(実行済み、§15.4) |
323: | M-2 | readiness: M1–M5、void / 窓外の除外、到達不能を別計上、合意効果なし | `misaka-release-readiness.py --self-test`(実行済み) |
324: 
325: driver は何も arm しない。切替高さを schedule しないバイナリでは BLOCKED になる。状態は `OBSERVED_CONSISTENT` / `BLOCKED` / `FAIL` だけで、承認ではない。
326: 
327: **R-7 / R-8 は設計上の受入条件であり、driver と E2E 実行証拠を追加するまでは未検証(BLOCKED)。既存の `switch` だけを通してローカル LLM 追加を PASS としてはならない。**
328: 
329: ## 12. 最初の開発段階 — 同じカタログでの rolling upgrade(testnet-12 DAA 14,000)
330: 
```

### `scripts/drills/rfc0019_rolling_upgrade_drill.py:381–404`

```text
381:         self.mesh(roles)
382:         self.fixture('bootstrap')
383:         start = min(self.daa(r) for r in roles)
384:         self.advance(roles, self.switch + self.margin, fixture='heartbeat')
385:         self.report['liveness'] = {'fromDaa': start, 'toDaa': min(self.daa(r) for r in roles),
386:                                    'note': 'heartbeat / BASE-0 fixtures only; no catalog-model miner ran'}
387: 
388: 
389: def main():
390:     parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
391:     parser.add_argument('scenario', choices=['rolling', 'switch', 'stale', 'boundary', 'restart', 'liveness'])
392:     parser.add_argument('--plan', required=True)
393:     parser.add_argument('--out', required=True)
394:     parser.add_argument('--interop-out', help='where to write the misaka/interop-record/v1 list (readiness M3)')
395:     args = parser.parse_args()
396:     raw = Path(args.plan).read_bytes()
397:     plan = json.loads(raw)
398:     ports = [n[k] for n in plan['nodes'].values() for k in ('wrpc', 'p2p')]
399:     assert all(p.startswith('127.0.0.1:') for p in ports) and len(ports) == len(set(ports)), 'distinct owned loopback ports'
400:     assert all('old_argv' in n and 'new_argv' in n for n in plan['nodes'].values()), 'every role names its old and new binary'
401:     root = Path(tempfile.mkdtemp(prefix='misaka-rfc0019-%s-' % args.scenario))
402:     drill = Drill(plan, root)
403:     result = {'scenario': args.scenario, 'planSha256': hashlib.sha256(raw).hexdigest(), 'artifactDirectory': str(root),
404:               'switchDaa': drill.switch, 'status': 'FAIL', 'approval': False}
```

<a id="s34"></a>

## S34 — RFC20現行スコープと必要な計測

### `docs/rfc/0020-long-context-calibration-memory.md:53–70`

```text
53: ## 2. 計測を先にする — RAM 計上モード(実装済み・未検証)
54: 
55: `misaka/palw/tir/lower/src/ram_account.rs`。`--ram-accounting FILE`(`palw-tir-convert`、`palw-tir-fidelity`)、パイプラインでは `run --ram-accounting`(`<model dir>/ram-<step>.jsonl`)。
56: 
57: * **種類:** `weights`(実行中 occurrence の常駐 param と lazy cache)、`carries`(RAM 上の carry)、`spilled_carries_on_disk`(退避分、RAM ではない)、`kv_history`、`recurrent_state`、`attention_temporaries`(1 回の attention 呼び出しの最大一時領域)、`stats`、`logits`。
58: * **段階:** `open`、`calibration`、`conversion`、`materialise`、`fidelity.float`、`fidelity.int`。段階に入るたびに peak を live 値から数え直す。
59: * **出力:** 1 行 1 JSON。occurrence(層)ごと、整数実行 1,024 位置ごと、段階の境で snapshot(live・peak・RAM の live 合計・OS の RSS)。最後に段階ごとの peak の要約行。
60: * **注意:** live は評価器のデータ構造を長さで数えた値(`f32` 行 × 4 B)であり、アロケータの見方ではない。アロケータは解放済みメモリを保持し得るので、RSS は live 合計より大きいままになり得る。両方を記録するのはそのためである。無効時は相対 atomic の読み 1 回だけで、結果は変わらない。
61: 
62: ## 3. carry の二重保持を除く(実装済み・未検証)
63: 
64: 旧実装の `run_layer_major` / `run_to_post` は、occurrence ごとに入力の carry 系列(`carries`)を読みながら出力系列(`outs`)を別に作り、occurrence の終わりで差し替えていた。その間、系列 2 本分を同時に持つ。
65: 
66: **変更(ユーザー提供パッチを、位置の run 評価に合わせて適用):** 入力行は読んだ時点で取り出し(RAM 上の行は空にして解放)、出力は次の occurrence の carry として書く。post block では入力行を退役させ、求められた logits だけを残す。session・絶対位置・評価順・site の観測順・`SiteStat` の merge 順は変わらない。
67: 
68: * **期待される削減:** 系列 1 本分の carry。32K × hidden × 4 B = #2 256 MiB、#3 512 MiB、#6 640 MiB(#3 はユーザー分析の例 ≈512 MiB に一致)。run の長さ分(既定 32 位置)の出力は一時的に重なる。
69: * **総 RSS については何も言わない**(アロケータ)。§2 の計上で live の減少を確認する。
70: * **ビット同一性:** `misaka/palw/tir/lower/tests/position_blocks.rs::consumed_and_spilled_carries_are_the_two_sequence_run_bit_for_bit` が、旧アルゴリズム(2 系列保持・1 位置ずつ・系列順 merge)をそのまま写した参照と、統計の digest(`stats_digest_hex`)・正準 JSON・全 logits のビットを 7 つのアーキテクチャの toy checkpoint で比較する。
```

### `docs/rfc/0020-long-context-calibration-memory.md:85–97`

```text
85: ### 5.1 監査: 全系列のスコア行列はどこにあるか(実施済み)
86: 
87: | 経路 | attention の形 | 32K での一時領域 |
88: | --- | --- | --- |
89: | 較正・忠実度の float 参照(`float_ref::attention`) | 1 query 位置 × その位置までの全履歴、head ごと(2 パス softmax) | #6: 24 head × 32,768 × (dot・score・prob の f64 + 観測用 f32)≈ 25 MiB |
90: | 位置の run 評価(`Session::eval_occurrence_run`) | 同上。run 内の各位置は**絶対位置で見える行だけ**を読む(§5.5) | 同上 |
91: | 整数の参照評価器・型付き実行器(`tir`、`misaka-palw-tir-exec`) | 1 token ずつの step。スコアは履歴長の行 | 同程度 |
92: | court・re-execution | 1 位置の step | 同程度 |
93: | A16 / PALWQ361 エンジン(base0) | 1 token ずつ | 同程度 |
94: | 全行列(N × N)を作る経路 | 双方向エンコーダ(`lower/bidir.rs`)、vision(`lower/vision.rs`)、diffusion(`diffusion/attn.rs`) | **genesis の範囲外**(テキスト生成のみ) |
95: | 全系列の **logits**(スコアではない) | `palw-tir-fidelity` を `--exec` なしで使う時、両側の logits を全位置分保持 | 32K × 248,320 × 12 B ≈ 97 GB → **8 GiB 超は拒否するよう変更**(§7) |
96: 
97: 結論: genesis のテキスト経路(較正・変換・忠実度 `--exec`・実行器・court)に 32K × 32K のスコア行列は存在しない。§5.2–§5.4 のタイル化は**実装しない**(ユーザー方針 5: 全行列経路が実在する所だけ)。全行列経路(双方向・vision・diffusion)を将来 genesis に入れる場合の仕様として残す。
```

### `docs/rfc/0020-long-context-calibration-memory.md:128–157`

```text
128: ### 5.5 位置の run 評価(実装済み・未検証)は chunk attention ではない
129: 
130: 32K 較正の速度のため、block の各 node を run 内の全位置について評価してから次の node へ進む(`Session::eval_occurrence_run`、既定 32 位置、`PALW_FLOAT_POSITION_BLOCK` で変更、1 で従来どおり)。行列は run ごとに 1 回読まれる。attention は run 内でも**1 query 位置ずつ**、その位置までに位置ごとの実行が append したはずの行(`key_pos ≤ query_pos`)だけを読む。系列分割・範囲縮小・`chunk`/`max_window` の流用は一切ない。admit する block は、履歴が append 1 つと attention の読みだけ、固定状態が 1 node だけ(GDN、causal conv1d)のものに限り、それ以外は位置ごとに戻る。統計・logits がビット一致することを `tests/position_blocks.rs` が 7 アーキテクチャで固定する。
131: 
132: ## 6. 32K を超える文脈(64K / 128K)
133: 
134: ### 6.1 線形に増えるもの
135: 
136: | | 32K | 64K | 128K |
137: | --- | --- | --- | --- |
138: | carry(#6、5,120 × 4 B) | 640 MiB | 1.25 GiB | 2.5 GiB |
139: | KV 履歴(#6、attention 層 1 つ、key+value 2,048 × 4 B) | 256 MiB | 512 MiB | 1 GiB |
140: | attention 一時領域(#6、1 query 位置) | ~25 MiB | ~50 MiB | ~100 MiB |
141: | 全 attention の演算量 | N² に比例 | ×4 | ×16 |
142: 
143: ### 6.2 carry のディスク退避(実装済み・未検証、opt-in)
144: 
145: `--spill-carries-dir DIR [--spill-carries-budget-mib N]`(両道具、パイプラインも同名)。ある occurrence の出力が系列全体で予算を超える見込み(最初の行のバイト × 位置数 > 予算、0 なら常に)なら、出力をファイルへ位置順に書き、次の occurrence は run ごとに読み戻す。行は同じ `f32` のビット(リトルエンディアン)なので、in-RAM の実行とビット一致する(`tests/position_blocks.rs` が予算 0 で検証)。RAM をディスク I/O と交換する。32K では不要(§9)。
146: 
147: ### 6.3 KV 履歴の退避(仕様、未実装)
148: 
149: 手順: 層ごとに重みを読む → 隠れ状態をブロック単位でディスクから読む → GDN 状態・conv 履歴は RAM に保つ → attention は KV をブロック単位で読む → 統計を更新 → 出力をディスクへ書く → バッファを解放。演算量は全 attention で O(N²) のまま。
150: 
151: 未実装の理由: layer-major では同時に存在する KV 履歴は attention 層 1 つ・系列 1 本分で、genesis では 128K でも #6 で 1 GiB に収まる。さらに 1 query 位置ずつ KV をディスクから読むと I/O が O(N²) になるため、KV 退避は query ブロックでまとめて読む構造(§5.2 の query タイル化)を前提とする。必要になった時点で §5.2(a) と同時に実装する。
152: 
153: ## 7. 検証
154: 
155: * **ビット同一性(試験 source、未実行):** `misaka/palw/tir/lower/tests/position_blocks.rs`(run 評価、消費型 carry、退避 carry を旧アルゴリズムと統計 digest・正準 JSON・全 logits で比較。7 アーキテクチャ)、`misaka/palw/tir/lower/src/float_ref/tests.rs`(スレッド化した `linear_raw` / `linear_raw_many` / `attention` / `gated_delta_traced` を旧カーネルの写しとビット比較。GDN 状態を含む)。
156: * **長文脈ドリフト検査(実装済み・未検証):** `palw-tir-fidelity --exec --drift-windows N` は、評価の全位置を作った端から比較し(logits 行を保持しない)、評価を N 等分した窓ごとの平均 KL と最大 KL を出す。パイプラインの `fit` は短い held-out(2 × 64 token)に加えて、較正に使った最初の 32K token の**次の** 32K token(RFC-0002、held-out)で 32 窓を測り、平均 KL ≤ 0.1、top-1 ≥ 0.8、各窓の平均 KL ≤ 0.1、後半/前半 ≤ 1.5(後半の KL ≤ 0.01 なら免除)を満たさなければ index を upload しない。
157: * **未計測:** 8K / 16K / 32K の peak RAM と時間。§2 の計上モードで測る(§9 のコマンド)。
```

### `misaka/palw/tir/lower/tests/position_blocks.rs:115–153`

```text
115:             for (k, v) in sess.sites.take().unwrap_or_default() {
116:                 stats.entry(k).or_default().merge(&v);
117:             }
118:         }
119:         carries = next;
120:     }
121:     (stats, logits)
122: }
123: 
124: #[test]
125: fn consumed_and_spilled_carries_are_the_two_sequence_run_bit_for_bit() {
126:     let quiet = |_: usize, _: usize| {};
127:     let spill_dir = std::env::temp_dir().join(format!("palw-carry-spill-{}", std::process::id()));
128:     for (rel, _) in FIXTURES {
129:         let dir = fixture(rel);
130:         let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap_or_else(|e| panic!("{rel}: {e}"));
131:         let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap_or_else(|e| panic!("{rel}: {e}"));
132:         let ck = Checkpoint::open(&dir).unwrap_or_else(|e| panic!("{rel}: {e}"));
133:         let (store, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap_or_else(|e| panic!("{rel}: {e}"));
134:         let loader = Resident(Arc::new(store));
135:         let seqs = fidelity::random_sequences(prep.hl.vocab, 2, 29, 5);
136:         let (want_stats, want_logits) = layer_major_before(&prep.hl, &loader, &seqs);
137:         let check = |label: &str, block: usize| {
138:             let got = run_layer_major_keep(&prep.hl, &loader, &seqs, true, &KeepLogits::All, block, &quiet)
139:                 .unwrap_or_else(|e| panic!("{rel} {label}: {e}"));
140:             assert_eq!(stats_digest_hex(&got.stats), stats_digest_hex(&want_stats), "{rel} {label}: the statistics digest");
141:             assert_eq!(stats_to_json(&got.stats), stats_to_json(&want_stats), "{rel} {label}: the statistics");
142:             for (si, (a, b)) in got.logits.iter().zip(&want_logits).enumerate() {
143:                 assert_eq!(bits(a), bits(b), "{rel} {label}: sequence {si}'s logits");
144:             }
145:         };
146:         check("consumed carries, position by position", 1);
147:         check("consumed carries, runs of 8", 8);
148:         // Every occurrence's carries on disk (budget 0), then back in RAM.
149:         set_carry_spill(Some(SpillCfg { dir: spill_dir.clone(), budget_bytes: 0 }));
150:         check("spilled carries, runs of 8", 8);
151:         set_carry_spill(None);
152:     }
153: }
```

<a id="s35"></a>

## S35 — 最新統合ログの未再実行・branchとの区別

### `docs/design/palw/remaining-work-2026-10-16.md:1–47`

```text
1: # 残作業(2026-10-16 時点)— G14・RFC06〜12・RFC02・リリース
2: 
3: ユーザー指示(10-16):トークンが少ないので、RFC07 の認証済み adapter を検証より先に実装する。それ以外は commit・push し、この文書に残作業として残す。
4: G14 の判定は [g14-completeness-proof-2026-10-16.md](g14-completeness-proof-2026-10-16.md)(本文と §7〜§9・付録 B〜D)、方針は
5: [t12-daa14000-release-plan-2026-10-15.md](t12-daa14000-release-plan-2026-10-15.md) の最終 3 節を参照。
6: 
7: ## 0. 現状の判定
8: 
9: - **G14 の主張(Panel 全員が共謀しても外部の bond 1 つで客観的に有罪)は、まだ証明されていない。** fold 水準と一部 node 水準の試験は通ったが、未解決の gap と未実施の drill が残る。
10: - **fence は DAA 14,000 から延期した。** 16,000 は目安で、確定値ではない。G14 を全部武装したまま、PALW 報酬・ブロック生成・Final が正常に進むことを、出荷 binary の drill で確かめてから fence する。
11: - integ(`integ/rfc02-rfc06-12-g14`)は、統合ブランチの検証済み状態と、5 本の G14 lane の merge を含む。新しい規則は全て休眠(全 preset で `None`)で、t12 の identity は変わっていない。
12: 
13: ## 1. RFC07 の認証済み selected-approval adapter(今回実装)
14: 
15: - `PalwKernelRouteStateV1::finals_read_v1` は、`PaidClaimV2` Final のうち、`palw_exec_v2_selected_approval_v1` が認めたものに限って beacon の `AttributedWorkV1` を出す。
16:   - 認める条件:canonical paid Final が、割り当てられた正の quorum(抽選された checker)・bound source・完了 round で native 受理・restore 検査を通っていること。EXEC credit と同じ gate である。
17:   - optimistic の timeout、Panel-licensed、未承認の paid claim は、従来どおり `None`。
18: - 残り:
19:   - 旧 OPV-only の fixture に依存する conformance / OPV bootstrap の試験、約 45 本と core の `rfc0010_production_fold::paid_panel_sources…`。抽選 checker の承認を持つ paid claim の fixture へ移す(迂回で通さない)。
20:   - Panel-licensed Final の event(route が Panel の seed・epoch を知らない)。
21:   - m・k の測定(Gate C と共通)。
22: 
23: ## 2. G14 の lane — 保存済みで未完了の作業(WIP branch、未 build・未試験)
24: 
25: | branch | head | 内容 | 残り |
26: |---|---|---|---|
27: | `integ/g14-tir-pricing` | `61ab3a9a8` | frame の課金・期限を測る harness の途中 | 32×32=1,024 step の課金が要るかの実測。debit ごとの inclusion margin と `reorg_replay_moves` の積み上げの見直し。実測にもとづく cost model と期限。T-N1(試験専用 config)。正直な IR FP claim が Final・支払いまで届く node 試験 |
28: | `integ/g14-court-capacity` | `5b64cac9c` | court 容量(`palw_court_capacity_v1.rs` / fold)、Panel 担保の試験の途中 | 処理予算の実測 harness、占有時間の課金、期限付きスケジューラ。42 枠飽和+継続的な埋め直し攻撃の試験。Panel の Active/Resolved 分離(`palw_final_lock_full_collateral`、R_new の式)。late conviction、資本効率の比較 |
29: | `integ/g14-drill` | `b8858af58` | 共謀 drill の driver の途中(`scripts/drills/g14_collusion_drill.py`、`g14_drill_factory.rs`) | D-01〜D-18、court/Panel の 7 条件、正常系 P-1〜P-5、RFC-0019 R-7/R-8 の実装と実行 |
30: 
31: ## 3. G14 の未解決 gap(監査の ID)
32: 
33: - **G-07** court の 42 枠:連合が全枠を占有できる。fused leaf 経路は、飢餓耐性を実証するまで (b) で「G14 経路あり」と数えない(ユーザー方針)。
34: - **G-13 の効率**:Panel 担保の Active/Resolved 分離が未実装。Final+1,000 の無条件解放へは戻さない。
35: - **TIR frame の価格・期限**:t12 の現行価格では、賃料が 2e7 MSK 以上、告発を開けるのが 0.9e6 DAA 後、Final が 6e7 DAA 後で、実用にならない。実モデルの leaf 数では u64 を超える可能性もある(未測定)。
36: - **native**:G-N1(v11/v29 の告発枠)、G-N2(v39 mining の Final 後の告発。veto の扱いは RFC-0018 U-6 のユーザー判断)、G-K2(EXEC の試験 seam GAP-81)。
37: - **G-K3 / F17** 検証者の誘因:正直な検証者が費用を回収できることの証明。自作自演は finder 49% 上限で損になる。
38: - **G-11** rule E、**G-12** bond 式 v2、**G-04** Gate C の武装:いずれも測定値と判断が待ち。
39: - **X3** の候補値は 5,520 DAA。未測定の項(B_cold_fetch など)が残る。
40: - T-N1 の本番経路 harness:IR class の登録 → `Active`、TIR family の認証、layout 公開、`TirPublicGrantReadyV1`、外部 owner/filer の service。
41: 
42: ## 4. 一括検証(`integ/validation` `37cf3fa89`、記録は consolidated-validation-2026-10-15.md)
43: 
44: - green:ladder 20 段、t12 identity、G14 対象の 150 試験。
45: - red:kaspad service E2E 16 件、RFC-0018 sibling 3 件、rfc9_v4、rfc0016 の 1 件、rfc12 ×4、t12_a2u_mixed ×2、SDK ×3、tir-lower ×4、base0 ×3。一部は旧 base でも失敗していた。
46: - clippy は警告 557 件。lane が落ち着いてから 1 回でまとめて直す(`--exclude misaka-palw-worker`)。
47: - 最新の integ head で、全 suite を再実行していない。
```

<a id="s36"></a>

## S36 — Constrained decode courtとrenderは現コードにある

### `consensus/core/src/palw_step_refute.rs:5955–5988`

```text
5955: /// 2. the committed token not admitted from the state convicts, unless it is the lowest end-of-generation id AND nothing is
5956: ///    admitted — which the challenger refutes by naming an admitted lane (`witness`, opened): then it convicts;
5957: /// 3. the committed token admitted: the two-disclosure arm over two admitted lanes, as the unconstrained v3 arm does, with the
5958: ///    beating lane's rendering opened.
5959: pub fn check_constrained_decode_token_v1(
5960:     binding: &PalwStepBindingV2,
5961:     accusation: &crate::palw_fp_constraint_job_v1::PalwConstrainedDecodeAccusationV1,
5962:     max_step_leaf_count: u64,
5963: ) -> Result<crate::palw_step_leg::PalwStepRefutationVerdictV1, PalwStepRefuteError> {
5964:     check_constrained_decode_token_at_v2(binding, accusation, max_step_leaf_count, false)
5965: }
5966: 
5967: /// Public-filer block-context rule: no committed token may follow an authenticated EOG.
5968: /// The height-free checker retains its prior stop reading.
5969: pub fn check_constrained_decode_token_at_v2(
5970:     binding: &PalwStepBindingV2,
5971:     accusation: &crate::palw_fp_constraint_job_v1::PalwConstrainedDecodeAccusationV1,
5972:     max_step_leaf_count: u64,
5973:     public_filer_active: bool,
5974: ) -> Result<crate::palw_step_leg::PalwStepRefutationVerdictV1, PalwStepRefuteError> {
5975:     let pin = &accusation.pin;
5976:     let (ctx_hash, vocab, tiles) = tiled_row_authenticate_v1(
5977:         binding,
5978:         &pin.generated_token_ids,
5979:         pin.position,
5980:         &pin.row_root,
5981:         &pin.row_opening,
5982:         max_step_leaf_count,
5983:     )?;
5984:     let beat = (!pin.committed_tile_lanes.is_empty() || !pin.beat_tile_lanes.is_empty()).then_some(pin.beat_lane);
5985:     constrained_decode_decision_v1(
5986:         binding,
5987:         &accusation.job,
5988:         &pin.generated_token_ids,
```

### `consensus/core/src/palw_state_v2/transition_builder.rs:5782–5800`

```text
5782:             | PalwPanelContradictionV1::StepArithmetic { .. }
5783:             | PalwPanelContradictionV1::StepStructural(_)
5784:             | PalwPanelContradictionV1::Legs(_)
5785:             | PalwPanelContradictionV1::ForgedOutput { .. } => {}
5786:             // V1 parity: refused right after decode by the caller; here too, as it was refused.
5787:             PalwPanelContradictionV1::IdentityMismatch { .. }
5788:             | PalwPanelContradictionV1::OutputMismatch { .. }
5789:             | PalwPanelContradictionV1::ForgedOutputTiled { .. }
5790:             | PalwPanelContradictionV1::LogitsNotStepOutput { .. }
5791:             | PalwPanelContradictionV1::PromptNotAnchored { .. }
5792:             | PalwPanelContradictionV1::ConstrainedDecode { .. }
5793:             | PalwPanelContradictionV1::ConstrainedDecodeFlat { .. }
5794:             | PalwPanelContradictionV1::RegisteredTableMismatch { .. }
5795:             | PalwPanelContradictionV1::PrefixStateMismatch { .. }
5796:             | PalwPanelContradictionV1::TokenizerTableMismatch { .. }
5797:             | PalwPanelContradictionV1::StagedCourt { .. } => {
5798:                 return Err(refused("PanelFalseValid evidence does not decode".into()));
5799:             }
5800:         }
```

### `misaka/palw/base0/src/fp_worker.rs:1248–1275`

```text
1248: /// comparison is an identity rather than two decoders that happen to agree.
1249: pub fn render_answer_v1(tokenizer: &QwenTokenizer, ids: &[u32]) -> Vec<u8> {
1250:     ids.iter().filter_map(|id| tokenizer.token_bytes(*id)).flatten().collect()
1251: }
1252: 
1253: /// **`render_answer_v2` (ADR-0096 Decision 6): the answer cut at the first committed end-of-generation id** — the ids up to and
1254: /// excluding the first id in `eog_token_ids`, rendered as `render_answer_v1` renders them. The ids after it stay executed,
1255: /// committed and priced, but are not the answer. Applies to a constrained job (version 6); every other job keeps `render_answer_v1`.
1256: pub fn render_answer_v2(tokenizer: &QwenTokenizer, ids: &[u32], eog_token_ids: &[u32]) -> Vec<u8> {
1257:     let end = ids.iter().position(|id| eog_token_ids.contains(id)).unwrap_or(ids.len());
1258:     render_answer_v1(tokenizer, &ids[..end])
1259: }
1260: 
1261: /// The run itself, reporting each id as it is selected with THAT id's bytes.
1262: ///
1263: /// The frame contract's own sentence — "`rendered` is this id's rendering alone" — and the reason
1264: /// it is right: the concatenation of the pieces is then the `rendered` field of the result by
1265: /// construction, whatever the tokenizer's table can and cannot spell. A multi-byte character
1266: /// straddling two tokens arrives as two pieces neither of which is valid UTF-8 on its own; that is
1267: /// the truth about the answer, and holding a partial sequence for display is the DISPLAY's
1268: /// business — the gateway's, which buffers before it writes an SSE event. A worker that held bytes
1269: /// back instead would be a worker whose stream and whose result are not the same bytes.
1270: fn execute_streaming_v1<B: PalwExecutionBackendV1>(
1271:     rt: &FpWorkerRuntime<B>,
1272:     job: &PalwFreePromptJobV3,
1273:     prompt_tokens: &[usize],
1274:     on_token: &mut dyn FnMut(u32, &[u8]),
1275: ) -> Result<(PalwFpRunV1, Vec<u32>), String> {
```
