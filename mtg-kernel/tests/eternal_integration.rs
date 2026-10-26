//! Consolidated integration tests; CI runs each module in its own process.

#[path = "eternal/eternal_catalog_v1.rs"]
mod eternal_catalog_v1;
#[path = "eternal/eternal_legacy_e2_v1.rs"]
mod eternal_legacy_e2_v1;
#[path = "eternal/eternal_pauper_e1_v1.rs"]
mod eternal_pauper_e1_v1;
