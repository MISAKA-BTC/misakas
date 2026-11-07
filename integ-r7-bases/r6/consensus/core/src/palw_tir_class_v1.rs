//! **RFC-0002 Phase F, step F2: an IR class — what it is, what identifies it, and what its
//! registration carries** (`docs/design/palw/tir/phase-f-integration.md` §2.2–§2.3).
//!
//! An IR class is a [`PalwTirClassV1`]: the canonical bytes of a `TirProgramV1` (spec 04b §4), the
//! commitment layout the step space is enumerated under ([`PalwTirLayoutV1`]), and the tokenizer
//! the class's prompts are ids of. Its identity is
//!
//! ```text
//! graph_ir_root   = H64(key "misaka-palw/tir/graph-ir-root/v1", program bytes)
//! tir_class_id_v1 = H64(key "misaka-palw/tir/class-id/v1",
//!                       graph_ir_root ‖ H64(key "misaka-palw/tir/layout/v1", borsh(layout)) ‖ artifact_root ‖ tokenizer_id)
//! ```
//!
//! — the program (which holds `prim_set_id` and `logits_scheme_id`), the layout, the weights (the
//! TIR inventory root) and the tokenizer; nothing node-local exists to commit. The key is not
//! `PALW_STEP_DOMAIN_SHAPE_PROFILE_V3`, so an IR class id can never equal a legacy profile id, and a
//! job context's opaque `shape_profile_id` field carries it wherever a legacy class carries its
//! profile id.
//!
//! **Carriage is inline** (design D3, the lead's decision): the whole program rides the
//! registration object, in one lifecycle carrier, under the network's `max_program_bytes`
//! ([`crate::palw_tir_v1::PalwTirCeilingsV1`]). The object is APPENDED to the lifecycle objects
//! (`PalwConsensusObjectV2::ClassRegisteredTirV1`, tag 61): an older build on a ruleset that declared
//! `palw_audit_2026_09_11` tolerates and skips a payload it cannot decode (A-2), and this build drops
//! the object by name until `palw_tir_v1` is armed — the same outcome, byte for byte.

use crate::Hash64;
use crate::palw_state_v2::{PalwBondKeyV2, PalwPwuRuleV2};
use crate::palw_v2::PalwJobContextV2;

/// The key of `graph_ir_root` — one spelling, the inventory module's (F3), which checks a
/// container's embedded program against it.
pub use crate::palw_tir_artifact_v1::PALW_TIR_GRAPH_IR_ROOT_DOMAIN_V1;
pub const PALW_TIR_LAYOUT_DOMAIN_V1: &[u8] = b"misaka-palw/tir/layout/v1";
pub const PALW_TIR_CLASS_ID_DOMAIN_V1: &[u8] = b"misaka-palw/tir/class-id/v1";
pub const PALW_TIR_CLASS_REGISTRATION_DOMAIN_V1: &[u8] = b"misaka-palw/tir/class-registration/message/v1";
/// The ML-DSA-87 context a registrant bond signs an IR registration under.
pub const PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/tir/class-registration/mldsa87/v1";
pub const PALW_TIR_CLASS_VERSION_V1: u16 = 1;
pub const PALW_TIR_LAYOUT_VERSION_V1: u16 = 1;

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The commitment layout** — what the step space of an IR class is enumerated under (design
/// §2.3, §2.5). Declared by the registrant, verified by admission, inside the class id. It never
/// prices work (PALW-TIR-16): the structural work vector reads none of it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirLayoutV1 {
    /// [`PALW_TIR_LAYOUT_VERSION_V1`].
    pub version: u16,
    /// The most positions a job of this class may touch (`prefill + decode − 1`, the legacy
    /// `n_ctx`); at most the program's `history_bound`.
    pub max_context: u32,
    /// `C`: the state after every `C`-th position is committed as step leaves (`Fixed` states).
    pub checkpoint_interval: u32,
    /// The canonical history chunk: every `h_tile` positions the last `h_tile` rows of each history
    /// are committed as one leaf per sub-row, and a dissection's bottom is one such chunk.
    pub h_tile: u32,
    /// `tile_len` of every committed node, in `(block, node)` order over the program's blocks.
    pub commit_tiles: Vec<u32>,
    /// Per state declaration, in order: a `Fixed` state's lanes per checkpoint tile; a `Hist`
    /// state's lanes per sub-row (a row is cut into sub-rows of this many lanes).
    pub state_tiles: Vec<u32>,
}

/// **An IR class**: the program, its layout and its tokenizer (design §2.3).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirClassV1 {
    /// [`PALW_TIR_CLASS_VERSION_V1`].
    pub version: u16,
    /// The canonical `TirProgramV1` bytes (spec 04b §4). Carried, never re-encoded: the bytes ARE
    /// the program.
    pub program: Vec<u8>,
    pub layout: PalwTirLayoutV1,
    /// The tokenizer the class's prompt ids are ids of.
    pub tokenizer_id: Hash64,
}

impl PalwTirClassV1 {
    /// `graph_ir_root`: the keyed hash of the program's canonical bytes (PALW-TIR-7).
    pub fn graph_ir_root(&self) -> Hash64 {
        crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&self.program)
    }

    /// The layout's digest, as the class id binds it.
    pub fn layout_digest(&self) -> Hash64 {
        keyed64(PALW_TIR_LAYOUT_DOMAIN_V1, &[&borsh::to_vec(&self.layout).expect("a layout is borsh-serializable")])
    }

    /// **The class id** over the weights it is registered with (`artifact_root`, the TIR
    /// inventory root). Every field is a fact about the model; none is a node's choice.
    pub fn class_id(&self, artifact_root: &Hash64) -> Hash64 {
        keyed64(
            PALW_TIR_CLASS_ID_DOMAIN_V1,
            &[
                &self.version.to_le_bytes(),
                self.graph_ir_root().as_byte_slice(),
                self.layout_digest().as_byte_slice(),
                artifact_root.as_byte_slice(),
                self.tokenizer_id.as_byte_slice(),
            ],
        )
    }

    /// The program, decoded strictly (spec 04b §4.4: the unique encoding of a program in normal
    /// form, within the format's byte cap). A network's own byte ceiling is admission's to apply.
    pub fn decode_program(&self) -> misaka_palw_tir::TirResult<misaka_palw_tir::TirProgramV1> {
        misaka_palw_tir::TirProgramV1::decode_canonical(&self.program)
    }
}

/// **What an IR registration carries** — the twin of `PalwClassAdmissionCarriageV2`, with the
/// class in place of the profile.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirAdmissionCarriageV1 {
    pub class: PalwTirClassV1,
    /// The job the class is paid per; `pwu_per_inference` is counted from it, never believed.
    pub canonical: PalwJobContextV2,
    /// An Active bond on this chain (launch blockers §3's "who").
    pub registrant_bond: PalwBondKeyV2,
    /// ML-DSA-87 over [`palw_tir_class_registration_message_v1`] under
    /// [`PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1`], by the registrant bond's registered key.
    pub signature: Vec<u8>,
}

/// **The message a registrant bond signs** for an IR registration: every field the object carries,
/// the class included, under the network domain — the legacy message
/// (`palw_state_v2::palw_class_registration_message_v2`) with the class in place of the profile,
/// under its own key so neither can be lifted onto the other.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_class_registration_message_v1(
    network_domain: Hash64,
    class_id: Hash64,
    share_permille: u16,
    activation_daa: u64,
    registrant_bond: &PalwBondKeyV2,
    artifact_root: Hash64,
    slash_value_per_pwu: u64,
    initial_target: u128,
    pwu_rule: &PalwPwuRuleV2,
    canonical: &PalwJobContextV2,
    class: &PalwTirClassV1,
) -> Hash64 {
    keyed64(
        PALW_TIR_CLASS_REGISTRATION_DOMAIN_V1,
        &[
            network_domain.as_byte_slice(),
            class_id.as_byte_slice(),
            &share_permille.to_le_bytes(),
            &activation_daa.to_le_bytes(),
            &borsh::to_vec(registrant_bond).expect("a bond key is borsh-serializable"),
            artifact_root.as_byte_slice(),
            &slash_value_per_pwu.to_le_bytes(),
            &initial_target.to_le_bytes(),
            &borsh::to_vec(pwu_rule).expect("a pwu rule is borsh-serializable"),
            &borsh::to_vec(canonical).expect("a job context is borsh-serializable"),
            &borsh::to_vec(class).expect("a class is borsh-serializable"),
        ],
    )
}

/// **Does a V2 bundle's genesis register an IR class?** IR genesis rows arrive in Phase H; until
/// then `Params::validate_palw_v2` refuses a genesis that carries one.
pub fn palw_genesis_registers_tir_class_v1(bundle: &crate::palw_mode_v2::PalwConsensusParamsV2) -> bool {
    bundle.genesis_objects.iter().any(|o| matches!(o, crate::palw_state_v2::PalwConsensusObjectV2::ClassRegisteredTirV1 { .. }))
}
