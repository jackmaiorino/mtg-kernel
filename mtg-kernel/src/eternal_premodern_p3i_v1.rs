//! Card program for the eternal catalog's Premodern wave P3i
//! (`docs/design/premodern_shapeshifter_alter_reality_v1.md`, step C2):
//! **Alter Reality** ({1}{U}, Instant; Flashback {1}{U}).
//!
//! "Change the text of target spell or permanent by replacing all instances of
//! one color word with another. *(This effect lasts indefinitely.)*
//! Flashback {1}{U}"
//!
//! Both words are chosen on resolution (CR 608.2) via two sequential
//! `PendingEffectChoice::ChooseOption` prompts carrying
//! `EffectOptionChoicePurpose::ChooseTextColorWordV1`:
//! 1. Stage `From` (`from: None`): the controller picks the replaced color word
//!    from the five color words in WUBRG order.
//! 2. Stage `To` (`from: Some(from)`): the controller picks the replacement
//!    color word from the four other color words in WUBRG order.
//!
//! Target legality is checked by the engine before entering resolution
//! (CR 608.2b), so an illegal target fizzles with zero choices. After both
//! choices, `text_color_change_v1::apply_change` runs the 305-card+9-token
//! Premodern audit (halting with
//! `UnsupportedMechanic::TextColorChangeOutsideAuditV1` on an unaudited
//! face-up target) and records the live Layer-3 change (CR 613.1c, 613.7,
//! 613.8, 400.7a).
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::effect::{
    EffectContinuation, EffectOp, EffectOptionChoicePurpose, ExecCtx, PendingEffectChoice,
};
use crate::eternal_cards_v1::EternalOpV1;
use crate::mana::ManaColor;
use crate::standard_cards_v1::StandardOpV1;
use crate::state::{GameState, Target, Zone};
use crate::text_color_change_v1::{apply_change, choice_colors, TextColorWordStageV1};
use serde::{Deserialize, Serialize};

/// Effect leaves owned by Premodern wave P3i, stored under
/// `eternal_cards_v1::EternalOpV1::P3iV1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum P3iOpV1 {
    /// Alter Reality stage 1: offer the five color words (`From`) in WUBRG
    /// order to the spell's controller.
    AlterRealityV1,
    /// Alter Reality stage 2: offer the four remaining color words (`To`) in
    /// WUBRG order after `from` was chosen.
    ChooseNewColorV1 { from: ManaColor },
    /// Alter Reality final step: audit the target and record `(from -> to)` on
    /// `state.text_color_changes_v1`.
    ApplyColorChangeV1 { from: ManaColor, to: ManaColor },
}

impl P3iOpV1 {
    pub(crate) fn contains_player_choice(&self) -> bool {
        matches!(self, Self::AlterRealityV1 | Self::ChooseNewColorV1 { .. })
    }
}

fn p3i(op: P3iOpV1) -> EffectOp {
    EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::P3iV1(op)))
}

/// Alter Reality: "Change the text of target spell or permanent by replacing
/// all instances of one color word with another."
pub fn alter_reality() -> EffectOp {
    p3i(P3iOpV1::AlterRealityV1)
}

fn target_is_live(state: &GameState, ctx: &ExecCtx) -> Option<crate::ids::ObjectId> {
    let &Target::Object(target) = ctx.targets.first()? else {
        return None;
    };
    state
        .objects
        .try_get(target)
        .is_some_and(|live| matches!(live.zone, Zone::Battlefield | Zone::Stack))
        .then_some(target)
}

/// Resumable driver for `P3iOpV1`, called from `eternal_cards_v1::e3::drive`.
pub(crate) fn drive(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    op: &P3iOpV1,
    path: Vec<u16>,
) -> Result<bool, String> {
    match op {
        P3iOpV1::AlterRealityV1 => {
            if target_is_live(state, &continuation.ctx).is_none() {
                return Ok(false);
            }
            let colors = choice_colors(TextColorWordStageV1::From, None)
                .ok_or("Alter Reality stage From has no color options")?;
            let options = colors
                .into_iter()
                .map(|from| p3i(P3iOpV1::ChooseNewColorV1 { from }))
                .collect();
            continuation.choice = Some(PendingEffectChoice::ChooseOption {
                player: continuation.ctx.controller,
                options,
                purpose: EffectOptionChoicePurpose::ChooseTextColorWordV1 {
                    stage: TextColorWordStageV1::From,
                    from: None,
                },
                path,
            });
            Ok(true)
        }
        P3iOpV1::ChooseNewColorV1 { from } => {
            if target_is_live(state, &continuation.ctx).is_none() {
                return Ok(false);
            }
            let colors = choice_colors(TextColorWordStageV1::To, Some(*from))
                .ok_or("Alter Reality stage To received an invalid source color")?;
            let options = colors
                .into_iter()
                .map(|to| p3i(P3iOpV1::ApplyColorChangeV1 { from: *from, to }))
                .collect();
            continuation.choice = Some(PendingEffectChoice::ChooseOption {
                player: continuation.ctx.controller,
                options,
                purpose: EffectOptionChoicePurpose::ChooseTextColorWordV1 {
                    stage: TextColorWordStageV1::To,
                    from: Some(*from),
                },
                path,
            });
            Ok(true)
        }
        P3iOpV1::ApplyColorChangeV1 { .. } => {
            execute(op, &continuation.ctx, state);
            Ok(false)
        }
    }
}

/// Non-suspending execution for `P3iOpV1`.
pub(crate) fn execute(op: &P3iOpV1, ctx: &ExecCtx, state: &mut GameState) {
    if let P3iOpV1::ApplyColorChangeV1 { from, to } = op {
        let Some(target) = target_is_live(state, ctx) else {
            return;
        };
        let _ = apply_change(state, ctx.source, target, *from, *to);
    }
}
