//! **The human report of a preflight** (RFC-0002 Part II §II.2.2): the mode line first, then the verdict table of
//! the three stages, the blockers with their numbers and the safe paths, then what each part of the verdict rests
//! on. The same data is the JSON form (`Report::to_json`).

use super::{Condition, Report, StageStatus, StageVerdict};
use std::fmt::Write;

/// `12,345,678`.
pub fn n(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `17.6 GiB`, `61 KiB`, `812 B`.
pub fn size(b: u64) -> String {
    const U: [(&str, u64); 4] = [("GiB", 1 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10), ("B", 1)];
    for (name, unit) in U {
        if b >= unit {
            return if unit == 1 {
                format!("{b} B")
            } else {
                let v = b as f64 / unit as f64;
                if v >= 100.0 {
                    format!("{v:.0} {name}")
                } else if v >= 10.0 {
                    format!("{v:.1} {name}")
                } else {
                    format!("{v:.2} {name}")
                }
            };
        }
    }
    "0 B".into()
}

fn status_word(v: &StageVerdict) -> String {
    match v.status {
        StageStatus::Ok => "ok".into(),
        StageStatus::Blocked => format!("BLOCKED ({})", v.blockers.len()),
        StageStatus::Unknown => format!("unknown ({})", v.unknown_because.as_deref().unwrap_or("depth too shallow")),
    }
}

fn cond_line(c: &Condition) -> String {
    let status = match c.ok {
        Some(true) => "ok",
        Some(false) => "OVER",
        None => "-",
    };
    let nums = match (c.needed, c.limit) {
        (Some(a), Some(b)) => format!("{} / {} {}", n(a), n(b), c.unit),
        (Some(a), None) => format!("{} {}", n(a), c.unit),
        (None, Some(b)) => format!("limit {} {}", n(b), c.unit),
        (None, None) => String::new(),
    };
    format!("  {status:<5} {:<22} {nums:<38} {}", c.id, c.what)
}

impl Report {
    /// The mode line.
    pub fn mode_line(&self) -> String {
        let net = match &self.network {
            Some(x) => format!(" · {} at DAA {}", x.id, n(x.daa)),
            None => String::new(),
        };
        let total = self.source.weight_bytes.map(|t| format!(" of {}", size(t))).unwrap_or_default();
        format!(
            "preflight: {} · {} · depth {} (requested {}){} · {}{} read",
            self.mode,
            self.input.kind.label(),
            self.depth.reached.name(),
            self.depth.requested.name(),
            net,
            size(self.input.bytes_read),
            total
        )
    }

    pub fn render(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(o, "{}", self.mode_line());
        if let Some(s) = &self.depth.stopped_at {
            let _ = writeln!(o, "  depth_reached: {}, stopped_at: \"{s}\"", self.depth.reached.name());
        }
        let _ = writeln!(o, "  input: {}", self.input.label);
        let _ = writeln!(o);
        let _ = writeln!(o, "verdict");
        let _ = writeln!(o, "  convert    {}", status_word(&self.verdict.convert));
        let _ = writeln!(o, "  register   {}", status_word(&self.verdict.register));
        let _ = writeln!(o, "  mine       {}", status_word(&self.verdict.mine));
        if let Some(w) = self.network.as_ref().and_then(|x| x.what_if.as_ref()) {
            let _ = writeln!(o, "  note: {w}");
        }

        let blockers = self.blockers();
        if !blockers.is_empty() {
            let _ = writeln!(o);
            let _ = writeln!(o, "blockers");
            for b in blockers {
                let arg = b.arg.as_ref().map(|a| format!("({a})")).unwrap_or_default();
                let _ = writeln!(o, "  [{}] {}{}", b.stage.name(), b.code, arg);
                let _ = writeln!(o, "      {}", b.what);
                if let (Some(have), Some(need)) = (b.have, b.need) {
                    let unit = b.unit.as_deref().unwrap_or("");
                    let _ = writeln!(o, "      have {} · need {} {}", n(have), n(need), unit);
                }
                for e in &b.evidence {
                    let _ = writeln!(o, "      evidence: {e}");
                }
                for p in &b.safe_paths {
                    let _ = writeln!(o, "      instead: {p}");
                }
            }
        }

        if let Some(m) = &self.model {
            let _ = writeln!(o);
            let _ = writeln!(o, "model");
            let _ = writeln!(
                o,
                "  model_type      {}   (informational only: it selects no code)",
                m.model_type.as_deref().unwrap_or("(none)")
            );
            if !m.architectures.is_empty() {
                let _ = writeln!(o, "  architectures   {}", m.architectures.join(", "));
            }
            let _ = writeln!(o, "  adapter         {}", m.adapter.describe());
            let _ = writeln!(o, "  level           {}", m.level);
            if let Some(d) = &m.spec_digest {
                let _ = writeln!(o, "  spec digest     {}", &d[..32.min(d.len())]);
            }
            if !m.features.is_empty() {
                let missing = m.features.iter().filter(|f| f.status == misaka_palw_tir_lower::model::FeatureStatus::Missing).count();
                let _ = writeln!(o, "  features        {} ({} missing)", m.features.len(), missing);
                let w = m.features.iter().map(|f| f.id.len()).max().unwrap_or(0);
                for f in &m.features {
                    let st = match (&f.status, &f.capability) {
                        (misaka_palw_tir_lower::model::FeatureStatus::Supported, _) => "SUPPORTED".to_string(),
                        (_, Some(c)) => format!("MISSING ({c})"),
                        (_, None) => "MISSING".to_string(),
                    };
                    let _ = writeln!(o, "    {:<w$}  {st}", f.id, w = w);
                }
            }
            if !m.assumed_defaults.is_empty() {
                let _ = writeln!(o, "  assumed class defaults (confirm against the class): {}", m.assumed_defaults.join(", "));
            }
        }
        if let Some(s) = &self.scope {
            let _ = writeln!(o);
            let _ = writeln!(o, "scope           {}", s.headline());
            for e in &s.excluded {
                let sz = match (e.tensors, e.bytes) {
                    (0, _) => String::new(),
                    (t, Some(b)) => format!(" — {t} tensors, {}", size(b)),
                    (t, None) => format!(" — {t} tensors"),
                };
                let _ = writeln!(o, "    left out: {} ({}){sz}", e.what, e.kind);
            }
        }

        let _ = writeln!(o);
        let _ = writeln!(o, "storage");
        if let Some(q) = &self.storage.quant_method {
            let d = self
                .storage
                .quant_descriptor
                .as_ref()
                .map(|d| format!(" -> descriptor {} ({})", d.name, &d.digest[..12.min(d.digest.len())]))
                .unwrap_or_default();
            let _ = writeln!(o, "  quantization_config {q}{d}");
        }
        for r in &self.storage.rows {
            let d = r.descriptor.as_ref().map(|d| d.name.clone()).unwrap_or_else(|| "-".into());
            let _ =
                writeln!(o, "  {:<14} {:>8} tensors {:>12}  {:<18} {}", r.storage, n(r.tensors as u64), size(r.bytes), r.status, d);
            if let Some(note) = &r.note {
                let _ = writeln!(o, "      {note}");
            }
        }
        let t = &self.tensors;
        let _ = writeln!(o);
        let _ = writeln!(
            o,
            "tensors         checked by {}: {} bound, {} missing, {} wrong shape, {} not checkable, {} unread ({})",
            t.checked,
            n(t.bound as u64),
            n(t.missing_total as u64),
            n(t.shape_mismatch_total as u64),
            n(t.unverified_total as u64),
            n(t.unused_total as u64),
            size(t.unused_bytes)
        );
        if let Some(a) = &self.artifact {
            let _ = writeln!(o);
            let _ = writeln!(
                o,
                "artifact        about {} (parameters {}, program {}, tokenizer {}); {} inventory leaves",
                size(a.estimate_bytes),
                size(a.params_bytes),
                size(a.program_bytes),
                size(a.tokenizer_bytes),
                n(a.inventory_leaves_estimate)
            );
            match (a.download_bytes_needed, a.download_bytes_total) {
                (Some(need), Some(total)) => {
                    let _ = writeln!(o, "  download      {} of the checkpoint's {} are read by the class", size(need), size(total));
                }
                (None, Some(total)) => {
                    let _ = writeln!(
                        o,
                        "  download      the checkpoint is {} (the shards' headers are not all here to say which tensors the class reads)",
                        size(total)
                    );
                }
                _ => {}
            }
            if a.left_out_bytes > 0 {
                let _ = writeln!(
                    o,
                    "  left out      {} of tensors the scope does not compute need not be fetched",
                    size(a.left_out_bytes)
                );
            }
        }

        if let Some(r) = &self.residency {
            let _ = writeln!(o);
            let _ = writeln!(
                o,
                "residency       floor {} (pinned {} + one token's routed rows {} + one admission in flight {}); the default \
                 budget, a fifth of the weights, is {}{}",
                size(r.floor_bytes),
                size(r.pinned_bytes),
                size(r.routed_token_bytes),
                size(r.in_flight_bytes),
                size(r.default_budget_bytes),
                if r.default_holds_floor { " and holds it" } else { ": SHORT of the floor" }
            );
            if !r.default_holds_floor {
                // The exact number to state, and the flag that states it: a short default is the page cache, and
                // the operator's way out is one number, not a guess.
                let _ = writeln!(
                    o,
                    "  to hold it    state --palw-class-resident-bytes {} (its floor, {}) or more; by default a node \
                     holds it through the page cache",
                    r.floor_bytes,
                    size(r.floor_bytes)
                );
            }
            let _ = writeln!(
                o,
                "  weights       {} on disk: {} pinned, {} routed ({} a token), {} gathered ({} a token, read a row at a time)",
                size(r.weight_bytes),
                size(r.pinned_bytes),
                size(r.routed_bytes),
                size(r.routed_token_bytes),
                size(r.gathered_bytes),
                size(r.gathered_token_bytes)
            );
            let job = match r.replay.job {
                Some((p, d)) => format!("the canonical job ({p} prefill, {d} decode: {} forward passes)", n(r.replay.forwards)),
                None => "one forward pass (no context was chosen)".to_string(),
            };
            let secs = |s: f64| {
                if s >= 10.0 {
                    format!("{s:.0} s")
                } else if s >= 1.0 {
                    format!("{s:.1} s")
                } else {
                    format!("{s:.3} s")
                }
            };
            let times: Vec<String> = r.replay.seconds_at.iter().map(|(mb, s)| format!("{} at {mb} MB/s", secs(*s))).collect();
            let _ = writeln!(
                o,
                "  one replay    {job} reads about {}: {} of routed rows (their expected union) and {} of gathered rows — \
                 about {} (estimates)",
                size(r.replay.bytes),
                size(r.replay.routed_union_bytes),
                size(r.replay.gathered_bytes),
                times.join(", ")
            );
            for x in &r.rows {
                let _ = writeln!(
                    o,
                    "  {:<13} {} × {}: {} rows of {}, {} a forward",
                    x.tier,
                    x.name,
                    x.instances,
                    n(u64::from(x.rows)),
                    size(x.row_bytes),
                    n(x.rows_per_forward)
                );
            }
            let _ = writeln!(o, "  {}", r.note);
        }

        if let Some(ad) = &self.admission {
            let _ = writeln!(o);
            let _ = writeln!(o, "admission       {}", ad.verdict);
            let _ = writeln!(o, "  ceilings      {}", ad.ceilings);
            let _ = writeln!(
                o,
                "  program       {} bytes, {} blocks, {} nodes, {} unrolled a position",
                n(ad.program_bytes as u64),
                ad.blocks,
                ad.nodes,
                n(ad.unrolled_nodes)
            );
            if let Some(l) = &ad.layout {
                let _ = writeln!(
                    o,
                    "  layout        context {} positions{}, checkpoint interval {}, history tile {}, {} commit tiles{}",
                    n(u64::from(l.max_context)),
                    if l.searched {
                        format!(" (the widest the gate admits; the program reads up to {})", n(u64::from(l.widest_context)))
                    } else {
                        String::new()
                    },
                    l.checkpoint_interval,
                    l.h_tile,
                    l.commit_tiles,
                    l.logits_tile.map(|t| format!(", logits tile {t}")).unwrap_or_default()
                );
            }
            let _ = writeln!(
                o,
                "  gate          admission v10: {}{}",
                ad.gate,
                ad.gate_detail.as_ref().map(|d| format!(" ({d})")).unwrap_or_default()
            );
            for x in &ad.not_asked {
                let _ = writeln!(o, "  not asked     {x}");
            }
        }
        if !self.chain.is_empty() {
            let _ = writeln!(o);
            let _ = writeln!(o, "chain conditions (needed / limit)");
            for c in &self.chain {
                let _ = writeln!(o, "{}", cond_line(c));
            }
        }
        if let Some(net) = &self.network {
            let _ = writeln!(o, "  height        DAA {}: {}", n(net.daa), net.daa_choice);
            for f in net.fences.iter().filter(|f| f.needed) {
                let _ = writeln!(
                    o,
                    "  fence         {} {}",
                    f.name,
                    match (f.activation, f.in_force) {
                        (Some(a), true) => format!("in force (from DAA {})", n(a)),
                        (Some(a), false) => format!("NOT in force (from DAA {})", n(a)),
                        (None, _) => "NOT in force (dormant on this network)".to_string(),
                    }
                );
            }
        }
        if let Some(s) = &self.seat {
            let _ = writeln!(o);
            let _ = writeln!(o, "seat            needs about {} to replay a claim ({})", size(s.needed_bytes), s.tiers_source);
            for t in &s.tiers {
                let _ = writeln!(
                    o,
                    "  {:<40} share {:>9}  {}{}",
                    t.name,
                    size(t.share_bytes),
                    if t.fits { "holds it" } else { "CANNOT hold it" },
                    t.fits_at_context.map(|c| format!(" (fits at a context of {})", n(u64::from(c)))).unwrap_or_default()
                );
            }
            let _ = writeln!(
                o,
                "  artifact {} · state {} · peak live {} · widest tile {}",
                size(s.artifact_bytes),
                size(s.state_bytes),
                size(s.peak_live_bytes),
                size(s.widest_tile_opened_bytes)
            );
        }
        if let Some(f) = &self.forecast {
            let _ = writeln!(o);
            let _ = writeln!(
                o,
                "forecast        {} ready seats, registration bond {} MSK, {} claim(s) in flight at most, {}/1000 claims a span admitted",
                f.required_ready_seats,
                n(f.registration_bond_sompi / 100_000_000),
                n(u64::from(f.max_inflight_claims)),
                n(f.admission_claims_per_span_milli)
            );
            for p in &f.path {
                let _ = writeln!(o, "  - {p}");
            }
            let _ = writeln!(o, "  {}", f.note);
            if let Some(i) = &f.independence {
                let share = i
                    .licensable_share_at_floor_permille
                    .map(|p| format!(" — {p} ‰ of its claims would be licensable at exactly the floor"))
                    .unwrap_or_default();
                let base = i.base_operators.map(|b| format!(" against {b} base operators on the network")).unwrap_or_default();
                let _ = writeln!(
                    o,
                    "  independence: {} ready operators and {} independent{base}{share} ({})",
                    i.seat_count,
                    i.independent_floor,
                    if i.fence_in_force { "palw_class_seating in force" } else { "palw_class_seating not in force" }
                );
            }
        }
        if let Some(full) = &self.full {
            let _ = writeln!(o);
            let _ = writeln!(o, "full            pack {}{}", full.pack, full.pack_digest.as_deref().map(|d| format!(" ({})", &d[..16.min(d.len())])).unwrap_or_default());
            for c in &full.checks {
                let _ = writeln!(o, "  {:<7} {:<18} {}", c.status, c.name, c.detail);
            }
            if let Some(root) = &full.artifact_root {
                let _ = writeln!(o, "  artifact root {}…", &root[..24.min(root.len())]);
            }
            for c in &full.on_chain {
                let _ = writeln!(o, "  on the chain: {c}");
            }
        }
        if let Some(node) = &self.node {
            let _ = writeln!(
                o,
                "node            {} at DAA {} · {} classes{}",
                node.network,
                node.tip_daa,
                node.classes,
                node.base_operators.map(|b| format!(" · {b} base operators")).unwrap_or_default()
            );
        }
        if !self.notes.is_empty() {
            let _ = writeln!(o);
            for x in &self.notes {
                let _ = writeln!(o, "note: {x}");
            }
        }
        let _ = writeln!(o);
        let r = &self.registries;
        let _ = writeln!(
            o,
            "registries      adapters {} · features {} · quant formats {} · {} · palw-class {}",
            &r.adapter_pack[..12.min(r.adapter_pack.len())],
            &r.feature_registry[..12.min(r.feature_registry.len())],
            &r.quant_registry[..12.min(r.quant_registry.len())],
            r.lowering,
            r.tool_version
        );
        o
    }
}
