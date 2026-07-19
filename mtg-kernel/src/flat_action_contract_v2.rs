//! Build-validated constants generated from the narrow V2 action contract.
//!
//! The semantic source is `data/flat_policy_v2/action_contract_v2.json`.
//! Keeping these constants in a separate module gives the action runtime and
//! scorer binding one shared commitment domain and one shared digest identity.

include!(concat!(env!("OUT_DIR"), "/flat_action_contract_v2.rs"));
