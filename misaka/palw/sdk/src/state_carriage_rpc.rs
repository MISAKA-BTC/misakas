//! Bounded live carriage capture against a caller-pinned header and local ruleset.
use kaspa_consensus_core::{Hash64, config::params::Params, palw_state_carriage_rpc_v1::*};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::KaspaRpcClient;

pub async fn fetch_state_carriage_v1(
    client: &KaspaRpcClient,
    params: &Params,
    pinned: Hash64,
) -> Result<PalwStateCarriageVerifiedV1, String> {
    let identity = |s: String| s;
    let limit = PALW_CARRIAGE_RPC_MAX_PAGE_BYTES_V1;
    let mut assembler: Option<PalwStateCarriageAssemblerV1> = None;
    loop {
        let offset = assembler.as_ref().map_or(0, |a| a.cursor());
        let response = client
            .get_palw_state_carriage(kaspa_rpc_core::GetPalwStateCarriageRequest {
                block_hash: pinned.to_string(),
                offset,
                limit_bytes: limit as u32,
            })
            .await
            .map_err(|e| format!("getPalwStateCarriage: {e}"))?;
        if !response.available {
            return Err(format!("the node serves no carriage for {pinned}: {}", response.reason));
        }
        let header = |h: Option<&kaspa_rpc_core::RpcHeader>, name: &str| {
            kaspa_consensus_core::header::Header::try_from(h.ok_or_else(|| identity(format!("the page omits the {name}")))?)
                .map_err(|e| identity(format!("{name}: {e}")))
        };
        let page = PalwStateCarriagePageV1 {
            header: header(response.header.as_ref(), "header")?,
            parent_header: header(response.parent_header.as_ref(), "parent header")?,
            fork_choice: if response.fork_choice_opening.is_empty() {
                None
            } else {
                Some(borsh::from_slice(&response.fork_choice_opening).map_err(|e| identity(format!("fork-choice opening: {e}")))?)
            },
            state_root: response.state_root.parse::<Hash64>().map_err(|e| identity(format!("served state root: {e}")))?,
            total_bytes: response.total_bytes,
            offset: response.offset,
            bytes: response.bytes,
        };
        // Exact page lengths: a node that trickles one byte per page cannot stretch the bounded download.
        let expected_len = (page.total_bytes.saturating_sub(offset)).min(limit as u64);
        if page.bytes.len() as u64 != expected_len {
            return Err(identity(format!("a page carries {} bytes where {expected_len} are due", page.bytes.len())));
        }
        if assembler.is_none() {
            assembler =
                Some(PalwStateCarriageAssemblerV1::new(pinned, &page, PALW_CARRIAGE_RPC_MAX_TOTAL_BYTES_V1).map_err(identity)?);
        }
        let a = assembler.as_mut().ok_or_else(|| identity("the carriage assembler is missing".into()))?;
        a.push(&page).map_err(identity)?;
        if a.is_complete() {
            break;
        }
    }
    assembler.ok_or_else(|| identity("no page was assembled".into()))?.finish(params).map_err(identity)
}
