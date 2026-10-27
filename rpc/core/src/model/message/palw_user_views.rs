//! **PALW user views (tooling lane; ops 225–228): one versioned JSON document per question a user asks** — a claim's whole standing
//! (`getPalwClaimDetail`), the court around a claim or session (`getPalwCourtState`), every PALW fence with its height and whether it
//! is armed (`getPalwFenceSchedule`), one bond's registry row, reservations and locks (`getPalwBondDetail`).
//!
//! Read-only and node-local: the documents are built by `kaspa_consensus_core::palw_user_views_v1` from the SAME functions the fold
//! and the older reads use; no rule reads them. Appended in the free 225–228 slots: a node built before them drops the WebSocket on
//! the op (wRPC has no unknown-op reply), so a client feature-detects (`misaka palw` falls back to the older reads).
//!
//! Every response has one shape: `available` (false off ConsensusV2 / with no PALW tip), `found` (the named object is held),
//! `now_daa` (the DAA the document is judged at) and `view_json` (the document, empty when not found).

use super::*;

macro_rules! palw_user_view_response {
    ($name:ident, $what:literal) => {
        #[doc = concat!("The `", $what, "` answer: availability, whether the object was found, the judging DAA and the JSON document.")]
        #[derive(Clone, Debug, Default, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            pub available: bool,
            pub found: bool,
            pub now_daa: u64,
            pub view_json: String,
        }

        impl Serializer for $name {
            fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
                store!(u16, &1, writer)?;
                store!(bool, &self.available, writer)?;
                store!(bool, &self.found, writer)?;
                store!(u64, &self.now_daa, writer)?;
                store!(String, &self.view_json, writer)?;
                Ok(())
            }
        }

        impl Deserializer for $name {
            fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
                let version = load!(u16, reader)?;
                if version != 1 {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, concat!("unknown ", $what, " response version")));
                }
                let available = load!(bool, reader)?;
                let found = load!(bool, reader)?;
                let now_daa = load!(u64, reader)?;
                let view_json = load!(String, reader)?;
                Ok(Self { available, found, now_daa, view_json })
            }
        }
    };
}

palw_user_view_response!(GetPalwClaimDetailResponse, "getPalwClaimDetail");
palw_user_view_response!(GetPalwCourtStateResponse, "getPalwCourtState");
palw_user_view_response!(GetPalwFenceScheduleResponse, "getPalwFenceSchedule");
palw_user_view_response!(GetPalwBondDetailResponse, "getPalwBondDetail");

/// `claim_hex`: the claim id, 128 hex digits.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwClaimDetailRequest {
    pub claim_hex: String,
}

impl Serializer for GetPalwClaimDetailRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwClaimDetailRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown getPalwClaimDetail request version"));
        }
        Ok(Self { claim_hex: load!(String, reader)? })
    }
}

/// Exactly one of `claim_hex` (every session of a claim) or `session_hex` (one court session), 128 hex digits.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwCourtStateRequest {
    pub claim_hex: String,
    pub session_hex: String,
}

impl Serializer for GetPalwCourtStateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.claim_hex, writer)?;
        store!(String, &self.session_hex, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwCourtStateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown getPalwCourtState request version"));
        }
        let claim_hex = load!(String, reader)?;
        let session_hex = load!(String, reader)?;
        Ok(Self { claim_hex, session_hex })
    }
}

/// **Op 227's view multiplexer.** `view` empty (or `fence-schedule`): every top-level PALW fence of the node's ruleset — the original
/// answer, and on the wire the original version-1 request byte for byte. Lane UX-A (2026-10-27, coordinator: no new op numbers) adds
/// view kinds, sent as a version-2 request with `view` and `key`:
///
/// | `view` | `key` |
/// |---|---|
/// | `claim-lifecycle` | a chain or kernel-route claim id (128 hex): Final / SecurityCertified / held / void, typed codes, Gate C k/m |
/// | `model-tier` | a class id, or empty for every class: tier, source, `c`, `κ` |
/// | `arming` | empty: G14 route families armed now, Gate C, pinned records and waivers |
/// | `bond-budget` | `<txid>:<index>`: caps, window, open claims (design op 250's reader) |
/// | `claim-budget` | a claim id: reservation, consumption, `reuse_not_before` (design 251) |
/// | `model-allocation` | a model id, or empty for every row: `S_m`, `A_m`, `ΣA`, available, reserved, tier (design 252) |
/// | `capital-assignment` | `<txid>:<index>`: effective / pending assignments (design 253) |
///
/// A node built before the view kinds refuses a version-2 request (an error, never a fence schedule mistaken for the view).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwFenceScheduleRequest {
    #[serde(default)]
    pub view: String,
    #[serde(default)]
    pub key: String,
}

impl Serializer for GetPalwFenceScheduleRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.view.is_empty() && self.key.is_empty() {
            store!(u16, &1, writer)?;
            return Ok(());
        }
        store!(u16, &2, writer)?;
        store!(String, &self.view, writer)?;
        store!(String, &self.key, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwFenceScheduleRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        match load!(u16, reader)? {
            1 => Ok(Self::default()),
            2 => {
                let view = load!(String, reader)?;
                let key = load!(String, reader)?;
                Ok(Self { view, key })
            }
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown getPalwFenceSchedule request version")),
        }
    }
}

/// `bond`: the bond outpoint, `<txid>:<index>`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPalwBondDetailRequest {
    pub bond: String,
}

impl Serializer for GetPalwBondDetailRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.bond, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPalwBondDetailRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        if version != 1 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown getPalwBondDetail request version"));
        }
        Ok(Self { bond: load!(String, reader)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T: Serializer + Deserializer>(value: &T) -> T {
        let mut bytes = Vec::new();
        value.serialize(&mut bytes).unwrap();
        T::deserialize(&mut bytes.as_slice()).unwrap()
    }

    #[test]
    fn user_view_messages_round_trip_on_the_borsh_wire() {
        let req = round_trip(&GetPalwCourtStateRequest { claim_hex: "ab".repeat(64), session_hex: String::new() });
        assert_eq!(req.claim_hex, "ab".repeat(64));
        assert!(req.session_hex.is_empty());
        let bond = round_trip(&GetPalwBondDetailRequest { bond: "00:1".into() });
        assert_eq!(bond.bond, "00:1");
        let plain = round_trip(&GetPalwFenceScheduleRequest::default());
        assert!(plain.view.is_empty() && plain.key.is_empty());
        let mut v1 = Vec::new();
        Serializer::serialize(&GetPalwFenceScheduleRequest::default(), &mut v1).unwrap();
        assert_eq!(v1, 1u16.to_le_bytes().to_vec(), "the plain fence-schedule request stays the version-1 bytes");
        let view = round_trip(&GetPalwFenceScheduleRequest { view: "claim-lifecycle".into(), key: "ef".repeat(64) });
        assert_eq!((view.view.as_str(), view.key.len()), ("claim-lifecycle", 128));
        let all = round_trip(&GetPalwFenceScheduleRequest { view: "model-allocation".into(), key: String::new() });
        assert_eq!(all.view, "model-allocation");
        let claim = round_trip(&GetPalwClaimDetailRequest { claim_hex: "cd".repeat(64) });
        assert_eq!(claim.claim_hex.len(), 128);
        let resp = round_trip(&GetPalwClaimDetailResponse { available: true, found: true, now_daa: 7, view_json: "{}".into() });
        assert!(resp.available && resp.found && resp.now_daa == 7 && resp.view_json == "{}");
    }

    #[test]
    fn an_unknown_response_version_is_refused_not_misread() {
        let mut bytes = Vec::new();
        Serializer::serialize(&GetPalwBondDetailResponse::default(), &mut bytes).unwrap();
        bytes[0] = 2;
        assert!(<GetPalwBondDetailResponse as Deserializer>::deserialize(&mut bytes.as_slice()).is_err());
    }
}
