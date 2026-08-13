use mtgo_blackbox_v1::mtgo_visible_duel_viewmodel_candidate_surface_v1;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

fn producer_source_v1() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(
        manifest_dir
            .parent()
            .expect("integrations directory")
            .join("mtgo_visible_duel_producer_v1")
            .join("VisibleDuelProducerV1.cs"),
    )
    .expect("read direct-source producer")
}

fn compiled_getter_entries_v1(source: &str) -> HashSet<String> {
    let start = source
        .find("private static readonly string[] AllowedGetters")
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
fn managed_producer_has_no_raw_output_or_side_effect_api_markers() {
    let source = producer_source_v1();
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
        "GetValue(",
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
    assert!(source.contains("? ProjectionIncomplete"));
}
