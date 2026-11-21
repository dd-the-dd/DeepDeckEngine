use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const DURESS: &str = "Target opponent reveals their hand. You choose a noncreature, nonland card from it. That player discards that card.";

fn parse_card(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn controller() -> Value {
    json!({
        "kind": "controllerOf",
        "object": { "kind": "self" },
    })
}

fn chosen_target_player() -> Value {
    json!({ "kind": "chosenTarget", "id": "targetPlayer" })
}

fn integer(value: i64) -> Value {
    json!({ "kind": "integer", "value": value })
}

fn card_type(value: &str) -> Value {
    json!({ "kind": "cardTypeContains", "value": value })
}

fn not(value: Value) -> Value {
    json!({ "kind": "not", "operand": value })
}

fn expected_rule(opponent_only: bool, filter: Value) -> Value {
    let target_player = chosen_target_player();
    let candidates = if opponent_only {
        json!({
            "kind": "players",
            "where": { "kind": "isOpponentOf", "player": controller() },
        })
    } else {
        json!({ "kind": "players" })
    };
    json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetPlayer",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": candidates,
            }],
        },
        "effects": [
            {
                "kind": "revealHand",
                "player": target_player.clone(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            },
            {
                "kind": "chooseCards",
                "id": "discardedCard",
                "player": controller(),
                "minimum": integer(1),
                "maximum": integer(1),
                "candidates": {
                    "kind": "cards",
                    "zone": { "kind": "hand", "player": target_player.clone() },
                    "where": filter,
                },
            },
            {
                "kind": "discardCards",
                "player": target_player,
                "cards": { "kind": "decisionResult", "decisionId": "discardedCard" },
            },
        ],
    })
}

fn append_effect(mut rule: Value, effect: Value) -> Value {
    rule["effects"]
        .as_array_mut()
        .expect("spell effects")
        .push(effect);
    rule
}

#[test]
fn exact_hand_disruption_is_native_and_composes_three_primitive_effects() {
    let parsed = parse_card("Duress", DURESS);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(
        parsed.abilities[0]
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&expected_rule(
            true,
            json!({
                "kind": "and",
                "operands": [not(card_type("Creature")), not(card_type("Land"))],
            }),
        ))
    );
}

#[test]
fn shared_filter_and_target_machines_cover_sibling_phrasings() {
    for (oracle_text, opponent_only, filter) in [
        (
            "Target player reveals their hand. You choose a nonland card from it. That player discards that card.",
            false,
            not(card_type("Land")),
        ),
        (
            "Target opponent reveals their hand. You choose an instant or sorcery card from it. That player discards that card.",
            true,
            json!({
                "kind": "or",
                "operands": [card_type("Instant"), card_type("Sorcery")],
            }),
        ),
    ] {
        let parsed = parse_card("Hand Interaction", oracle_text);
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(
            parsed.abilities[0].rule.as_ref(),
            Some(&expected_rule(opponent_only, filter)),
            "{oracle_text}"
        );
    }
}

#[test]
fn malformed_claimed_family_members_fail_at_the_responsible_local_machine() {
    for (oracle_text, automaton, state, found) in [
        (
            "Target opponent reveals their hand. You choose a noncreature or card from it. That player discards that card.",
            "targeted-hand-disruption",
            "hand-card-selector.rhs",
            "card",
        ),
        (
            "Target opponent reveals their hand. You choose a noncreature, nonland card from it. That opponent discards that card.",
            "targeted-hand-disruption",
            "player-subject.bound.reference",
            "opponent",
        ),
    ] {
        let parsed = parse_card("Malformed Hand Interaction", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        let diagnostic = ability
            .diagnostics
            .first()
            .expect("claimed malformed grammar exposes a diagnostic");
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}

#[test]
fn neighboring_hand_sentences_remain_outside_the_promoted_boundary() {
    // The shared contextual program owns these neighbors; the
    // hand-disruption family never claims them. Its player verbs
    // (`reveals their hand`, `exiles that card`) and the choice from the
    // revealed hand compose them, while unrepresented grammar still fails.
    for (oracle_text, canonical) in [
        ("Target opponent reveals their hand.", true),
        (
            "Each opponent reveals their hand. You choose a nonland card from each of them. Those players discard those cards.",
            false,
        ),
        (
            "Target opponent reveals their hand. You choose a nonland card from it and exile that card.",
            true,
        ),
        (
            "Target opponent reveals their hand. You choose a noncreature, nonland card from it. That player exiles that card.",
            true,
        ),
    ] {
        let parsed = parse_card("Other Hand Interaction", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.rule.is_some(), canonical, "{oracle_text}");
        assert_eq!(
            parsed.status,
            if canonical {
                "canonical"
            } else {
                "unsupported"
            },
            "{oracle_text}"
        );
        if canonical {
            let rule = ability.rule.as_ref().expect("canonical rule");
            assert_eq!(rule["effects"][0]["kind"], "revealHand", "{oracle_text}");
        } else {
            assert!(
                ability
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.automaton.as_deref()
                        != Some("targeted-hand-disruption")),
                "{oracle_text}"
            );
        }
    }
}

#[test]
fn thoughtseize_is_native_and_appends_atomic_life_loss() {
    let oracle_text = "Target player reveals their hand. You choose a nonland card from it. That player discards that card. You lose 2 life.";
    let parsed = parse_card("Thoughtseize", oracle_text);
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert!(
        parsed.abilities[0]
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&append_effect(
            expected_rule(false, not(card_type("Land"))),
            json!({
                "kind": "loseLife",
                "player": controller(),
                "amount": integer(2),
            }),
        ))
    );
}

#[test]
fn hand_disruption_core_composes_other_targetless_followups() {
    let parsed = parse_card(
        "Restorative Interrogation",
        "Target opponent reveals their hand. You choose an instant or sorcery card from it. That player discards that card. You gain 4 life. Draw a card.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
        .as_array()
        .unwrap();
    assert_eq!(effects.len(), 5);
    assert_eq!(effects[3]["kind"], "gainLife");
    assert_eq!(effects[3]["amount"], integer(4));
    assert_eq!(effects[4]["kind"], "drawCards");
    assert_eq!(effects[4]["count"], integer(1));
}

#[test]
fn hand_disruption_followup_near_misses_fail_at_the_responsible_child() {
    for (oracle_text, state, expected) in [(
        "Target player reveals their hand. You choose a nonland card from it. That player discards that card. You lose life.",
        "effect.life.amount-after-resource",
        "`equal to` followed by a numeric expression",
    )] {
        let parsed = parse_card("Hand Probe", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        let diagnostic = ability.diagnostics.first().unwrap();
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.expected.as_deref(), Some(expected));
    }
}

#[test]
fn a_targeted_followup_composes_through_the_shared_program() {
    // The promoted family's targetless suffix cannot declare a target; the
    // shared program reads the same reveal/choice/discard children and then
    // the ordinary targeted damage action, with both declarations.
    let parsed = parse_card(
        "Hand Probe",
        "Target player reveals their hand. You choose a nonland card from it. That player discards that card. Hand Probe deals 1 damage to target creature.",
    );
    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0].rule.as_ref().expect("canonical rule");
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 2);
    let kinds = rule["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "revealHand",
            "bind",
            "chooseCards",
            "discardCards",
            "dealDamage"
        ]
    );
}
