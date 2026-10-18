//! Exact typed checkpoint coverage. These are allocation and byte-shape
//! visitors; semantic authority comes from the executed liability fold.
use crate::palw_claim_local_v1::{PalwClaimLocalCheckpointV1, PalwClaimLocalRecordV1};
use crate::palw_native_resource_v1::native_struct;
use crate::palw_native_resource_v1::wire::wire_struct;
use crate::{Hash64, palw_state_v2::PalwBlockContextV2};
use std::collections::{BTreeMap, BTreeSet};

native_struct!(PalwClaimLocalRecordV1; claim, accepted_daa, accepted_blue_score, accepting_block, anchors, inputs,
    execution_root, reservations, final_claim, certified, voided, liability_closed, material);
native_struct!(PalwClaimLocalCheckpointV1; point, records, liability_state, material);
wire_struct!(PalwClaimLocalRecordV1; Hash64, u64, u64, Hash64, bool, BTreeSet<Hash64>, Hash64, BTreeSet<Hash64>,
    bool, bool, bool, bool, BTreeSet<Hash64>);
wire_struct!(PalwClaimLocalCheckpointV1; PalwBlockContextV2, BTreeMap<Hash64, PalwClaimLocalRecordV1>, Vec<u8>, BTreeMap<Hash64, Vec<u8>>);
