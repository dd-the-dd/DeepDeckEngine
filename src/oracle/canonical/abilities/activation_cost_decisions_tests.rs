use super::*;
use crate::oracle::syntax::recognize_entities;

fn costs(text: &str) -> ActivationCostSequenceNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("activation-cost-decisions-test", &tokens);
    let costs = recognize_activation_cost_sequence(&mut cursor).expect("typed activation costs");
    cursor.finish().expect("complete activation costs");
    costs
}

#[test]
fn variable_mana_cost_declares_one_x_value_before_effect_choices() {
    for text in ["{X}, {T}", "{X}{X}{U}, {T}", "{X}, {X}, {T}"] {
        let decisions = lower_activation_cost_decisions(&costs(text));
        assert_eq!(decisions, vec![x_value()]);
        assert_eq!(decisions[0]["minimum"], 0);
    }
}

#[test]
fn fixed_mana_and_other_costs_do_not_invent_a_numeric_binding() {
    for text in ["{2}{U}, {T}", "{T}", "Pay 2 life, Sacrifice this creature"] {
        assert!(lower_activation_cost_decisions(&costs(text)).is_empty());
    }
}

#[test]
fn selected_costs_declare_non_targets_and_share_their_payment_ids() {
    let sequence = costs("{X}, Sacrifice a creature, Sacrifice an artifact");
    let decisions = lower_activation_cost_decisions(&sequence);
    assert_eq!(decisions[0], x_value());
    for (cost_index, decision) in [(1, &decisions[1]), (2, &decisions[2])] {
        // A counted selected cost lowers through the plural payment reader;
        // one chosen sacrifice is exactly one payment.
        let payments = lower_local_activation_costs(&sequence.costs[cost_index]);
        assert_eq!(payments.len(), 1);
        let payment = &payments[0];
        assert_eq!(decision["kind"], "chooseObjects");
        assert_eq!(decision["candidates"]["controller"], controller());
        assert_eq!(
            decision["quantity"],
            json!({ "kind": "exactly", "value": integer(1) })
        );
        assert_eq!(payment["permanent"]["kind"], "chosenObject");
        assert_eq!(decision["id"], payment["permanent"]["id"]);
    }
    assert_ne!(decisions[1]["id"], decisions[2]["id"]);
}

#[test]
fn specialized_parent_adapter_preserves_old_shapes_and_prepends_object_choices() {
    let existing = json!({ "kind": "activatedAbility", "costs": [], "effects": [] });
    assert_eq!(
        with_activation_object_cost_decisions(existing.clone(), &costs("{T}")),
        existing
    );
    let existing = json!({ "kind": "activatedAbility", "declaration": { "kind": "castingDeclaration", "decisions": [
        target_decision("targetPermanent", json!({ "kind": "permanents", "where": null }), 1, 1)
    ] } });
    let adapted =
        with_activation_object_cost_decisions(existing.clone(), &costs("Sacrifice a creature"));
    assert_eq!(
        adapted["declaration"]["decisions"][0]["kind"],
        "chooseObjects"
    );
    assert_eq!(
        adapted["declaration"]["decisions"][1],
        existing["declaration"]["decisions"][0]
    );
}

#[test]
fn selected_cost_adapter_preserves_existing_numeric_producers_and_their_dependencies() {
    let choose_mode = json!({ "kind": "chooseModes", "id": "chosenModes", "options": ["mode1"], "minimum": 1, "maximum": 1 });
    let mut original_x = x_value();
    original_x["maximum"] = integer(7);
    original_x["condition"] = selection("chosenModes", "mode1");
    let target = target_decision(
        "targetPermanent",
        json!({ "kind": "permanents", "where": null }),
        1,
        1,
    );
    let existing = json!({
        "kind": "activatedAbility",
        "declaration": { "kind": "castingDeclaration", "decisions": [choose_mode.clone(), original_x.clone(), target.clone()] }
    });
    let adapted = with_activation_object_cost_decisions(
        existing,
        &costs("{X}, Sacrifice a creature with power X or less"),
    );
    let decisions = adapted["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 4);
    assert_eq!(decisions[0], choose_mode);
    assert_eq!(decisions[1], original_x);
    assert_eq!(decisions[2]["kind"], "chooseObjects");
    assert_eq!(decisions[3], target);
    assert_eq!(
        decisions
            .iter()
            .filter(|decision| decision["id"] == "xValue")
            .count(),
        1
    );
}

#[test]
fn selected_cost_adapter_gets_a_missing_numeric_producer_from_typed_mana_only() {
    let existing = json!({ "kind": "activatedAbility", "effects": [] });
    let adapted = with_activation_object_cost_decisions(
        existing.clone(),
        &costs("{X}, Sacrifice a creature with power X or less"),
    );
    assert_eq!(adapted["declaration"]["decisions"][0], x_value());
    assert_eq!(
        adapted["declaration"]["decisions"][1]["kind"],
        "chooseObjects"
    );
    // Unproduced X is now rejected by the typed cost-sequence parent before
    // reaching this lowering adapter; its negative is covered in that domain.
}

#[test]
fn earthbend_selected_cost_uses_its_local_x_before_filtering_or_targeting() {
    use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};
    for quantity in ["X", "2"] {
        let request = OracleCardParseRequest {
            card_name: "Synthetic Selected Earthbend Cost".into(),
            type_line: "Artifact".into(),
            mana_cost: Some("{2}".into()),
            oracle_text: Some(format!(
                "{{X}}{{G}}, {{T}}, Sacrifice a creature with power X or less: Earthbend {quantity}. Activate only as a sorcery."
            )),
            layout: Some("normal".into()),
            faces: vec![],
        };
        for parsed in [
            parse_oracle_card_native(request.clone()),
            parse_oracle_card(request),
        ] {
            assert_eq!(parsed.status, "canonical");
            assert_eq!(
                parsed.abilities[0].recognizer,
                crate::oracle::model::RecognizerProvenance::Native
            );
            let rule = parsed.abilities[0].rule.as_ref().unwrap();
            let decisions = rule["declaration"]["decisions"].as_array().unwrap();
            assert_eq!(decisions[0], x_value());
            assert_eq!(decisions[1]["kind"], "chooseObjects");
            assert_eq!(decisions[2]["kind"], "chooseTargets");
            assert_eq!(rule["costs"][2]["permanent"]["id"], decisions[1]["id"]);
        }
    }
    let unbound = OracleCardParseRequest {
        card_name: "Synthetic Unbound Selected Earthbend Cost".into(),
        type_line: "Artifact".into(), mana_cost: Some("{X}{G}".into()),
        oracle_text: Some("{T}, Sacrifice a creature with power X or less: Earthbend 2. Activate only as a sorcery.".into()),
        layout: Some("normal".into()), faces: vec![],
    };
    for parsed in [
        parse_oracle_card_native(unbound.clone()),
        parse_oracle_card(unbound),
    ] {
        assert_eq!(parsed.status, "unsupported");
        assert!(
            parsed.abilities[0]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.state.as_deref()
                    == Some("activation.compound.selected-cost-x-producer"))
        );
    }
}
