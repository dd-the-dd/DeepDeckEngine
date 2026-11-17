use crate::oracle::syntax::recognize_entities;

use super::{
    LocalEffectNode, OracleTokenCursor, TargetlessDeclarationSupport, TargetlessLoweringError,
    lower_targetless_local_effect, lower_targetless_local_effects, recognize_surveil_effect,
};

fn recognize(text: &str) -> LocalEffectNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.surveil-lowering", &tokens);
    let effect = recognize_surveil_effect(&mut cursor, None).expect("surveil effect parses");
    cursor.finish().expect("surveil effect consumes its input");
    LocalEffectNode::Surveil(effect)
}

#[test]
fn literal_surveil_lowers_to_a_sequence_of_existing_atomic_primitives() {
    let lowered = lower_targetless_local_effects(
        &recognize("surveil 1"),
        TargetlessDeclarationSupport::None,
        None,
    )
    .expect("literal surveil needs no declaration");

    assert_eq!(lowered.len(), 7);
    assert_eq!(lowered[0]["kind"], "lookAtTopCards");
    assert_eq!(lowered[3]["to"]["kind"], "graveyard");
    assert_eq!(lowered[6]["abilityId"], "surveil");
    assert_eq!(
        lower_targetless_local_effect(&recognize("surveil 1"), TargetlessDeclarationSupport::None),
        Err(TargetlessLoweringError::UnsupportedEffect)
    );
}

#[test]
fn variable_surveil_requires_the_parent_to_supply_x() {
    let effect = recognize("surveil X");

    assert_eq!(
        lower_targetless_local_effects(&effect, TargetlessDeclarationSupport::None, None),
        Err(TargetlessLoweringError::RequiresDeclaration)
    );
    assert!(
        lower_targetless_local_effects(&effect, TargetlessDeclarationSupport::XValue, None).is_ok()
    );
}
