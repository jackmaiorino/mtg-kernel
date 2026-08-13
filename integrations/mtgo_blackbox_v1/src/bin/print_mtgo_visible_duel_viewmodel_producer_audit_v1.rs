fn main() {
    let audit = mtgo_blackbox_v1::mtgo_visible_duel_viewmodel_producer_audit_v1();
    match mtgo_blackbox_v1::check_untrusted_visible_duel_viewmodel_producer_audit_v1(audit) {
        Ok(checked) => {
            let output = serde_json::json!({
                "producer_audit_commitment_sha256": checked.commitment_sha256_v1(),
                "allowed_property_count": checked.allowed_property_count_v1(),
                "visible_chrome_getter_layer_present": checked.visible_chrome_getter_layer_present_v1(),
                "visible_chrome_getter_count": checked.visible_chrome_getter_count_v1(),
                "producer_execution_attested": checked.producer_execution_attested_v1(),
                "full_projection_implemented": checked.full_projection_implemented_v1(),
                "safe_for_live_semantic_evidence": checked.safe_for_live_semantic_evidence_v1(),
                "safe_for_model_scoring": checked.safe_for_model_scoring_v1(),
                "safe_for_input": checked.safe_for_input_v1(),
            });
            match serde_json::to_string_pretty(&output) {
                Ok(serialized) => println!("{serialized}"),
                Err(error) => {
                    eprintln!("MTGO_VISIBLE_DUEL_VIEWMODEL_PRODUCER_AUDIT_REJECTED:{error}");
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!(
                "MTGO_VISIBLE_DUEL_VIEWMODEL_PRODUCER_AUDIT_REJECTED:{}:{}",
                error.code(),
                error.detail()
            );
            std::process::exit(1);
        }
    }
}
