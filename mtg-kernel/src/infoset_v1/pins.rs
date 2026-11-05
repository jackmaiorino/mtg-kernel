//! Joint pinned allocation (W1 design 6.3): the canonical zone of every
//! opponent-owned pinned object in a hidden zone, jointly feasible under the
//! C1 transfer inventory and the hand and library capacities.

use super::projection::{TransferDirV1, TransferV1};
use crate::state::Zone;

/// One untracked pinned object of the owner, in contract order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PinV1 {
    /// The hidden zone V last saw it in (hand or library).
    pub last_zone: Zone,
    /// The stream item from which transfers may have carried it.
    pub since: usize,
}

/// More pins than this refuse (R7) rather than search assignments.
pub(crate) const MAX_PINS_V1: usize = 12;

/// Capacities for C2 and C3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PinCapacityV1 {
    /// Unknown hand slots of the owner.
    pub hand_slots: usize,
    /// Step-5 occupants the survivor tree requires (computed with every
    /// pinned definition removed from Pool5: pins never count toward nodes).
    pub required_hand: usize,
    /// Unknown library slots of the owner.
    pub library_slots: usize,
}

fn usable(pin: &PinV1, zone: Zone, t: &TransferV1) -> bool {
    t.item >= pin.since
        && matches!(
            (pin.last_zone, zone, t.dir),
            (Zone::Library, Zone::Hand, TransferDirV1::Draw)
                | (Zone::Hand, Zone::Library, TransferDirV1::PutBack)
        )
}

/// C1: every moved pin is matched to a distinct usable transfer (Hall's
/// condition, checked by augmenting paths).
fn transfers_match(pins: &[PinV1], zones: &[Zone], transfers: &[TransferV1]) -> bool {
    let moved: Vec<usize> = (0..pins.len())
        .filter(|&i| zones[i] != pins[i].last_zone)
        .collect();
    let mut owner_of: Vec<Option<usize>> = vec![None; transfers.len()];
    fn augment(
        p: usize,
        pins: &[PinV1],
        zones: &[Zone],
        transfers: &[TransferV1],
        seen: &mut [bool],
        owner_of: &mut [Option<usize>],
    ) -> bool {
        for (t, transfer) in transfers.iter().enumerate() {
            if seen[t] || !usable(&pins[p], zones[p], transfer) {
                continue;
            }
            seen[t] = true;
            if owner_of[t].is_none()
                || augment(
                    owner_of[t].expect("checked"),
                    pins,
                    zones,
                    transfers,
                    seen,
                    owner_of,
                )
            {
                owner_of[t] = Some(p);
                return true;
            }
        }
        false
    }
    moved.into_iter().all(|p| {
        let mut seen = vec![false; transfers.len()];
        augment(p, pins, zones, transfers, &mut seen, &mut owner_of)
    })
}

fn feasible(pins: &[PinV1], zones: &[Zone], transfers: &[TransferV1], cap: PinCapacityV1) -> bool {
    let in_hand = zones.iter().filter(|z| **z == Zone::Hand).count();
    let in_library = zones.len() - in_hand;
    in_hand + cap.required_hand <= cap.hand_slots
        && in_library <= cap.library_slots
        && transfers_match(pins, zones, transfers)
}

fn other(zone: Zone) -> Zone {
    if zone == Zone::Hand {
        Zone::Library
    } else {
        Zone::Hand
    }
}

fn completes(
    pins: &[PinV1],
    fixed: &mut Vec<Zone>,
    transfers: &[TransferV1],
    cap: PinCapacityV1,
) -> bool {
    if fixed.len() == pins.len() {
        return feasible(pins, fixed, transfers, cap);
    }
    let pin = pins[fixed.len()];
    for zone in [pin.last_zone, other(pin.last_zone)] {
        fixed.push(zone);
        let ok = completes(pins, fixed, transfers, cap);
        fixed.pop();
        if ok {
            return true;
        }
    }
    false
}

/// The canonical choice: in contract order, each pin keeps its last observed
/// zone if some feasible completion exists, otherwise it takes the other
/// zone. `None` is R7 (no feasible joint allocation).
pub(crate) fn allocate_pins(
    pins: &[PinV1],
    transfers: &[TransferV1],
    cap: PinCapacityV1,
) -> Option<Vec<Zone>> {
    if pins.len() > MAX_PINS_V1 {
        return None;
    }
    let mut fixed = Vec::with_capacity(pins.len());
    for pin in pins {
        let mut chosen = None;
        for zone in [pin.last_zone, other(pin.last_zone)] {
            fixed.push(zone);
            if completes(pins, &mut fixed, transfers, cap) {
                chosen = Some(zone);
            }
            fixed.pop();
            if chosen.is_some() {
                break;
            }
        }
        fixed.push(chosen?);
    }
    Some(fixed)
}

/// Mutant L9: v4's independent per-pin placement. A pin goes to the hand if
/// any unidentified draw since it entered the library could have carried it,
/// regardless of the other pins competing for the same draw.
pub(crate) fn allocate_pins_independent_v4(pins: &[PinV1], transfers: &[TransferV1]) -> Vec<Zone> {
    pins.iter()
        .map(|pin| {
            if pin.last_zone == Zone::Library
                && transfers.iter().any(|t| usable(pin, Zone::Hand, t))
            {
                Zone::Hand
            } else {
                pin.last_zone
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;

    fn draw(item: usize) -> TransferV1 {
        TransferV1 {
            item,
            owner: PlayerId::P1,
            dir: TransferDirV1::Draw,
        }
    }

    const CAP: PinCapacityV1 = PinCapacityV1 {
        hand_slots: 3,
        required_hand: 0,
        library_slots: 30,
    };

    #[test]
    fn w_pin_2_two_pins_one_draw_stay_in_the_library() {
        let pins = [
            PinV1 {
                last_zone: Zone::Library,
                since: 10,
            },
            PinV1 {
                last_zone: Zone::Library,
                since: 12,
            },
        ];
        let transfers = [draw(20)];
        assert_eq!(
            allocate_pins(&pins, &transfers, CAP),
            Some(vec![Zone::Library, Zone::Library])
        );
        // Both in the hand would need two draws: infeasible by C1.
        assert!(!feasible(&pins, &[Zone::Hand, Zone::Hand], &transfers, CAP));
        assert!(feasible(
            &pins,
            &[Zone::Hand, Zone::Library],
            &transfers,
            CAP
        ));
        // v4's placement puts both in the hand.
        assert_eq!(
            allocate_pins_independent_v4(&pins, &transfers),
            vec![Zone::Hand, Zone::Hand]
        );
    }

    #[test]
    fn capacity_forces_a_pin_out_of_its_last_zone_when_a_transfer_allows() {
        let pins = [PinV1 {
            last_zone: Zone::Library,
            since: 0,
        }];
        let none = PinCapacityV1 {
            library_slots: 0,
            ..CAP
        };
        assert_eq!(
            allocate_pins(&pins, &[draw(5)], none),
            Some(vec![Zone::Hand])
        );
        assert_eq!(allocate_pins(&pins, &[], none), None, "R7");
        let early = [draw(0)];
        let late_pin = [PinV1 {
            last_zone: Zone::Library,
            since: 1,
        }];
        assert_eq!(
            allocate_pins(&late_pin, &early, none),
            None,
            "a draw before the pin cannot carry it"
        );
    }
}
