use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn zone_lists_support_arbitrary_length_connectors_and_preserve_parent_boundary() {
    for text in [
        "graveyards and libraries can't",
        "graveyards or libraries can't",
        "graveyard, library, graveyards, and libraries can't",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("zone-list", &tokens);
        assert_eq!(
            recognize_card_zone_list(&mut cursor).unwrap(),
            vec![CardZoneName::Graveyard, CardZoneName::Library]
        );
        assert_eq!(cursor.peek(), Some("can't"));
    }
}
#[test]
fn zone_child_rejects_dangling_connectors_and_unregistered_locations() {
    for text in [
        "graveyards or",
        "graveyards, and unknownlands",
        "unknownlands",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("zone-list", &tokens);
        assert!(recognize_card_zone_list(&mut cursor).is_err(), "{text}");
    }
}
