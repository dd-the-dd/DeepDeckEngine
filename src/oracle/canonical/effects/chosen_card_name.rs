use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn token_is(input: &AbilityInput<'_>, index: usize, expected: &str) -> bool {
    input
        .tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

fn is_permanent_type(raw: &str) -> bool {
    matches!(
        raw.to_ascii_lowercase().as_str(),
        "artifact" | "battle" | "creature" | "enchantment" | "land" | "planeswalker"
    )
}

/// Claims the structural `as SOURCE enters, choose ... card name` family.
/// The source width and optional card-type domain remain owned by the FSM.
pub(in crate::oracle::canonical) fn claims_promoted_as_enters_named_object_choice_family(
    input: &AbilityInput<'_>,
) -> bool {
    if !token_is(input, 0, "as") {
        return false;
    }
    input.tokens.windows(5).any(|window| {
        window[0].raw.eq_ignore_ascii_case("enters")
            && window[1].raw == ","
            && window[2].raw.eq_ignore_ascii_case("choose")
            && (window[3].raw.eq_ignore_ascii_case("a")
                || window[3].raw.eq_ignore_ascii_case("any"))
            && (window[4].raw.eq_ignore_ascii_case("card") || is_permanent_type(&window[4].raw))
    })
}

pub(in crate::oracle::canonical) fn claims_promoted_chosen_name_spell_tax_family(
    input: &AbilityInput<'_>,
) -> bool {
    token_is(input, 0, "spells")
        && token_is(input, 1, "with")
        && token_is(input, 2, "the")
        && token_is(input, 3, "chosen")
        && token_is(input, 4, "name")
        && token_is(input, 5, "cost")
}

/// Claims `Activated abilities of ... can't be activated`: the structural
/// head and the prohibition verb phrase at top level. Cost changes to the
/// same abilities (`... cost {2} more to activate`) are another family.
pub(in crate::oracle::canonical) fn claims_promoted_activation_prohibition_family(
    input: &AbilityInput<'_>,
) -> bool {
    token_is(input, 0, "activated")
        && token_is(input, 1, "abilities")
        && token_is(input, 2, "of")
        && input.tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("can't")
                && window[1].raw.eq_ignore_ascii_case("be")
                && window[2].raw.eq_ignore_ascii_case("activated")
        })
}

fn lower_player(reference: &LocalPlayerReference) -> Value {
    match reference {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_)
        | LocalPlayerReference::ControllerOfQualified(_)
        | LocalPlayerReference::Bound(_) => {
            unreachable!("card-name entry choices bind the entering object's controller")
        }
    }
}

fn lower_card_name_domain(domain: CardNameDomain) -> Value {
    match domain {
        CardNameDomain::Any => Value::Null,
        CardNameDomain::CardType(card_type_node) => card_type(card_type_node.ir_name()),
    }
}

fn lower_stored_card_name(predicate: ChosenCardNamePredicateNode) -> Value {
    json!({
        "kind": "nameEquals",
        "value": {
            "kind": "storedDecision",
            "object": self_ref(),
            "decisionId": lower_choice_binding(predicate.binding),
        },
    })
}

/// `choose a [CHARACTERISTICS] card name` while an effect program resolves
/// (CR 608.2d, 201.3): the controller names a card whose characteristics
/// satisfy the shared card predicate. The name is recorded under the
/// resolution card-name decision that `with the chosen name` reads; the
/// card-context validator proves one producer and a consumer per rule.
pub(in crate::oracle::canonical) fn lower_resolution_card_name_choice(
    domain: &CardNameDomainNode,
) -> Value {
    json!({
        "kind": "chooseCardName",
        "id": crate::oracle::contextual_binding::RESOLUTION_CARD_NAME_DECISION,
        "player": controller(),
        "where": domain
            .predicate
            .as_ref()
            .map_or(Value::Null, lower_card_object_predicate),
    })
}

fn trace(cursor: &OracleTokenCursor<'_>) -> String {
    cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ")
}

pub(in crate::oracle::canonical) fn parse_promoted_as_enters_named_object_choice(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("as-enters-card-name-choice", input.tokens);
    let node = recognize_as_enters_card_name_choice(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("as-enters-card-name-choice.binding-validation");
    cursor.require(
        node.source == LocalObjectReference::SourceSelf,
        "the entering source object",
    )?;
    cursor.require(
        node.choice.binding == ChoiceBinding::CardName,
        "a card-name choice binding",
    )?;

    let _asserted_source_type = node.asserted_source_type;
    let decision_id = lower_choice_binding(node.choice.binding);
    // Entry replacement choices are made by the entering object's controller
    // in the established primitive contract. Validate the typed player now,
    // while preserving that stable IR shape.
    let _choice_player = lower_player(&node.choice.player);
    let domain = lower_card_name_domain(node.choice.domain);
    let transition_trace = trace(&cursor);
    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": self_ref(),
            },
            "decisions": [{
                "id": decision_id,
                "kind": "chooseCardName",
                "where": domain,
                "lookAtOpponentHand": false,
            }],
            "replacement": [{
                "kind": "storeDecision",
                "decisionId": decision_id,
            }],
        }),
        &[
            "Recognize the entering source without reparsing text",
            "Bind a free card-name decision independently from lexical name resolution",
            "Persist the typed choice on the entering object",
            &transition_trace,
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_chosen_name_spell_tax(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("chosen-name-spell-tax", input.tokens);
    let node = recognize_chosen_name_spell_tax(&mut cursor)?;
    cursor.enter("chosen-name-spell-tax.binding-validation");
    cursor.require(
        node.predicate.binding == ChoiceBinding::CardName,
        "the stored card-name choice",
    )?;
    let transition_trace = trace(&cursor);
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "additionalCastingCost",
                "players": { "kind": "eachPlayer" },
                "where": lower_stored_card_name(node.predicate),
                "cost": {
                    "kind": "payMana",
                    "manaCost": node.cost.as_oracle_cost(),
                },
            }],
        }),
        &[
            "Resolve a stored card-name predicate through the shared choice binding",
            "Delegate the variable tax to the mana-cost automaton",
            "Lower one atomic additional-casting-cost modifier",
            &transition_trace,
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_promoted_activation_prohibition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("activation-prohibition", input.tokens);
    let node = recognize_activation_prohibition(&mut cursor)?;
    cursor.enter("activation-prohibition.sources");
    let source_where = match &node.sources {
        ActivationSourcesNode::ChosenName(predicate) => {
            cursor.require(
                predicate.binding == ChoiceBinding::CardName,
                "the stored card-name choice",
            )?;
            lower_stored_card_name(*predicate)
        }
        ActivationSourcesNode::Permanents(objects) => {
            // The prohibition's source filter has no player relation slot.
            cursor.require(
                objects.controller.is_none(),
                "a source collection without a controller relation",
            )?;
            lower_permanent_object_predicate(&objects.predicate)
        }
    };
    let ability_predicate = if node.excludes_mana_abilities {
        not(json!({ "kind": "isManaAbility" }))
    } else {
        Value::Null
    };
    let transition_trace = trace(&cursor);
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "prohibitActivation",
                "abilities": {
                    "kind": "activatedAbilities",
                    "sourceWhere": source_where,
                    "where": ability_predicate,
                },
            }],
        }),
        &[
            "Read the sources as a stored card-name predicate or a shared permanent collection",
            "Select activated abilities independently from their sources",
            "Model the mana-ability exception as a negated ability predicate",
            "Lower one atomic activation prohibition",
            &transition_trace,
        ],
    ))
}
