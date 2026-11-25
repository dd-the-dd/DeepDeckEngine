use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the compact single-sentence counter family. The envelope limits the
/// promoted boundary to `Counter target [QUALIFIER] spell.`; the selector
/// automaton still owns and diagnoses the optional qualifier.
pub(in crate::oracle::canonical) fn claims_promoted_simple_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    matches!(tokens.len(), 4 | 5)
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens
            .get(tokens.len() - 2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("spell"))
        && tokens.last().is_some_and(|token| token.raw == ".")
}

/// Claims a counter target whose card-type alternatives share a trailing
/// `spell` domain. The selector child owns Boolean union composition and keeps
/// malformed union tails as committed native failures.
pub(in crate::oracle::canonical) fn claims_promoted_type_union_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 5
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens
            .get(2)
            .is_some_and(|token| token.token_type == "typeWord")
        && tokens
            .get(3)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("or"))
        && tokens
            .get(tokens.len() - 2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("spell"))
        && tokens.last().is_some_and(|token| token.raw == ".")
        && tokens
            .iter()
            .take(tokens.len() - 1)
            .all(|token| token.raw != ".")
}

/// Claims counter effects followed by a conditional replacement of the
/// normal graveyard destination. The stable second-clause prefix commits this
/// grammar family while the local automaton diagnoses malformed tails.
pub(in crate::oracle::canonical) fn claims_promoted_counter_replacement_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 9
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens.windows(6).any(|window| {
            window[0].raw == "."
                && window[1].raw.eq_ignore_ascii_case("if")
                && window[2].raw.eq_ignore_ascii_case("that")
                && window[3].raw.eq_ignore_ascii_case("spell")
                && window[4].raw.eq_ignore_ascii_case("is")
                && window[5].raw.eq_ignore_ascii_case("countered")
        })
}

/// Claims counter effects whose single target slot is a heterogeneous stack
/// predicate, such as a triggered ability or a colorless spell.
pub(in crate::oracle::canonical) fn claims_promoted_stack_item_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 5
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens.get(2).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("activated")
                || token.raw.eq_ignore_ascii_case("triggered")
                || token.raw.eq_ignore_ascii_case("colorless")
        })
        && tokens.last().is_some_and(|token| token.raw == ".")
}

pub(in crate::oracle::canonical) fn lower_stack_item_filter(
    predicate: &StackItemPredicate,
) -> Value {
    match predicate {
        StackItemPredicate::Ability(StackAbilityKind::Activated) => {
            json!({ "kind": "isActivatedAbility" })
        }
        StackItemPredicate::Ability(StackAbilityKind::Triggered) => {
            json!({ "kind": "isTriggeredAbility" })
        }
        StackItemPredicate::Spell => json!({ "kind": "isSpell" }),
        StackItemPredicate::Colorless => json!({ "kind": "isColorless" }),
        StackItemPredicate::Targets(targeted) => lower_targets_relation(targeted),
        StackItemPredicate::Intersection(operands) => {
            and(operands.iter().map(lower_stack_item_filter).collect())
        }
        StackItemPredicate::Union(operands) => {
            or(operands.iter().map(lower_stack_item_filter).collect())
        }
    }
}

/// One counter action and its single stack-object target declaration:
/// `(target id, candidates, action)`. Shared by the compact counter family
/// and the ordinary effect-sequence lowering.
pub(in crate::oracle::canonical) fn lower_counter_action(
    slot: &TargetSlot,
    exile_instead: bool,
) -> (&'static str, Value, Value) {
    match &slot.selector {
        TargetSelector::Object(predicate) => {
            let candidates = lower_spell_target_candidates(predicate, None);
            let mut counter = json!({
                "kind": "counterSpell",
                "spell": chosen_target("targetSpell"),
            });
            if exile_instead {
                counter["exileInstead"] = json!(true);
            }
            ("targetSpell", candidates, counter)
        }
        TargetSelector::ControlledObject {
            predicate,
            controller: controller_scope,
        } => {
            let candidates = lower_spell_target_candidates(predicate, Some(*controller_scope));
            let mut counter = json!({
                "kind": "counterSpell",
                "spell": chosen_target("targetSpell"),
            });
            if exile_instead {
                counter["exileInstead"] = json!(true);
            }
            ("targetSpell", candidates, counter)
        }
        TargetSelector::StackItem(predicate) => (
            "targetStackObject",
            json!({
                "kind": "stackItems",
                "where": lower_stack_item_filter(predicate),
            }),
            json!({
                "kind": "counterStackObject",
                "object": chosen_target("targetStackObject"),
            }),
        ),
        TargetSelector::Any | TargetSelector::Union(_) | TargetSelector::Player(_) => {
            unreachable!("counter automaton validates a stack-domain selector")
        }
    }
}

pub(in crate::oracle::canonical) fn lower_counter_effect(
    effect: &CounterEffectNode,
    transitions: &[MachineTransition],
    exile_instead: bool,
) -> CanonicalRuleDraft {
    let slot = effect
        .target
        .slots
        .first()
        .expect("counter automaton validates one target slot");
    let (target_id, candidates, effect) = lower_counter_action(slot, exile_instead);
    let trace = transitions
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    target_id,
                    candidates,
                    i64::from(slot.cardinality.minimum),
                    i64::from(slot.cardinality.maximum),
                )],
            },
            "effects": [effect],
        }),
        &[
            "Recognize counter and target selection as independent local automata",
            "Separate spell and ability domains from typed stack predicates",
            "Lower to one chooseTargets decision and one atomic counter primitive",
            &trace,
        ],
    )
}

pub(in crate::oracle::canonical) fn parse_promoted_simple_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    parse_promoted_counter(input, "simple-counter")
}

pub(in crate::oracle::canonical) fn parse_promoted_type_union_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    parse_promoted_counter(input, "type-union-spell-counter")
}

pub(in crate::oracle::canonical) fn parse_promoted_counter_replacement(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("counter-destination-replacement", input.tokens);
    let sequence = recognize_counter_replacement_sequence(&mut cursor)?;
    let CounterDestinationReplacement::Exile = sequence.destination;
    cursor.enter("counter-replacement.spell-domain");
    cursor.require(
        sequence.counter.target.slots.first().is_some_and(|slot| {
            matches!(
                slot.selector,
                TargetSelector::Object(_) | TargetSelector::ControlledObject { .. }
            )
        }),
        "a spell target for the `that spell` antecedent",
    )?;
    Ok(lower_counter_effect(
        &sequence.counter,
        cursor.transitions(),
        true,
    ))
}

/// Claims the promoted numeric selector family at its explicit boundary.
/// Once the stable `Counter target spell with mana value` prefix is present,
/// malformed thresholds stay visible as committed native failures.
pub(in crate::oracle::canonical) fn claims_promoted_mana_value_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 6
        && ["counter", "target", "spell", "with", "mana", "value"]
            .iter()
            .enumerate()
            .all(|(index, expected)| {
                tokens
                    .get(index)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
            })
}

pub(in crate::oracle::canonical) fn parse_promoted_mana_value_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    parse_promoted_counter(input, "mana-value-counter")
}

pub(in crate::oracle::canonical) fn claims_promoted_colored_counter_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 5
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens
            .get(2)
            .is_some_and(|token| card_color_from_oracle_word(&token.raw).is_some())
        && tokens.last().is_some_and(|token| token.raw == ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_colored_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    parse_promoted_counter(input, "colored-spell-counter")
}

pub(in crate::oracle::canonical) fn parse_promoted_stack_item_counter(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    parse_promoted_counter(input, "stack-item-counter")
}

fn parse_promoted_counter(
    input: &AbilityInput<'_>,
    automaton: &'static str,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new(automaton, input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.counter.validation");
    let [LocalEffectNode::CounterSpell(effect)] = sequence.effects.as_slice() else {
        return Err(cursor.fail("one counter-spell effect"));
    };
    cursor.require(
        sequence.connectors.is_empty(),
        "a single effect without a sequence connector",
    )?;
    Ok(lower_counter_effect(effect, cursor.transitions(), false))
}
