//! **The rights decision** (RFC-0002 §II.10.1, `D_rights`): permission for the proposed download, redistribution/serving and on-chain
//! use. "A card's license field is evidence to review, not an automatic permission decision; unknown terms remain
//! `RIGHTS_UNCONFIRMED`." A census therefore names the policy its rights column was decided under:
//!
//! * [`RightsPolicy::None`] — **the default and the headline's**: no registrant has established permission for any repository, so
//!   nothing is confirmed. `D_rights` is empty and the strict view stops every repository at `source`.
//! * [`RightsPolicy::PermissiveCardV0`] — **a proposal, not adopted**: a repository whose card license, and every resolved base's, is
//!   one of a short list of licenses that grant use, modification and redistribution without field-of-use restrictions. It exists so the
//!   lead can see what such a decision would change; a report computed under it says so on every page.

use super::listing::ListingV1;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightsPolicy {
    None,
    PermissiveCardV0,
}

impl RightsPolicy {
    pub fn parse(s: &str) -> Option<RightsPolicy> {
        match s {
            "none" => Some(RightsPolicy::None),
            "permissive-card-v0" => Some(RightsPolicy::PermissiveCardV0),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            RightsPolicy::None => "none",
            RightsPolicy::PermissiveCardV0 => "permissive-card-v0",
        }
    }
}

/// The licenses `permissive-card-v0` would accept (Hub license ids).
pub const PERMISSIVE_CARD_V0: &[&str] = &[
    "apache-2.0",
    "mit",
    "bsd",
    "bsd-2-clause",
    "bsd-3-clause",
    "bsd-3-clause-clear",
    "isc",
    "zlib",
    "unlicense",
    "cc0-1.0",
    "cc-by-4.0",
    "cc-by-3.0",
    "cc-by-2.0",
    "bsl-1.0",
    "postgresql",
    "ncsa",
    "wtfpl",
    "artistic-2.0",
    "pddl",
    "odc-by",
    "cdla-permissive-2.0",
    "cdla-permissive-1.0",
    "mpl-2.0",
    "ecl-2.0",
    "afl-3.0",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RightsV1 {
    pub policy: String,
    pub confirmed: bool,
    /// The card's license (evidence, not a decision).
    pub license: Option<String>,
    pub license_name: Option<String>,
    pub why: String,
}

pub fn rights_of(l: &ListingV1, policy: RightsPolicy) -> RightsV1 {
    let lic = l.license.clone().map(|x| x.to_ascii_lowercase());
    let (confirmed, why) = match policy {
        RightsPolicy::None => {
            (false, "policy `none`: no registrant has established permission; a card license is evidence, not a decision".to_string())
        }
        RightsPolicy::PermissiveCardV0 => {
            let own = lic.as_deref().is_some_and(|x| PERMISSIVE_CARD_V0.contains(&x));
            let bases_ok = l.base_resolved.iter().all(|b| {
                b.found
                    && b.license.as_deref().map(|x| x.to_ascii_lowercase()).is_some_and(|x| PERMISSIVE_CARD_V0.contains(&x.as_str()))
            }) && (l.base_ids.is_empty() || !l.base_resolved.is_empty());
            match (own, bases_ok) {
                (true, true) => {
                    (true, format!("permissive-card-v0 (PROPOSED, not adopted): license {}", lic.clone().unwrap_or_default()))
                }
                (false, _) => (false, format!("permissive-card-v0: license {:?} is not on the list", lic)),
                (true, false) => (false, "permissive-card-v0: a base's license is unknown or not on the list".to_string()),
            }
        }
    };
    RightsV1 { policy: policy.name().into(), confirmed, license: lic, license_name: l.license_name.clone(), why }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_policy_confirms_nothing_and_the_proposal_needs_every_base() {
        let mut l = ListingV1 { license: Some("Apache-2.0".into()), ..Default::default() };
        assert!(!rights_of(&l, RightsPolicy::None).confirmed);
        assert!(rights_of(&l, RightsPolicy::PermissiveCardV0).confirmed);
        l.base_ids = vec!["b/x".into()];
        assert!(!rights_of(&l, RightsPolicy::PermissiveCardV0).confirmed, "an unresolved base's license is unknown");
        l.base_resolved = vec![super::super::listing::BaseResolvedV1 {
            id: "b/x".into(),
            found: true,
            license: Some("llama3".into()),
            renamed_from: None,
            ..Default::default()
        }];
        assert!(!rights_of(&l, RightsPolicy::PermissiveCardV0).confirmed);
        l.license = Some("other".into());
        assert!(!rights_of(&l, RightsPolicy::PermissiveCardV0).confirmed);
    }
}
