use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn is_saga_type_line(type_line: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("Saga"))
}

fn is_roman_shaped(raw: &str) -> bool {
    !raw.is_empty()
        && raw
            .chars()
            .all(|character| matches!(character, 'I' | 'V' | 'X'))
}

/// Claims only the standardized reminder or a Roman chapter header on a Saga
/// type line. The type boundary prevents unrelated Roman-labelled abilities
/// from being stolen from legacy parsers.
pub(in crate::oracle::canonical) fn claims_promoted_saga_family(input: &AbilityInput<'_>) -> bool {
    if !is_saga_type_line(input.face_type_line) {
        return false;
    }
    let tokens = input.tokens;
    let reminder = tokens.get(0..4).is_some_and(|prefix| {
        prefix[0].raw == "("
            && prefix[1].raw.eq_ignore_ascii_case("as")
            && prefix[2].raw.eq_ignore_ascii_case("this")
            && prefix[3].raw.eq_ignore_ascii_case("Saga")
    });
    let chapter = tokens
        .first()
        .is_some_and(|token| is_roman_shaped(&token.raw));
    reminder || chapter
}

fn scaled_count(count: Value, factor: i64) -> Value {
    match factor {
        0 => integer(0),
        1 => count,
        _ => json!({
            "kind": "multiply",
            "left": integer(factor),
            "right": count,
        }),
    }
}

fn lower_characteristic_token_activation(activation: &CharacteristicTokenActivationNode) -> Value {
    let token = &activation.effect.token;
    let counted = json!({
        "kind": "countPermanents",
        "player": controller(),
        "where": lower_permanent_object_predicate(&token.ability.counted),
    });
    let characteristic_ability = json!({
        "kind": "staticAbility",
        "source": self_ref(),
        "activeWhile": active_while_battlefield(),
        "modifiers": [{
            "kind": "modifyPowerToughness",
            "objects": self_ref(),
            "power": scaled_count(counted.clone(), token.ability.delta.power),
            "toughness": scaled_count(counted, token.ability.delta.toughness),
        }],
    });
    with_activation_object_cost_decisions(
        json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": activation
                .costs
                .costs
                .iter()
                .flat_map(lower_local_activation_costs)
                .collect::<Vec<_>>(),
            "effects": [{
                "kind": "createTokens",
                "controller": controller(),
                "quantity": lower_numeric_atom(activation.effect.quantity.clone()),
                "token": {
                    "colors": token
                        .colors
                        .iter()
                        .copied()
                        .map(canonical_color_name)
                        .collect::<Vec<_>>(),
                    "power": token.power,
                    "toughness": token.toughness,
                    "types": token
                        .types
                        .iter()
                        .copied()
                        .map(ObjectCardType::ir_name)
                        .collect::<Vec<_>>(),
                    "subtypes": token.subtypes.clone(),
                    "abilities": [characteristic_ability],
                },
            }],
        }),
        &activation.costs,
    )
}

fn lower_granted_ability(
    cursor: &mut OracleTokenCursor<'_>,
    ability: SagaGrantedAbilityNode,
) -> MachineResult<Value> {
    match ability {
        SagaGrantedAbilityNode::Mana(ability) => {
            match lower_recognized_add_led_activation(cursor, ability) {
                EffectProgramParseAttempt::Parsed(draft) => Ok(draft.rule),
                EffectProgramParseAttempt::Unpromoted(failure)
                | EffectProgramParseAttempt::Failed(failure)
                | EffectProgramParseAttempt::FailedSupport(failure) => Err(failure),
                EffectProgramParseAttempt::NotApplicable => {
                    unreachable!("a recognized granted activation has a structural context")
                }
            }
        }
        SagaGrantedAbilityNode::CharacteristicToken(ability) => {
            Ok(lower_characteristic_token_activation(&ability))
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_saga(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let reminder = input.tokens.first().is_some_and(|token| token.raw == "(");
    if reminder {
        let mut cursor = OracleTokenCursor::new("saga-progression-reminder", input.tokens);
        let reminder = recognize_saga_progression_reminder(&mut cursor)?;
        return Ok(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "marker": "sagaProgression",
                "finalChapter": integer(reminder.final_chapter),
            }),
            &[
                "Recognize the Saga progression reminder from tokens",
                "Retain the final Roman chapter as typed documentary metadata",
                "Leave runtime progression driven by chapter rules",
            ],
        ));
    }

    let mut cursor = OracleTokenCursor::new("saga-chapter", input.tokens);
    let chapter =
        recognize_saga_chapter(&mut cursor, input.name_links, input.name_resolution_failure)?;
    let effects = match chapter.body {
        SagaChapterBodyNode::Grant(ability) => vec![json!({
            "kind": "grantAbility",
            "object": self_ref(),
            "duration": { "kind": "permanent" },
            "ability": lower_granted_ability(&mut cursor, ability)?,
        })],
        SagaChapterBodyNode::LibrarySearch(search) => {
            cursor.require(
                search.player == LocalPlayerReference::Controller,
                "the Saga controller as the searching player",
            )?;
            let SearchDestination::Battlefield { .. } = search.destination else {
                return Err(cursor.fail("a battlefield destination for a Saga library search"));
            };
            lower_library_search_effects(&mut cursor, &search, controller())?
        }
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": chapter.chapters.into_iter().map(integer).collect::<Vec<_>>(),
            },
            "effects": effects,
        }),
        &[
            "Recognize one or more Saga chapter numerals from the shared token stream",
            "Delegate the chapter body to native quoted-ability or library-search children",
            "Lower the chapter event and body to reusable primitive IR",
            &trace,
        ],
    ))
}
