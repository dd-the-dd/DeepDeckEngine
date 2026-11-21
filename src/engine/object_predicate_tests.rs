use serde_json::json;

use super::{DefinitionView, matches_object_predicate};
use crate::engine::CardDefinition;

fn definition(id: &str, type_line: &str, is_commander: bool) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: String::new(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

#[test]
fn commander_predicate_uses_the_designation_not_the_legendary_supertype() {
    let commander = definition("designated-commander", "Creature", true);
    let ordinary_legend = definition("ordinary-legend", "Legendary Creature", false);
    let predicate = json!({ "kind": "isCommander" });

    assert!(matches_object_predicate(
        DefinitionView::new(&commander),
        &predicate,
    ));
    assert!(!matches_object_predicate(
        DefinitionView::new(&ordinary_legend),
        &predicate,
    ));
}
