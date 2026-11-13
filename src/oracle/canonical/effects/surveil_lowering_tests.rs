use crate::oracle::syntax::recognize_entities;

use super::{
    LocalEffectNode, OracleTokenCursor, TargetlessDeclarationSupport, TargetlessLoweringError,
    lower_targetless_local_effect, recognize_surveil_effect,
};

fn recognize(text: &str) -> LocalEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.surveil-lowering", &tokens);
    let effect = recognize_surveil_effect(&mut cursor, None).expect("surveil effect parses");
    cursor.finish().expect("surveil effect consumes its input");
    LocalEffectNode::Surveil(effect)
}

#[test]
fn literal_surveil_lowers_to_the_existing_runtime_primitive() {
    let lowered =
        lower_targetless_local_effect(&recognize("surveil 1"), TargetlessDeclarationSupport::None)
            .expect("literal surveil needs no declaration");

    assert_eq!(
        lowered,
        serde_json::json!({
            "kind": "surveil",
            "player": {
                "kind": "controllerOf",
                "object": { "kind": "self" },
            },
            "count": { "kind": "integer", "value": 1 },
        })
    );
}

#[test]
fn variable_surveil_requires_the_parent_to_supply_x() {
    let effect = recognize("surveil X");

    assert_eq!(
        lower_targetless_local_effect(&effect, TargetlessDeclarationSupport::None),
        Err(TargetlessLoweringError::RequiresDeclaration)
    );
    assert!(lower_targetless_local_effect(&effect, TargetlessDeclarationSupport::XValue).is_ok());
}
