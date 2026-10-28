//! Genuine FDN154 engine source. Partial printing, no consumer admission.
use crate::{
    effect::EffectOp,
    state::{StackItem, StackItemKind},
};
pub(crate) fn root() -> EffectOp {
    EffectOp::CreateAnotherControlledNonlandTargetCopyV1
}
pub(crate) fn owns_item(item: &StackItem) -> bool {
    item.kind == StackItemKind::TriggeredAbility && item.inline_effect.as_ref() == Some(&root())
}
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
