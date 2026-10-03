fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: public_cost_catalog_v1 NEW_OUTPUT.json")?;
    let catalog = mtg_kernel::public_cost_features_v1::catalog_v1()?;
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    serde_json::to_writer_pretty(&mut output, catalog)?;
    output.sync_all()?;
    Ok(())
}
