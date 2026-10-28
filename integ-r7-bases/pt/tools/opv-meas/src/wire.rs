//! The chain a world writes and a fresh verifier replays: genesis constants plus blocks of ledger inputs, where every signed object
//! is carried as its strict wire bytes (`KernelRouteObjectV1::encode`), so a verifier pays the real decode.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{AuthV1, KernelLedgerV1, KernelRouteObjectV1, LedgerBlockV1, LedgerPolicyV1, LedgerTxV1};
use misaka_palw_kernel::opv::OpvPolicyV1;

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum WTx {
    Sync { bond: Digest, collateral: u64 },
    Attest { root: Digest },
    Admit { class: Digest },
    Obj { signer: Digest, bytes: Vec<u8> },
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct WBlock {
    pub daa: u64,
    pub txs: Vec<WTx>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct WChain {
    pub policy: LedgerPolicyV1,
    pub opv: Option<OpvPolicyV1>,
    pub blocks: Vec<WBlock>,
}

impl WTx {
    pub fn obj(signer: Digest, o: &KernelRouteObjectV1) -> WTx {
        WTx::Obj { signer, bytes: o.encode() }
    }
}

impl WBlock {
    pub fn to_ledger(&self) -> Result<LedgerBlockV1, String> {
        let mut txs = Vec::with_capacity(self.txs.len());
        for t in &self.txs {
            txs.push(match t {
                WTx::Sync { bond, collateral } => LedgerTxV1::SyncBond { bond: *bond, collateral: *collateral },
                WTx::Attest { root } => LedgerTxV1::AttestArtifact { artifact_root: *root },
                WTx::Admit { class } => LedgerTxV1::AdmitOptimisticClass { class: *class },
                WTx::Obj { signer, bytes } => LedgerTxV1::Object {
                    auth: AuthV1 { signer_bond: *signer },
                    object: KernelRouteObjectV1::decode(bytes).map_err(|r| format!("{}: {}", r.object, r.why))?,
                },
            });
        }
        Ok(LedgerBlockV1 { daa: self.daa, txs })
    }
}

impl WChain {
    pub fn genesis(&self) -> KernelLedgerV1 {
        kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_template_opv_v1(self.policy, self.opv)
    }

    pub fn ledger_blocks(&self) -> Result<Vec<LedgerBlockV1>, String> {
        self.blocks.iter().map(WBlock::to_ledger).collect()
    }
}
