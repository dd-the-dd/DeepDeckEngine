use crate::oracle::model::RecognizerProvenance;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};

fn request(costs: &str) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Synthetic Conditional Mana Payment Source".into(),
        type_line: "Artifact".into(),
        mana_cost: Some("{X}{G}".into()),
        oracle_text: Some(format!(
            "{costs}: Add {{G}}. Activate only if you control a creature."
        )),
        layout: Some("normal".into()),
        faces: vec![],
    }
}

#[test]
fn conditional_mana_keeps_fixed_source_and_selected_object_costs_native() {
    for cost in ["{T}", "{T}, {1}, Sacrifice a creature"] {
        let native = parse_oracle_card_native(request(cost));
        let full = parse_oracle_card(request(cost));
        assert_eq!(native.status, "canonical", "{cost}");
        assert_eq!(full.status, "canonical", "{cost}");
        assert_eq!(native.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(full.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(native.abilities[0].rule, full.abilities[0].rule);
        let rule = native.abilities[0].rule.as_ref().unwrap();
        assert_eq!(rule["kind"], "manaAbility");
        assert_eq!(rule["activationCondition"]["kind"], "controlsPermanent");
        if cost.contains("Sacrifice") {
            assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseObjects");
            assert_eq!(rule["costs"][2]["permanent"]["kind"], "chosenObject");
        } else {
            assert!(rule.get("declaration").is_none());
        }
    }
}

#[test]
fn conditional_mana_reuses_backend_rejection_for_variable_and_hand_costs() {
    for cost in [
        "{T}, {X}",
        "{T}, {X}, Sacrifice a creature",
        "{T}, {X}, Sacrifice a creature with power X or less",
        "{T}, Discard this card",
    ] {
        for parsed in [
            parse_oracle_card_native(request(cost)),
            parse_oracle_card(request(cost)),
        ] {
            assert_eq!(parsed.status, "unsupported", "{cost}");
            assert!(
                parsed.abilities[0].diagnostics.iter().any(|diagnostic| {
                    diagnostic.state.as_deref() == Some("program.mana.backend-cost-context")
                }),
                "{cost}: {:?}",
                parsed.abilities[0].diagnostics
            );
        }
    }
}
