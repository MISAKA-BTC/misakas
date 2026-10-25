//! Reject an unrepresentable old parent-level vector before its conversion.
//! Serialize is unchanged: the compatibility field remains Vec<Vec<BlockHash>>.
use kaspa_consensus_core::{BlockHash, errors::header::CompressedParentsError};
use serde::{Deserialize, Deserializer, de::Error};

pub(super) fn deserialize_parent_levels<'de, D>(deserializer: D) -> Result<Vec<Vec<BlockHash>>, D::Error>
where
    D: Deserializer<'de>,
{
    let parents = Vec::<Vec<BlockHash>>::deserialize(deserializer)?;
    if parents.len() > usize::from(u8::MAX) {
        return Err(D::Error::custom(CompressedParentsError::LevelsExceeded));
    }
    Ok(parents)
}
