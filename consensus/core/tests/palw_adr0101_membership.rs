//! ADR-0101 — a membership is proven by the chain and served by anyone, and a Position never moves
//! between holders. The invariants that are consensus-core's to hold.

/// The function a line of `lines` sits in: the nearest `fn` above it.
fn enclosing_fn(lines: &[&str], at: usize) -> String {
    lines[..at]
        .iter()
        .rev()
        .find_map(|l| {
            let t = l.trim_start();
            let rest = t.strip_prefix("pub fn ").or_else(|| t.strip_prefix("fn ")).or_else(|| t.strip_prefix("pub(crate) fn "))?;
            Some(rest.split(['(', '<']).next().unwrap_or("").to_string())
        })
        .unwrap_or_default()
}

/// **Every place `source` writes the `model_positions` map without going through a writer
/// function**, by the function it sits in, in source order: a mutating method on the map (whatever
/// the line breaks between the receiver and the call), an assignment to the field, a `&mut` borrow
/// of it, or the map handed whole to a macro. Comments and reads are not writes, and neither is a
/// name that merely starts with `model_positions` (`model_positions_of`, …).
fn direct_position_writers(source: &str) -> Vec<String> {
    const MAP: &str = "model_positions";
    const MUTATORS: &[&str] = &[
        "insert",
        "remove",
        "remove_entry",
        "entry",
        "get_mut",
        "first_entry",
        "last_entry",
        "retain",
        "clear",
        "append",
        "extend",
        "iter_mut",
        "values_mut",
        "range_mut",
        "pop_first",
        "pop_last",
        "split_off",
        "extract_if",
    ];
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let lines: Vec<&str> = source.lines().collect();
    let mut writers = Vec::new();
    for (at, _) in source.match_indices(MAP) {
        let (before, after) = (&source[..at], &source[at + MAP.len()..]);
        if before.ends_with(is_ident) || after.starts_with(is_ident) {
            continue;
        }
        let line = before.matches('\n').count();
        if lines[line].trim_start().starts_with("//") {
            continue;
        }
        let rest = after.trim_start();
        let method = rest.strip_prefix('.').map(|r| r.trim_start().split(|c: char| !is_ident(c)).next().unwrap_or(""));
        // `head` is everything before the place expression and `path` the expression up to the map:
        // for `&mut self.state.model_positions`, `…&` and `mut self.state.`.
        let head = before.trim_end_matches(|c: char| is_ident(c) || c == '.' || c.is_whitespace());
        let path = &before[head.len()..];
        let writes = method.is_some_and(|m| MUTATORS.contains(&m))
            || (before.trim_end().ends_with('.') && rest.starts_with('=') && !rest.starts_with("=="))
            || (head.ends_with('&') && path.split_whitespace().next() == Some("mut"))
            || (head.ends_with("!(") && (rest.starts_with(',') || rest.starts_with(')')));
        if writes {
            writers.push(enclosing_fn(&lines, line));
        }
    }
    writers
}

/// **Decision 5 — a Position moves only between a holder and the curve** (settled by the operator
/// 2026-09-10: a Position has no payment or settlement role, so it has no transfer). The fold has
/// exactly two writers of a holding: the buy credits its buyer from the curve, the sell debits its
/// seller to the curve. The reward's buyback (ADR-0091) retires units into the market row and
/// credits no holder; a line's owner transfer (ADR-0088) moves no Position. A third writer — a
/// transfer, a payment, a "pay with Positions" — fails this test, and the answer to that failure
/// is an ADR that supersedes ADR-0101 Decision 5 by name, not an edit here.
#[test]
fn a_position_moves_only_between_a_holder_and_the_curve() {
    let source = include_str!("../src/palw_state_v2.rs");
    let lines: Vec<&str> = source.lines().collect();
    let writers: Vec<String> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            l.contains("write_model_position(") && !l.trim_start().starts_with("fn ") && !l.trim_start().starts_with("//")
        })
        .map(|(i, _)| enclosing_fn(&lines, i))
        .collect();
    assert_eq!(
        writers,
        vec!["model_buy_v1".to_string(), "model_sell_v1".to_string()],
        "a holding is written by the buy and the sell and by nothing else"
    );
    // **And nothing reaches around that writer** (the 2026-09-23 Position route matrix, P22). The
    // count above sees only callers of `write_model_position(`; a line in the fold that writes the
    // map itself — `.model_positions.insert(`, `.remove(`, `.entry(`, a `&mut` of it — is a third
    // writer that count never sees, and the matrix's proof that no transfer exists rested on the
    // count alone. So every direct write of the map in the fold is named by its function: the
    // builder's writer, which records each change as a delta entry, and the delta's replay
    // (`apply_delta_entry`, for apply and for revert), which installs what an entry says. The replay
    // is only as narrow as the entries it is given, so the one place a `ModelPosition` entry is
    // BUILT (a match arm's pattern is a read) is pinned to the writer too.
    let fold = &source[..source.find("\n#[cfg(test)]\npub(crate) mod tests {").expect("the tests follow the fold")];
    let direct: std::collections::BTreeSet<String> = direct_position_writers(fold).into_iter().collect();
    assert_eq!(
        direct,
        ["apply_delta_entry", "write_model_position"].map(String::from).into(),
        "the position map is written by the builder's writer and the delta's replay and by nothing else"
    );
    let entries_built: std::collections::BTreeSet<String> = fold
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
        .filter(|(_, l)| l.match_indices("::ModelPosition {").any(|(at, _)| !l[at..].contains("=>")))
        .map(|(i, _)| enclosing_fn(&lines, i))
        .collect();
    assert_eq!(entries_built, ["write_model_position"].map(String::from).into(), "a position delta entry is the writer's alone");
    // The object enum names no Position transfer; the one "Transferred" object moves a LINE's
    // ownership, and its arm writes no holding (the count above).
    let transfer_objects: Vec<&str> = lines
        .iter()
        .filter_map(|l| {
            let t = l.trim_start();
            let name = t.strip_suffix(" {")?;
            (l.starts_with("    ")
                && !l.starts_with("     ")
                && name.contains("Transfer")
                && name.chars().all(|c| c.is_ascii_alphanumeric()))
            .then_some(name)
        })
        .collect();
    assert_eq!(transfer_objects, vec!["ModelLineOwnerTransferred"], "no object moves a Position from one holder to another");
}

/// **The pin above sees the writers the old grep could not** (the 2026-09-23 Position route
/// matrix, P22). Each planted function writes a holding without calling `write_model_position(`,
/// so the count of that call finds none of them; the scan finds every one, in order, and passes
/// over the reads the fold really makes — a comment, a lookup, the root's own borrow, a struct
/// literal's field, the carriage's local of the same name, a longer name, an assertion.
#[test]
fn the_position_pin_sees_a_writer_that_reaches_around_the_builder() {
    let planted = r#"
    fn model_transfer_v1(&mut self, line: Hash64, from: Hash64, to: Hash64, units: u64) {
        self.state.model_positions.insert((line, to), units);
        self.state
            .model_positions
            .remove(&(line, from));
    }
    fn model_gift_v1(&mut self, key: (Hash64, Hash64)) {
        *self.state.model_positions.entry(key).or_insert(0) += 1;
    }
    fn model_borrow_v1(state: &mut PalwChainStateV2) {
        let held = &mut state.model_positions;
    }
    fn model_reset_v1(&mut self) {
        self.state.model_positions = BTreeMap::new();
    }
    fn model_replay_v1(state: &mut PalwChainStateV2) {
        swap_write!(state.model_positions, key, old, new);
    }
    fn model_reads_v1(&self, other: &Self) {
        // self.model_positions.insert(key, units) in a comment writes nothing
        let a = self.model_positions.get(&key);
        let b = collection_root(b"model_positions", &self.model_positions);
        let c = Carriage { model_positions: self.model_positions.clone() };
        model_positions = BTreeMap::deserialize_reader(reader)?;
        let d = self.model_positions_of(&holder);
        let e = self.model_positions == other.model_positions;
        assert!(self.model_positions.is_empty());
    }
"#;
    assert!(!planted.contains("write_model_position("), "the count of the writer's call sees none of these");
    assert_eq!(
        direct_position_writers(planted),
        ["model_transfer_v1", "model_transfer_v1", "model_gift_v1", "model_borrow_v1", "model_reset_v1", "model_replay_v1"]
            .map(String::from)
            .to_vec(),
    );
}

/// **P11's "no fence needed" stays true: the tier sums have no consensus reader** (the 2026-09-23
/// Position route matrix, P11). `model_position_across` was repaired to count each holder id once
/// without a fence, on the premise that nothing in the fold, the processor or the EVM window reads
/// it or the two tier functions built on it. This pins the premise: outside the three functions
/// themselves (and the RPC read surface in `api/`), no production line of consensus-core, of the
/// consensus pipeline and processes, or of kaspa-evm calls any of them. A new consensus reader must
/// arrive behind its own fence, and it fails here first.
#[test]
fn the_tier_sums_have_no_caller_in_the_fold_the_processor_or_the_evm() {
    use std::path::{Path, PathBuf};
    const NAMES: &[&str] = &["model_position_across", "model_benefit_tier_across", "model_benefit_tier"];
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    // The production half of a source file: everything above its test module.
    fn production(source: &str) -> &str {
        let cut =
            ["pub(crate) mod tests {", "#[cfg(test)]\nmod tests", "#[cfg(test)]\npub(crate) mod tests", "#[cfg(test)]\npub mod tests"]
                .iter()
                .filter_map(|marker| source.find(marker))
                .min()
                .unwrap_or(source.len());
        &source[..cut]
    }
    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|x| x == "rs") {
                out.push(path);
            }
        }
    }
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&core.join("src"), &mut files);
    rust_files(&core.join("../src/pipeline"), &mut files);
    rust_files(&core.join("../src/processes"), &mut files);
    rust_files(&core.join("../../kaspa-evm/src"), &mut files);
    assert!(files.iter().any(|f| f.ends_with("palw_state_v2.rs")) && files.iter().any(|f| f.ends_with("processor.rs")));
    let mut callers = Vec::new();
    for file in &files {
        // The RPC read surface is the one place a tier is meant to be read (P-B2's C1 provider).
        if file.components().any(|c| c.as_os_str() == "api") && file.starts_with(core.join("src")) {
            continue;
        }
        let text = std::fs::read_to_string(file).unwrap();
        let source = production(&text);
        let lines: Vec<&str> = source.lines().collect();
        for name in NAMES {
            for (at, _) in source.match_indices(name) {
                let (before, after) = (&source[..at], &source[at + name.len()..]);
                if before.ends_with(is_ident) || !after.starts_with('(') {
                    continue; // a longer name, or not a call
                }
                let line = before.matches('\n').count();
                let text_line = lines[line].trim_start();
                if text_line.starts_with("//") || before.trim_end().ends_with("fn") {
                    continue; // a comment, or the definition itself
                }
                let within = enclosing_fn(&lines, line);
                if NAMES.contains(&within.as_str()) && file.ends_with("palw_state_v2.rs") {
                    continue; // the three call each other
                }
                callers.push(format!("{}:{} in {within}", file.display(), line + 1));
            }
        }
    }
    assert!(callers.is_empty(), "a consensus path reads the P11 tier sums, so the dedup repair would need a fence: {callers:#?}");
}

/// **Decision 3 — the check a client runs holds for a descriptor built by the SDK's transport
/// form**: JSON in, the same id out, and the check's verdict turns on the declared grants, the
/// chain's roots and the line's keys — the unit tests in `palw_service_descriptor_v1` hold the
/// refusals one by one; this holds that the pieces compose over the transport.
#[test]
fn a_descriptor_survives_its_transport_and_keeps_its_id() {
    use kaspa_consensus_core::palw_model_benefits_v1::grant;
    use kaspa_consensus_core::palw_service_descriptor_v1::{
        PalwLineServiceFactsV1, PalwServiceDescriptorV1, PalwServiceProviderKindV1, palw_service_descriptor_check_v1,
        palw_service_descriptor_id_v1,
    };
    use kaspa_hashes::Hash64;
    let domain = Hash64::from_u64_word(0xD0);
    let d = PalwServiceDescriptorV1 {
        version: 1,
        line_id: Hash64::from_u64_word(7),
        grants: grant::PRIORITY_INFERENCE,
        roots: vec![Hash64::from_u64_word(100)],
        endpoints: vec!["https://provider.example/v1".into()],
        valid_from_daa: 1,
        expires_daa: 10,
        provider_pubkey: b"k".to_vec(),
        signature: b"sig".to_vec(),
    };
    let json = serde_json::to_string_pretty(&d).unwrap();
    let back: PalwServiceDescriptorV1 = serde_json::from_str(&json).unwrap();
    assert_eq!(palw_service_descriptor_id_v1(domain, &back), palw_service_descriptor_id_v1(domain, &d));
    let facts = PalwLineServiceFactsV1 {
        line_id: d.line_id,
        declared_grants: grant::PRIORITY_INFERENCE,
        roots: vec![Hash64::from_u64_word(100)],
        origin_pubkeys: vec![],
        now_daa: 5,
    };
    let expect_id = palw_service_descriptor_id_v1(domain, &d);
    let verdict = palw_service_descriptor_check_v1(domain, &back, &facts, |pk, msg, sig, _| {
        pk == b"k" && sig == b"sig" && msg == expect_id.as_byte_slice()
    });
    assert_eq!(verdict, Ok(PalwServiceProviderKindV1::Open), "a stranger serving by root, over the transport");
}
