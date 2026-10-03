//! **Why a class in the model registry is not mining at full rate, and at which stage** (RFC-0002 Part II §II.7.4).
//!
//! `getPalwModelRegistry` has always said *where* a class is (its lifecycle state) and, as a sentence, why. This module
//! says it as a structured fact: the **stage** that lacks something — `convert` (the artifact's graph is the problem),
//! `register` (the network has not admitted the class) or `mine` (the class is registered but not yet admitted to claims,
//! or not at full rate) — a stable **code**, what is missing with its numbers, and what lifts it.
//!
//! It is one pure function over a class's row and the registry's globals, so the node fills the response with it and a CLI
//! can derive the same reading from the registry of a node that serves none (`GetPalwModelRegistryResponse::blocking` is
//! empty on a version-1 node). It reads what the node already serves and changes nothing the chain decides.
//!
//! The lifecycle is `palw_model_registry_v1.rs`'s: `Registered` → (`Candidate` for a bought class) → `Prefetching` →
//! `Probation` → `ActiveLimited` → `Active`, and `Held`. The mapping is in RFC-0002 Part II §II.7.4's table.

use super::message::{GetPalwModelRegistryResponse, RpcPalwClassBlocking, RpcPalwModelLifecycle};

/// The artifact's graph (or what was committed about it) is the problem: fixed by converting again, not by waiting.
pub const STAGE_CONVERT: &str = "convert";
/// The network has not admitted the class.
pub const STAGE_REGISTER: &str = "register";
/// Registered, but not yet admitted to claims, or not at full rate.
pub const STAGE_MINE: &str = "mine";

/// The number after `field:` in a lifecycle state's debug text (`Probation { probes_passed: 3 }`), 0 if absent.
fn state_number(state: &str, field: &str) -> u32 {
    let Some(at) = state.find(field) else { return 0 };
    let rest = &state[at + field.len()..];
    let rest = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    rest.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)
}

/// The lifecycle state's bare name (`Probation` of `Probation { probes_passed: 3 }`).
fn state_name(state: &str) -> &str {
    state.split(|c: char| c.is_whitespace() || c == '{').next().unwrap_or("")
}

/// A strict majority of a panel: `seats / 2 + 1`, the admission jury's quorum.
fn quorum(seats: u16) -> u64 {
    u64::from(seats) / 2 + 1
}

fn make(
    class: &RpcPalwModelLifecycle,
    stage: &str,
    code: &str,
    what: String,
    count: Option<(u64, u64)>,
    next: &str,
) -> RpcPalwClassBlocking {
    RpcPalwClassBlocking {
        class_id: class.class_id.clone(),
        stage: stage.to_string(),
        code: code.to_string(),
        what,
        has_count: count.is_some(),
        have: count.map_or(0, |c| c.0),
        need: count.map_or(0, |c| c.1),
        next: next.to_string(),
    }
}

/// What blocks `class`, read from the registry `r` it is in; `None` where nothing does (the base class, a class with no row, an
/// `Active` class).
pub fn palw_registry_blocking(r: &GetPalwModelRegistryResponse, class: &RpcPalwModelLifecycle) -> Option<RpcPalwClassBlocking> {
    if class.is_base_class || !class.has_row {
        return None;
    }
    let state = class.state.as_str();
    let ready = u64::from(class.ready_seats_now);
    let required = u64::from(class.required_ready_seats);
    let seats = u64::from(r.seat_count);
    let seats_note = |have: u64, need: u64| {
        let mut what = format!(
            "{have} of {need} seats hold a fresh possession proof of the artifact with free collateral for {}× a seat's exposure",
            r.readiness_collateral_multiple
        );
        if u64::from(r.bonds_with_headroom) < need {
            what.push_str(&format!("; only {} bonds on the network have the headroom to be a seat", r.bonds_with_headroom));
        } else {
            what.push_str(&format!("; {} bonds on the network have the headroom to be a seat", r.bonds_with_headroom));
        }
        what
    };
    const RUN_A_SEAT: &str = "run seats that hold the artifact — a seat needs memory for the artifact plus the replay working set, and a possession proof \
                              fresher than the readiness age; `misaka model readiness <class>` lists each seat";
    // **RFC-0002 Part II §II.7.5 Proposal A (`palw_class_seating`): the independence floor, as the node's seating read serves it.**
    // Past the fence a class whose ready operators are too few independent of the registrant (and of the claim's executor) takes
    // no claim and does not enter `Probation`; the possession count (`READY_SEATS`, `PANEL_NOT_DRAWABLE`) is named first where it
    // is the one that fails.
    let name = state_name(state);
    if let Some(seating) = r.seating.iter().find(|x| x.class_id == class.class_id)
        && seating.independent_operators < seating.needed_independent
        && !matches!(name, "Registered" | "Candidate")
        && !(matches!(name, "Prefetching" | "Held") && ready < required)
    {
        return Some(make(
            class,
            STAGE_MINE,
            "INDEPENDENT_OPERATORS",
            format!(
                "{} of the {} operators that hold the class are independent of its registrant and of a claim's executor and serve the \
                 network's liveness floor; {} are needed before it admits a claim. At {} ‰ of its claims' outsiders it would be licensed \
                 ({} base operators)",
                seating.independent_operators,
                seating.ready_operators,
                seating.needed_independent,
                seating.licensable_share_permille,
                seating.base_operators
            ),
            Some((u64::from(seating.independent_operators), u64::from(seating.needed_independent))),
            "operators other than the registrant's must run seats for the class and keep their possession proofs fresh: adoption is how \
             a model gets claims, and a registrant's own keys do not count",
        ));
    }
    match name {
        "Registered" => {
            let (code, what) = if !class.ops_supported {
                (
                    "VM_BOUNDARY",
                    "the graph names an operation the canonical VM does not define at this runtime version: a VM upgrade, not a registration".to_string(),
                )
            } else if class.artifact_bytes == 0 {
                ("NO_ARTIFACT", "the manifest commits no artifact bytes: there is nothing to prefetch or verify".to_string())
            } else {
                ("NO_WORK", "no work is derived from the graph: nothing to verify, so the class never admits a claim".to_string())
            };
            Some(make(
                class,
                STAGE_CONVERT,
                code,
                what,
                None,
                "a registered class cannot be repaired in place: convert the model again with a lowering the VM expresses and register a new class",
            ))
        }
        "Candidate" => Some(make(
            class,
            STAGE_REGISTER,
            "ADMISSION_JURY",
            format!(
                "an admission jury of {seats} operators drawn from the network (never the registrant's) must hold the class ready, and {} of them decide; \
                 {ready} seats hold it ready now (the jury counts only its drawn operators)",
                quorum(r.seat_count)
            ),
            Some((ready, quorum(r.seat_count))),
            "run seats for the class and keep their possession proofs fresh: the jury sits at each admission audit, and a lottery that could be re-drawn \
             every span would be passed by waiting",
        )),
        "Prefetching" => {
            if ready >= required {
                Some(make(
                    class,
                    STAGE_MINE,
                    "PENDING_STEP",
                    format!("{ready} of {required} seats are ready now; the class enters Probation at the next span boundary"),
                    Some((ready, required)),
                    "wait one span",
                ))
            } else {
                Some(make(class, STAGE_MINE, "READY_SEATS", seats_note(ready, required), Some((ready, required)), RUN_A_SEAT))
            }
        }
        "Probation" => {
            let passed = u64::from(state_number(state, "probes_passed"));
            let need = u64::from(r.probation_claims);
            let mut what = format!(
                "{passed} of {need} probe claims have reached Final with none failing ({} failed since entry); the class admits a twentieth of its derived volume",
                class.probes_failed
            );
            if class.no_capable_panel_voids > 0 {
                what.push_str(&format!("; {} claims were voided for want of a capable panel", class.no_capable_panel_voids));
            }
            Some(make(
                class,
                STAGE_MINE,
                "PROBATION",
                what,
                Some((passed, need)),
                "produce claims for the class (`misaka mining setup --model <class>`) and keep its seats ready: a failed probe starts the count again",
            ))
        }
        "ActiveLimited" => {
            let stable = u64::from(state_number(state, "stable_epochs"));
            let need = u64::from(r.stable_epochs);
            if stable < need {
                Some(make(
                    class,
                    STAGE_MINE,
                    "STABLE_SPANS",
                    format!("stable for {stable} of {need} spans at a tenth of the derived volume"),
                    Some((stable, need)),
                    "keep the seats ready and the claims verifying: an unstable span starts the count again",
                ))
            } else {
                Some(make(
                    class,
                    STAGE_MINE,
                    "CAP_SATURATED",
                    format!(
                        "the class has been stable, but its cap utilization is {} ‰ of the escrow at the rate: it is not activatable above the ceiling",
                        class.cap_utilization_permille
                    ),
                    None,
                    "it moves when the rate, the target or the escrow does",
                ))
            }
        }
        "Held" => {
            if ready < seats {
                Some(make(
                    class,
                    STAGE_MINE,
                    "PANEL_NOT_DRAWABLE",
                    format!(
                        "only {ready} seats are ready now and a panel needs {seats}; the class admits no claim until its seats are back"
                    ),
                    Some((ready, seats)),
                    RUN_A_SEAT,
                ))
            } else if ready < required {
                Some(make(
                    class,
                    STAGE_MINE,
                    "READY_SEATS",
                    format!("{} — it re-enters Probation once {required} are ready", seats_note(ready, required)),
                    Some((ready, required)),
                    RUN_A_SEAT,
                ))
            } else if class.reason.contains("verification window") {
                Some(make(
                    class,
                    STAGE_MINE,
                    "WINDOW_DOES_NOT_FIT",
                    format!(
                        "the class's verification window ({} spans) does not fit the receipt deadline: replay would outrun every claim it admitted",
                        class.verification_window_spans
                    ),
                    None,
                    "a class-specific receipt deadline (RFC-0002 Part II §II.2.6) or a wider global window admits it; a smaller context shortens the window",
                ))
            } else {
                Some(make(
                    class,
                    STAGE_MINE,
                    "HELD_RECOVERING",
                    "seats and window are back; the class re-enters Probation at the next span boundary".to_string(),
                    None,
                    "wait one span",
                ))
            }
        }
        // `Active`, and `Legacy` for a class with no row (already returned above).
        "Active" | "Legacy" => None,
        other => Some(make(
            class,
            STAGE_MINE,
            "UNKNOWN_STATE",
            format!("the registry reports a lifecycle state this reader does not know: `{other}`"),
            None,
            "update the reader",
        )),
    }
}

/// The blocking reading of every class of the registry `r` that has one, in the registry's order.
pub fn palw_registry_blockings(r: &GetPalwModelRegistryResponse) -> Vec<RpcPalwClassBlocking> {
    r.classes.iter().filter_map(|c| palw_registry_blocking(r, c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> GetPalwModelRegistryResponse {
        GetPalwModelRegistryResponse {
            available: true,
            active: true,
            seat_count: 5,
            spare_seats: 2,
            probation_claims: 10,
            stable_epochs: 3,
            readiness_collateral_multiple: 3,
            bonds_with_headroom: 14,
            ..Default::default()
        }
    }

    fn class(state: &str) -> RpcPalwModelLifecycle {
        RpcPalwModelLifecycle {
            class_id: "cd".repeat(64),
            has_row: true,
            state: state.to_string(),
            ops_supported: true,
            artifact_bytes: 1 << 30,
            required_ready_seats: 7,
            ready_seats_now: 3,
            ..Default::default()
        }
    }

    fn block(state: &str, edit: impl FnOnce(&mut RpcPalwModelLifecycle)) -> Option<RpcPalwClassBlocking> {
        let mut c = class(state);
        edit(&mut c);
        palw_registry_blocking(&registry(), &c)
    }

    #[test]
    fn state_text_is_read_for_its_name_and_its_progress() {
        assert_eq!(state_name("Probation { probes_passed: 3 }"), "Probation");
        assert_eq!(state_name("Held"), "Held");
        assert_eq!(state_number("Probation { probes_passed: 3 }", "probes_passed"), 3);
        assert_eq!(state_number("ActiveLimited { stable_epochs: 12 }", "stable_epochs"), 12);
        assert_eq!(state_number("Active", "stable_epochs"), 0);
        assert_eq!(state_number("Probation { probes_passed: x }", "probes_passed"), 0);
    }

    #[test]
    fn a_class_that_nothing_blocks_has_no_reading() {
        assert!(block("Active", |_| {}).is_none());
        assert!(block("Legacy", |c| c.has_row = false).is_none());
        assert!(block("Prefetching", |c| c.is_base_class = true).is_none());
        assert!(palw_registry_blockings(&GetPalwModelRegistryResponse::default()).is_empty());
    }

    #[test]
    fn each_state_names_its_stage_and_what_is_missing() {
        // Registered: the graph is the problem — convert.
        let b = block("Registered", |c| c.ops_supported = false).unwrap();
        assert_eq!((b.stage.as_str(), b.code.as_str(), b.has_count), ("convert", "VM_BOUNDARY", false));
        assert_eq!(block("Registered", |c| c.artifact_bytes = 0).unwrap().code, "NO_ARTIFACT");
        assert_eq!(block("Registered", |_| {}).unwrap().code, "NO_WORK");
        // Candidate: the network has not admitted it — register; the jury's quorum is a strict majority of a panel.
        let b = block("Candidate", |_| {}).unwrap();
        assert_eq!((b.stage.as_str(), b.code.as_str(), b.have, b.need), ("register", "ADMISSION_JURY", 3, 3));
        // Prefetching: possession — mine.
        let b = block("Prefetching", |_| {}).unwrap();
        assert_eq!((b.stage.as_str(), b.code.as_str(), b.have, b.need, b.has_count), ("mine", "READY_SEATS", 3, 7, true));
        assert!(b.what.contains("14 bonds on the network have the headroom"), "{}", b.what);
        let short = {
            let mut r = registry();
            r.bonds_with_headroom = 4;
            palw_registry_blocking(&r, &class("Prefetching")).unwrap()
        };
        assert!(short.what.contains("only 4 bonds on the network have the headroom"), "{}", short.what);
        assert_eq!(block("Prefetching", |c| c.ready_seats_now = 7).unwrap().code, "PENDING_STEP");
        // Probation: verified service.
        let b = block("Probation { probes_passed: 4 }", |c| c.probes_failed = 1).unwrap();
        assert_eq!((b.code.as_str(), b.have, b.need), ("PROBATION", 4, 10));
        assert!(b.what.contains("1 failed since entry"), "{}", b.what);
        assert!(
            block("Probation { probes_passed: 0 }", |c| c.no_capable_panel_voids = 2).unwrap().what.contains("2 claims were voided")
        );
        // ActiveLimited: stability, then the cap.
        let b = block("ActiveLimited { stable_epochs: 1 }", |_| {}).unwrap();
        assert_eq!((b.code.as_str(), b.have, b.need), ("STABLE_SPANS", 1, 3));
        let b = block("ActiveLimited { stable_epochs: 3 }", |c| c.cap_utilization_permille = 910).unwrap();
        assert_eq!((b.code.as_str(), b.has_count), ("CAP_SATURATED", false));
        assert!(b.what.contains("910 ‰"), "{}", b.what);
    }

    #[test]
    fn a_held_class_says_whether_it_lacks_a_panel_seats_or_a_window() {
        let b = block("Held", |c| c.ready_seats_now = 2).unwrap();
        assert_eq!((b.code.as_str(), b.have, b.need), ("PANEL_NOT_DRAWABLE", 2, 5));
        let b = block("Held", |c| c.ready_seats_now = 6).unwrap();
        assert_eq!((b.code.as_str(), b.have, b.need), ("READY_SEATS", 6, 7));
        assert!(b.what.contains("re-enters Probation once 7 are ready"), "{}", b.what);
        let b = block("Held", |c| {
            c.ready_seats_now = 8;
            c.reason = "held: its verification window (4 spans) does not fit the receipt deadline".to_string();
            c.verification_window_spans = 4;
        })
        .unwrap();
        assert_eq!(b.code, "WINDOW_DOES_NOT_FIT");
        assert!(b.what.contains("4 spans"), "{}", b.what);
        assert_eq!(block("Held", |c| c.ready_seats_now = 8).unwrap().code, "HELD_RECOVERING");
    }

    #[test]
    fn an_unknown_state_is_reported_not_hidden() {
        let b = block("Quarantined", |_| {}).unwrap();
        assert_eq!(b.code, "UNKNOWN_STATE");
        assert!(b.what.contains("Quarantined"));
    }

    #[test]
    fn the_registry_reading_lists_every_class_that_has_one_in_order() {
        let mut r = registry();
        r.classes = vec![class("Active"), class("Prefetching"), class("Registered")];
        let all = palw_registry_blockings(&r);
        assert_eq!(all.iter().map(|b| b.code.as_str()).collect::<Vec<_>>(), ["READY_SEATS", "NO_WORK"]);
    }

    #[test]
    fn the_independence_floor_is_named_once_possession_holds() {
        use crate::model::message::RpcPalwClassSeating;
        let seating = |independent: u32| RpcPalwClassSeating {
            class_id: "cd".repeat(64),
            ready_operators: 7,
            needed_operators: 5,
            independent_operators: independent,
            needed_independent: 3,
            base_operators: 20,
            licensable_share_permille: (independent * 50) as u16,
        };
        let with = |state: &str, ready: u32, independent: u32| {
            let mut r = registry();
            r.seating = vec![seating(independent)];
            let mut c = class(state);
            c.ready_seats_now = ready;
            palw_registry_blocking(&r, &c)
        };
        // Possession first: below the required seats the count that fails is READY_SEATS.
        assert_eq!(with("Prefetching", 3, 1).unwrap().code, "READY_SEATS");
        // Seven ready, two independent: INDEPENDENT_OPERATORS 2/3 with the licensable share in its sentence.
        let b = with("Prefetching", 7, 2).unwrap();
        assert_eq!((b.stage.as_str(), b.code.as_str(), b.have, b.need, b.has_count), ("mine", "INDEPENDENT_OPERATORS", 2, 3, true));
        assert!(b.what.contains("100 ‰") && b.what.contains("20 base operators"), "{}", b.what);
        // The floor met: the reading is the lifecycle's (PENDING_STEP for a Prefetching class), and an Active class with it met
        // is nothing; unmet it is named (its claims are refused).
        assert_eq!(with("Prefetching", 7, 3).unwrap().code, "PENDING_STEP");
        assert!(with("Active", 7, 3).is_none());
        assert_eq!(with("Active", 7, 2).unwrap().code, "INDEPENDENT_OPERATORS");
        assert_eq!(with("Probation { probes_passed: 1 }", 7, 2).unwrap().code, "INDEPENDENT_OPERATORS");
        assert_eq!(with("Held", 7, 2).unwrap().code, "INDEPENDENT_OPERATORS");
        // A registry that serves no seating (below the fence, or a version-2 node) reads as before.
        assert_eq!(block("Prefetching", |c| c.ready_seats_now = 7).unwrap().code, "PENDING_STEP");
    }
}
