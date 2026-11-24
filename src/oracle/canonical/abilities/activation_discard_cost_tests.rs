use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities, recognize_oracle_entities};

fn parse_costs(text: &str) -> Result<ActivationCostSequenceNode, MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("activation-discard-cost-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor)?;
    cursor.finish()?;
    Ok(costs)
}

fn parse_activation(type_line: &str, text: &str) -> EffectProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_contextual_effect_program(&AbilityInput {
        face_name: "Generic Program Source",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed_rule(type_line: &str, text: &str) -> Value {
    let EffectProgramParseAttempt::Parsed(parsed) = parse_activation(type_line, text) else {
        panic!("expected a complete activated program: {text}");
    };
    parsed.rule
}

#[test]
fn murmuring_volume_discards_a_chosen_hand_card_as_an_activation_cost() {
    let rule = parsed_rule("Artifact", "{2}, {T}, Discard a card: Draw a card.");
    assert_eq!(rule["kind"], "activatedAbility");
    // Another card is discarded, so the source keeps its default zone.
    assert!(rule.get("activationZone").is_none());
    let discard = &rule["costs"][2];
    assert_eq!(discard["kind"], "discardCard");
    assert_eq!(discard["card"]["kind"], "chosenObject");
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    let choice = decisions
        .iter()
        .find(|decision| decision["id"] == discard["card"]["id"])
        .expect("the discard cost has its own non-targeting choice");
    assert_eq!(choice["kind"], "chooseObjects");
    assert_eq!(choice["candidates"]["kind"], "cards");
    assert_eq!(choice["candidates"]["zone"], hand(controller()));
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
}

#[test]
fn discard_card_composes_with_unrelated_costs_effects_and_card_qualifiers() {
    // Previously unused combinations: discard + scry, discard + sacrifice,
    // and a typed card query reused from the shared card selector.
    let scry = parsed_rule("Creature", "{1}, Discard a card: Scry 1.");
    assert_eq!(scry["costs"][1]["kind"], "discardCard");
    assert_eq!(scry["effects"][0]["kind"], "lookAtTopCards");

    let both = parse_costs("Discard a card, Sacrifice a creature").unwrap();
    let decisions = lower_activation_cost_decisions(&both);
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0]["candidates"]["kind"], "cards");
    assert_eq!(decisions[1]["candidates"]["kind"], "permanents");
    assert_ne!(decisions[0]["id"], decisions[1]["id"]);
    assert_eq!(
        decisions[0]["id"],
        lower_local_activation_costs(&both.costs[0])[0]["card"]["id"]
    );
    assert!(!both.source_zone_requirements().hand);

    let land = parse_costs("{T}, Discard a land card").unwrap();
    let decisions = lower_activation_cost_decisions(&land);
    assert_eq!(decisions[0]["candidates"]["where"]["value"], "Land");
}

#[test]
fn discard_this_card_still_forces_hand_activation() {
    let sequence = parse_costs("{1}, Discard this card").unwrap();
    assert!(matches!(
        sequence.costs[1],
        LocalActivationCostNode::DiscardSelf(_)
    ));
    assert!(sequence.source_zone_requirements().hand);
    let rule = parsed_rule("Instant", "{1}, Discard this card: Draw a card.");
    assert_eq!(rule["activationZone"], "hand");
}

#[test]
fn counted_discard_declares_one_choice_and_pays_one_discard_per_card() {
    // Jolrael, Empress of Beasts: `{2}{G}, {T}, Discard two cards: ...`.
    let costs = parse_costs("{2}{G}, {T}, Discard two cards").unwrap();
    let decisions = lower_activation_cost_decisions(&costs);
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["kind"], "chooseObjects");
    assert_eq!(decisions[0]["quantity"]["value"], integer(2));
    let paid = lower_local_activation_costs(&costs.costs[2]);
    let id = decisions[0]["id"].as_str().unwrap();
    assert_eq!(
        paid,
        vec![
            json!({"kind": "discardCard", "card": {"kind": "chosenObject", "id": format!("{id}:0")}}),
            json!({"kind": "discardCard", "card": {"kind": "chosenObject", "id": format!("{id}:1")}}),
        ]
    );
}

#[test]
fn random_discard_is_selected_by_the_engine_while_paying() {
    // Stormscale Anarch and the sibling `Discard two cards at random`: the
    // payer declares no card; the counted random discard is one payment.
    for (text, count) in [
        ("{2}{R}, Discard a card at random", 1),
        ("Discard two cards at random", 2),
    ] {
        let costs = parse_costs(text).unwrap();
        assert!(lower_activation_cost_decisions(&costs).is_empty(), "{text}");
        let discard = costs.costs.last().unwrap();
        assert_eq!(
            lower_local_activation_costs(discard),
            vec![json!({
                "kind": "discardCards",
                "player": controller(),
                "count": integer(count),
                "where": Value::Null,
                "selection": {"kind": "random"},
            })],
            "{text}"
        );
    }
    let rule = parsed_rule(
        "Creature — Dragon",
        "{2}{R}, Discard a card at random: This creature deals 2 damage to any target.",
    );
    assert_eq!(rule["costs"][1]["kind"], "discardCards");
}

#[test]
fn unsupported_discard_shapes_fail_explicitly_at_their_own_state() {
    for (text, state) in [
        (
            "Discard another card",
            "activation.discard-card.source-exclusion",
        ),
        // The activation announces X, but X declared cards have no
        // declaration schema: a typed engine gap, never a guessed count.
        ("{1}, Discard X cards", "engine.gap.activation-cost-action"),
        (
            "Discard a card with the same name as a permanent",
            "selector.search-card.basic-land-type",
        ),
    ] {
        let failure = parse_costs(text).expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
}
