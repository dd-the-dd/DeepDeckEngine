use crate::oracle::syntax::recognize_entities;

use super::super::{AmassArmySubtype, LocalPlayerReference, NumericAtom, NumericExpression};
use super::{OracleTokenCursor, recognize_amass_effect};

fn recognize(
    text: &str,
) -> Result<super::super::AmassEffectNode, super::super::super::MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("amass-effect-test", &tokens);
    let effect = recognize_amass_effect(&mut cursor, None)?;
    cursor.finish()?;
    Ok(effect)
}

#[test]
fn supported_amass_subtypes_lower_to_typed_singular_values() {
    for (plural, subtype) in [
        ("Goblins", AmassArmySubtype::Goblin),
        ("Orcs", AmassArmySubtype::Orc),
        ("Zombies", AmassArmySubtype::Zombie),
    ] {
        let effect = recognize(&format!("amass {plural} 2")).expect("amass effect parses");
        assert_eq!(effect.player, LocalPlayerReference::Controller);
        assert_eq!(effect.army_subtype, subtype);
        assert_eq!(
            effect.count,
            NumericExpression::Atom(NumericAtom::Integer(2))
        );
    }
}

#[test]
fn amass_requires_the_oracle_plural_subtype_form() {
    let failure = recognize("amass Orc 1").expect_err("singular subtype must fail");

    assert_eq!(failure.actual.as_deref(), Some("Orc"));
    assert_eq!(failure.expected, "a plural supported amass subtype");
}

#[test]
fn amass_never_guesses_an_unknown_subtype() {
    let failure = recognize("amass Elves 1").expect_err("unknown subtype must fail");

    assert_eq!(failure.actual.as_deref(), Some("Elves"));
    assert_eq!(failure.expected, "a plural supported amass subtype");
}

#[test]
fn zero_is_a_complete_amass_action_not_an_unsupported_or_missing_count() {
    let effect = recognize("amass Orcs 0").expect("amassing zero still follows CR701.47a-b");
    assert_eq!(
        effect.count,
        NumericExpression::Atom(NumericAtom::Integer(0))
    );
}
