//! Owner/response decisions from authenticated prospective source snapshots. A missing
//! local operand is pending; a late native collection is the only silence decision.
use kaspa_consensus_core::{
    Hash64,
    palw_selected_event_source_v1::{
        PALW_SELECTED_EVENT_MAX_REQUESTS_V1, PalwSelectedEventCoordinateV1, PalwSelectedEventDemandV1, PalwSelectedEventSessionV1,
    },
    palw_state_v2::PalwBondKeyV2,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SelectedEventScheduleV1 {
    Demand(PalwSelectedEventDemandV1),
    Collect { coordinate: PalwSelectedEventCoordinateV1, demand_id: Hash64 },
    Ready,
    Wait,
}

pub(super) fn request_v1(
    session: &PalwSelectedEventSessionV1,
    coordinates: &[PalwSelectedEventCoordinateV1],
    now: u64,
) -> Result<SelectedEventScheduleV1, String> {
    if coordinates.is_empty()
        || coordinates.len() > PALW_SELECTED_EVENT_MAX_REQUESTS_V1
        || coordinates.iter().enumerate().any(|(i, c)| coordinates[..i].contains(c))
        || session.requests.keys().any(|c| !coordinates.contains(c))
    {
        return Err("selected actor changes the bounded original nominee coordinates".into());
    }
    if now < session.reserved_daa || now >= session.collect_until_daa {
        return Err("selected actor is outside original paid acquisition/collection horizon".into());
    }
    if let Some(coordinate) = coordinates.iter().find(|c| !session.requests.contains_key(c)) {
        if now >= session.respond_until_daa {
            return Err("selected original demand never landed before its immutable response deadline".into());
        }
        return Ok(SelectedEventScheduleV1::Demand(PalwSelectedEventDemandV1 { coordinate: *coordinate }));
    }
    if now >= session.respond_until_daa {
        for coordinate in coordinates {
            let request = &session.requests[coordinate];
            if !request.collected && request.answered != session.custodians {
                return Ok(SelectedEventScheduleV1::Collect { coordinate: *coordinate, demand_id: request.demand_id });
            }
        }
    }
    if coordinates.iter().any(|c| {
        let request = &session.requests[c];
        request.event.is_none() || (!request.collected && request.answered != session.custodians)
    }) {
        return Ok(SelectedEventScheduleV1::Wait);
    }
    Ok(SelectedEventScheduleV1::Ready)
}

pub(super) fn response_v1(
    session: &PalwSelectedEventSessionV1,
    mine: PalwBondKeyV2,
    now: u64,
) -> Result<Option<(PalwSelectedEventCoordinateV1, Hash64)>, String> {
    if !session.custodians.contains(&mine) {
        return Err("selected source has no literal signed custody for this key".into());
    }
    if now < session.reserved_daa || now >= session.respond_until_daa {
        return Ok(None);
    }
    Ok(session
        .requests
        .values()
        .find(|request| !request.collected && !request.answered.contains(&mine))
        .map(|request| (request.coordinate, request.demand_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{palw_selected_event_source_v1::PalwSelectedEventRequestV1, tx::TransactionOutpoint};
    use std::collections::BTreeSet;
    fn session() -> PalwSelectedEventSessionV1 {
        PalwSelectedEventSessionV1::new_v1(
            Hash64::from_u64_word(1),
            PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(9), 0)),
            2,
            Hash64::from_u64_word(3),
            10,
            20,
            BTreeSet::from([PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(4), 0))]),
        )
        .unwrap()
    }
    #[test]
    fn selected_actor_does_not_treat_submission_or_silence_as_ready() {
        let mut snapshot = session();
        let c = PalwSelectedEventCoordinateV1 { row: 2, tile: 1023 };
        assert_eq!(
            request_v1(&snapshot, &[c], 5).unwrap(),
            SelectedEventScheduleV1::Demand(PalwSelectedEventDemandV1 { coordinate: c })
        );
        snapshot.requests.insert(
            c,
            PalwSelectedEventRequestV1 {
                coordinate: c,
                demand_id: Hash64::from_u64_word(6),
                answered: BTreeSet::new(),
                event: None,
                collected: false,
            },
        );
        assert_eq!(request_v1(&snapshot, &[c], 9).unwrap(), SelectedEventScheduleV1::Wait);
        assert_eq!(
            request_v1(&snapshot, &[c], 10).unwrap(),
            SelectedEventScheduleV1::Collect { coordinate: c, demand_id: Hash64::from_u64_word(6) }
        );
        snapshot.requests.get_mut(&c).unwrap().collected = true;
        assert_eq!(request_v1(&snapshot, &[c], 11).unwrap(), SelectedEventScheduleV1::Wait);
        assert!(request_v1(&snapshot, &[c], 20).is_err());
        assert!(request_v1(&snapshot, &[PalwSelectedEventCoordinateV1 { tile: 255, ..c }], 9).is_err());
    }
    #[test]
    fn response_selection_requires_actual_signed_custody_and_original_clock() {
        let mut snapshot = session();
        let mine = *snapshot.custodians.first().unwrap();
        let c = PalwSelectedEventCoordinateV1 { row: 0, tile: 1023 };
        snapshot.requests.insert(
            c,
            PalwSelectedEventRequestV1 {
                coordinate: c,
                demand_id: Hash64::from_u64_word(6),
                answered: BTreeSet::new(),
                event: None,
                collected: false,
            },
        );
        assert_eq!(response_v1(&snapshot, mine, 9).unwrap(), Some((c, Hash64::from_u64_word(6))));
        assert_eq!(response_v1(&snapshot, mine, 10).unwrap(), None);
        assert!(response_v1(&snapshot, snapshot.owner, 9).is_err());
    }
}
