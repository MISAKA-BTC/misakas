//! **RFC-0012 R12-AUTHORITY item 4: the removal inventory, enforced on the source** (port of `codex/rfc12-remove-dns-bft`, 2026-10-15).
//!
//! The Codex snapshot removed the DNS BFT gate, DNS finality and the validator service physically, which re-identifies every
//! preset and cannot replay testnet-12's history (its `dns_bft_gate` is armed at DAA 6,001 and the chain carries DNS anchors and
//! votes). RFC-0012 §5 instead retires the overlay at `palw_dns_retirement_v1` and keeps the readers that replay old history. What
//! the deletion bought — "nothing past the fence consults DNS for safety" — is therefore kept here as a checked inventory: every
//! read of a DNS authority input in the consensus crate must sit in a function this list names, and that function must either
//! test the retirement itself (`dns_retired_at` / `palw_dns_retired_at`), only delegate to such functions, or be reachable only
//! from one. A new reader fails this test until it is classified (and its retirement predicate written).
//!
//! What this checks is textual: a gate that is present but wrong is not caught here — `rfc12_zero_dns_matrix` drives the
//! behaviour. Source scan only; nothing about the live network is read.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Call/read tokens that consult DNS state or the DNS-derived gate. `dns_state_store.read()` is matched across a line break.
const TOKENS: &[&str] = &[
    "dns_state_store.read()",
    "dns_bft_gate_refusal(",
    "dns_stake_preferred_tip(",
    "dns_finality_fresh_for_bridge(",
    "dns_bft_confirmed_state(",
    "advance_dns_confirmation(",
    "dns_reorg_outcome(",
    "dns_reorg_outcome_checked(",
    "bridge_finality_is_fresh(",
    "dns_coinbase_settlement_at(",
    "dns_bft_precommit_duty(",
    "dns_bft_evaluate(",
];

#[derive(Clone, Copy, Debug)]
enum Why {
    /// The function tests the retirement before (or instead of) the DNS read.
    Gated,
    /// Every DNS token in the function is a call to one of these, each of which is `Gated`.
    Delegates(&'static [&'static str]),
    /// Every call site of the function lies in one of these, each of which is `Gated`.
    OnlyCalledFrom(&'static [&'static str]),
}

/// The inventory. Key: function name. Every entry names the RFC-0012 reason in a comment.
fn inventory() -> BTreeMap<&'static str, Why> {
    use Why::*;
    BTreeMap::from([
        // ADR-0109 bridge freshness: returns `true` past the retirement (no DNS Pause on the new rules).
        ("bridge_finality_is_fresh", Gated),
        // DNS confirmation recompute: returns before reading anything past the retirement (no new anchors).
        ("update_dns_state", Gated),
        // EVM heads: past the retirement the native PALW snapshot supplies latest/safe/finalized; the DNS-anchor `safe` is the
        // legacy arm below the fence only.
        ("update_evm_canonical_heads", Gated),
        // The sink search asks only the two gated entry points below.
        ("sink_search_algorithm", Delegates(&["dns_stake_preferred_tip", "dns_reorg_outcome_checked"])),
        // Rule E's search finish asks the reorg gate, which abstains from DNS past the retirement.
        ("palw_rule_e_finish_search_v1", Delegates(&["dns_reorg_outcome_checked"])),
        // ADR-0128 DNS-final anchor: called only from the gated recompute, which skips it past the retirement.
        ("dns_bft_confirmed_state", OnlyCalledFrom(&["update_dns_state"])),
        // ADR-0128 Decision 5 veto: abstains past the retirement at the incumbent sink (and its one caller skips it there).
        ("dns_bft_gate_refusal", Gated),
        // ADR-0128 Decision 6 duty read: `round_active: false` past the retirement.
        ("dns_bft_precommit_duty", Gated),
        // The deep-reorg gate: the BFT veto, stake fallback and Work/Stake dominance are all skipped past the retirement.
        // (Its capacity-checked form since RFC-0008's native selection capacity; the old name is the tests' unwrapping wrapper.)
        ("dns_reorg_outcome_checked", Gated),
        ("dns_reorg_outcome", Delegates(&["dns_reorg_outcome_checked"])),
        // Coinbase settlement: the frozen DNS anchor no longer releases coinbases early past the retirement.
        ("dns_coinbase_settlement_at", Gated),
        ("dns_coinbase_settlement", Delegates(&["dns_coinbase_settlement_at"])),
        ("validate_mempool_transaction_in_utxo_context", Delegates(&["dns_coinbase_settlement_at"])),
        // Stale-stake preferred tip: `None` past the retirement.
        ("dns_stake_preferred_tip", Gated),
        // Op 203: serves the abstaining gate facts past the retirement (lane R12).
        ("palw_fork_choice_openings_v2_impl", Gated),
        // The EVM template's Pause policy asks the gated freshness read.
        ("build_block_template_with_selector_provider", Delegates(&["bridge_finality_is_fresh"])),
        // ConsensusApi reads: `None` / empty past the retirement.
        ("get_dns_confirmation", Gated),
        ("get_attestation_quality_deficits", Gated),
        ("get_precommit_duty", Delegates(&["dns_bft_precommit_duty"])),
    ])
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Non-test sources of the virtual processor and the consensus API implementation.
fn sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).expect("readable source dir").map(|e| e.unwrap().path()).collect();
        entries.sort();
        for p in entries {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if p.is_dir() {
                if name != "tests" && name != "testdata" {
                    walk(&p, out);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" && name != "test_block_builder.rs" {
                out.push(p);
            }
        }
    }
    let root = crate_root();
    let mut files = Vec::new();
    walk(&root.join("src/pipeline/virtual_processor"), &mut files);
    files.push(root.join("src/consensus/mod.rs"));
    files
        .into_iter()
        .map(|p| (p.strip_prefix(&root).unwrap().display().to_string(), std::fs::read_to_string(&p).expect("readable source")))
        .collect()
}

/// The identifier after a `fn ` keyword on this line, if the line declares a function.
fn declared_fn(line: &str) -> Option<String> {
    let t = line.trim_start();
    if t.starts_with("//") {
        return None;
    }
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(i) = line[from..].find("fn ") {
        let at = from + i;
        let boundary = at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        if boundary {
            let ident: String = line[at + 3..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if !ident.is_empty() {
                return Some(ident);
            }
        }
        from = at + 3;
    }
    None
}

struct Hit {
    file: String,
    line: usize,
    func: String,
    token: &'static str,
    text: String,
}

/// Lines of a file up to its first `#[cfg(test)] mod`, with each line ending in `dns_state_store` joined to the next.
fn scanned_lines(text: &str) -> Vec<String> {
    let raw: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let t = raw[i].trim();
        if t == "#[cfg(test)]" && raw.get(i + 1).is_some_and(|n| n.trim_start().starts_with("mod ")) {
            break;
        }
        if t.ends_with("dns_state_store") && i + 1 < raw.len() {
            out.push(format!("{}{}", raw[i].trim_end(), raw[i + 1].trim()));
            out.push(String::new());
            i += 2;
            continue;
        }
        out.push(raw[i].to_string());
        i += 1;
    }
    out
}

fn hits_and_bodies() -> (Vec<Hit>, BTreeMap<String, String>) {
    let mut hits = Vec::new();
    let mut bodies: BTreeMap<String, String> = BTreeMap::new();
    for (file, text) in sources() {
        let mut current: Option<String> = None;
        for (n, line) in scanned_lines(&text).iter().enumerate() {
            if let Some(f) = declared_fn(line) {
                current = Some(f);
            }
            if let Some(f) = &current {
                let body = bodies.entry(f.clone()).or_default();
                body.push_str(line);
                body.push('\n');
            }
            if line.trim_start().starts_with("//") {
                continue;
            }
            for token in TOKENS {
                let name = token.trim_end_matches('(');
                if line.contains(token) && !line.contains(&format!("fn {name}")) {
                    hits.push(Hit {
                        file: file.clone(),
                        line: n + 1,
                        func: current.clone().unwrap_or_default(),
                        token: *token,
                        text: line.trim().to_string(),
                    });
                }
            }
        }
    }
    (hits, bodies)
}

fn is_gated(inventory: &BTreeMap<&'static str, Why>, bodies: &BTreeMap<String, String>, f: &str) -> bool {
    matches!(inventory.get(f), Some(Why::Gated)) && bodies.get(f).is_some_and(|b| b.contains("dns_retired_at("))
}

#[test]
fn rfc12_every_dns_authority_read_is_inventoried_and_gated_by_the_retirement() {
    let inventory = inventory();
    let (hits, bodies) = hits_and_bodies();
    assert!(!hits.is_empty(), "the scan found no DNS reader at all: the source layout moved, fix `sources()`");
    let mut problems = Vec::new();
    for h in &hits {
        let at = format!("{}:{} in `{}` ({}): {}", h.file, h.line, h.func, h.token, h.text);
        match inventory.get(h.func.as_str()) {
            None => problems.push(format!("not in the RFC-0012 inventory: {at}")),
            Some(Why::Gated) => {
                if !is_gated(&inventory, &bodies, &h.func) {
                    problems.push(format!("listed as gated but never asks `dns_retired_at`: {at}"));
                }
            }
            Some(Why::Delegates(callees)) => {
                let callee = h.token.trim_end_matches('(');
                if !callees.contains(&callee) {
                    problems.push(format!("reads a DNS input it does not delegate: {at}"));
                } else if !is_gated(&inventory, &bodies, callee) {
                    problems.push(format!("delegates to `{callee}`, which is not gated: {at}"));
                }
            }
            Some(Why::OnlyCalledFrom(callers)) => {
                for caller in *callers {
                    if !is_gated(&inventory, &bodies, caller) {
                        problems.push(format!("`{}` is reachable from `{caller}`, which is not gated: {at}", h.func));
                    }
                }
            }
        }
    }
    // Every `OnlyCalledFrom` function really is called only from its listed callers.
    for (f, why) in &inventory {
        if let Why::OnlyCalledFrom(callers) = why {
            for h in hits.iter().filter(|h| h.token.trim_end_matches('(') == *f) {
                if !callers.iter().any(|c| *c == h.func) {
                    problems.push(format!("`{f}` is called from `{}` at {}:{}, outside its inventory callers", h.func, h.file, h.line));
                }
            }
        }
    }
    assert!(problems.is_empty(), "RFC-0012 DNS reader inventory:\n{}", problems.join("\n"));
}

/// The one veto the Codex snapshot deleted: `dns_bft_gate_refusal` is asked only by the reorg gate, and only below the retirement
/// resolved at the incumbent sink (R12-AUTHORITY item 2).
#[test]
fn rfc12_the_dns_bft_veto_is_asked_only_below_the_retirement() {
    let (hits, _) = hits_and_bodies();
    let calls: Vec<&Hit> = hits.iter().filter(|h| h.token == "dns_bft_gate_refusal(").collect();
    assert_eq!(calls.len(), 1, "exactly one call site of the DNS BFT veto: {:?}", calls.iter().map(|h| &h.text).collect::<Vec<_>>());
    let call = calls[0];
    assert_eq!(call.func, "dns_reorg_outcome_checked", "the veto is asked by the reorg gate only");
    assert!(call.text.contains("!dns_retired"), "the call is skipped past the retirement: {}", call.text);
}
