//! D3 search wrapper on the public collector's opponent seat. The wrapper is
//! the unchanged evaluation `SearchPlayV3`; budget, algorithm, sampler and
//! descriptor are not parameters here. See docs/search_opponent_collection_v1.md.

/// Byte copy of the reviewed D3 descriptor,
/// E:/mtg-g115-lineage-20260923/d3-search-descriptor-reviewed.json.
#[cfg(test)]
pub(crate) const REVIEWED_DESCRIPTOR_JSON: &str = r#"{"algorithm":"v4-depth-keyed-estimate-library-independent-chance/v3","depth":8,"embedding_table_sha256":"9f2ba50d7097345caf1930edd2e911bad87383524b30bbe09726fb344096f2bb","experiment_seed":20260922,"feature_contract_digest":"c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b","feature_encoding_digest":"271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de","interior_bonus":"prior_free","model_parameter_sha256":"614326d2ec55c94583b1b050451f770ce9404e03bc21b9fb5fb6cb4f7d32263f","root_allocation":"round_robin","schema":"mtg-kernel-v4-information-set-estimate-search/v3","simulations":128,"transitions":1024,"weights_sha256":"e2ca2f2b5dd750a59e24c71a4bac325ed7449d97b5892a79a80132e45d538333"}"#;
#[cfg(test)]
pub(crate) const REVIEWED_DESCRIPTOR_SHA256: &str =
    "5eb1d55d13b78b341f8ff0c4df2589f8ee725974dc9fcadd691db6e12b2133d7";

#[cfg(test)]
mod boundary_tests;
