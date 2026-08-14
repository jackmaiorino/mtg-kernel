use crate::{CheckedUntrustedMtgoVisibleGameLogProjectionV1, MtgoContractErrorV1};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const MTGO_VISIBLE_GAME_LOG_SEMANTIC_PROJECTION_SCHEMA_V1: u32 = 1;

const VISIBLE_GAME_LOG_SEMANTIC_PROJECTION_DOMAIN_V1: &[u8] =
    b"mtgo-visible-game-log-semantic-projection-v1";
const VISIBLE_GAME_LOG_RENDERED_PREFIX_DOMAIN_V1: &[u8] =
    b"mtgo-visible-game-log-rendered-prefix-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoVisibleGameLogPlayerRoleV1 {
    ActingPlayer,
    Opponent,
}

impl MtgoVisibleGameLogPlayerRoleV1 {
    fn opponent_v1(self) -> Self {
        match self {
            Self::ActingPlayer => Self::Opponent,
            Self::Opponent => Self::ActingPlayer,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoVisibleGameLogEventKindV1 {
    JoinedGame,
    TurnStarted,
    OpeningHand,
    Mulligan,
    BottomedOpeningHand,
    DrewCards,
    PlayedCard,
    CastSpell,
    ActivatedAbility,
    TriggeredAbilityOnStack,
    DiscardedCard,
    AttackedPlayer,
    ConcededGame,
    WonGame,
    WonMatch,
    ForcedComplete,
}

#[derive(Serialize)]
struct MtgoVisibleGameLogSemanticEventRecordV1 {
    source_sequence: u32,
    source_visible_text_sha256: String,
    kind: MtgoVisibleGameLogEventKindV1,
    actor_role: Option<MtgoVisibleGameLogPlayerRoleV1>,
    turn_number: Option<u32>,
    primary_count: Option<u8>,
    secondary_count: Option<u8>,
    visible_card_names: Vec<String>,
}

pub struct MtgoVisibleGameLogSemanticEventViewV1<'a> {
    record: &'a MtgoVisibleGameLogSemanticEventRecordV1,
}

impl<'a> MtgoVisibleGameLogSemanticEventViewV1<'a> {
    pub fn source_sequence_v1(&self) -> u32 {
        self.record.source_sequence
    }

    pub fn kind_v1(&self) -> MtgoVisibleGameLogEventKindV1 {
        self.record.kind
    }

    pub fn actor_role_v1(&self) -> Option<MtgoVisibleGameLogPlayerRoleV1> {
        self.record.actor_role
    }

    pub fn turn_number_v1(&self) -> Option<u32> {
        self.record.turn_number
    }

    pub fn primary_count_v1(&self) -> Option<u8> {
        self.record.primary_count
    }

    pub fn secondary_count_v1(&self) -> Option<u8> {
        self.record.secondary_count
    }

    pub fn visible_card_name_count_v1(&self) -> usize {
        self.record.visible_card_names.len()
    }

    pub fn visible_card_name_v1(&self, index: usize) -> Option<&str> {
        self.record
            .visible_card_names
            .get(index)
            .map(String::as_str)
    }
}

/// A conservative typed projection of public facts stated by the rendered
/// MTGO Game Log.
///
/// Player names are reduced to acting-player or opponent roles. Source UUIDs,
/// timestamps, record flags, raw link markup, and numeric object identifiers
/// never enter this type. Unrecognized visible wording remains available in
/// the source text projection but is counted rather than guessed here.
///
/// This projection remains checked-untrusted until a Windows source binder
/// proves that its file belongs to the current signed client and seated-player
/// window. It supplements visible state and cannot independently prove a
/// complete current observation or legal-action set.
pub struct CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1 {
    events: Vec<MtgoVisibleGameLogSemanticEventRecordV1>,
    source_visible_text_sha256s: Vec<String>,
    source_projection_commitment_sha256: String,
    acting_player_alias_sha256: String,
    opponent_alias_sha256: Option<String>,
    classified_source_record_count: usize,
    unclassified_source_record_count: usize,
    projection_commitment_sha256: String,
}

impl CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1 {
    pub fn event_count_v1(&self) -> usize {
        self.events.len()
    }

    pub fn event_v1(&self, index: usize) -> Option<MtgoVisibleGameLogSemanticEventViewV1<'_>> {
        self.events
            .get(index)
            .map(|record| MtgoVisibleGameLogSemanticEventViewV1 { record })
    }

    pub fn source_projection_commitment_sha256_v1(&self) -> &str {
        &self.source_projection_commitment_sha256
    }

    /// Commitment over the exact rendered-text prefix, including records
    /// whose wording is not recognized by the conservative semantic grammar.
    /// It contains no raw text, transport identifier, timestamp, or link
    /// metadata.
    pub(crate) fn visible_source_record_prefix_commitment_v1(
        &self,
        record_count: usize,
    ) -> Option<String> {
        let prefix = self.source_visible_text_sha256s.get(..record_count)?;
        let mut hasher = Sha256::new();
        hasher.update(VISIBLE_GAME_LOG_RENDERED_PREFIX_DOMAIN_V1);
        hasher.update((record_count as u64).to_be_bytes());
        for digest in prefix {
            hasher.update((digest.len() as u64).to_be_bytes());
            hasher.update(digest.as_bytes());
        }
        Some(format!("{:x}", hasher.finalize()))
    }

    pub(crate) fn source_record_count_v1(&self) -> usize {
        self.source_visible_text_sha256s.len()
    }

    pub fn acting_player_alias_sha256_v1(&self) -> &str {
        &self.acting_player_alias_sha256
    }

    pub fn opponent_alias_sha256_v1(&self) -> Option<&str> {
        self.opponent_alias_sha256.as_deref()
    }

    pub fn classified_source_record_count_v1(&self) -> usize {
        self.classified_source_record_count
    }

    pub fn unclassified_source_record_count_v1(&self) -> usize {
        self.unclassified_source_record_count
    }

    pub fn projection_commitment_sha256_v1(&self) -> &str {
        &self.projection_commitment_sha256
    }

    pub fn complete_for_current_state_reconstruction_v1(&self) -> bool {
        false
    }

    pub fn safe_for_model_scoring_v1(&self) -> bool {
        false
    }

    pub fn safe_for_input_v1(&self) -> bool {
        false
    }
}

pub fn classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(
    source: &CheckedUntrustedMtgoVisibleGameLogProjectionV1,
    acting_player_alias: &str,
) -> Result<CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1, MtgoContractErrorV1> {
    validate_player_alias_v1(acting_player_alias)?;
    let mut events = Vec::new();
    let mut source_visible_text_sha256s = Vec::with_capacity(source.record_count_v1());
    let mut opponent_alias = None;
    for index in 0..source.record_count_v1() {
        let record = source.record_v1(index).ok_or_else(|| {
            error_v1(
                "visible_game_log_semantic_source",
                "source record disappeared during semantic projection",
            )
        })?;
        source_visible_text_sha256s.push(record.visible_text_sha256_v1().to_owned());
        if let Some(event) = classify_record_v1(&record, acting_player_alias, &mut opponent_alias)?
        {
            events.push(event);
        }
    }
    let classified_source_record_count = events.len();
    let unclassified_source_record_count = source
        .record_count_v1()
        .checked_sub(classified_source_record_count)
        .ok_or_else(|| {
            error_v1(
                "visible_game_log_semantic_count",
                "classified record count exceeds its source",
            )
        })?;
    let acting_player_alias_sha256 = sha256_hex_v1(acting_player_alias.as_bytes());
    let opponent_alias_sha256 = opponent_alias
        .as_deref()
        .map(|alias| sha256_hex_v1(alias.as_bytes()));
    let events_json = serde_json::to_vec(&events)
        .map_err(|error| error_v1("visible_game_log_semantic_serialization", error.to_string()))?;
    let projection_commitment_sha256 = commitment_v1(
        VISIBLE_GAME_LOG_SEMANTIC_PROJECTION_DOMAIN_V1,
        &[
            &MTGO_VISIBLE_GAME_LOG_SEMANTIC_PROJECTION_SCHEMA_V1.to_be_bytes(),
            source.projection_commitment_sha256_v1().as_bytes(),
            acting_player_alias_sha256.as_bytes(),
            opponent_alias_sha256.as_deref().unwrap_or("").as_bytes(),
            &(classified_source_record_count as u64).to_be_bytes(),
            &(unclassified_source_record_count as u64).to_be_bytes(),
            &events_json,
        ],
    );
    Ok(CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1 {
        events,
        source_visible_text_sha256s,
        source_projection_commitment_sha256: source.projection_commitment_sha256_v1().to_owned(),
        acting_player_alias_sha256,
        opponent_alias_sha256,
        classified_source_record_count,
        unclassified_source_record_count,
        projection_commitment_sha256,
    })
}

fn classify_record_v1(
    record: &crate::MtgoVisibleGameLogTextViewV1<'_>,
    acting_player_alias: &str,
    opponent_alias: &mut Option<String>,
) -> Result<Option<MtgoVisibleGameLogSemanticEventRecordV1>, MtgoContractErrorV1> {
    let text = record.visible_text_v1();
    let mut kind = None;
    let mut actor_role = None;
    let mut turn_number = None;
    let mut primary_count = None;
    let mut secondary_count = None;
    let mut visible_card_names = Vec::new();

    if text == "Your game has not completed within allowed time limits.  To facilitate progress, the match has been forced complete.  If you believe you are entitled to reimbursement, please contact customer support."
    {
        kind = Some(MtgoVisibleGameLogEventKindV1::ForcedComplete);
    } else if let Some(actor) = text.strip_suffix(" joined the game.") {
        actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
        kind = Some(MtgoVisibleGameLogEventKindV1::JoinedGame);
    } else if let Some(rest) = text.strip_prefix("Turn ") {
        if let Some((number, actor)) = rest.split_once(": ") {
            turn_number = Some(parse_decimal_u32_v1(number, "turn number")?);
            actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
            kind = Some(MtgoVisibleGameLogEventKindV1::TurnStarted);
        }
    } else if let Some((actor, score)) = text.split_once(" wins the match ") {
        let score = score.strip_suffix('.').unwrap_or(score);
        let (won, lost) = score.split_once('-').ok_or_else(|| {
            error_v1(
                "visible_game_log_match_score",
                "a match-win record lacks its visible score",
            )
        })?;
        let won = parse_decimal_u8_v1(won, "match winner score")?;
        let lost = parse_decimal_u8_v1(lost, "match loser score")?;
        if won == 0 || won > 2 || lost > 1 || won <= lost {
            return Err(error_v1(
                "visible_game_log_match_score",
                "the visible match-win score is not canonical",
            ));
        }
        actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
        primary_count = Some(won);
        secondary_count = Some(lost);
        kind = Some(MtgoVisibleGameLogEventKindV1::WonMatch);
    } else if let Some(actor) = text.strip_suffix(" wins the game.") {
        actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
        kind = Some(MtgoVisibleGameLogEventKindV1::WonGame);
    } else if let Some(actor) = text.strip_suffix(" has conceded from the game.") {
        actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
        kind = Some(MtgoVisibleGameLogEventKindV1::ConcededGame);
    } else if let Some((actor, rest)) = text.split_once(" puts a triggered ability from ") {
        if let Some(source_card) = first_linked_card_if_prefix_v1(record, rest) {
            let suffix = rest
                .strip_prefix(source_card)
                .expect("the linked source card was checked as a prefix");
            if is_exact_triggered_ability_stack_suffix_v1(suffix) {
                actor_role = Some(role_for_actor_v1(
                    actor,
                    acting_player_alias,
                    opponent_alias,
                )?);
                visible_card_names.extend(
                    (0..record.visible_card_name_count_v1())
                        .filter_map(|index| record.visible_card_name_v1(index))
                        .map(str::to_owned),
                );
                kind = Some(MtgoVisibleGameLogEventKindV1::TriggeredAbilityOnStack);
            }
        }
    } else if let Some((actor, rest)) = text.split_once(" puts ") {
        if let Some((bottomed, opening, singular_bottomed_card)) = rest
            .strip_suffix(" card in hand.")
            .or_else(|| rest.strip_suffix(" cards in hand."))
            .and_then(split_bottomed_opening_counts_v1)
        {
            actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
            primary_count = Some(parse_bottomed_count_phrase_v1(
                bottomed,
                singular_bottomed_card,
            )?);
            secondary_count = Some(parse_card_count_phrase_v1(opening)?);
            kind = Some(MtgoVisibleGameLogEventKindV1::BottomedOpeningHand);
        }
    } else if let Some((actor, count_phrase)) = strip_card_count_suffix_v1(text, " in hand.")
        .and_then(|value| value.split_once(" begins the game with "))
    {
        actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
        primary_count = Some(parse_card_count_phrase_v1(count_phrase)?);
        kind = Some(MtgoVisibleGameLogEventKindV1::OpeningHand);
    } else if let Some((actor, count_phrase)) = strip_card_count_suffix_v1(text, ".")
        .and_then(|value| value.split_once(" mulligans to "))
    {
        actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
        primary_count = Some(parse_number_word_v1(count_phrase)?);
        kind = Some(MtgoVisibleGameLogEventKindV1::Mulligan);
    } else if let Some(actor) = text.strip_suffix(" draws a card.") {
        actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
        primary_count = Some(1);
        kind = Some(MtgoVisibleGameLogEventKindV1::DrewCards);
    } else if let Some((actor, rest)) = text.split_once(" draws ") {
        if let Some((count, _source)) = rest.split_once(" cards with ") {
            actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
            primary_count = Some(parse_number_word_v1(count)?);
            kind = Some(MtgoVisibleGameLogEventKindV1::DrewCards);
        }
    } else if let Some((actor, rest)) = text.split_once(" plays ") {
        if let Some(card) = first_linked_card_if_prefix_v1(record, rest) {
            actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
            visible_card_names.push(card.to_owned());
            kind = Some(MtgoVisibleGameLogEventKindV1::PlayedCard);
        }
    } else if let Some((actor, rest)) = text.split_once(" casts ") {
        if let Some(card) = first_linked_card_if_prefix_v1(record, rest) {
            actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
            visible_card_names.push(card.to_owned());
            kind = Some(MtgoVisibleGameLogEventKindV1::CastSpell);
        }
    } else if let Some((actor, rest)) = text.split_once(" activates an ability of ") {
        if let Some(card) = first_linked_card_if_prefix_v1(record, rest) {
            actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
            visible_card_names.push(card.to_owned());
            kind = Some(MtgoVisibleGameLogEventKindV1::ActivatedAbility);
        }
    } else if let Some((actor, rest)) = text.split_once(" discards ") {
        if let Some(card) = first_linked_card_if_prefix_v1(record, rest) {
            actor_role = Some(role_for_actor_v1(actor, acting_player_alias, opponent_alias)?);
            visible_card_names.push(card.to_owned());
            kind = Some(MtgoVisibleGameLogEventKindV1::DiscardedCard);
        }
    } else if let Some((defender, _attackers)) = text.split_once(" is being attacked by ") {
        if record.visible_card_name_count_v1() > 0 {
            actor_role = Some(
                role_for_actor_v1(defender, acting_player_alias, opponent_alias)?.opponent_v1(),
            );
            visible_card_names.extend(
                (0..record.visible_card_name_count_v1())
                    .filter_map(|index| record.visible_card_name_v1(index))
                    .map(str::to_owned),
            );
            kind = Some(MtgoVisibleGameLogEventKindV1::AttackedPlayer);
        }
    }

    Ok(kind.map(|kind| MtgoVisibleGameLogSemanticEventRecordV1 {
        source_sequence: record.sequence_v1(),
        source_visible_text_sha256: record.visible_text_sha256_v1().to_owned(),
        kind,
        actor_role,
        turn_number,
        primary_count,
        secondary_count,
        visible_card_names,
    }))
}

fn first_linked_card_if_prefix_v1<'a>(
    record: &'a crate::MtgoVisibleGameLogTextViewV1<'_>,
    visible_rest: &str,
) -> Option<&'a str> {
    record
        .visible_card_name_v1(0)
        .filter(|card| visible_rest.starts_with(card))
}

fn is_exact_triggered_ability_stack_suffix_v1(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(" onto the stack (") else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    rest.strip_suffix(").")
        .is_some_and(|ability| !ability.is_empty())
        || rest
            .strip_suffix('.')
            .and_then(|body| body.rsplit_once(") targeting "))
            .is_some_and(|(ability, targets)| !ability.is_empty() && !targets.is_empty())
}

fn role_for_actor_v1(
    actor: &str,
    acting_player_alias: &str,
    opponent_alias: &mut Option<String>,
) -> Result<MtgoVisibleGameLogPlayerRoleV1, MtgoContractErrorV1> {
    validate_player_alias_v1(actor)?;
    if actor == acting_player_alias {
        return Ok(MtgoVisibleGameLogPlayerRoleV1::ActingPlayer);
    }
    if opponent_alias
        .as_deref()
        .is_some_and(|known| known != actor)
    {
        return Err(error_v1(
            "visible_game_log_semantic_players",
            format!(
                "typed game events refer to opposing-player commitments {} and {}",
                sha256_hex_v1(
                    opponent_alias
                        .as_deref()
                        .expect("known opposing alias")
                        .as_bytes()
                ),
                sha256_hex_v1(actor.as_bytes())
            ),
        ));
    }
    if opponent_alias.is_none() {
        *opponent_alias = Some(actor.to_owned());
    }
    Ok(MtgoVisibleGameLogPlayerRoleV1::Opponent)
}

fn validate_player_alias_v1(value: &str) -> Result<(), MtgoContractErrorV1> {
    if value.is_empty()
        || value.len() > 64
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(error_v1(
            "visible_game_log_player_alias",
            "a visible player alias is not canonical",
        ));
    }
    Ok(())
}

fn strip_card_count_suffix_v1<'a>(value: &'a str, tail: &str) -> Option<&'a str> {
    value
        .strip_suffix(&format!(" cards{tail}"))
        .or_else(|| value.strip_suffix(&format!(" card{tail}")))
}

fn split_bottomed_opening_counts_v1(value: &str) -> Option<(&str, &str, bool)> {
    value
        .split_once(" cards on the bottom of their library and begins the game with ")
        .map(|(bottomed, opening)| (bottomed, opening, false))
        .or_else(|| {
            value
                .split_once(" card on the bottom of their library and begins the game with ")
                .map(|(bottomed, opening)| (bottomed, opening, true))
        })
}

fn parse_bottomed_count_phrase_v1(
    value: &str,
    singular_bottomed_card: bool,
) -> Result<u8, MtgoContractErrorV1> {
    if singular_bottomed_card && value == "a" {
        return Ok(1);
    }
    parse_number_word_v1(value)
}

fn parse_card_count_phrase_v1(value: &str) -> Result<u8, MtgoContractErrorV1> {
    parse_number_word_v1(value)
}

fn parse_number_word_v1(value: &str) -> Result<u8, MtgoContractErrorV1> {
    match value {
        "zero" => Ok(0),
        "one" => Ok(1),
        "two" => Ok(2),
        "three" => Ok(3),
        "four" => Ok(4),
        "five" => Ok(5),
        "six" => Ok(6),
        "seven" => Ok(7),
        "eight" => Ok(8),
        "nine" => Ok(9),
        "ten" => Ok(10),
        _ => Err(error_v1(
            "visible_game_log_count",
            "a visible card count is outside the admitted grammar",
        )),
    }
}

fn parse_decimal_u8_v1(value: &str, label: &str) -> Result<u8, MtgoContractErrorV1> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error_v1(
            "visible_game_log_number",
            format!("{label} is not a canonical decimal integer"),
        ));
    }
    value.parse::<u8>().map_err(|_| {
        error_v1(
            "visible_game_log_number",
            format!("{label} is outside the admitted range"),
        )
    })
}

fn parse_decimal_u32_v1(value: &str, label: &str) -> Result<u32, MtgoContractErrorV1> {
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(error_v1(
            "visible_game_log_number",
            format!("{label} is not a canonical positive decimal integer"),
        ));
    }
    value.parse::<u32>().map_err(|_| {
        error_v1(
            "visible_game_log_number",
            format!("{label} is outside the admitted range"),
        )
    })
}

fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn commitment_v1(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn error_v1(code: &'static str, detail: impl Into<String>) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE_ID_V1: &str = "82ba951e-fe1f-423a-9cdb-bfe879ea2c6b";

    fn write_dotnet_string_v1(target: &mut Vec<u8>, value: &str) {
        let mut length = value.len() as u32;
        while length >= 0x80 {
            target.push((length as u8) | 0x80);
            length >>= 7;
        }
        target.push(length as u8);
        target.extend_from_slice(value.as_bytes());
    }

    fn source_v1(lines: &[&str]) -> CheckedUntrustedMtgoVisibleGameLogProjectionV1 {
        source_with_hidden_metadata_v1(SOURCE_ID_V1, 638_905_999_999_999_999, lines)
    }

    fn source_with_hidden_metadata_v1(
        source_id: &str,
        first_timestamp: u64,
        lines: &[&str],
    ) -> CheckedUntrustedMtgoVisibleGameLogProjectionV1 {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, source_id);
        bytes.extend_from_slice(&4_u16.to_le_bytes());
        write_dotnet_string_v1(&mut bytes, source_id);
        for (index, line) in lines.iter().enumerate() {
            bytes.extend_from_slice(&(first_timestamp + index as u64).to_le_bytes());
            bytes.push(0);
            write_dotnet_string_v1(&mut bytes, line);
        }
        crate::parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes).unwrap()
    }

    #[test]
    fn exact_public_events_reduce_names_to_roles_and_keep_visible_cards() {
        let source = source_v1(&[
            "@PUnbuckledPie joined the game.",
            "Turn 1: @PUnbuckledPie",
            "@PUnbuckledPie begins the game with seven cards in hand.",
            "@PUnbuckledPie mulligans to one card.",
            "@PUnbuckledPie puts six cards on the bottom of their library and begins the game with one card in hand.",
            "@PUnbuckledPie draws a card.",
            "@PUnbuckledPie plays @[Island@:123,456:@].",
            "@POpponent casts @[Lightning Bolt@:999,888:@] targeting @PUnbuckledPie.",
            "@POpponent puts a triggered ability from @[Young Pyromancer@:321,654:@] onto the stack (Whenever you cast an instant or sorcery spell, create a 1/1 red Elemental creature token.) targeting @[Goblin Token@:777,666:@].",
            "@PUnbuckledPie is being attacked by @[Goblin Token@:777,666:@]",
            "@POpponent has conceded from the game.",
            "@PUnbuckledPie wins the game.",
            "@PUnbuckledPie wins the match 2-1",
        ]);
        let projection =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .unwrap();
        assert_eq!(projection.event_count_v1(), 13);
        assert_eq!(projection.classified_source_record_count_v1(), 13);
        assert_eq!(projection.unclassified_source_record_count_v1(), 0);
        assert_eq!(
            projection.event_v1(6).unwrap().kind_v1(),
            MtgoVisibleGameLogEventKindV1::PlayedCard
        );
        assert_eq!(
            projection.event_v1(6).unwrap().visible_card_name_v1(0),
            Some("Island")
        );
        assert_eq!(
            projection.event_v1(7).unwrap().actor_role_v1(),
            Some(MtgoVisibleGameLogPlayerRoleV1::Opponent)
        );
        assert_eq!(
            projection.event_v1(9).unwrap().actor_role_v1(),
            Some(MtgoVisibleGameLogPlayerRoleV1::Opponent)
        );
        let trigger = projection.event_v1(8).unwrap();
        assert_eq!(
            trigger.kind_v1(),
            MtgoVisibleGameLogEventKindV1::TriggeredAbilityOnStack
        );
        assert_eq!(trigger.visible_card_name_v1(0), Some("Young Pyromancer"));
        assert_eq!(trigger.visible_card_name_v1(1), Some("Goblin Token"));
        assert_eq!(projection.event_v1(3).unwrap().primary_count_v1(), Some(1));
        assert_eq!(projection.event_v1(4).unwrap().primary_count_v1(), Some(6));
        assert_eq!(
            projection.event_v1(4).unwrap().secondary_count_v1(),
            Some(1)
        );
        let match_win = projection.event_v1(12).unwrap();
        assert_eq!(match_win.primary_count_v1(), Some(2));
        assert_eq!(match_win.secondary_count_v1(), Some(1));
        assert!(projection.opponent_alias_sha256_v1().is_some());
        assert!(!projection.complete_for_current_state_reconstruction_v1());
        assert!(!projection.safe_for_model_scoring_v1());
        assert!(!projection.safe_for_input_v1());
    }

    #[test]
    fn unknown_visible_wording_is_counted_without_becoming_a_fact() {
        let source = source_v1(&[
            "@PUnbuckledPie joined the game.",
            "A future visible MTGO message.",
            "@PUnbuckledPie draws a card.",
        ]);
        let projection =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .unwrap();
        assert_eq!(projection.event_count_v1(), 2);
        assert_eq!(projection.unclassified_source_record_count_v1(), 1);
        assert_eq!(projection.event_v1(0).unwrap().source_sequence_v1(), 1);
        assert_eq!(
            projection.event_v1(0).unwrap().kind_v1(),
            MtgoVisibleGameLogEventKindV1::JoinedGame
        );
        assert_eq!(projection.event_v1(1).unwrap().source_sequence_v1(), 3);
    }

    #[test]
    fn singular_bottomed_card_wording_is_typed_exactly() {
        let source = source_v1(&[
            "@PUnbuckledPie puts one card on the bottom of their library and begins the game with six cards in hand.",
        ]);
        let projection =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .unwrap();
        let event = projection.event_v1(0).unwrap();
        assert_eq!(
            event.kind_v1(),
            MtgoVisibleGameLogEventKindV1::BottomedOpeningHand
        );
        assert_eq!(event.primary_count_v1(), Some(1));
        assert_eq!(event.secondary_count_v1(), Some(6));
    }

    #[test]
    fn rendered_article_bottomed_card_wording_is_typed_exactly() {
        let source = source_v1(&[
            "@PUnbuckledPie puts a card on the bottom of their library and begins the game with six cards in hand.",
        ]);
        let projection =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .unwrap();
        let event = projection.event_v1(0).unwrap();
        assert_eq!(
            event.kind_v1(),
            MtgoVisibleGameLogEventKindV1::BottomedOpeningHand
        );
        assert_eq!(event.primary_count_v1(), Some(1));
        assert_eq!(event.secondary_count_v1(), Some(6));
    }

    #[test]
    fn article_with_plural_bottomed_card_wording_rejects() {
        let source = source_v1(&[
            "@PUnbuckledPie puts a cards on the bottom of their library and begins the game with six cards in hand.",
        ]);
        let error =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .err()
                .unwrap();
        assert_eq!(error.code(), "visible_game_log_count");
    }

    #[test]
    fn two_distinct_opponents_cannot_share_one_duel_projection() {
        let source = source_v1(&["@POpponentOne draws a card.", "@POpponentTwo draws a card."]);
        let error =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .err()
                .unwrap();
        assert_eq!(error.code(), "visible_game_log_semantic_players");
    }

    #[test]
    fn malformed_match_score_rejects_instead_of_becoming_untyped_text() {
        let source = source_v1(&["@PUnbuckledPie wins the match two-one"]);
        let error =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .err()
                .unwrap();
        assert!(matches!(
            error.code(),
            "visible_game_log_match_score" | "visible_game_log_number"
        ));
    }

    #[test]
    fn impossible_best_of_three_match_score_rejects() {
        let source = source_v1(&["@PUnbuckledPie wins the match 9-1"]);
        let error =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .err()
                .unwrap();
        assert_eq!(error.code(), "visible_game_log_match_score");
    }

    #[test]
    fn source_link_metadata_never_appears_in_typed_card_name() {
        let source = source_v1(&["@PUnbuckledPie plays @[Island@:296710,467:@]."]);
        let projection =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
                .unwrap();
        let event = projection.event_v1(0).unwrap();
        assert_eq!(event.visible_card_name_v1(0), Some("Island"));
        assert!(!event.visible_card_name_v1(0).unwrap().contains("296710"));
    }

    #[test]
    fn hidden_source_metadata_cannot_change_player_visible_semantics() {
        let first = source_with_hidden_metadata_v1(
            SOURCE_ID_V1,
            638_905_999_999_999_999,
            &["@PUnbuckledPie plays @[Island@:296710,467:@]."],
        );
        let second = source_with_hidden_metadata_v1(
            "11111111-2222-3333-4444-555555555555",
            638_906_000_000_000_000,
            &["@PUnbuckledPie plays @[Island@:999999,888:@]."],
        );
        let first =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&first, "UnbuckledPie")
                .unwrap();
        let second =
            classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&second, "UnbuckledPie")
                .unwrap();

        let visible_facts =
            |projection: &CheckedUntrustedMtgoVisibleGameLogSemanticProjectionV1| {
                let event = projection.event_v1(0).expect("one visible semantic event");
                (
                    event.source_sequence_v1(),
                    event.kind_v1(),
                    event.actor_role_v1(),
                    event.turn_number_v1(),
                    event.primary_count_v1(),
                    event.secondary_count_v1(),
                    event.visible_card_name_v1(0).map(str::to_owned),
                )
            };
        assert_eq!(visible_facts(&first), visible_facts(&second));
        assert_eq!(
            first.visible_source_record_prefix_commitment_v1(1),
            second.visible_source_record_prefix_commitment_v1(1)
        );
    }
}
