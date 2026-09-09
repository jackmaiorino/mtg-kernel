//! Prints the deployment slot report for the placeholder deployment of the
//! canonical Burn mainboard. Non-actuating: reads no MTGO process state,
//! captures no pixels, sends no input, and grants no authority.

#[cfg(not(target_os = "windows"))]
compile_error!("check_mtgo_deployment_slots_v1 is Windows-only");

use mtgo_dxgi_capture_v1::{
    build_placeholder_deployment_slots_v1, MtgoCompetitiveNativeSideboardCardCountV1,
    MtgoCompetitiveNativeSideboardConfigurationV1,
};

fn main() {
    let deck = MtgoCompetitiveNativeSideboardConfigurationV1 {
        mainboard: vec![
            MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Lightning Bolt".to_owned(),
                count: 4,
            },
            MtgoCompetitiveNativeSideboardCardCountV1 {
                visible_card_name: "Mountain".to_owned(),
                count: 56,
            },
        ],
        sideboard: vec![MtgoCompetitiveNativeSideboardCardCountV1 {
            visible_card_name: "Lightning Bolt".to_owned(),
            count: 15,
        }],
    };
    match build_placeholder_deployment_slots_v1(&deck) {
        Ok(slots) => {
            let report = slots.slot_report_v1();
            println!(
                "{}",
                serde_json::to_string_pretty(&report).expect("slot report serializes")
            );
        }
        Err(error) => {
            eprintln!("MTGO_DEPLOYMENT_SLOTS_V1_REJECTED:{error}");
            std::process::exit(1);
        }
    }
}
