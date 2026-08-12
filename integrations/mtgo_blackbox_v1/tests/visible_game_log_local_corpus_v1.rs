use mtgo_blackbox_v1::{
    classify_checked_untrusted_mtgo_visible_game_log_semantics_v1,
    parse_checked_untrusted_mtgo_visible_game_log_v1,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
#[ignore = "requires local MTGO ClickOnce Game Log files"]
fn every_unique_local_visible_game_log_parses_and_semantically_projects() {
    let local_app_data = std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA");
    let root = PathBuf::from(local_app_data)
        .join("Apps")
        .join("2.0")
        .join("Data");
    let mut paths = Vec::new();
    collect_game_logs_v1(&root, 0, &mut paths).unwrap();
    let mut source_hashes = HashSet::new();
    let mut unique = 0_usize;
    let mut seated_player_semantic_successes = 0_usize;
    let mut spectator_shape_rejections = 0_usize;
    for path in paths {
        let bytes = fs::read(path).unwrap();
        let source = parse_checked_untrusted_mtgo_visible_game_log_v1(&bytes).unwrap();
        if !source_hashes.insert(source.source_file_sha256_v1().to_owned()) {
            continue;
        }
        match classify_checked_untrusted_mtgo_visible_game_log_semantics_v1(&source, "UnbuckledPie")
        {
            Ok(semantics) => {
                assert_eq!(
                    semantics.classified_source_record_count_v1()
                        + semantics.unclassified_source_record_count_v1(),
                    source.record_count_v1()
                );
                seated_player_semantic_successes += 1;
            }
            Err(error) if error.code() == "visible_game_log_semantic_players" => {
                spectator_shape_rejections += 1;
            }
            Err(error) => panic!("{}: {}", error.code(), error.detail()),
        }
        unique += 1;
    }
    assert!(unique >= 15, "expected the reviewed local corpus");
    assert!(seated_player_semantic_successes > 0);
    assert!(spectator_shape_rejections > 0);
}

fn collect_game_logs_v1(
    directory: &Path,
    depth: usize,
    paths: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if depth > 12 {
        return Err("local corpus depth exceeded".to_owned());
    }
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            return Err("local corpus contains a symbolic link".to_owned());
        }
        if file_type.is_dir() {
            collect_game_logs_v1(&entry.path(), depth + 1, paths)?;
        } else if file_type.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("Match_GameLog_") && name.ends_with(".dat"))
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}
