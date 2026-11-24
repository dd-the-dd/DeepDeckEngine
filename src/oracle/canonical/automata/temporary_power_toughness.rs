use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectDurationNode, GRANTED_OBJECT_TYPE_LINE, GrantedKeyword, NumericExpression,
    PowerToughnessDeltaNode, QuotedAbilityNode, RecognizedEffectFragment, TargetListNode,
    recognize_effect_owned_granted_ability_list, recognize_granted_ability_list,
    recognize_signed_numeric_power_toughness_delta, recognize_target_list,
    recognize_trailing_or_supplied_duration,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TemporaryPowerToughnessNode<Subject = TargetListNode> {
    pub targets: Subject,
    pub exclude_source_from_first_target: bool,
    pub delta: PowerToughnessDeltaNode<NumericExpression>,
    pub keywords: Vec<GrantedKeyword>,
    pub abilities: Vec<QuotedAbilityNode>,
    pub duration: EffectDurationNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemporaryPowerToughnessState {
    OptionalOther,
    Targets,
    Action,
    Delta,
    OptionalKeywordGrant,
    Duration,
    Complete,
}

/// Recognizes the generic targeted temporary-stat grammar. Selector, signed
/// statistics, keyword list, and duration remain independent child machines;
/// the parent only owns their Oracle ordering.
pub(in crate::oracle::canonical) fn recognize_temporary_power_toughness(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RecognizedEffectFragment<TemporaryPowerToughnessNode>> {
    recognize_temporary_power_toughness_subject(cursor, recognize_target_list, None)
}

/// The payload grammar is shared; only the typed subject reader changes.
pub(in crate::oracle::canonical) fn recognize_temporary_power_toughness_subject<
    'tokens,
    Subject,
>(
    cursor: &mut OracleTokenCursor<'tokens>,
    mut subject: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    preposed_duration: Option<&EffectDurationNode>,
) -> MachineResult<RecognizedEffectFragment<TemporaryPowerToughnessNode<Subject>>> {
    let mut state = TemporaryPowerToughnessState::OptionalOther;
    let mut exclude_source_from_first_target = false;
    let mut targets = None;
    let mut delta = None;
    let mut keywords = Vec::new();
    let mut abilities = Vec::new();
    let mut duration = None;
    let mut leading_grant = false;

    loop {
        state = match state {
            TemporaryPowerToughnessState::OptionalOther => {
                cursor.enter("effect.temporary-power-toughness.optional-other");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("another"))
                {
                    cursor.consume("another")?;
                    exclude_source_from_first_target = true;
                }
                TemporaryPowerToughnessState::Targets
            }
            TemporaryPowerToughnessState::Targets => {
                cursor.enter("effect.temporary-power-toughness.targets");
                targets = Some(subject(cursor)?);
                TemporaryPowerToughnessState::Action
            }
            TemporaryPowerToughnessState::Action => {
                cursor.enter("effect.temporary-power-toughness.action");
                if gain_verb_at(cursor, 0) {
                    // `gain KEYWORDS and get +N/+N DURATION` (CR 611.2a): both
                    // coordinated predicates share the subject and the one
                    // trailing duration, like the `get ... and gain` order.
                    cursor.enter("effect.temporary-power-toughness.leading-keyword-grant");
                    let action = cursor.consume_type("actionWord")?;
                    cursor.require(
                        gain_verb(&action.raw),
                        "`gain` or `gains` before the coordinated stat change",
                    )?;
                    let list = recognize_effect_owned_granted_ability_list(
                        cursor,
                        GRANTED_OBJECT_TYPE_LINE,
                    )?;
                    keywords = list.keywords;
                    abilities = list.abilities;
                    leading_grant = true;
                    cursor.enter("effect.temporary-power-toughness.leading-keyword-grant.and");
                    cursor.consume("and")?;
                }
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("get")
                        || action.raw.eq_ignore_ascii_case("gets"),
                    "`get` or `gets`",
                )?;
                TemporaryPowerToughnessState::Delta
            }
            TemporaryPowerToughnessState::Delta => {
                cursor.enter("effect.temporary-power-toughness.delta");
                delta = Some(recognize_signed_numeric_power_toughness_delta(cursor)?);
                TemporaryPowerToughnessState::OptionalKeywordGrant
            }
            TemporaryPowerToughnessState::OptionalKeywordGrant => {
                cursor.enter("effect.temporary-power-toughness.optional-keyword-grant");
                // `and` belongs to this action only when it introduces a
                // keyword grant of the same subject; any other continuation
                // (`and target creature gets ...`) is a sequence connector.
                if !leading_grant
                    && cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    && gain_verb_at(cursor, 1)
                {
                    cursor.consume("and")?;
                    let action = cursor.consume_type("actionWord")?;
                    cursor.require(
                        gain_verb(&action.raw),
                        "`gain` or `gains` after the effect connector",
                    )?;
                    let list = recognize_granted_ability_list(cursor, GRANTED_OBJECT_TYPE_LINE)?;
                    keywords = list.keywords;
                    abilities = list.abilities;
                }
                TemporaryPowerToughnessState::Duration
            }
            TemporaryPowerToughnessState::Duration => {
                cursor.enter("effect.temporary-power-toughness.duration");
                duration = Some(recognize_trailing_or_supplied_duration(
                    cursor,
                    preposed_duration,
                )?);
                TemporaryPowerToughnessState::Complete
            }
            TemporaryPowerToughnessState::Complete => {
                cursor.enter("effect.temporary-power-toughness.complete");
                return Ok(RecognizedEffectFragment {
                    node: TemporaryPowerToughnessNode {
                        targets: cursor.require_value(targets, "one or more target slots")?,
                        exclude_source_from_first_target,
                        delta: cursor.require_value(delta, "a signed power/toughness delta")?,
                        keywords,
                        abilities,
                        duration: cursor.require_value(duration, "an effect duration")?,
                    },
                    end_position: cursor.position(),
                });
            }
        };
    }
}

fn gain_verb(raw: &str) -> bool {
    is_verb_form(raw, VerbLemma::Gain, VerbForm::Base)
        || is_verb_form(raw, VerbLemma::Gain, VerbForm::ThirdPersonSingular)
}

fn gain_verb_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(gain_verb)
}

/// True at `gain(s) KEYWORDS and get(s)`: a keyword grant coordinated with a
/// stat change of the same subject. The predicate parent routes this clause
/// to the stat machine, which owns the shared trailing duration; a grant
/// followed by anything else stays a plain keyword grant.
pub(in crate::oracle::canonical) fn granted_stats_coordination_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    if !gain_verb_at(cursor, 0) {
        return false;
    }
    let mut trial = cursor.clone();
    trial.consume_type("actionWord").is_ok()
        && recognize_effect_owned_granted_ability_list(&mut trial, GRANTED_OBJECT_TYPE_LINE).is_ok()
        && trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
        && trial.peek_ahead(1).is_some_and(|raw| {
            is_verb_form(raw, VerbLemma::Get, VerbForm::Base)
                || is_verb_form(raw, VerbLemma::Get, VerbForm::ThirdPersonSingular)
        })
}

#[cfg(test)]
#[path = "temporary_power_toughness_tests.rs"]
mod tests;
