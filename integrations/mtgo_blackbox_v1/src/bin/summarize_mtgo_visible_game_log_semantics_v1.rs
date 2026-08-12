use mtgo_blackbox_v1::{
    classify_checked_untrusted_mtgo_visible_game_log_semantics_v1,
    parse_checked_untrusted_mtgo_visible_game_log_v1, MtgoVisibleGameLogEventKindV1,
};
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Serialize)]
struct VisibleGameLogSemanticSummaryV1 {
    schema_version: u32,
    source_file_count: usize,
    semantically_supported_file_count: usize,
    semantically_unsupported_file_count: usize,
    source_record_count: usize,
    unsupported_source_record_count: usize,
    classified_source_record_count: usize,
    unclassified_source_record_count: usize,
    event_kind_counts: BTreeMap<&'static str, usize>,
    raw_visible_text_emitted: bool,
    player_aliases_emitted: bool,
    source_identifiers_emitted: bool,
    card_names_emitted: bool,
    complete_for_current_state_reconstruction: bool,
    safe_for_model_scoring: bool,
    safe_for_input: bool,
}

fn main() {
    match run_v1() {
        Ok(summary) => match serde_json::to_string_pretty(&summary) {
            Ok(json) => println!("{json}"),
            Err(error) => exit_error_v1(&format!("serialize semantic summary: {error}")),
        },
        Err(error) => exit_error_v1(&error),
    }
}

fn run_v1() -> Result<VisibleGameLogSemanticSummaryV1, String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let [acting_player_alias, paths @ ..] = args.as_slice() else {
        return Err(
            "usage: summarize_mtgo_visible_game_log_semantics_v1 <acting-player-alias> <Match_GameLog_*.dat> [...]"
                .to_owned(),
        );
    };
    if paths.is_empty() {
        return Err("at least one Game Log path is required".to_owned());
    }
    let acting_player_alias = acting_player_alias
        .to_str()
        .ok_or("acting-player alias is not valid UTF-8")?;
    let mut source_record_count = 0_usize;
    let source_file_count = paths.len();
    let mut semantically_supported_file_count = 0_usize;
    let mut semantically_unsupported_file_count = 0_usize;
    let mut classified_source_record_count = 0_usize;
    let mut unclassified_source_record_count = 0_usize;
    let mut unsupported_source_record_count = 0_usize;
    let mut event_kind_counts = BTreeMap::new();
    for path in paths {
        let path = PathBuf::from(path);
        let bytes =
            std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        let source = parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes)
            .map_err(|error| format!("{}: {}", error.code(), error.detail()))?;
        source_record_count = source_record_count
            .checked_add(source.record_count_v1())
            .ok_or("source record count overflow")?;
        let semantics = match classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(
            &source,
            acting_player_alias,
        ) {
            Ok(semantics) => semantics,
            Err(_) => {
                semantically_unsupported_file_count = semantically_unsupported_file_count
                    .checked_add(1)
                    .ok_or("unsupported file count overflow")?;
                unsupported_source_record_count = unsupported_source_record_count
                    .checked_add(source.record_count_v1())
                    .ok_or("unsupported source record count overflow")?;
                continue;
            }
        };
        semantically_supported_file_count = semantically_supported_file_count
            .checked_add(1)
            .ok_or("supported file count overflow")?;
        classified_source_record_count = classified_source_record_count
            .checked_add(semantics.classified_source_record_count_v1())
            .ok_or("classified source record count overflow")?;
        unclassified_source_record_count = unclassified_source_record_count
            .checked_add(semantics.unclassified_source_record_count_v1())
            .ok_or("unclassified source record count overflow")?;
        for index in 0..semantics.event_count_v1() {
            let event = semantics
                .event_v1(index)
                .ok_or("semantic event disappeared during summary")?;
            let label = event_kind_label_v1(event.kind_v1());
            let count = event_kind_counts.entry(label).or_insert(0_usize);
            *count = count.checked_add(1).ok_or("event kind count overflow")?;
        }
    }
    Ok(VisibleGameLogSemanticSummaryV1 {
        schema_version: 1,
        source_file_count,
        semantically_supported_file_count,
        semantically_unsupported_file_count,
        source_record_count,
        unsupported_source_record_count,
        classified_source_record_count,
        unclassified_source_record_count,
        event_kind_counts,
        raw_visible_text_emitted: false,
        player_aliases_emitted: false,
        source_identifiers_emitted: false,
        card_names_emitted: false,
        complete_for_current_state_reconstruction: false,
        safe_for_model_scoring: false,
        safe_for_input: false,
    })
}

fn event_kind_label_v1(kind: MtgoVisibleGameLogEventKindV1) -> &'static str {
    use MtgoVisibleGameLogEventKindV1::*;
    match kind {
        TurnStarted => "turn_started",
        OpeningHand => "opening_hand",
        Mulligan => "mulligan",
        BottomedOpeningHand => "bottomed_opening_hand",
        DrewCards => "drew_cards",
        PlayedCard => "played_card",
        CastSpell => "cast_spell",
        ActivatedAbility => "activated_ability",
        DiscardedCard => "discarded_card",
        AttackedPlayer => "attacked_player",
        ConcededGame => "conceded_game",
        WonGame => "won_game",
        WonMatch => "won_match",
        ForcedComplete => "forced_complete",
    }
}

fn exit_error_v1(error: &str) -> ! {
    eprintln!("{error}");
    std::process::exit(1)
}
