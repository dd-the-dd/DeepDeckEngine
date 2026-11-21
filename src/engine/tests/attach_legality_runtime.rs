//! CR 701.3: an attach instruction moves an object only when the attachment
//! is legal; an illegal attachment is not an error and changes nothing.
use super::*;

fn attached_to_after(
    attachment_type_line: &str,
    recipient_type_line: &str,
    attach_to_self: bool,
) -> Option<String> {
    let mut engine = test_engine(2);
    let attachment = test_instance(
        "attachment",
        test_definition("attachment", attachment_type_line),
        "player-0",
    );
    let recipient = test_instance(
        "recipient",
        test_definition("recipient", recipient_type_line),
        "player-1",
    );
    engine.state.players[0].battlefield.push(attachment.clone());
    engine.state.players[1].battlefield.push(recipient);
    let stack_object = StackObject {
        id: "stack:attach".to_string(),
        controller: "player-0".to_string(),
        card: attachment,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let mut bindings = BTreeMap::from([(
        "recipient".to_string(),
        RuntimeBinding::Objects(vec![if attach_to_self {
            "attachment".to_string()
        } else {
            "recipient".to_string()
        }]),
    )]);
    let effects = json!([{
        "kind": "attachPermanent",
        "attachment": { "kind": "self" },
        "to": { "kind": "boundObjects", "binding": "recipient" },
    }]);
    engine
        .execute_effects(
            &effects,
            &stack_object,
            &mut bindings,
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("an attach instruction never fails the resolution");
    engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "attachment")
        .and_then(|card| card.attached_to.clone())
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn legal_attachments_move_to_the_recipient() {
    for (attachment, recipient) in [
        ("Artifact - Equipment", "Creature - Dwarf"),
        ("Artifact Creature - Equipment", "Artifact Creature - Robot"),
        ("Artifact - Fortification", "Land"),
        ("Enchantment - Aura", "Creature - Bear"),
    ] {
        assert_eq!(
            attached_to_after(attachment, recipient, false).as_deref(),
            Some("recipient"),
            "{attachment} to {recipient}"
        );
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn illegal_attachments_do_nothing() {
    for (attachment, recipient) in [
        ("Artifact - Equipment", "Land"),
        ("Artifact - Equipment", "Artifact"),
        ("Artifact - Fortification", "Creature - Dwarf"),
        ("Artifact", "Creature - Dwarf"),
        ("Creature - Dwarf", "Creature - Dwarf"),
    ] {
        assert_eq!(
            attached_to_after(attachment, recipient, false),
            None,
            "{attachment} to {recipient}"
        );
    }
    assert_eq!(
        attached_to_after("Artifact Creature - Equipment", "Creature", true),
        None,
        "an object is never attached to itself"
    );
}
