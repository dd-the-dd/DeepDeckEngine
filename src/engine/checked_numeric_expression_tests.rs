use super::super::{
    CanonicalRuleExecution, CardDefinition, DecisionProvider, EngineDecisionRequest, GameSetup,
    GameState, PlayerDeck, card_instance,
};
use super::*;
use rand::RngCore;
use serde_json::json;

fn evaluate(expression: &Value, condition: bool) -> Option<i32> {
    evaluate_checked_numeric_tree(
        expression,
        &mut |leaf| {
            assert!(
                !matches!(
                    leaf["kind"].as_str(),
                    Some(
                        "add"
                            | "subtract"
                            | "multiply"
                            | "divide"
                            | "negate"
                            | "absoluteDifference"
                            | "minimum"
                            | "maximum"
                            | "conditionalValue"
                    )
                ),
                "math parent reached legacy leaf resolver"
            );
            if leaf["kind"] == "contextValue" {
                Some(2)
            } else {
                super::super::integer_value(leaf)
            }
        },
        &mut |_| Some(condition),
    )
}

#[test]
fn dynamic_arithmetic_never_saturates_before_later_subtraction() {
    let expression = json!({"kind":"subtract","left":{"kind":"multiply",
        "left":{"kind":"contextValue"},"right":i32::MAX},"right":i32::MAX});
    assert_eq!(evaluate(&expression, true), None);
    assert_eq!(
        evaluate(
            &json!({"kind":"multiply","value":{"kind":"contextValue"},"factor":2}),
            true
        ),
        Some(4)
    );
    assert_eq!(
        evaluate(
            &json!({"kind":"add","left":{"kind":"decisionResult","decisionId":"missing"},"right":1}),
            true
        ),
        None
    );
}

#[test]
fn checked_division_and_unary_operations_preserve_bounded_sign_and_rounding() {
    assert_eq!(
        evaluate(
            &json!({"kind":"divide","left":-3,"right":2,"round":"down"}),
            true
        ),
        Some(-2)
    );
    assert_eq!(
        evaluate(
            &json!({"kind":"divide","left":-3,"right":2,"round":"up"}),
            true
        ),
        Some(-1)
    );
    assert_eq!(
        evaluate(
            &json!({"kind":"divide","left":2,"right":0,"round":"down"}),
            true
        ),
        None
    );
    assert_eq!(
        evaluate(&json!({"kind":"negate","operand":i32::MIN}), true),
        None
    );
    assert_eq!(
        evaluate(
            &json!({"kind":"absoluteDifference","left":-2,"right":3}),
            true
        ),
        Some(5)
    );
    assert_eq!(
        evaluate(
            &json!({"kind":"absoluteDifference","left":i32::MIN,"right":i32::MAX}),
            true
        ),
        None
    );
}

#[test]
fn recursive_wrappers_do_not_hide_overflow_and_condition_keeps_branch_laziness() {
    let bad = json!({"kind":"add","left":i32::MAX,"right":1});
    for expression in [
        json!({"kind":"negate","operand":bad}),
        json!({"kind":"absoluteDifference","left":bad,"right":0}),
        json!({"kind":"minimum","operands":[0,bad]}),
        json!({"kind":"maximum","operands":[0,bad]}),
    ] {
        assert_eq!(evaluate(&expression, true), None);
    }
    let conditional =
        json!({"kind":"conditionalValue","condition":{"kind":"test"},"ifTrue":3,"ifFalse":bad});
    assert_eq!(evaluate(&conditional, true), Some(3));
    assert_eq!(evaluate(&conditional, false), None);
}

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "unexpected numeric-test provider callback",
        ))
    }
}

#[test]
fn isolated_random_numeric_error_restores_prior_life_history_rng_and_frames() {
    let definition = |id: &str| {
        serde_json::from_value::<CardDefinition>(
            json!({"id":id,"name":id,"typeLine":"Artifact","rules":[]}),
        )
        .unwrap()
    };
    let filler = definition("filler");
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        8341,
    )
    .unwrap();
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &definition("source"), 50));
    engine.state.players[0].hand.extend([
        card_instance("p0", &definition("one"), 51),
        card_instance("p0", &definition("two"), 52),
    ]);
    let mut before = engine.clone();
    let quantity = json!({"kind":"multiply","left":{"kind":"countCards","zone":{"kind":"hand","player":{"kind":"abilityController"}},"where":null},"right":i32::MAX});
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
        {"kind":"gainLife","player":{"kind":"abilityController"},"amount":1},
        {"kind":"chooseCards","id":"randomCount","player":{"kind":"abilityController"},"minimum":quantity,"maximum":quantity,"selection":{"kind":"random"},"candidates":{"kind":"cards","zone":{"kind":"hand","player":{"kind":"abilityController"}},"where":null}}
    ]});
    assert!(super::super::rule_is_executable(&rule));
    let error = engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "p0".into(),
                source_definition_id: "source".into(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut NoChoices,
        )
        .unwrap_err();
    assert!(
        error
            .message
            .contains("numeric expression for chooseCards.minimum")
    );
    assert_eq!(
        serde_json::to_value(&engine.state).unwrap(),
        serde_json::to_value(&before.state).unwrap()
    );
    assert_eq!(engine.rng.next_u64(), before.rng.next_u64());
    assert_eq!(
        engine.target_numeric_contexts,
        before.target_numeric_contexts
    );
    assert!(!engine.quantity_transaction_in_progress());
}
