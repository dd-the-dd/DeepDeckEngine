use super::super::*;
use crate::oracle::characteristic_vocabulary::{AcceptedForms, CardType, lookup_card_type};
use crate::oracle::syntax::{
    AbilityInput, classify_promoted_ability_from_tokens, root_activation_effect_start,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EffectProgramContext {
    Spell,
    Activated,
    Mana,
}

pub(in crate::oracle::canonical) enum EffectProgramParseAttempt {
    Parsed(CanonicalRuleDraft),
    Unpromoted(MachineFailure),
    Failed(MachineFailure),
    FailedSupport(MachineFailure),
    NotApplicable,
}

fn spell_face(type_line: &str) -> bool {
    type_line
        .split('\u{2014}')
        .next()
        .unwrap_or(type_line)
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .any(|word| {
            matches!(
                lookup_card_type(word, AcceptedForms::SingularOnly),
                Some(CardType::Instant | CardType::Sorcery)
            )
        })
}

/// A root replacement event is a different envelope from an imperative
/// program. `if` alone is not a discriminator: ordinary conditional effects
/// remain available to their shared child. Quotes and reminders own their text.
fn root_replacement_event(input: &AbilityInput<'_>) -> bool {
    if !input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"))
    {
        return false;
    }
    let mut quoted = false;
    let mut parentheses = 0_usize;
    let mut before_separator = true;
    let mut would = false;
    let mut instead = false;
    for token in input.tokens {
        match token.raw.as_str() {
            "\"" => quoted = !quoted,
            "(" if !quoted => parentheses += 1,
            ")" if !quoted => parentheses = parentheses.saturating_sub(1),
            _ if !quoted && parentheses == 0 => {
                if token.raw == "." {
                    break;
                }
                if token.raw == "," {
                    before_separator = false;
                }
                would |= before_separator && token.raw.eq_ignore_ascii_case("would");
                instead |= token.raw.eq_ignore_ascii_case("instead");
            }
            _ => {}
        }
    }
    would && instead
}

fn admit_context(input: &AbilityInput<'_>) -> Option<EffectProgramContext> {
    if claims_promoted_loyalty_ability_family(input)
        || matches!(
            classify_promoted_ability_from_tokens(input),
            Some("triggeredAbility" | "replacementEffect")
        )
        || root_replacement_event(input)
    {
        return None;
    }
    if root_activation_effect_start(input).is_some() {
        Some(EffectProgramContext::Activated)
    } else if spell_face(input.face_type_line) {
        Some(EffectProgramContext::Spell)
    } else {
        None
    }
}

fn activation_zone_from_costs(
    cursor: &mut OracleTokenCursor<'_>,
    costs: &ActivationCostSequenceNode,
) -> MachineResult<Option<&'static str>> {
    let requirements = costs.source_zone_requirements();
    let needs_hand = requirements.hand;
    let needs_battlefield = requirements.battlefield;
    cursor.enter("program.activation.source-zone-conflict");
    cursor.require(
        !(needs_hand && needs_battlefield),
        "one coherent source zone: discard-self costs require hand, while tap-self and sacrifice-self costs require battlefield",
    )?;
    Ok(needs_hand.then_some("hand"))
}

/// Context chooses when this program executes. The shared children choose
/// which actions it contains: no first-verb allowlist or combined-effect leaf.
/// Costs and body are each consumed once, then one shared lowering pass emits
/// independent primitive actions and declaration bindings.
pub(in crate::oracle::canonical) fn try_parse_contextual_effect_program(
    input: &AbilityInput<'_>,
) -> EffectProgramParseAttempt {
    let Some(context) = admit_context(input) else {
        return EffectProgramParseAttempt::NotApplicable;
    };
    let mut cursor = OracleTokenCursor::new("contextual-effect-program", input.tokens);
    // Activation X belongs to this activation. Never borrow the source's
    // retained casting X from a trigger-only recognition context.
    let recognition_context = EffectRecognitionContext::default();
    let (costs, effects) = match context {
        EffectProgramContext::Activated => {
            match try_recognize_activated_effect_sequence_with_context(
                &mut cursor,
                input.name_links,
                input.name_resolution_failure,
                &recognition_context,
            ) {
                ActivatedEffectSequenceParseAttempt::Parsed(program) => {
                    (Some(program.costs), program.effects)
                }
                ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure) => {
                    return EffectProgramParseAttempt::Unpromoted(failure);
                }
                ActivatedEffectSequenceParseAttempt::FailedCommitted(failure) => {
                    return EffectProgramParseAttempt::Failed(failure);
                }
            }
        }
        EffectProgramContext::Spell => {
            match try_recognize_effect_sequence_with_context(
                &mut cursor,
                input.name_links,
                input.name_resolution_failure,
                &recognition_context,
            ) {
                EffectSequenceParseAttempt::Parsed(effects) => (None, effects),
                EffectSequenceParseAttempt::RejectedCandidate(failure) => {
                    return EffectProgramParseAttempt::Unpromoted(failure);
                }
                EffectSequenceParseAttempt::FailedCommitted(failure) => {
                    return EffectProgramParseAttempt::Failed(failure);
                }
            }
        }
        EffectProgramContext::Mana => unreachable!("mana context is derived from typed effects"),
    };
    lower_contextual_effect_program(&mut cursor, context, costs, effects)
}

/// Lower already recognized nodes with one shared binding/context policy.
/// Both the general program parent and compact Add-led envelope call this
/// adapter without reading costs or effects again.
pub(in crate::oracle::canonical) fn lower_contextual_effect_program(
    cursor: &mut OracleTokenCursor<'_>,
    mut context: EffectProgramContext,
    costs: Option<ActivationCostSequenceNode>,
    effects: EffectSequenceNode,
) -> EffectProgramParseAttempt {
    let activation_zone = if let Some(costs) = costs.as_ref() {
        match activation_zone_from_costs(cursor, costs) {
            Ok(zone) => zone,
            Err(failure) => return EffectProgramParseAttempt::FailedSupport(failure),
        }
    } else {
        None
    };
    let lowered = match try_lower_local_effect_sequence_with_context(
        cursor,
        &effects,
        EffectSequenceLoweringContext::ValidateXAfterCardComposition,
    ) {
        EffectSequenceLoweringAttempt::Lowered(lowered) => lowered,
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure) => {
            return EffectProgramParseAttempt::Unpromoted(failure);
        }
        EffectSequenceLoweringAttempt::FailedSupport(failure) => {
            return EffectProgramParseAttempt::FailedSupport(failure);
        }
        EffectSequenceLoweringAttempt::FailedInvariant(failure) => {
            return EffectProgramParseAttempt::Failed(failure);
        }
    };
    let mut decisions = costs
        .as_ref()
        .map(lower_activation_cost_decisions)
        .unwrap_or_default();
    decisions.extend(lowered.decisions);
    if context == EffectProgramContext::Activated {
        let Some(costs) = costs.as_ref() else {
            unreachable!("activated programs retain their parsed activation costs");
        };
        let facts = ManaProgramFacts::from_effect_sequence(&effects);
        let requires_target = decisions
            .iter()
            .any(|decision| decision["kind"].as_str() == Some("chooseTargets"));
        if facts.qualifies_as_mana_ability(costs, requires_target) {
            if let Err(failure) = require_supported_mana_program(cursor, costs, &facts) {
                return EffectProgramParseAttempt::FailedSupport(failure);
            }
            context = EffectProgramContext::Mana;
        }
    }
    let mut rule = json!({
        "kind": match context {
            EffectProgramContext::Spell => "spellAbility",
            EffectProgramContext::Activated => "activatedAbility",
            EffectProgramContext::Mana => "manaAbility",
        },
        "source": self_ref(),
        "effects": lowered.effects,
    });
    if let Some(costs) = costs {
        rule["costs"] = json!(
            costs
                .costs
                .iter()
                .map(lower_local_activation_cost)
                .collect::<Vec<_>>()
        );
    }
    if let Some(zone) = activation_zone {
        rule["activationZone"] = json!(zone);
    }
    // Preserve the established activated-program declaration shape while
    // targetless spell and mana rules omit an unnecessary empty declaration.
    if context == EffectProgramContext::Activated || !decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        });
    }
    EffectProgramParseAttempt::Parsed(draft(
        rule,
        &[
            "Admit a program from its source-face and root ability envelope",
            "Consume typed costs and the shared effect sequence once",
            "Lower every action and binding through the shared primitive adapter",
            "Derive mana execution context from immediate typed actions and target declarations",
        ],
    ))
}

/// The compact envelope has already consumed one AddMana action and
/// its sentence-separated followups. Preserve that order in the ordinary
/// shared program AST; CR605.1a decides its context in the same lowerer.
pub(in crate::oracle::canonical) fn lower_recognized_add_led_activation(
    cursor: &mut OracleTokenCursor<'_>,
    ability: ActivatedManaAbilityNode,
) -> EffectProgramParseAttempt {
    let mut effects = vec![LocalEffectNode::AddMana(ability.effect)];
    let mut connectors = Vec::new();
    if let Some(followup) = ability.followup {
        cursor.enter("activation.mana.program.followup");
        if let Err(failure) = cursor.require(
            !followup.effects.is_empty() && followup.connectors.len() + 1 == followup.effects.len(),
            "a nonempty followup with one connector between each action",
        ) {
            return EffectProgramParseAttempt::Failed(failure);
        }
        connectors.push(EffectConnector::Then);
        connectors.extend(followup.connectors);
        effects.extend(followup.effects);
    }
    lower_contextual_effect_program(
        cursor,
        EffectProgramContext::Activated,
        Some(ability.costs),
        EffectSequenceNode {
            effects,
            connectors,
        },
    )
}

#[cfg(test)]
#[path = "contextual_effect_program_tests.rs"]
mod tests;
