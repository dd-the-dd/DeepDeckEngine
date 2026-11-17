use crate::oracle::{canonical::OracleTokenCursor, syntax::recognize_entities};

use super::recognize_card_type_partition_sequence;

fn parse(text: &str) -> super::CardTypePartitionSequenceNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("card-type-partition-test", &tokens);
    recognize_card_type_partition_sequence(&mut cursor, &[], None)
        .expect("card-type partition parses")
}

#[test]
fn sequence_exports_the_shared_exhaustive_card_type_vocabulary() {
    let node = parse(
        "When this creature enters, reveal the top ten cards of your library. For each card type, you may put a card of that type from among the revealed cards into your hand. Put the rest on the bottom of your library in a random order. (Artifact, battle, creature, enchantment, instant, land, planeswalker, and sorcery are card types.)",
    );
    assert_eq!(node.count, 10);
    assert!(
        node.card_types
            .iter()
            .any(|card_type| card_type.ir_name() == "Kindred")
    );
    assert_eq!(node.reminder_card_types.as_ref().map(Vec::len), Some(8));
}

#[test]
fn invalid_selection_zone_reports_the_local_destination_state() {
    let text = "When this creature enters, reveal the top three cards of your library. For each card type, you may put a card of that type from among the revealed cards into your graveyard. Put the rest on the bottom of your library in a random order.";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("card-type-partition-test", &tokens);
    let failure = recognize_card_type_partition_sequence(&mut cursor, &[], None)
        .expect_err("wrong destination must fail");
    assert_eq!(failure.state, "selection.per-card-type.destination");
}

#[test]
fn duplicate_reminder_type_reports_the_typed_list_state() {
    let text = "When this creature enters, reveal the top three cards of your library. For each card type, you may put a card of that type from among the revealed cards into your hand. Put the rest on the bottom of your library in a random order. (Artifact, artifact, and land are card types.)";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("card-type-partition-test", &tokens);
    let failure = recognize_card_type_partition_sequence(&mut cursor, &[], None)
        .expect_err("duplicate reminder type must fail");
    assert_eq!(failure.state, "reminder.card-types.unique");
}
