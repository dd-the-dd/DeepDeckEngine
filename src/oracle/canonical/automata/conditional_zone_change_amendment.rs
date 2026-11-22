use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ConditionalZoneChangeAmendmentNode, TargetBinding, ZoneChangeAction,
    recognize_bound_object_numeric_condition,
    recognize_controlled_permanent_left_battlefield_this_turn_condition,
};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_card_type, lookup_object_domain,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AmendmentState {
    AbilityWord,
    LabelSeparator,
    Action,
    TargetAnaphor,
    TargetNoun,
    TargetCondition,
    Instead,
    AmendmentCondition,
    Terminator,
    Complete,
}

/// Recognizes a replacement of an earlier spell effect, usually printed after
/// an ability word. The label has no rules meaning (CR 207.2c); the actual condition
/// and the inherited target are represented explicitly.
pub(in crate::oracle::canonical) fn recognize_conditional_zone_change_amendment(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionalZoneChangeAmendmentNode> {
    let mut state = AmendmentState::AbilityWord;
    let mut action = None;
    let mut target_condition = None;
    let mut amendment_condition = None;
    let inherited_target = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    loop {
        state = match state {
            AmendmentState::AbilityWord => {
                // The ability boundary strips a top-level label (CR 207.2c);
                // a label is read here only when this reader runs unlabeled
                // input directly.
                cursor.enter("zone-change-amendment.ability-word");
                if cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "abilityWord")
                {
                    while cursor
                        .peek_token()
                        .is_some_and(|token| token.token_type == "abilityWord")
                    {
                        cursor.consume_type("abilityWord")?;
                    }
                    AmendmentState::LabelSeparator
                } else {
                    AmendmentState::Action
                }
            }
            AmendmentState::LabelSeparator => {
                cursor.enter("zone-change-amendment.label-separator");
                cursor.consume("—")?;
                AmendmentState::Action
            }
            AmendmentState::Action => {
                cursor.enter("zone-change-amendment.action");
                action = Some(
                    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("destroy") => {
                            cursor.consume("destroy")?;
                            ZoneChangeAction::Destroy
                        }
                        Some("exile") => {
                            crate::oracle::canonical::automata::consume_lemma(
                                cursor,
                                crate::oracle::canonical::automata::VerbLemma::Exile,
                                crate::oracle::canonical::automata::VerbAgreement::Base,
                            )?;
                            ZoneChangeAction::Exile
                        }
                        _ => return Err(cursor.fail("`destroy` or `exile`")),
                    },
                );
                AmendmentState::TargetAnaphor
            }
            AmendmentState::TargetAnaphor => {
                cursor.enter("zone-change-amendment.target-anaphor");
                cursor.consume("that")?;
                AmendmentState::TargetNoun
            }
            AmendmentState::TargetNoun => {
                cursor.enter("zone-change-amendment.target-noun");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a permanent-domain target noun"))?
                    .to_string();
                let supported = lookup_card_type(&raw, AcceptedForms::SingularOnly)
                    .is_some_and(|card_type| card_type.is_permanent_type())
                    || lookup_object_domain(&raw, AcceptedForms::SingularOnly)
                        .is_some_and(|domain| domain.oracle_singular() == "permanent");
                cursor.require(supported, "a permanent-domain target noun")?;
                cursor.advance_to(cursor.position() + 1)?;
                AmendmentState::TargetCondition
            }
            AmendmentState::TargetCondition => {
                cursor.enter("zone-change-amendment.target-condition");
                target_condition = Some(recognize_bound_object_numeric_condition(
                    cursor,
                    inherited_target,
                )?);
                AmendmentState::Instead
            }
            AmendmentState::Instead => {
                cursor.enter("zone-change-amendment.instead");
                cursor.consume("instead")?;
                AmendmentState::AmendmentCondition
            }
            AmendmentState::AmendmentCondition => {
                cursor.enter("zone-change-amendment.condition");
                amendment_condition = Some(
                    recognize_controlled_permanent_left_battlefield_this_turn_condition(cursor)?,
                );
                AmendmentState::Terminator
            }
            AmendmentState::Terminator => {
                cursor.enter("zone-change-amendment.terminator");
                cursor.consume(".")?;
                AmendmentState::Complete
            }
            AmendmentState::Complete => {
                cursor.enter("zone-change-amendment.complete");
                cursor.finish()?;
                return Ok(ConditionalZoneChangeAmendmentNode {
                    action: cursor.require_value(action, "a zone-change amendment action")?,
                    target_condition: cursor
                        .require_value(target_condition, "a condition over the inherited target")?,
                    amendment_condition: cursor
                        .require_value(amendment_condition, "a condition enabling the amendment")?,
                });
            }
        };
    }
}
