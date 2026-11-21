use super::*;
use crate::oracle::syntax::recognize_entities;

fn lower(text: &str) -> MachineResult<LoweredEffectSequence> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("target-numeric-sequence", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)?;
    lower_local_effect_sequence(&mut cursor, &sequence)
}

#[test]
fn live_numeric_read_remains_after_the_characteristic_modifier() {
    let result=lower("Target creature gets +2/+2 until end of turn. You gain life equal to that creature's power.").unwrap();
    assert_eq!(result.decisions.len(), 1);
    assert_eq!(result.effects[0]["kind"], "modifyPowerToughness");
    assert_eq!(
        result.effects[1],
        json!({"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"powerOf","object":{"kind":"chosenTarget","id":"targetPermanent"}}})
    );
}

#[test]
fn qualified_reference_cannot_rename_the_original_target() {
    let failure=lower("Target creature gets +2/+2 until end of turn. You gain life equal to that artifact's power.").err().expect("noun mismatch must reject");
    assert_eq!(failure.state, "sequence.target-numeric.nominal");
}

#[test]
fn permanent_property_read_after_destruction_preserves_primitive_sequence() {
    let result=lower("Destroy target creature or enchantment an opponent controls. You lose life equal to that permanent's mana value.").unwrap();
    assert_eq!(result.decisions.len(), 1);
    assert_eq!(result.effects[0]["kind"], "destroyPermanent");
    assert_eq!(
        result.effects[1],
        json!({"kind":"loseLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"manaValueOf","object":{"kind":"chosenTarget","id":"targetPermanent"}}})
    );
}

#[test]
fn post_zone_controller_reference_reads_the_original_target() {
    // The target's departure record now carries its controller (CR 608.2h);
    // the runtime proof lives in engine/tests/reference_anaphora_runtime.rs.
    let result =
        lower("Destroy target creature. Its controller gains life equal to its mana value.")
            .unwrap();
    assert_eq!(result.effects[0]["kind"], "destroyPermanent");
    assert_eq!(
        result.effects[1],
        json!({"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"chosenTarget","id":"targetPermanent"}},"amount":{"kind":"manaValueOf","object":{"kind":"chosenTarget","id":"targetPermanent"}}})
    );
}
