#[cfg(target_os = "windows")]
use mtgo_blackbox_v1::{
    check_untrusted_authorization_correspondence_v1, MtgoAuthorizationCorrespondenceReviewV1,
    MtgoCompetitiveEventKindV1,
};
#[cfg(target_os = "windows")]
use mtgo_dxgi_capture_v1::review_competitive_duel_pass_ratification_candidate_from_correspondence_v2;
#[cfg(target_os = "windows")]
use serde_json::json;
#[cfg(target_os = "windows")]
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::fs;
#[cfg(target_os = "windows")]
use std::path::Path;
use std::process::ExitCode;

#[cfg(target_os = "windows")]
const MAX_REVIEW_BYTES_V2: u64 = 1_048_576;
#[cfg(target_os = "windows")]
const MAX_CORRESPONDENCE_BYTES_V2: u64 = 4 * 1_048_576;
#[cfg(target_os = "windows")]
const MAX_ACCOUNT_ALIAS_BYTES_V2: u64 = 64;

#[cfg(target_os = "windows")]
fn main() -> ExitCode {
    match run_v2(std::env::args_os().collect()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() -> ExitCode {
    eprintln!("this correspondence ratification review command requires Windows");
    ExitCode::FAILURE
}

#[cfg(target_os = "windows")]
fn run_v2(args: Vec<OsString>) -> Result<String, String> {
    if args.len() != 4 {
        return Err(
            "usage: review_mtgo_authorization_correspondence_v2 <review.json> <exact-correspondence.bin> <exact-account-alias.txt>"
                .to_owned(),
        );
    }
    let review_bytes = read_bounded_v2(
        Path::new(&args[1]),
        MAX_REVIEW_BYTES_V2,
        "correspondence review",
    )?;
    let correspondence_bytes = read_bounded_v2(
        Path::new(&args[2]),
        MAX_CORRESPONDENCE_BYTES_V2,
        "exact correspondence",
    )?;
    let account_alias_bytes = read_bounded_v2(
        Path::new(&args[3]),
        MAX_ACCOUNT_ALIAS_BYTES_V2,
        "exact account alias",
    )?;
    let visible_account_alias = std::str::from_utf8(&account_alias_bytes)
        .map_err(|error| format!("exact account alias is not UTF-8: {error}"))?;
    if visible_account_alias.trim() != visible_account_alias {
        return Err(
            "exact account alias file must contain only the alias with no newline or padding"
                .to_owned(),
        );
    }
    let review: MtgoAuthorizationCorrespondenceReviewV1 = serde_json::from_slice(&review_bytes)
        .map_err(|error| format!("parse correspondence review JSON: {error}"))?;
    let checked = check_untrusted_authorization_correspondence_v1(
        review,
        &correspondence_bytes,
        visible_account_alias,
    )
    .map_err(|error| format!("check exact correspondence review: {error}"))?;

    let mut modes = Vec::new();
    for event_kind in checked.approved_competitive_modes() {
        let candidate = review_competitive_duel_pass_ratification_candidate_from_correspondence_v2(
            &checked,
            visible_account_alias,
            *event_kind,
        )?;
        let event_kind_label = match event_kind {
            MtgoCompetitiveEventKindV1::League => "league",
            MtgoCompetitiveEventKindV1::Challenge => "challenge",
        };
        modes.push(json!({
            "event_kind": event_kind_label,
            "mode_authorization_commitment_sha256": candidate.mode_authorization_commitment_sha256,
            "ratification_commitment_sha256": candidate.ratification_commitment_sha256,
            "safe_for_live_input": candidate.safe_for_live_input_v2(),
            "permits_event_entry": candidate.permits_event_entry_v2(),
            "permits_spending": candidate.permits_spending_v2(),
        }));
    }
    serde_json::to_string_pretty(&json!({
        "schema": "mtgo-reviewed-competitive-pass-ratification-candidates/v2",
        "correspondence_sha256": checked.correspondence_sha256(),
        "approved_account_alias_sha256": checked.approved_account_alias_sha256(),
        "permission_review_commitment_sha256": checked.review_commitment_sha256(),
        "correspondence_byte_length": checked.correspondence_byte_length(),
        "modes": modes,
        "private_correspondence_bytes_retained": false,
        "account_alias_text_emitted": false,
        "safe_for_live_input": false,
        "permits_event_entry": false,
        "permits_spending": false,
    }))
    .map_err(|error| format!("serialize ratification candidate report: {error}"))
}

#[cfg(target_os = "windows")]
fn read_bounded_v2(path: &Path, maximum_bytes: u64, label: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("inspect {label} file: {error}"))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(format!(
            "{label} file must be nonempty, regular, and at most {maximum_bytes} bytes"
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("read {label} file: {error}"))?;
    if bytes.len() as u64 != metadata.len() {
        return Err(format!("{label} file changed while it was read"));
    }
    Ok(bytes)
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use mtgo_blackbox_v1::MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1;
    use sha2::{Digest, Sha256};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn report_emits_only_non_authorizing_commitments_for_each_reviewed_mode() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "mtgo-correspondence-review-v2-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        let correspondence = b"Daybreak visible-only League and Challenge approval fixture";
        let account_alias = "UnbuckledPie";
        let review = MtgoAuthorizationCorrespondenceReviewV1 {
            schema_version: MTGO_AUTHORIZATION_CORRESPONDENCE_REVIEW_SCHEMA_V1,
            review_id: "review-fixture-v2".to_owned(),
            correspondence_sha256: format!("{:x}", Sha256::digest(correspondence)),
            approved_account_alias_sha256: format!(
                "{:x}",
                Sha256::digest(account_alias.as_bytes())
            ),
            approved_competitive_modes: vec![
                MtgoCompetitiveEventKindV1::League,
                MtgoCompetitiveEventKindV1::Challenge,
            ],
            visible_channels_only: true,
            hidden_information_access_prohibited: true,
            hidden_information_reverse_engineering_prohibited: true,
            cheating_or_hacking_prohibited: true,
            account_owner_event_entry_confirmation_required: true,
            account_owner_spending_confirmation_required: true,
            reviewer_alias_sha256: "a".repeat(64),
            reviewed_at_utc: "2026-08-10T20:00:00Z".to_owned(),
        };
        let review_path = directory.join("review.json");
        let correspondence_path = directory.join("correspondence.bin");
        let alias_path = directory.join("alias.txt");
        fs::write(&review_path, serde_json::to_vec(&review).unwrap()).unwrap();
        fs::write(&correspondence_path, correspondence).unwrap();
        fs::write(&alias_path, account_alias).unwrap();

        let output = run_v2(vec![
            OsString::from("review_mtgo_authorization_correspondence_v2"),
            review_path.as_os_str().to_owned(),
            correspondence_path.as_os_str().to_owned(),
            alias_path.as_os_str().to_owned(),
        ])
        .unwrap();
        assert!(!output.contains(account_alias));
        assert!(!output.contains("approval fixture"));
        let value: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(value["modes"].as_array().unwrap().len(), 2);
        assert_eq!(value["safe_for_live_input"], false);
        assert_eq!(value["permits_event_entry"], false);
        assert_eq!(value["permits_spending"], false);

        fs::write(&alias_path, format!("{account_alias}\n")).unwrap();
        assert!(run_v2(vec![
            OsString::from("review_mtgo_authorization_correspondence_v2"),
            review_path.as_os_str().to_owned(),
            correspondence_path.as_os_str().to_owned(),
            alias_path.as_os_str().to_owned(),
        ])
        .is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
