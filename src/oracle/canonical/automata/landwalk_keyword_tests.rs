use serde_json::json;

use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

use super::super::{GrantedKeyword, KeywordAtomNode};
use super::{OracleTokenCursor, recognize_standalone_keyword_ability};

fn recognize(text: &str) -> super::super::super::MachineResult<Vec<KeywordAtomNode>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("landwalk-keyword-test", &tokens);
    recognize_standalone_keyword_ability(&mut cursor).map(|list| list.keywords)
}

fn landwalk_names(text: &str) -> Vec<&'static str> {
    recognize(text)
        .unwrap_or_else(|failure| panic!("{text}: {failure:?}"))
        .into_iter()
        .map(|atom| match atom {
            KeywordAtomNode::Parameterless(keyword @ GrantedKeyword::Landwalk(_)) => {
                keyword.canonical_name()
            }
            other => panic!("expected a landwalk atom, got {other:?}"),
        })
        .collect()
}

#[test]
fn printed_basic_and_nonbasic_subtype_reminders_validate_against_the_parameter() {
    for (text, name) in [
        (
            "Swampwalk (This creature can't be blocked as long as defending player controls a Swamp.)",
            "swampwalk",
        ),
        (
            "Islandwalk (This creature can't be blocked as long as defending player controls an Island.)",
            "islandwalk",
        ),
        (
            "Plainswalk (This creature can't be blocked as long as defending player controls a Plains.)",
            "plainswalk",
        ),
        (
            "Desertwalk (This creature can't be blocked as long as defending player controls a Desert.)",
            "desertwalk",
        ),
    ] {
        assert_eq!(landwalk_names(text), vec![name], "{text}");
    }
}

#[test]
fn any_land_subtype_of_the_shared_vocabulary_is_a_landwalk_without_a_printed_card() {
    assert_eq!(
        landwalk_names(
            "Gatewalk (This creature can't be blocked as long as defending player controls a Gate.)"
        ),
        vec!["gatewalk"]
    );
    assert_eq!(
        landwalk_names("Towerwalk, locuswalk and swampwalk"),
        vec!["towerwalk", "locuswalk", "swampwalk"]
    );
}

#[test]
fn a_reminder_naming_another_land_type_or_altered_wording_is_rejected() {
    for text in [
        "Swampwalk (This creature can't be blocked as long as defending player controls an Island.)",
        "Swampwalk (This creature can't be blocked as long as defending player controls a Swamps.)",
        "Islandwalk (This creature can't be blocked as long as defending player controls a Island.)",
        "Swampwalk (This creature can't be blocked as long as defending player controls an Swamp.)",
        "Swampwalk (This creature can't be blocked as long as defending player controls a Swamp)",
        "Swampwalk (This creature can't be blocked as long as defending player controls a Swamp or a Forest.)",
        "Swampwalk (This creature can't be blocked as long as defending player controls a land.)",
        "Swampwalk (They can't be blocked as long as defending player controls a Swamp.)",
        "Swampwalk (Draw a card.)",
    ] {
        assert!(recognize(text).is_err(), "{text}");
    }
}

#[test]
fn words_that_only_end_in_walk_are_not_landwalk() {
    for text in ["Planeswalk", "Blightwalk", "Landwalk", "Walk"] {
        assert!(recognize(text).is_err(), "{text}");
    }
}

#[test]
fn a_creature_collection_grant_validates_its_plural_reminder_against_the_parameter() {
    let parse = |text: &str| {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("landwalk-collection-test", &tokens);
        matches!(
            super::super::try_recognize_static_collection_keyword_grant(&mut cursor),
            super::super::StaticCollectionKeywordRecognition::Parsed(_)
        )
    };
    assert!(parse(
        "Creatures you control have swampwalk. (They can't be blocked as long as defending player controls a Swamp.)"
    ));
    assert!(parse(
        "Creatures you control have townwalk. (They can't be blocked as long as defending player controls a Town.)"
    ));
    assert!(!parse(
        "Creatures you control have swampwalk. (They can't be blocked as long as defending player controls a Forest.)"
    ));
}

fn parse_card(type_line: &str, oracle: &str) -> serde_json::Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Landwalker".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:?}",
        parsed.diagnostics
    );
    serde_json::to_value(&parsed.abilities[0].rule).expect("rule serializes")
}

#[test]
fn whole_card_lowers_to_the_keyword_the_engine_reads() {
    let reminder =
        "Swampwalk (This creature can't be blocked as long as defending player controls a Swamp.)";
    let bare = parse_card("Creature - Zombie", "Swampwalk");
    assert_eq!(parse_card("Creature - Zombie", reminder), bare);
    assert_eq!(bare["ability"], json!({ "kind": "swampwalk" }), "{bare}");
    let desert = parse_card(
        "Legendary Creature - Human",
        "Desertwalk (This creature can't be blocked as long as defending player controls a Desert.)",
    );
    assert_eq!(
        desert["ability"],
        json!({ "kind": "desertwalk" }),
        "{desert}"
    );
}
