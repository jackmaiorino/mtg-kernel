//! Fixed-chair JSONL draft and build stages for the human Cube pod.
//! The transport sees the human view. The caller owns the private result and
//! hands the committed decks to the play stage.

use crate::card_def::{card_id_by_visible_name, CARD_DEFS};
use crate::cube_draft_v1::{basic_land_ids, Deck, DraftRecord, DECK_CARDS, SEATS};
use crate::cube_pod_runner_v1::{build_seed_for_chair, pod_draft_seed, PodUse, CLIENT_NAMESPACE};
use crate::cube_seat_package_v1::{SeatPackage, SeatPackageKind};
use crate::cube_seat_v1::{
    canonical_deck, run_seat_draft, PickOutcome, SeatError, SeatPickView, SeatPicker,
};
use crate::human_bo3_v1::HumanVisibleStateV1;
use crate::state::SplitMix64;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Read, Write};
use std::path::Path;
use std::sync::Mutex;

pub mod play;
pub mod pod;

const MAX_REQUEST_BYTES: u64 = 16 * 1024;
const MAX_CARD_TEXT_BYTES: u64 = 16 * 1024 * 1024;
pub const DRAFT_BUILD_RESPONSE_SCHEMA: &str = "mtg-kernel-cube-client-draft-build-response/v1";

/// Operator-owned, local text. No protocol request can change the input path
/// or read arbitrary entries from this map.
pub struct LocalCardText(BTreeMap<u16, String>);

impl LocalCardText {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = crate::human_match_v2::read_bounded(path, MAX_CARD_TEXT_BYTES)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let value = crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
        let Value::Object(entries) = value else {
            return Err("Local card text must be a JSON object keyed by registry ID.".into());
        };
        let mut cards = BTreeMap::new();
        for (key, value) in entries {
            let id = key.parse::<u16>().map_err(|_| "Invalid card text ID.")?;
            let Value::String(text) = value else {
                return Err("Each local card text entry must be a string.".into());
            };
            if key != id.to_string() || usize::from(id) >= CARD_DEFS.len() {
                return Err("Invalid card text ID.".into());
            }
            cards.insert(id, text);
        }
        Ok(Self(cards))
    }

    /// Map only names from the fixed human projection to local text. The
    /// response keys are already-visible names, including visible alternate
    /// faces, so the gameplay response needs no registry IDs.
    fn render_decision<'a>(&'a self, view: &'a HumanVisibleStateV1) -> BTreeMap<&'a str, &'a str> {
        let public = &view.public;
        let public_cards = public
            .battlefield
            .iter()
            .flatten()
            .chain(public.graveyards.iter().flatten())
            .chain(&public.exile)
            .map(|card| &card.stable);
        let zone_cards = view
            .own_hand
            .iter()
            .chain(view.known_hand_cards.iter().flatten())
            .chain(
                view.known_library_cards
                    .iter()
                    .flatten()
                    .map(|known| &known.card),
            )
            .chain(
                view.extensions
                    .decision_local_library
                    .iter()
                    .flat_map(|choice| &choice.cards),
            )
            .map(|card| &card.stable);
        public_cards
            .chain(zone_cards)
            .chain(public.stack.iter().map(|item| &item.source))
            .map(|card| card.name.as_str())
            .chain(
                view.known_face_down_cards
                    .iter()
                    .map(|known| known.name.as_str()),
            )
            .filter_map(|name| {
                let (id, _) = card_id_by_visible_name(name)?;
                self.0.get(&id).map(|text| (name, text.as_str()))
            })
            .collect()
    }

    fn render(&self, ids: &[u16]) -> Vec<Value> {
        ids.iter()
            .map(|id| {
                json!({
                    "id": id,
                    "name": CARD_DEFS[usize::from(*id)].name,
                    "text": self.0.get(id),
                })
            })
            .collect()
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Pick { round: u8, pick: u8, card: u16 },
    Build { cards: Vec<u16> },
    Suggest {},
}

struct DraftTransport<I, O> {
    schema: &'static str,
    input: I,
    output: O,
    line: Vec<u8>,
}

impl<I: BufRead, O: Write> DraftTransport<I, O> {
    fn send(&mut self, event: &str, view: Value) -> Result<(), String> {
        write_json_line(
            &mut self.output,
            &json!({
                "schema": self.schema,
                "event": event,
                "view": view,
            }),
        )
        .map_err(|_| "The client response could not be written.".into())
    }

    fn command<T: DeserializeOwned>(&mut self) -> Result<Option<T>, String> {
        self.line.clear();
        let bytes = (&mut self.input)
            .take(MAX_REQUEST_BYTES + 1)
            .read_until(b'\n', &mut self.line)
            .map_err(|_| "The client request could not be read.")?;
        if bytes == 0 {
            return Err("The client disconnected.".into());
        }
        if bytes as u64 > MAX_REQUEST_BYTES {
            return Err("The client request exceeds its byte limit.".into());
        }
        let command = std::str::from_utf8(&self.line)
            .ok()
            .and_then(|line| crate::rl::parse_strict_json_value(line).ok())
            .and_then(|value| serde_json::from_value(value).ok());
        if command.is_none() {
            self.error("Invalid request.")?;
        }
        Ok(command)
    }

    fn error(&mut self, message: &str) -> Result<(), String> {
        self.send("error", json!({"message": message}))
    }
}

struct HumanPicker<'a, I, O> {
    transport: &'a mut DraftTransport<I, O>,
    card_text: &'a LocalCardText,
    journal: &'a PodJournal,
}

impl<I: BufRead, O: Write> HumanPicker<'_, I, O> {
    fn choose(&mut self, view: &SeatPickView<'_>) -> Result<PickOutcome, String> {
        self.transport.send(
            "pick",
            json!({
                "round": view.round,
                "pick": view.pick,
                "direction": view.direction,
                "pack": self.card_text.render(view.pack),
                "pool": view.pool,
                "seen_not_taken": view.seen_not_taken,
            }),
        )?;
        loop {
            match self.transport.command()? {
                Some(Command::Pick { round, pick, card }) => {
                    if round == view.round && pick == view.pick {
                        if let Some(index) =
                            view.pack.iter().position(|&candidate| candidate == card)
                        {
                            journal(
                                self.journal,
                                "human_pick",
                                json!({
                                    "round": round, "pick": pick, "card": card,
                                }),
                            )?;
                            return Ok(PickOutcome { index, row: None });
                        }
                    }
                    self.transport.error("Pick a card from the current pack.")?;
                }
                Some(Command::Suggest {}) => self.transport.error("Suggestions are disabled.")?,
                Some(Command::Build { .. }) => {
                    self.transport.error("Finish drafting before building.")?
                }
                None => {}
            }
        }
    }
}

impl<I: BufRead, O: Write> SeatPicker for HumanPicker<'_, I, O> {
    fn pick(
        &mut self,
        view: &SeatPickView<'_>,
        _: &mut SplitMix64,
    ) -> Result<PickOutcome, SeatError> {
        self.choose(view).map_err(SeatError::Seat)
    }
}

/// Serializes durable private records from the human game and bot workers.
pub struct PodJournal(Mutex<File>);

/// Server-private output. Send only the human view through the transport.
pub struct PrivateDraftBuild {
    pub draft: DraftRecord,
    pub decks: [Deck; SEATS],
    pub journal: PodJournal,
}

pub struct DraftBuildConfig<'a> {
    pub run: &'a str,
    pub pod_index: u64,
    pub human_chair: usize,
    pub journal_path: &'a Path,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperatorConfig {
    schema: String,
    seat_package: crate::expanded_deck_training_v1::PinnedFileV1,
    run: String,
    pod_index: u64,
    human_chair: usize,
    journal_path: std::path::PathBuf,
    card_text_path: std::path::PathBuf,
}

pub const DRAFT_BUILD_CONFIG_SCHEMA: &str = "mtg-kernel-cube-client-draft-build-config/v1";

/// Opens the operator's local inputs once, then serves the fixed-chair
/// draft/build phase. The same phase is used by the complete pod service.
pub fn serve_draft_build_config(
    config_path: &Path,
    input: impl BufRead,
    output: impl Write,
) -> Result<PrivateDraftBuild, String> {
    let (config, package, card_text) = prepare_operator_config(config_path)?;
    serve_draft_build(
        &package,
        DraftBuildConfig {
            run: &config.run,
            pod_index: config.pod_index,
            human_chair: config.human_chair,
            journal_path: &config.journal_path,
        },
        &card_text,
        input,
        output,
    )
}

fn prepare_operator_config(
    config_path: &Path,
) -> Result<(OperatorConfig, SeatPackage, LocalCardText), String> {
    let config_bytes = crate::human_match_v2::read_bounded(config_path, 64 * 1024)?;
    let text = std::str::from_utf8(&config_bytes).map_err(|e| e.to_string())?;
    let value = crate::rl::parse_strict_json_value(text).map_err(|e| e.to_string())?;
    let config: OperatorConfig = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if config.schema != DRAFT_BUILD_CONFIG_SCHEMA
        || !config.seat_package.path.is_absolute()
        || !config.card_text_path.is_absolute()
    {
        return Err("Invalid draft/build schema or operator file paths.".into());
    }
    let package = SeatPackage::load(&config.seat_package.path, &config.seat_package.sha256)?;
    let card_text = LocalCardText::load(&config.card_text_path)?;
    Ok((config, package, card_text))
}

pub fn serve_draft_build(
    package: &SeatPackage,
    config: DraftBuildConfig<'_>,
    card_text: &LocalCardText,
    input: impl BufRead,
    output: impl Write,
) -> Result<PrivateDraftBuild, String> {
    if config.human_chair >= SEATS || !config.journal_path.is_absolute() {
        return Err("Invalid human chair or private journal path.".into());
    }
    if package.kind() == SeatPackageKind::Release {
        return Err("Release sessions require a verified native player.".into());
    }
    let draft_seed = pod_draft_seed(CLIENT_NAMESPACE, config.run, config.pod_index)
        .map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let private = PodJournal(Mutex::new(
        options
            .open(config.journal_path)
            .map_err(|e| format!("Fresh private journal required: {e}"))?,
    ));
    journal(
        &private,
        "session",
        json!({
            "namespace": CLIENT_NAMESPACE,
            "use": PodUse::Client,
            "run": config.run,
            "pod_index": config.pod_index,
            "human_chair": config.human_chair,
            "draft_seed": draft_seed,
            "seat_package": package.kind().label(),
            "seat_package_sha256": package.manifest_sha256(),
        }),
    )?;

    let mut transport = DraftTransport {
        schema: DRAFT_BUILD_RESPONSE_SCHEMA,
        input,
        output,
        line: Vec::new(),
    };
    transport.send(
        "start",
        json!({
            "human_chair": config.human_chair,
            "seat_package": package.kind().label(),
        }),
    )?;
    let draft = {
        let mut human: Option<Box<dyn SeatPicker + '_>> = Some(Box::new(HumanPicker {
            transport: &mut transport,
            card_text,
            journal: &private,
        }));
        let mut pickers = std::array::from_fn(|chair| {
            if chair == config.human_chair {
                human.take().expect("exactly one human chair")
            } else {
                package.make_picker()
            }
        });
        run_seat_draft(package.pool(), draft_seed, &mut pickers)
            .map_err(|e| e.to_string())?
            .record
    };
    journal(&private, "draft", json!(draft))?;

    let mut decks: [Deck; SEATS] = std::array::from_fn(|_| Deck::default());
    for (chair, deck) in decks.iter_mut().enumerate() {
        if chair == config.human_chair {
            continue;
        }
        let pool = &draft.pools[chair];
        let mut stream = SplitMix64::seed(build_seed_for_chair(draft_seed, chair, 0));
        let built = package
            .make_builder()
            .build(pool, &mut stream)
            .map_err(|e| e.to_string())?;
        built.deck.validate(pool).map_err(|e| e.to_string())?;
        *deck = canonical_deck(&built.deck.card_ids());
    }
    let human_pool = &draft.pools[config.human_chair];
    let basics = basic_land_ids().map_err(|e| e.to_string())?;
    transport.send(
        "build",
        json!({
            "pool": card_text.render(human_pool),
            "basics": card_text.render(&basics),
        }),
    )?;
    decks[config.human_chair] = loop {
        match transport.command()? {
            Some(Command::Build { cards }) => {
                if cards.len() == DECK_CARDS as usize
                    && cards.iter().all(|&id| usize::from(id) < CARD_DEFS.len())
                {
                    let deck = canonical_deck(&cards);
                    if deck.validate(human_pool).is_ok() {
                        break deck;
                    }
                }
                transport.error("Submit exactly 40 cards from your own pool and basic lands.")?;
            }
            Some(Command::Suggest {}) => transport.error("Suggestions are disabled.")?,
            Some(Command::Pick { .. }) => transport.error("Drafting is complete.")?,
            None => {}
        }
    };
    journal(&private, "decks", json!(decks))?;
    transport.send("build_accepted", json!({"deck": decks[config.human_chair]}))?;
    Ok(PrivateDraftBuild {
        draft,
        decks,
        journal: private,
    })
}

fn journal(journal: &PodJournal, event: &str, payload: Value) -> Result<(), String> {
    let mut file = journal
        .0
        .lock()
        .map_err(|_| "The private pod journal could not be locked.")?;
    write_json_line(&mut *file, &json!({"event": event, "payload": payload}))
        .and_then(|()| file.sync_data())
        .map_err(|_| "The private pod journal could not be saved.".into())
}

fn write_json_line(output: &mut impl Write, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube_draft_v1::POOL_CARDS;
    use crate::cube_seat_package_v1::tests::{load_manifest, test_directory, uniform_manifest};
    use std::collections::BTreeSet;
    use std::io::Cursor;

    pub(super) struct FirstCard;

    impl SeatPicker for FirstCard {
        fn pick(
            &mut self,
            _: &SeatPickView<'_>,
            _: &mut SplitMix64,
        ) -> Result<PickOutcome, SeatError> {
            Ok(PickOutcome {
                index: 0,
                row: None,
            })
        }
    }

    #[test]
    fn draft_build_transport_reuses_runner_and_keeps_other_seats_private() {
        let dir = test_directory();
        let package = load_manifest(&dir, &uniform_manifest(&dir, false)).unwrap();
        let human_chair = 3;
        let draft_seed = pod_draft_seed(CLIENT_NAMESPACE, "fixture", 1).unwrap();
        let mut pickers: [Box<dyn SeatPicker>; SEATS] = std::array::from_fn(|chair| {
            if chair == human_chair {
                Box::new(FirstCard)
            } else {
                package.make_picker()
            }
        });
        let expected = run_seat_draft(package.pool(), draft_seed, &mut pickers)
            .unwrap()
            .record;

        let mut requests = Vec::new();
        requests.extend([0xff, b'\n']);
        write_json_line(&mut requests, &json!({"kind":"suggest"})).unwrap();
        write_json_line(
            &mut requests,
            &json!({"kind":"pick","round":3,"pick":3,"card":0}),
        )
        .unwrap();
        for pick in expected
            .picks
            .iter()
            .filter(|pick| pick.seat == human_chair)
        {
            write_json_line(
                &mut requests,
                &json!({
                    "kind":"pick","round":pick.round,"pick":pick.pick,"card":pick.chosen,
                }),
            )
            .unwrap();
        }
        let basic = basic_land_ids().unwrap()[0];
        write_json_line(&mut requests, &json!({"kind":"build","cards":[basic]})).unwrap();
        let cards = vec![basic; DECK_CARDS as usize];
        write_json_line(&mut requests, &json!({"kind":"build","cards":cards})).unwrap();
        let secrets = LocalCardText(
            package
                .pool()
                .ids()
                .iter()
                .map(|&id| (id, format!("local-{id}")))
                .collect(),
        );
        let journal_path = dir.join("private.jsonl");
        let mut replies = Vec::new();
        let completed = serve_draft_build(
            &package,
            DraftBuildConfig {
                run: "fixture",
                pod_index: 1,
                human_chair,
                journal_path: &journal_path,
            },
            &secrets,
            Cursor::new(requests),
            &mut replies,
        )
        .unwrap();
        assert_eq!(completed.draft, expected);
        assert_eq!(completed.decks[human_chair].card_ids(), cards);
        for (chair, deck) in completed
            .decks
            .iter()
            .enumerate()
            .filter(|(chair, _)| *chair != human_chair)
        {
            let mut stream = SplitMix64::seed(build_seed_for_chair(draft_seed, chair, 0));
            let expected = package
                .make_builder()
                .build(&expected.pools[chair], &mut stream)
                .unwrap();
            assert_eq!(*deck, canonical_deck(&expected.deck.card_ids()));
        }
        let published: Vec<Value> = BufRead::lines(Cursor::new(&replies))
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
        assert!(published.iter().all(|frame| matches!(
            frame["event"].as_str(),
            Some("start" | "pick" | "error" | "build" | "build_accepted")
        )));
        let frames: Vec<_> = published
            .iter()
            .filter(|frame| frame["event"] == "pick")
            .collect();
        assert_eq!(frames.len(), POOL_CARDS);
        let mut human_pool = Vec::new();
        let mut seen = BTreeSet::new();
        for (frame, pick) in frames.iter().zip(
            expected
                .picks
                .iter()
                .filter(|pick| pick.seat == human_chair),
        ) {
            let pack: Vec<_> = pick.pack.iter().map(|&id| json!({
                "id": id, "name": CARD_DEFS[usize::from(id)].name, "text": format!("local-{id}"),
            })).collect();
            assert_eq!(
                **frame,
                json!({
                    "schema": DRAFT_BUILD_RESPONSE_SCHEMA,
                    "event": "pick",
                    "view": {
                        "round": pick.round,
                        "pick": pick.pick,
                        "direction": if pick.round == 1 { "right" } else { "left" },
                        "pack": pack,
                        "pool": human_pool,
                        "seen_not_taken": seen,
                    },
                })
            );
            seen.extend(pick.pack.iter().copied());
            seen.remove(&pick.chosen);
            human_pool.push(pick.chosen);
        }
        let start = published.first().unwrap();
        assert_eq!(
            *start,
            json!({
                "schema": DRAFT_BUILD_RESPONSE_SCHEMA,
                "event": "start",
                "view": {"human_chair": human_chair, "seat_package": "Uniform stand-in"},
            })
        );
        let build = published
            .iter()
            .find(|frame| frame["event"] == "build")
            .unwrap();
        assert_eq!(
            *build,
            json!({
                "schema": DRAFT_BUILD_RESPONSE_SCHEMA,
                "event": "build",
                "view": {"pool": secrets.render(&expected.pools[human_chair]), "basics": secrets.render(&basic_land_ids().unwrap())},
            })
        );
        assert_eq!(
            *published.last().unwrap(),
            json!({
                "schema": DRAFT_BUILD_RESPONSE_SCHEMA,
                "event": "build_accepted",
                "view": {"deck": completed.decks[human_chair]},
            })
        );
        assert_eq!(
            published
                .iter()
                .filter(|frame| frame["event"] == "error")
                .count(),
            4
        );
        let journal = std::fs::read_to_string(journal_path).unwrap();
        assert!(journal.contains("client-pods") && journal.contains("\"event\":\"decks\""));
        assert!(!journal.contains("local-"));
        assert!(!String::from_utf8(replies).unwrap().contains("pod_index"));
        drop(completed);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
