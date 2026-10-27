//! Consolidated integration tests; CI runs each module in its own process.

#[path = "eternal/eternal_catalog_v1.rs"]
mod eternal_catalog_v1;
#[path = "eternal/eternal_legacy_e2_v1.rs"]
mod eternal_legacy_e2_v1;
#[path = "eternal/eternal_legacy_e3_v1.rs"]
mod eternal_legacy_e3_v1;
#[path = "eternal/eternal_legacy_e5b_v1.rs"]
mod eternal_legacy_e5b_v1;
#[path = "eternal/eternal_legacy_e7a_v1.rs"]
mod eternal_legacy_e7a_v1;
#[path = "eternal/eternal_pauper_e1_v1.rs"]
mod eternal_pauper_e1_v1;
#[path = "eternal/eternal_pauper_e6a_v1.rs"]
mod eternal_pauper_e6a_v1;
#[path = "eternal/eternal_vintage_e4_v1.rs"]
mod eternal_vintage_e4_v1;
#[path = "eternal/eternal_vintage_e4b_v1.rs"]
mod eternal_vintage_e4b_v1;
#[path = "eternal/eternal_vintage_e5a_v1.rs"]
mod eternal_vintage_e5a_v1;
