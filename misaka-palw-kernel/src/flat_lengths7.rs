//! Explicit job-admission specialization of bounded token/patch/audio axes.
//! The frozen TIR v1 grammar is unchanged: all substituted axes become Fixed
//! before execution, checking and court admission. This contract does not add
//! H-axis TopK or supply a media stage's authenticated public input by itself.
use crate::{
    flat_replay7::FlatReplayEnvelopeV7,
    flat_scope7::FlatResourceScopeV7,
    hash::{Digest, object_id},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{Dim, TirProgramV1};

pub const FLAT_LENGTH_VERSION_V7: u16 = 7;
pub const FLAT_LENGTH_MAX_BINDINGS_V7: usize = 4096;
pub const FLAT_LENGTH_MAX_WIRE_BYTES_V7: usize = misaka_palw_tir::program::MAX_PROGRAM_BYTES + 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum FlatLengthAxisV7 {
    Token,
    Patch,
    Audio,
}

/// Zero means that a modality is absent, not an unbounded dimension.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatJobLengthsV7 {
    pub token: u32,
    pub patch: u32,
    pub audio: u32,
}
impl FlatJobLengthsV7 {
    pub fn get(self, axis: FlatLengthAxisV7) -> u32 {
        match axis {
            FlatLengthAxisV7::Token => self.token,
            FlatLengthAxisV7::Patch => self.patch,
            FlatLengthAxisV7::Audio => self.audio,
        }
    }
    pub fn within(self, maxima: Self) -> Result<(), String> {
        for axis in [FlatLengthAxisV7::Token, FlatLengthAxisV7::Patch, FlatLengthAxisV7::Audio] {
            let (actual, maximum) = (self.get(axis), maxima.get(axis));
            if maximum > misaka_palw_tir::types::MAX_DIM || actual > maximum || (maximum == 0) != (actual == 0) {
                return Err("actual job length is absent, enlarged or outside its declared bound".into());
            }
        }
        Ok(())
    }
}

/// An axis is changed in exactly one declared place. References, primitive
/// attributes, dtypes, schedules, history rules and constant bytes are frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum FlatShapeSlotV7 {
    Parameter { parameter: u16, axis: u8 },
    State { state: u16, axis: u8 },
    Carry { block: u8, carry: u8, axis: u8 },
    Node { block: u8, node: u16, axis: u8 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatShapeBindingV7 {
    pub slot: FlatShapeSlotV7,
    pub length: FlatLengthAxisV7,
}

/// The model's canonical maximum-shape program plus a bounded declarative map.
/// Both maximum and requested specialization must pass normal-form/type/range
/// and complete graph court/replay derivation; no arbitrary code is executed.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatLengthTemplateV7 {
    pub version: u16,
    pub maxima: FlatJobLengthsV7,
    pub max_program_bytes: Vec<u8>,
    pub bindings: Vec<FlatShapeBindingV7>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatLengthAdmissionV7 {
    pub version: u16,
    pub template_root: Digest,
    pub actual: FlatJobLengthsV7,
    pub program_root: Digest,
    pub positions: u32,
    pub scope_root: Digest,
    pub replay_root: Digest,
}
impl FlatLengthAdmissionV7 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/flat-length-admission/v7", self)
    }
    /// A decoder job's token count includes its prompt and all fed-back tokens;
    /// the final delivered token is not fed back. Media counts require a typed
    /// stage-input binding and are refused on the text-only candidate route.
    pub fn check_decoder_job(&self, prompt: usize, generated: u32, positions: u32) -> Result<(), String> {
        let expected = u64::try_from(prompt).ok().and_then(|n| n.checked_add(generated as u64)?.checked_sub(1));
        if prompt == 0
            || generated == 0
            || self.version != 7
            || self.actual.patch != 0
            || self.actual.audio != 0
            || expected != Some(self.actual.token as u64)
            || positions != self.positions
            || positions != self.actual.token
        {
            return Err("job differs from its admitted text lengths or requires an unbound media stage".into());
        }
        Ok(())
    }
    /// A one-position encoder consumes the admitted prompt count. A separate
    /// immutable input-count/root contract must authenticate media modalities.
    pub fn check_encoder_job(&self, prompt: usize, positions: u32) -> Result<(), String> {
        if self.version != 7
            || prompt == 0
            || u32::try_from(prompt).ok() != Some(self.actual.token)
            || self.actual.patch != 0
            || self.actual.audio != 0
            || positions != 1
            || self.positions != 1
        {
            return Err("encoder input count differs from admitted lengths or has an unbound media stage".into());
        }
        Ok(())
    }
    pub fn check_stage_counts(&self, lengths: FlatJobLengthsV7) -> Result<(), String> {
        if self.version != 7 || self.actual != lengths {
            return Err("typed stage counts differ from job admission".into());
        }
        Ok(())
    }
}
pub struct FlatLengthSpecializationV7 {
    pub admission: FlatLengthAdmissionV7,
    pub program: TirProgramV1,
    pub scope: FlatResourceScopeV7,
    pub replay: FlatReplayEnvelopeV7,
}

impl FlatLengthTemplateV7 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/flat-length-template/v7", self)
    }
    pub(crate) fn program(&self, actual: FlatJobLengthsV7) -> Result<TirProgramV1, String> {
        if self.version != FLAT_LENGTH_VERSION_V7 || self.bindings.len() > FLAT_LENGTH_MAX_BINDINGS_V7 {
            return Err("unknown or oversized bounded-length template".into());
        }
        actual.within(self.maxima)?;
        if self.bindings.windows(2).any(|x| x[0].slot >= x[1].slot) {
            return Err("shape bindings must be strictly ordered with no duplicate slot".into());
        }
        let mut p = TirProgramV1::decode_canonical(&self.max_program_bytes).map_err(|e| e.to_string())?;
        for binding in &self.bindings {
            let (maximum, length) = (self.maxima.get(binding.length), actual.get(binding.length));
            if maximum == 0 {
                return Err("an absent modality cannot bind a tensor axis".into());
            }
            let assign = |d: &mut u32| -> Result<(), String> {
                if *d != maximum {
                    return Err("template slot does not contain its declared maximum".into());
                }
                *d = length;
                Ok(())
            };
            match binding.slot {
                FlatShapeSlotV7::Parameter { parameter, axis } => assign(
                    p.params
                        .get_mut(parameter as usize)
                        .and_then(|d| d.shape.get_mut(axis as usize))
                        .ok_or("no declared parameter axis")?,
                )?,
                FlatShapeSlotV7::State { state, axis } => assign(
                    p.states.get_mut(state as usize).and_then(|d| d.shape.get_mut(axis as usize)).ok_or("no declared state axis")?,
                )?,
                FlatShapeSlotV7::Carry { block, carry, axis } => {
                    let d = p
                        .blocks
                        .get_mut(block as usize)
                        .and_then(|b| b.carry_in.get_mut(carry as usize))
                        .and_then(|t| t.shape.get_mut(axis as usize))
                        .ok_or("no declared carry axis")?;
                    let Dim::Fixed(n) = d else {
                        return Err("bounded admission cannot rewrite frozen H".into());
                    };
                    assign(n)?;
                }
                FlatShapeSlotV7::Node { block, node, axis } => {
                    let d = p
                        .blocks
                        .get_mut(block as usize)
                        .and_then(|b| b.nodes.get_mut(node as usize))
                        .and_then(|n| n.out.shape.get_mut(axis as usize))
                        .ok_or("no declared node axis")?;
                    let Dim::Fixed(n) = d else {
                        return Err("bounded admission cannot rewrite frozen H".into());
                    };
                    assign(n)?;
                }
            }
        }
        TirProgramV1::decode_canonical(&p.encode()).map_err(|e| e.to_string())
    }
    pub fn specialize(&self, actual: FlatJobLengthsV7, positions: u32, encoder: bool) -> Result<FlatLengthSpecializationV7, String> {
        let derive = |p: &TirProgramV1| -> Result<(FlatResourceScopeV7, FlatReplayEnvelopeV7), String> {
            let e = if encoder {
                let e = crate::seg_encoder::encoder_binding_v1(p)?;
                crate::seg_encoder::prove_encoder_ranges_v1(p, &e)?;
                Some(e)
            } else {
                misaka_palw_tir::interval::analyze_ranges(p).map_err(|e| e.to_string())?;
                None
            };
            let scope = crate::flat_scope7::flat_resource_scope_v7(p, positions, e.as_ref()).map_err(|e| format!("{e:?}"))?;
            let replay = crate::flat_replay7::flat_replay_envelope_v7(p, &scope, e.as_ref()).map_err(|e| format!("{e:?}"))?;
            Ok((scope, replay))
        };
        // The maximum graph must be safe too; a small job cannot hide an
        // inadmissible maximum relation. This does not certify every arbitrary
        // intermediate length: each requested actual shape is checked anew.
        let maximum = self.program(self.maxima)?;
        derive(&maximum)?;
        let program = self.program(actual)?;
        let (scope, replay) = derive(&program)?;
        let admission = FlatLengthAdmissionV7 {
            version: 7,
            template_root: self.root(),
            actual,
            program_root: crate::public::program_root_v1(&program.encode()),
            positions,
            scope_root: scope.root(),
            replay_root: replay.root(),
        };
        Ok(FlatLengthSpecializationV7 { admission, program, scope, replay })
    }
    pub fn check_admission(&self, admitted: &FlatLengthAdmissionV7, encoder: bool) -> Result<FlatLengthSpecializationV7, String> {
        let actual = self.specialize(admitted.actual, admitted.positions, encoder)?;
        if actual.admission != *admitted {
            return Err("bounded-length admission differs from exact independent specialization".into());
        }
        Ok(actual)
    }
    /// Check the outer byte count and both variable counts before allocation.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        use std::io::{Cursor, Read};
        if bytes.len() > FLAT_LENGTH_MAX_WIRE_BYTES_V7 {
            return Err("bounded-length wire exceeds cap".into());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        if version != 7 {
            return Err("unknown bounded-length template version".into());
        }
        let maxima = FlatJobLengthsV7::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        maxima.within(maxima)?;
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n > misaka_palw_tir::program::MAX_PROGRAM_BYTES || n > bytes.len().saturating_sub(r.position() as usize) {
            return Err("bounded-length program count exceeds wire".into());
        }
        let mut max_program_bytes = vec![0; n];
        r.read_exact(&mut max_program_bytes).map_err(|e| e.to_string())?;
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n > FLAT_LENGTH_MAX_BINDINGS_V7 || n > bytes.len().saturating_sub(r.position() as usize) / 5 {
            return Err("bounded-length binding count exceeds wire".into());
        }
        let mut bindings = Vec::with_capacity(n);
        for _ in 0..n {
            bindings.push(FlatShapeBindingV7::deserialize_reader(&mut r).map_err(|e| e.to_string())?);
        }
        let out = Self { version, maxima, max_program_bytes, bindings };
        if r.position() as usize != bytes.len() || borsh::to_vec(&out).map_err(|e| e.to_string())? != bytes {
            return Err("noncanonical or surplus bounded-length bytes".into());
        }
        out.program(maxima)?;
        Ok(out)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let bytes = borsh::to_vec(self).map_err(|e| e.to_string())?;
        Self::from_bytes(&bytes)?;
        Ok(bytes)
    }
}
