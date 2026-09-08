#[cfg(not(target_os = "windows"))]
compile_error!("check_mtgo_player_visible_duel_gesture_target_schema_v1 is Windows-only");

fn main() {
    let review = mtgo_dxgi_capture_v1::review_player_visible_duel_gesture_target_schema_v1();
    match serde_json::to_string_pretty(&review) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("MTGO_PLAYER_VISIBLE_DUEL_GESTURE_TARGET_SCHEMA_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
