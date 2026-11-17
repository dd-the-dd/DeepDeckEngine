use crate::oracle::syntax::recognize_entities;

use super::{
    AddManaEffectNode, LocalEffectNode, OracleTokenCursor, TargetlessDeclarationSupport,
    lower_add_mana_effect, lower_targetless_local_effect, recognize_add_mana_effect,
};

fn recognize(text: &str) -> AddManaEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.add-mana-lowering", &tokens);
    let effect = recognize_add_mana_effect(&mut cursor).expect("mana effect parses");
    cursor.finish().expect("mana effect consumes its input");
    effect
}

#[test]
fn fixed_mana_lowering_preserves_the_complete_symbol_sequence() {
    let effect = recognize("Add {B}{B}{B}");

    assert_eq!(
        lower_add_mana_effect(&effect),
        serde_json::json!({
            "kind": "addMana",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" },
            },
            "mana": "{B}{B}{B}",
        })
    );
}

#[test]
fn targetless_lowering_delegates_to_the_shared_mana_lowerer() {
    let effect = LocalEffectNode::AddMana(recognize("Add two mana of any color"));

    assert_eq!(
        lower_targetless_local_effect(&effect, TargetlessDeclarationSupport::None)
            .expect("mana production needs no declaration"),
        serde_json::json!({
            "kind": "addMana",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" },
            },
            "mana": {
                "kind": "chooseColor",
                "amount": 2,
            },
        })
    );
}

#[test]
fn opposing_land_color_domain_lowers_to_the_existing_atomic_mana_value() {
    let effect =
        recognize("Add two mana of any color that a land an opponent controls could produce");

    assert_eq!(
        lower_add_mana_effect(&effect),
        serde_json::json!({
            "kind": "addMana",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" },
            },
            "mana": {
                "kind": "manaColorsLandsOpponentsControlCouldProduce",
                "amount": 2,
            },
        })
    );
}
