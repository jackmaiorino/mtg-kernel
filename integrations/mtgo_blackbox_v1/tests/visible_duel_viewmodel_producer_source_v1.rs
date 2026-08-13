use mtgo_blackbox_v1::mtgo_visible_duel_viewmodel_candidate_surface_v1;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

fn producer_root_v1() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("integrations directory")
        .join("mtgo_visible_duel_producer_v1")
}

fn producer_source_v1() -> String {
    fs::read_to_string(producer_root_v1().join("VisibleDuelProducerV1.cs"))
        .expect("read direct-source producer")
}

fn complete_producer_source_v1() -> String {
    ["VisibleDuelProducerV1.cs", "SanitizedVisibleDecisionV1.cs"]
        .into_iter()
        .map(|name| {
            fs::read_to_string(producer_root_v1().join(name)).expect("read producer source")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compiled_getter_entries_v1(source: &str) -> HashSet<String> {
    compiled_entries_v1(source, "AllowedGetters")
}

fn compiled_entries_v1(source: &str, array_name: &str) -> HashSet<String> {
    let declaration = format!("private static readonly string[] {array_name}");
    let start = source
        .find(&declaration)
        .expect("producer getter allowlist start");
    let body = &source[start..];
    let end = body
        .find("        };")
        .expect("producer getter allowlist end");
    body[..end]
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim().trim_end_matches(',');
            trimmed
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect()
}

#[test]
fn managed_producer_getter_allowlist_exactly_matches_reviewed_surface() {
    let source = producer_source_v1();
    let actual = compiled_getter_entries_v1(&source);
    let expected = mtgo_visible_duel_viewmodel_candidate_surface_v1()
        .candidates
        .into_iter()
        .map(|candidate| {
            format!(
                "{}|{}|{}",
                candidate
                    .assembly_file_name
                    .strip_suffix(".dll")
                    .expect("reviewed assembly suffix"),
                candidate.declaring_type,
                candidate.property_name
            )
        })
        .collect::<HashSet<_>>();
    assert_eq!(actual.len(), 46);
    assert_eq!(actual, expected);
}

#[test]
fn managed_producer_private_visible_action_join_allowlist_matches_audit() {
    let source = producer_source_v1();
    let actual = compiled_entries_v1(&source, "PrivateVisibleActionJoinGetters");
    let expected = mtgo_blackbox_v1::mtgo_visible_duel_viewmodel_producer_audit_v1()
        .private_visible_action_join_getters
        .into_iter()
        .map(|getter| {
            format!(
                "{}|{}|{}",
                getter
                    .assembly_file_name
                    .strip_suffix(".dll")
                    .expect("reviewed assembly suffix"),
                getter.declaring_type,
                getter.property_name
            )
        })
        .collect::<HashSet<_>>();
    assert_eq!(actual.len(), 15);
    assert_eq!(actual, expected);
}

#[test]
fn managed_producer_has_no_raw_output_or_side_effect_api_markers() {
    let source = complete_producer_source_v1();
    for forbidden in [
        "ReadProcessMemory",
        "WriteProcessMemory",
        "CreateRemoteThread",
        "VirtualAllocEx",
        "HttpClient",
        "WebRequest",
        "System.IO.File",
        "System.IO.Directory",
        "FileStream",
        "StreamReader",
        "StreamWriter",
        "System.Net",
        "System.Diagnostics",
        "Console.",
        "GetFields(",
        "GetMembers(",
        "GetProperties(",
        "InvokeMember(",
        "SetValue(",
        "ExecuteAction",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden managed producer marker: {forbidden}"
        );
    }
    assert!(source.contains("private static readonly string[] AllowedGetters"));
    assert!(source.contains("private static bool ValidateExactGetterSurface()"));
    assert!(source.contains("MemoryMappedFile.OpenExisting"));
    assert!(source.contains("private static bool IsExactChannelName"));
    assert_eq!(source.matches("property.GetValue(target, null)").count(), 2);
    assert!(source.contains("private static bool TryReadExactPropertyV1("));
    assert!(source.contains("private static bool TryReadExactPrivateVisibleActionPropertyV1("));
    assert!(source.contains("private static bool TryValidateVisibleChromeProjectionV1("));
    assert!(source.contains("private static bool TryValidateVisibleZonesAndCardsV1("));
    assert!(source.contains("private static bool TryValidateNeverEnumeratedZoneRootV1("));
    assert!(source.contains("private static bool TryValidatePrivateVisibleCardActionJoinsV1("));
    assert!(source.contains("private static bool TryValidatePrivateVisibleActionV1("));
    assert!(source.contains("PrivateVisibleActionJoinGetters.Contains("));
    assert!(source.contains("AllowedGetters.Contains(exactKey, StringComparer.Ordinal)"));
    assert!(source.contains("private const int MaximumVisibleTextCharacters = 4096;"));
    assert!(source.contains("private const int MaximumVisibleCollectionItems = 1024;"));
    assert!(source.contains("return built ? visibleDecision : ProjectionIncomplete;"));
}

#[test]
fn managed_producer_visible_chrome_getters_are_exact_and_output_stays_fixed() {
    let source = producer_source_v1();
    for marker in [
        "\"CurrentPhase\"",
        "\"GameTurnText\"",
        "\"Players\"",
        "\"PromptBox\"",
        "\"LocalPlayer\"",
        "\"Active\"",
        "\"MatActive\"",
        "\"Health\"",
        "\"HandTotal\"",
        "\"DeckTotal\"",
        "\"ManaPoolItems\"",
        "\"ColorString\"",
        "\"Count\"",
        "\"IsPromptBoxActive\"",
        "\"Text\"",
        "\"StandardButtons\"",
        "\"Visible\"",
        "\"Enabled\"",
        "\"Name\"",
    ] {
        assert!(
            source.contains(marker),
            "missing exact visible getter: {marker}"
        );
    }
    assert!(source.contains("TryBuildFirstSanitizedVisibleDecisionV1("));
    assert!(source.contains("return built ? visibleDecision : ProjectionIncomplete;"));
    assert!(!source.contains("JavaScriptSerializer"));
}

#[test]
fn managed_producer_never_enumerates_libraries_and_gates_hidden_names() {
    let source = producer_source_v1();
    assert!(source.contains("TryValidateNeverEnumeratedZoneRootV1(libraryZone)"));
    assert!(!source.contains("TryValidateEnumeratedVisibleZoneV1(libraryZone"));
    assert!(source.contains("TryValidateConditionalVisibleZoneV1(\n                        handZone,\n                        localPlayer,\n                        localPlayer)"));
    assert!(source.contains("if (faceDown && !faceDownNameVisible)"));
    assert!(source.contains("if (!enumerateRegardless && !visible)"));
    assert!(source.contains("if (localPlayer)\n                    {\n                        seatedPlayerVisibleActionCards.Add(card);"));
    assert!(source.contains(
        "TryValidatePrivateVisibleCardActionJoinsV1(\n                seatedPlayerVisibleActionCards)"
    ));
    assert!(!source.contains(
        "TryValidatePrivateVisibleCardActionJoinsV1(\n                battlefieldCards)"
    ));
}
