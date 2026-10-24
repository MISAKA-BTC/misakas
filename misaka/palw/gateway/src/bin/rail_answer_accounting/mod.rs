//! The real rail's native quote handshake. No client switch enables a consensus fence.
use kaspa_hashes::Hash64;
use kaspa_rpc_core::GetPalwFreePromptPriceResponse;

pub(super) fn check_native_quote_v1(quote: &GetPalwFreePromptPriceResponse, network: Hash64) -> Result<(), String> {
    if !quote.available || quote.accounting_quote_version != 1 {
        return Err("node lacks a native V2 answer-aware quote".into());
    }
    if quote.network_domain != network.to_string() || quote.chain_point.parse::<Hash64>().is_err() {
        return Err("native quote network or chain point mismatch".into());
    }
    Ok(())
}

pub(super) fn check_preclaim_price_context_v1(
    preclaim: &kaspa_rpc_core::GetPalwStagedPreclaimQuoteResponse,
    price: &GetPalwFreePromptPriceResponse,
) -> Result<(), String> {
    if preclaim.snapshot_block != price.chain_point || preclaim.quote_daa != Some(price.daa_score) {
        return Err("public family and accounting quote snapshot/DAA differ; retry before signing".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rail_native_quote_never_falls_back_on_old_unavailable_or_wrong_chain_context() {
        let network = Hash64::from_u64_word(9);
        let quote = GetPalwFreePromptPriceResponse {
            available: true,
            accounting_quote_version: 1,
            accounting_active: true,
            network_domain: network.to_string(),
            chain_point: Hash64::from_u64_word(8).to_string(),
            ..Default::default()
        };
        check_native_quote_v1(&quote, network).unwrap();
        for n in 0..4 {
            let mut bad = quote.clone();
            match n {
                0 => bad.available = false,
                1 => bad.accounting_quote_version = 0,
                2 => bad.network_domain = Hash64::from_u64_word(10).to_string(),
                _ => bad.chain_point = "malformed".into(),
            }
            assert!(check_native_quote_v1(&bad, network).is_err());
        }
        let inactive = GetPalwFreePromptPriceResponse { accounting_active: false, ..quote };
        check_native_quote_v1(&inactive, network).unwrap();
        assert!(!inactive.accounting_active, "absence of native activation never becomes explicit carriage");
    }
    #[test]
    fn rail_quote_requires_same_snapshot_and_explicit_native_clock() {
        let price =
            GetPalwFreePromptPriceResponse { chain_point: Hash64::from_u64_word(1).to_string(), daa_score: 7, ..Default::default() };
        let preclaim = kaspa_rpc_core::GetPalwStagedPreclaimQuoteResponse {
            snapshot_block: price.chain_point.clone(),
            quote_daa: Some(7),
            ..Default::default()
        };
        check_preclaim_price_context_v1(&preclaim, &price).unwrap();
        for clock in [None, Some(6), Some(8)] {
            let other = kaspa_rpc_core::GetPalwStagedPreclaimQuoteResponse { quote_daa: clock, ..preclaim.clone() };
            assert!(check_preclaim_price_context_v1(&other, &price).is_err());
        }
        let other =
            kaspa_rpc_core::GetPalwStagedPreclaimQuoteResponse { snapshot_block: Hash64::from_u64_word(2).to_string(), ..preclaim };
        assert!(check_preclaim_price_context_v1(&other, &price).is_err());
    }
}
