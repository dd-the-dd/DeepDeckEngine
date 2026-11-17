use serde_json::{Value, json};

use crate::oracle::model::RecognizerProvenance;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn aura(type_line: &str, text: &str) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Synthetic Aura".into(),
        type_line: type_line.into(),
        mana_cost: Some("{1}{W}".into()),
        oracle_text: Some(text.into()),
        layout: Some("normal".into()),
        faces: vec![],
    }
}

fn enchant_ability(text: &str) -> Value {
    let parsed = parse_oracle_card_native(aura("Enchantment \u{2014} Aura", text));
    let ability = &parsed.abilities[0];
    assert_eq!(
        ability.status, "canonical",
        "{text}: {:?}",
        ability.diagnostics
    );
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("a canonical rule");
    assert_eq!(rule["kind"], "keywordAbility");
    rule["ability"].clone()
}

#[test]
fn enchant_creature_restricts_the_aura_to_the_shared_creature_predicate() {
    assert_eq!(
        enchant_ability("Enchant creature"),
        json!({"kind": "enchant", "where": {"kind": "cardTypeContains", "value": "Creature"}})
    );
}

#[test]
fn enchant_composes_the_shared_object_query_without_a_new_recipe() {
    // A previously unused combination: union of types plus a controller relation.
    let ability = enchant_ability("Enchant artifact or land you control");
    assert_eq!(ability["kind"], "enchant");
    assert_eq!(ability["controller"]["kind"], "controllerOf");
    assert_eq!(ability["where"]["kind"], "or");
    let opponent = enchant_ability("Enchant creature an opponent controls");
    assert_eq!(opponent["controller"]["kind"], "opponentsOf");
    let qualified = enchant_ability("Enchant nonblack creature");
    assert_eq!(qualified["where"]["kind"], "and");
    enchant_ability("Enchant Forest or Plains");
    assert!(enchant_ability("Enchant permanent")["where"].is_null());
    assert_eq!(
        enchant_ability(
            "Enchant land (Target a land as you cast this. This card enters attached to that land.)"
        ),
        enchant_ability("Enchant land")
    );
    enchant_ability("Enchant creature with power 3 or less");
}

#[test]
fn enchant_without_a_representable_permanent_restriction_fails_explicitly() {
    for text in [
        "Enchant player",
        "Enchant opponent",
        "Enchant creature card in a graveyard",
        "Enchant two creatures",
        "Enchant creature. Draw a card.",
        "Enchant land (Target a creature as you cast this. This card enters attached to that land.)",
        "Enchant land (Target a land as you cast this. This card enters attached to that land)",
    ] {
        let parsed = parse_oracle_card_native(aura("Enchantment \u{2014} Aura", text));
        assert_eq!(parsed.abilities[0].status, "unsupported", "{text}");
        assert!(parsed.abilities[0].rule.is_none(), "{text}");
    }
    let off_face = parse_oracle_card_native(aura("Enchantment", "Enchant creature"));
    assert_eq!(off_face.abilities[0].status, "unsupported");
}
