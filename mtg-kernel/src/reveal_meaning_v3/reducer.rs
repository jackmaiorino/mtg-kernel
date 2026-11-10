//! A transactional reducer of the same viewer-local transcript Spellbench
//! validates. It does not read GameState or native copy bindings.
use crate::{
    card_def::{card_id_by_name, CARD_DEFS},
    ids::PlayerId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, &'static str>;
const MAX_SAFE: u64 = (1 << 53) - 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CountV3 {
    arrivals: usize,
    present: usize,
}

impl CountV3 {
    fn minimum(&self) -> usize {
        self.arrivals.max(self.present)
    }
}

/// State at an operation boundary. A link is only usable in its own
/// operation, so settled arrivals need counts but no tokens or copy IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct RevealKnowledgeV3([BTreeMap<u16, CountV3>; 2]);

impl RevealKnowledgeV3 {
    pub(crate) fn hand(&self, owner: PlayerId) -> impl Iterator<Item = (u16, usize)> + '_ {
        self.0[owner.index()]
            .iter()
            .map(|(&name, count)| (name, count.minimum()))
    }

    pub(super) fn only_opponent(mut self, viewer: PlayerId) -> Self {
        self.0[viewer.index()].clear();
        self
    }

    pub(super) fn arrive(&mut self, owner: PlayerId, name: u16) {
        self.0[owner.index()].entry(name).or_default().arrivals += 1;
    }

    pub(super) fn present(&mut self, owner: PlayerId, name: u16) {
        self.present_copies(owner, name, 1);
    }

    pub(crate) fn present_copies(&mut self, owner: PlayerId, name: u16, copies: usize) {
        let count = self.0[owner.index()].entry(name).or_default();
        count.present = count.present.max(copies);
    }

    pub(crate) fn depart(&mut self, owner: PlayerId, name: Option<u16>, count: usize) {
        self.0[owner.index()].retain(|&printing, current| {
            if name.is_none_or(|named| named == printing) {
                *current = CountV3 {
                    arrivals: 0,
                    present: current.minimum().saturating_sub(count),
                };
            }
            current.minimum() > 0
        });
    }
}

#[derive(Clone, Debug, Default)]
pub struct RevealReducerV3 {
    seen: BTreeMap<u64, Value>,
    knowledge: RevealKnowledgeV3,
    possible_eventless_moves: [BTreeSet<u16>; 2],
    next: u64,
}

fn fields<'a>(value: &'a Value, keys: &[&str]) -> Result<&'a Map<String, Value>> {
    let map = value.as_object().ok_or("expected an object")?;
    if map.len() != keys.len() || keys.iter().any(|key| !map.contains_key(*key)) {
        return Err("unexpected reveal transcript fields");
    }
    Ok(map)
}

fn integer(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .filter(|&i| i <= MAX_SAFE)
        .ok_or("invalid reveal counter")
}

fn owner(value: &Value) -> Result<PlayerId> {
    match value.as_str() {
        Some("p0") => Ok(PlayerId::P0),
        Some("p1") => Ok(PlayerId::P1),
        _ => Err("invalid public owner seat"),
    }
}

fn card(value: &Value) -> Result<u16> {
    let name = value.as_str().ok_or("expected a revealed printing name")?;
    card_id_by_name(name)
        .filter(|&id| CARD_DEFS[usize::from(id)].name == name)
        .ok_or("unrecognized public printing name")
}

impl RevealReducerV3 {
    /// Conservative name minima in one seat's hand. Keys are exact public
    /// registry printings and carry no physical copy authority.
    pub fn hand(&self, owner: PlayerId) -> BTreeMap<u16, usize> {
        self.knowledge.hand(owner).collect()
    }

    pub(crate) fn opponent_knowledge(self, viewer: PlayerId) -> RevealKnowledgeV3 {
        self.knowledge.only_opponent(viewer)
    }

    pub(crate) fn possible_eventless_moves(&self, owner: PlayerId) -> Vec<u16> {
        self.possible_eventless_moves[owner.index()]
            .iter()
            .copied()
            .collect()
    }

    /// Validate and absorb one prefix or delta, atomically. Exact re-delivery
    /// is idempotent and only a current operation's arrival can bind a reveal.
    pub fn absorb(&mut self, transcript: &Value) -> Result<()> {
        let stream = fields(transcript, &["schema", "operations"])?;
        if stream["schema"] != "x_reveal_meaning_v3" {
            return Err("unsupported reveal transcript");
        }
        let operations = stream["operations"]
            .as_array()
            .ok_or("operations are not an array")?;
        let mut next = self.clone();
        for operation in operations {
            let op = fields(operation, &["sequence", "events"])?;
            let sequence = integer(&op["sequence"])?;
            let events = op["events"].as_array().ok_or("events are not an array")?;
            if let Some(seen) = next.seen.get(&sequence) {
                if seen != operation {
                    return Err("changed duplicate reveal operation");
                }
                continue;
            }
            if sequence != next.next {
                return Err("noncontiguous reveal operation sequence");
            }
            next.operation(sequence, events)?;
            next.seen.insert(sequence, operation.clone());
            next.next += 1;
        }
        *self = next;
        Ok(())
    }

    fn operation(&mut self, sequence: u64, events: &[Value]) -> Result<()> {
        let mut eligible = BTreeMap::<String, (PlayerId, Option<u16>)>::new();
        let mut unresolved = BTreeSet::new();
        for (index, event) in events.iter().enumerate() {
            let seat = owner(&event["owner_seat"])?;
            match event["kind"].as_str() {
                Some("hand_arrival") => {
                    let row = fields(event, &["kind", "owner_seat", "card_name", "evidence"])?;
                    let evidence = &row["evidence"];
                    let name = match evidence["kind"].as_str() {
                        Some("public_transfer") => {
                            fields(evidence, &["kind", "from_zone"])?;
                            if !matches!(
                                evidence["from_zone"].as_str(),
                                Some("battlefield" | "graveyard" | "stack" | "exile" | "command")
                            ) {
                                return Err("transfer source is not continuously public");
                            }
                            let name = card(&row["card_name"])?;
                            // A stack return may be an aborted cast without
                            // a committed movement in the engine replay.
                            if evidence["from_zone"] == "stack" {
                                self.possible_eventless_moves[seat.index()].insert(name);
                            }
                            Some(name)
                        }
                        Some("search_reveal") => {
                            fields(evidence, &["kind"])?;
                            if !row["card_name"].is_null() {
                                return Err("search arrival must bind its reveal");
                            }
                            None
                        }
                        _ => return Err("unsupported arrival evidence"),
                    };
                    let token = format!("arrival:{sequence}:{index}");
                    if let Some(id) = name {
                        self.knowledge.arrive(seat, id);
                    } else {
                        unresolved.insert(token.clone());
                    }
                    eligible.insert(token, (seat, name));
                }
                Some("card_revealed") => {
                    let name = card(&event["card_name"])?;
                    match event["meaning"].as_str() {
                        Some("present") => {
                            fields(event, &["kind", "owner_seat", "card_name", "meaning"])?;
                            self.knowledge.present(seat, name);
                            self.possible_eventless_moves[seat.index()].insert(name);
                        }
                        Some("moved") => {
                            fields(
                                event,
                                &["kind", "owner_seat", "card_name", "meaning", "arrival"],
                            )?;
                            let token = event["arrival"].as_str().ok_or("invalid arrival link")?;
                            let arrival = eligible
                                .get_mut(token)
                                .ok_or("missing or expired arrival link")?;
                            if arrival.0 != seat || arrival.1.is_some_and(|known| known != name) {
                                return Err("arrival owner or name changed");
                            }
                            if arrival.1.is_none() {
                                arrival.1 = Some(name);
                                self.knowledge.arrive(seat, name);
                                unresolved.remove(token);
                            }
                        }
                        _ => return Err("unknown reveal meaning"),
                    }
                }
                Some("hand_departure") => {
                    let row = fields(event, &["kind", "owner_seat", "card_name", "count"])?;
                    let count = integer(&row["count"])?;
                    if count == 0 {
                        return Err("departure must move at least one card");
                    }
                    let count = usize::try_from(count).unwrap_or(usize::MAX);
                    let named = if row["card_name"].is_null() {
                        None
                    } else {
                        Some(card(&row["card_name"])?)
                    };
                    self.knowledge.depart(seat, named, count);
                    eligible.retain(|_, (owner, name)| {
                        *owner != seat || matches!((named, *name), (Some(a), Some(b)) if a != b)
                    });
                }
                Some("library_shuffled" | "public_correspondence_lost") => {
                    fields(event, &["kind", "owner_seat"])?;
                    eligible.retain(|_, (owner, _)| *owner != seat);
                }
                _ => return Err("unknown public reveal event"),
            }
        }
        if !unresolved.is_empty() {
            return Err("search outcome lacks its same-operation reveal");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stream(ops: Vec<Value>) -> Value {
        json!({"schema":"x_reveal_meaning_v3","operations":ops})
    }

    fn operation(sequence: u64, events: Vec<Value>) -> Value {
        json!({"sequence":sequence,"events":events})
    }

    fn arrival(name: &str) -> Value {
        json!({"kind":"hand_arrival","owner_seat":"p1","card_name":name,
            "evidence":{"kind":"public_transfer","from_zone":"battlefield"}})
    }

    fn reveal(name: &str, token: Option<&str>) -> Value {
        let mut event = json!({"kind":"card_revealed","owner_seat":"p1","card_name":name,
            "meaning": if token.is_some() {"moved"} else {"present"}});
        if let Some(token) = token {
            event["arrival"] = json!(token);
        }
        event
    }

    #[test]
    fn reveal_meaning_v3_reducer_overlap_departures_and_exact_replay() {
        let swamp = card_id_by_name("Swamp").unwrap();
        let forest = card_id_by_name("Forest").unwrap();
        let mut reducer = RevealReducerV3::default();
        let first = stream(vec![
            operation(0, vec![reveal("Swamp", None), reveal("Swamp", None)]),
            operation(
                1,
                vec![arrival("Swamp"), reveal("Swamp", Some("arrival:1:0"))],
            ),
        ]);
        reducer.absorb(&first).unwrap();
        reducer.absorb(&first).unwrap();
        assert_eq!(reducer.hand(PlayerId::P1), BTreeMap::from([(swamp, 1)]));
        let two = operation(
            2,
            vec![arrival("Swamp"), reveal("Swamp", Some("arrival:2:0"))],
        );
        let forest_search = operation(
            3,
            vec![
                json!({"kind":"hand_arrival","owner_seat":"p1",
            "card_name":null,"evidence":{"kind":"search_reveal"}}),
                reveal("Forest", Some("arrival:3:0")),
            ],
        );
        reducer.absorb(&stream(vec![two, forest_search])).unwrap();
        assert_eq!(
            reducer.hand(PlayerId::P1),
            BTreeMap::from([(swamp, 2), (forest, 1)])
        );
        reducer
            .absorb(&stream(vec![operation(
                4,
                vec![json!({"kind":"hand_departure",
            "owner_seat":"p1","card_name":"Forest","count":1})],
            )]))
            .unwrap();
        assert_eq!(reducer.hand(PlayerId::P1), BTreeMap::from([(swamp, 2)]));
        reducer
            .absorb(&stream(vec![operation(
                5,
                vec![json!({"kind":"hand_departure",
            "owner_seat":"p1","card_name":null,"count":1})],
            )]))
            .unwrap();
        assert_eq!(reducer.hand(PlayerId::P1), BTreeMap::from([(swamp, 1)]));
        assert!(reducer.hand(PlayerId::P0).is_empty());
    }

    #[test]
    fn reveal_meaning_v3_only_unlinked_and_stack_arrivals_can_hide_eventless_moves() {
        let mut stack = arrival("Island");
        stack["evidence"]["from_zone"] = json!("stack");
        let transcript = stream(vec![operation(
            0,
            vec![
                arrival("Forest"),
                reveal("Forest", Some("arrival:0:0")),
                stack,
                reveal("Island", Some("arrival:0:2")),
                reveal("Swamp", None),
            ],
        )]);
        let mut reducer = RevealReducerV3::default();
        reducer.absorb(&transcript).unwrap();
        let definitions: BTreeSet<_> = reducer
            .possible_eventless_moves(PlayerId::P1)
            .into_iter()
            .collect();
        assert_eq!(
            definitions,
            BTreeSet::from([
                card_id_by_name("Island").unwrap(),
                card_id_by_name("Swamp").unwrap(),
            ])
        );
        assert!(reducer.possible_eventless_moves(PlayerId::P0).is_empty());
    }

    #[test]
    fn reveal_meaning_v3_reducer_refuses_stale_or_forged_links_atomically() {
        let mut reducer = RevealReducerV3::default();
        let valid = operation(
            0,
            vec![arrival("Swamp"), reveal("Swamp", Some("arrival:0:0"))],
        );
        let delayed = operation(1, vec![reveal("Swamp", Some("arrival:0:0"))]);
        assert!(reducer
            .absorb(&stream(vec![valid.clone(), delayed]))
            .is_err());
        assert!(reducer.hand(PlayerId::P1).is_empty());
        reducer.absorb(&stream(vec![valid])).unwrap();
        assert!(reducer
            .absorb(&stream(vec![operation(0, vec![reveal("Forest", None)])]))
            .is_err());
        let shuffled = operation(
            1,
            vec![
                arrival("Forest"),
                json!({"kind":"library_shuffled",
            "owner_seat":"p1"}),
                reveal("Forest", Some("arrival:1:0")),
            ],
        );
        assert!(reducer.absorb(&stream(vec![shuffled])).is_err());
        let mut malformed = reveal("Forest", None);
        malformed["arrival"] = Value::Null;
        assert!(reducer
            .absorb(&stream(vec![operation(1, vec![malformed])]))
            .is_err());
        reducer
            .absorb(&stream(vec![operation(1, vec![reveal("Forest", None)])]))
            .unwrap();
        assert_eq!(reducer.hand(PlayerId::P1).len(), 2);
    }
}
