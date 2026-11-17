use crate::oracle::{canonical::OracleTokenCursor, syntax::recognize_entities};

use super::{recognize_earthbend_action, recognize_earthbend_activation};

const REMINDER: &str = "(Target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)";

#[test]
fn keyword_action_core_is_reusable_without_an_activation_envelope() {
    let tokens = recognize_entities("Earthbend 4");
    let mut cursor = OracleTokenCursor::new("earthbend-action-test", &tokens);
    let action = recognize_earthbend_action(&mut cursor).expect("action parses");
    cursor.finish().expect("action consumes all tokens");
    assert_eq!(action.quantity, super::super::NumericAtom::Integer(4));
}

#[test]
fn activated_action_exports_costs_quantity_and_typed_reminder() {
    let text = format!("{{2}}{{G}}, {{T}}: Earthbend 2. Activate only as a sorcery. {REMINDER}");
    let tokens = recognize_entities(&text);
    let mut cursor = OracleTokenCursor::new("earthbend-test", &tokens);
    let node = recognize_earthbend_activation(&mut cursor).expect("Earthbend parses");
    assert_eq!(node.costs.costs.len(), 2);
    assert_eq!(node.quantity, super::super::NumericAtom::Integer(2));
    assert!(node.reminder.is_some());
}

#[test]
fn variable_and_zero_quantities_share_the_numeric_child() {
    for (text, expected) in [(
        "{G}, {T}: Earthbend 0. Activate only as a sorcery.",
        super::super::NumericAtom::Integer(0),
    )] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("earthbend-test", &tokens);
        assert_eq!(
            recognize_earthbend_activation(&mut cursor)
                .expect("generic quantity parses")
                .quantity,
            expected
        );
    }

    let text = "{X}{G}, {T}: Earthbend X. Activate only as a sorcery. (Target land you control becomes a 0/0 creature with haste that's still a land. Put X +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("earthbend-test", &tokens);
    recognize_earthbend_activation(&mut cursor).expect("matching variable reminder parses");
}

#[test]
fn unbound_variable_reports_the_local_binding_state() {
    let text = "{G}, {T}: Earthbend X. Activate only as a sorcery.";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("earthbend-test", &tokens);
    let failure = recognize_earthbend_activation(&mut cursor).expect_err("unbound X fails");
    assert_eq!(failure.state, "earthbend.action.variable-binding");
}

#[test]
fn nonland_reminder_target_is_rejected_by_the_target_contract() {
    let text = "{G}, {T}: Earthbend 2. Activate only as a sorcery. (Target creature you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("earthbend-test", &tokens);
    let failure = recognize_earthbend_activation(&mut cursor).expect_err("nonland fails");
    assert_eq!(failure.state, "earthbend.reminder.target-shape");
}

#[test]
fn reminder_quantity_mismatch_reports_the_local_validation_state() {
    let text = format!("{{G}}, {{T}}: Earthbend 3. Activate only as a sorcery. {REMINDER}");
    let tokens = recognize_entities(&text);
    let mut cursor = OracleTokenCursor::new("earthbend-test", &tokens);
    let failure = recognize_earthbend_activation(&mut cursor).expect_err("mismatch fails");
    assert_eq!(failure.state, "earthbend.reminder.quantity-match");
}

#[test]
fn divergent_return_destination_reports_the_return_child() {
    let text = "{G}, {T}: Earthbend 2. Activate only as a sorcery. (Target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to its owner's hand tapped.)";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("earthbend-test", &tokens);
    let failure = recognize_earthbend_activation(&mut cursor).expect_err("near miss fails");
    assert_eq!(failure.state, "earthbend.reminder.return.action");
}
