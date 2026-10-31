//! Owning program39 grammar. No V1/V2 program is decoded or executed as39.
//! A primitive template may be imported node-by-node; the source of authority is
//! always this canonical body and its explicit profile.
use crate::{
    DType, Prim,
    program::{ConstDecl, ParamDecl, Ref, Schedule, StateDecl},
    program_v2::{InputDecl, OutputDecl},
};
use borsh::{BorshDeserialize, BorshSerialize};
/// Owning39 semantics descriptor. Frozen arithmetic remains byte-for-byte V1;
/// the outer sum type and dynamic selection have their own original identity.
pub const PRIM_SET_ID_DOMAIN_V39: &[u8] = b"misaka-palw/tir-prim-set-id/v39";
pub fn prim_set_descriptor_v39() -> Vec<u8> {
    let mut d = crate::prim::prim_set_descriptor_v1();
    d.extend_from_slice(b";extension39/rev1=Frozen(tag0,unchanged-v1-attributes);HistorySelect(tag1,axis:u8,cap:u32,k=min(H,cap),descending-score-ties-low-index,return-ascending-original-indices,Idx);SelectedH=min(H,cap);global-state-single-original-writer");
    d
}
pub const PRIM_SET_ID_V39: [u8; 64] = [
    0xda, 0xc0, 0x19, 0xf0, 0xed, 0xc2, 0xad, 0x37, 0x27, 0x28, 0x26, 0x4a, 0x19, 0x8d, 0xfe, 0x03, 0x14, 0x84, 0xd8, 0xae, 0xee,
    0x38, 0x02, 0x77, 0x43, 0xb0, 0x8a, 0xca, 0x7c, 0x35, 0x87, 0x78, 0xba, 0x24, 0x42, 0x4f, 0xee, 0x72, 0x83, 0x16, 0x3a, 0x9b,
    0xe8, 0x41, 0xa8, 0x93, 0x9c, 0x67, 0xda, 0x92, 0x69, 0xcb, 0xb3, 0x2f, 0xe6, 0x48, 0x86, 0x9f, 0xf5, 0x4c, 0x36, 0x78, 0x1e,
    0x70,
];
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceProfileV39 {
    pub version: u16,
    pub max_program_bytes: u32,
    pub max_blocks: u16,
    pub max_nodes_per_block: u16,
    pub max_states: u16,
    pub max_parameters: u16,
    pub max_parameter_elements: u64,
    pub max_value_elements: u64,
    pub max_input_raw_bytes: u64,
    pub max_input_expanded_bytes: u64,
}
impl SourceProfileV39 {
    pub fn full_source() -> Self {
        Self {
            version: 39,
            max_program_bytes: 16 << 20,
            max_blocks: 256,
            max_nodes_per_block: 4096,
            max_states: 512,
            max_parameters: 16384,
            max_parameter_elements: 1 << 34,
            max_value_elements: 1 << 30,
            max_input_raw_bytes: 256 << 20,
            max_input_expanded_bytes: 2 << 30,
        }
    }
    pub fn check(&self) -> Result<(), String> {
        let cap = Self::full_source();
        if self.version != 39
            || self.max_program_bytes == 0
            || self.max_program_bytes > cap.max_program_bytes
            || self.max_blocks == 0
            || self.max_blocks > cap.max_blocks
            || self.max_parameters == 0
            || self.max_parameters > cap.max_parameters
            || self.max_nodes_per_block == 0
            || self.max_nodes_per_block > cap.max_nodes_per_block
            || self.max_states > cap.max_states
            || self.max_parameter_elements == 0
            || self.max_parameter_elements > cap.max_parameter_elements
            || self.max_value_elements == 0
            || self.max_value_elements > cap.max_value_elements
            || self.max_input_raw_bytes == 0
            || self.max_input_raw_bytes > cap.max_input_raw_bytes
            || self.max_input_expanded_bytes == 0
            || self.max_input_expanded_bytes > cap.max_input_expanded_bytes
        {
            return Err("source39 explicit profile outside owning bounds".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum DimV39 {
    Fixed(u32),
    H,
    SelectedH(u32),
}
impl DimV39 {
    pub fn at(self, h: u32) -> u32 {
        match self {
            Self::Fixed(n) => n,
            Self::H => h,
            Self::SelectedH(cap) => h.min(cap),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct TensorTypeV39 {
    pub dtype: DType,
    pub shape: Vec<DimV39>,
}
impl TensorTypeV39 {
    pub fn fixed(dtype: DType, shape: &[u32]) -> Self {
        Self { dtype, shape: shape.iter().copied().map(DimV39::Fixed).collect() }
    }
    pub fn scalar(dtype: DType) -> Self {
        Self { dtype, shape: Vec::new() }
    }
    pub fn from_frozen(t: &crate::TensorType) -> Self {
        Self {
            dtype: t.dtype,
            shape: t
                .shape
                .iter()
                .map(|d| match d {
                    crate::Dim::Fixed(n) => DimV39::Fixed(*n),
                    crate::Dim::H => DimV39::H,
                })
                .collect(),
        }
    }
    pub fn elements_at(&self, h: u32) -> Result<u64, String> {
        self.shape.iter().try_fold(1u64, |n, d| n.checked_mul(d.at(h) as u64).ok_or("source39 tensor product overflow".into()))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NodePrimV39 {
    Frozen(Prim),
    HistorySelect { axis: u8, cap: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NodeV39 {
    pub prim: NodePrimV39,
    pub inputs: Vec<Ref>,
    pub out: TensorTypeV39,
    pub commit: bool,
}
impl NodeV39 {
    pub fn from_frozen(n: &crate::Node) -> Self {
        Self {
            prim: NodePrimV39::Frozen(n.prim.clone()),
            inputs: n.inputs.clone(),
            out: TensorTypeV39::from_frozen(&n.out),
            commit: n.commit,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockV39 {
    pub name: String,
    pub carry_in: Vec<TensorTypeV39>,
    pub nodes: Vec<NodeV39>,
    pub carry_out: Vec<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TirProgramV39 {
    pub version: u16,
    pub profile: SourceProfileV39,
    pub prim_set_id: [u8; 64],
    pub token_bound: u32,
    pub history_bound: u32,
    pub inputs: Vec<InputDecl>,
    pub params: Vec<ParamDecl>,
    pub consts: Vec<ConstDecl>,
    pub states: Vec<StateDecl>,
    pub blocks: Vec<BlockV39>,
    pub schedule: Schedule,
    pub output: OutputDecl,
}
impl TirProgramV39 {
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("program39 Vec encoding")
    }
    pub fn decode_canonical(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > 16 << 20 {
            return Err("program39 canonical byte extent".into());
        }
        // The descriptor walker bounds every allocation before Borsh materializes it.
        crate::validate_v39::preflight_program_v39(raw)?;
        let p: Self = borsh::from_slice(raw).map_err(|_| "program39 exact canonical grammar")?;
        if p.encode() != raw {
            return Err("program39 noncanonical body".into());
        }
        crate::validate_v39::validate_v39(&p)?;
        Ok(p)
    }
    pub fn occurrences(&self) -> Vec<(u8, Option<u16>)> {
        let mut o = vec![(self.schedule.pre, None)];
        o.extend(self.schedule.layers.iter().enumerate().map(|(i, b)| (*b, Some(i as u16))));
        o.push((self.schedule.post, None));
        o
    }
}
