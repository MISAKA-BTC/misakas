//! RFC-0002 §II.14.60: the typed window class, version 37 — a Text29-sourced class
//! that adopts `LengthContractV36` (rule `Window8`) with a complete `WindowGraphV37`
//! (KV/state append binding and state continuity). Registration only: it creates no
//! job, reward, consensus weight or Final right. Dormant behind its own fence
//! `palw_window_class_v37` (absent on every preset); below it funded action 45 rides
//! unjudged (A-2). Unverified source checkpoint (2026-10-15).
use crate::{
    config::params::{ForkActivation, Params},
    palw_mode_v2::PalwModeV2Error,
    palw_native_text_stop_v29::NativeTextClassV29,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_lengths36::{HistoryRuleV36, LengthContractV36},
    flat_window37::WindowGraphV37,
    hash::{Digest, object_id},
};

pub const TEXT_WINDOW_CLASS_ACTION_V37: u8 = 45;
/// Producer opt-in that binds one Funded (pre-Commit) Text29 job to a window class:
/// `u16 37 || job || window class` (130 bytes). Courts 3/4 then apply to that job.
pub const TEXT_WINDOW_ATTACH_ACTION_V37: u8 = 46;
pub const WINDOW_ATTACH_BYTES_V37: usize = 130;
/// Stored per job: `(window class, graph root, attached DAA)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeWindowJobBindingV37 {
    pub version: u16,
    pub job: Digest,
    pub window_class: Digest,
    pub graph: Digest,
    pub attached_daa: u64,
}
pub const WINDOW_CLASS_MAX_BODY_BYTES_V37: usize = 96 << 10;

/// Signed body of funded action 45.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeWindowClassRegisterV37 {
    pub version: u16,
    /// The registered Text29 class that supplies the mature original source/program/real PC.
    pub text_class: Digest,
    pub length: LengthContractV36,
    pub graph: WindowGraphV37,
}
/// Stored, independently derived class record.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeWindowClassV37 {
    pub version: u16,
    pub text_class: Digest,
    pub program: Digest,
    pub real_parameter: Digest,
    pub inventory: Digest,
    pub length_contract: Digest,
    pub graph: Digest,
    pub graph_scope: Digest,
    pub selection_scope: Digest,
    pub maximum_positions: u32,
    pub window: u32,
}
impl NativeWindowClassV37 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/native/typed-window-class/v37", self)
    }
}
impl NativeWindowClassRegisterV37 {
    /// Bounded canonical decode BEFORE any derivation.
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > WINDOW_CLASS_MAX_BODY_BYTES_V37 || raw.get(..2) != Some(&37u16.to_le_bytes()[..]) {
            return Err("window37 class bounded grammar".into());
        }
        let r: Self = borsh::from_slice(raw).map_err(|_| "window37 class exact grammar")?;
        if borsh::to_vec(&r).map_err(|e| e.to_string())? != raw {
            return Err("window37 class noncanonical bytes".into());
        }
        Ok(r)
    }
    /// Every field is re-derived from the authenticated Text29 class and the
    /// program's frozen header; nothing declared is trusted.
    pub fn derive(&self, text: &NativeTextClassV29, program_history_bound: u32) -> Result<NativeWindowClassV37, String> {
        let HistoryRuleV36::Window8 { plan, window, .. } = self.length.rule else {
            return Err("window37 class adopts only the Window8 length rule".into());
        };
        let checked = self.length.check(program_history_bound, Some(&self.graph.selection))?;
        self.graph.validate()?;
        let graph_scope = self.graph.scope()?;
        let selection_scope = checked.history_scope().ok_or("window37 selection scope absent")?.root();
        if self.version != 37
            || text.id() != self.text_class
            || self.length.program_root != text.original_program
            || self.graph.program_root != text.original_program
            || plan != self.graph.selection.root()
            || window != self.graph.window
            || self.length.maximum_positions != self.graph.max_positions
        {
            return Err("window37 class differs from its Text29 source, length contract or graph".into());
        }
        Ok(NativeWindowClassV37 {
            version: 37,
            text_class: self.text_class,
            program: text.original_program,
            real_parameter: text.real_parameter,
            inventory: text.inventory,
            length_contract: self.length.root(),
            graph: self.graph.root(),
            graph_scope: graph_scope.root(),
            selection_scope,
            maximum_positions: self.length.maximum_positions,
            window: self.graph.window,
        })
    }
}
impl Params {
    pub fn palw_window_class_active_at_v37(&self, daa: u64) -> bool {
        self.palw_window_class_v37.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa))
            && self.palw_native_final_active_at_v27(daa)
            && self.palw_funded_execution_active_at_v11(daa)
    }
    pub fn validate_palw_window_class_v37(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_window_class_v37 {
            None => Ok(()),
            Some(f) if f == ForkActivation::never() => Ok(()),
            // Only the isolated calibration34 path may carry it, at genesis, beside Final27.
            Some(f) if self.has_isolated_native_calibration_v34() && f == ForkActivation::always() => Ok(()),
            Some(_) => Err(PalwModeV2Error::Invalid(
                "palw_window_class_v37 cannot be armed outside an isolated calibration34 devnet before window-graph Final/court evidence",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::{
        flat_history8::{HistoryNodeV8, HistorySelectionPlanV8},
        flat_lengths36::ModalityLengthV36,
        flat_lengths7::FlatLengthAxisV7,
        flat_window37::WindowAppendV37,
    };
    use misaka_palw_tir::DType;
    fn text() -> NativeTextClassV29 {
        NativeTextClassV29 {
            version: 29,
            source_binding: [1; 64],
            original_program: [2; 64],
            real_parameter: [3; 64],
            inventory: [4; 64],
            original_source_components: [5; 64],
            maximum_positions: 64,
            court_profile25: [6; 64],
            maximum_scope25: [7; 64],
            maximum_resource29: [8; 64],
        }
    }
    fn register() -> NativeWindowClassRegisterV37 {
        let mut graph = WindowGraphV37 {
            version: 37,
            program_root: [2; 64],
            nodes_per_position: 4,
            window: 8,
            max_positions: 1024,
            appends: vec![WindowAppendV37 {
                history: HistoryNodeV8 { occurrence: 0, node: 2, slot: 2 },
                writer: HistoryNodeV8 { occurrence: 0, node: 1, slot: 1 },
                dtype: DType::I32,
                row_shape: vec![2],
            }],
            selection: HistorySelectionPlanV8 {
                version: 8,
                graph_root: [0; 64],
                source: HistoryNodeV8 { occurrence: 0, node: 2, slot: 2 },
                output: HistoryNodeV8 { occurrence: 0, node: 3, slot: 3 },
                nodes_per_position: 4,
                dtype: DType::I32,
                row_shape: vec![2],
                window: 8,
                selection_cap: 2,
                max_positions: 1024,
            },
        };
        graph.selection.graph_root = graph.graph_root();
        NativeWindowClassRegisterV37 {
            version: 37,
            text_class: text().id(),
            length: LengthContractV36 {
                version: 36,
                program_root: [2; 64],
                rule: HistoryRuleV36::Window8 { plan: graph.selection.root(), window: 8, selection_cap: 2 },
                maximum_positions: 1024,
                token: ModalityLengthV36 { axis: FlatLengthAxisV7::Token, maximum: 1024, unit_bytes: 4, profile: [0; 64] },
                patch: None,
                audio: None,
            },
            graph,
        }
    }
    #[test]
    fn window_class_rederives_from_its_text_source_and_stays_dormant() {
        let r = register();
        let bytes = borsh::to_vec(&r).unwrap();
        assert_eq!(NativeWindowClassRegisterV37::from_bytes(&bytes).unwrap(), r);
        let record = r.derive(&text(), misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL).unwrap();
        assert_eq!(record.maximum_positions, 1024);
        let mut other = r.clone();
        other.length.program_root = [9; 64];
        assert!(other.derive(&text(), misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL).is_err());
        let mut v1 = r.clone();
        v1.length.rule = HistoryRuleV36::FrozenV1 { history_bound: misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL };
        assert!(v1.derive(&text(), misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL).is_err(), "Window8 only");
        // Fence: absent on every preset, refused when armed outside an isolated calibration devnet.
        for p in [crate::config::params::MAINNET_PARAMS.clone(), crate::config::params::devnet_shipped_params()] {
            assert_eq!(p.palw_window_class_v37, None);
            p.validate_palw_window_class_v37().unwrap();
            let mut armed = p.clone();
            armed.palw_window_class_v37 = Some(ForkActivation::new(1_000));
            assert!(armed.validate_palw_window_class_v37().is_err());
            armed.palw_window_class_v37 = Some(ForkActivation::never());
            armed.validate_palw_window_class_v37().unwrap();
            assert!(p.palw_fences_v1().contains(&("palw_window_class_v37", None)));
        }
    }
}
