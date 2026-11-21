use super::*;
use crate::oracle::canonical::{claims_promoted_typecycling_family, parse_promoted_typecycling};
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};
use serde_json::{Value, json};

fn node(text: &str) -> MachineResult<TypecyclingKeywordNode> {
    let tokens = recognize_oracle_entities(text);
    recognize_typecycling_keyword(&mut OracleTokenCursor::new("test.typecycling", &tokens))
}

fn lowered(text: &str) -> Result<Value, MachineFailure> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Typecycler",
        face_type_line: "Creature - Beast",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    assert!(claims_promoted_typecycling_family(&input), "{text}");
    parse_promoted_typecycling(&input).map(|draft| draft.rule)
}

fn reminder(cost: &str, article: &str, label: &str) -> String {
    format!(
        "({cost}, Discard this card: Search your library for {article} {label} card, reveal it, put it into your hand, then shuffle.)"
    )
}

#[test]
fn basic_landcycling_expands_to_a_hand_discard_with_a_typed_library_search() {
    let text = format!(
        "Basic landcycling {{2}} {}",
        reminder("{2}", "a", "basic land")
    );
    let rule = lowered(&text).expect("real-card typecycling parses");
    let ability = &rule["ability"];
    assert_eq!(rule["kind"], "keywordAbility");
    assert_eq!(ability["kind"], "typecycling");
    assert_eq!(ability["cost"], json!({"kind":"payMana","manaCost":"{2}"}));
    assert_eq!(ability["activationZone"], "hand");
    assert_eq!(
        ability["costs"],
        json!([{"kind":"discardCard","card":{"kind":"self"}}])
    );
    assert!(!ability["where"].is_null());
    let effects = ability["effects"].as_array().unwrap();
    let kinds: Vec<_> = effects
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "chooseCards",
            "revealCards",
            "moveCards",
            "shuffleZone",
            "recordLibrarySearch"
        ]
    );
    assert_eq!(effects[0]["candidates"]["where"], ability["where"]);
    assert_eq!(effects[0]["maximum"], 1);
}

#[test]
fn typecycling_composes_each_shared_selector_and_stays_reminder_optional() {
    // Land subtypes, a creature subtype, a card type and a coordinated
    // supertype all come from the shared search-card selector.
    for (label, article, text_label) in [
        ("Mountaincycling", "a", "Mountain"),
        ("Islandcycling", "an", "Island"),
        ("Halflingcycling", "a", "Halfling"),
        ("Landcycling", "a", "land"),
        ("Artifactcycling", "an", "artifact"),
        ("Basic landcycling", "a", "basic land"),
    ] {
        let bare = format!("{label} {{1}}{{R}}");
        let full = format!("{bare} {}", reminder("{1}{R}", article, text_label));
        let bare_rule = lowered(&bare).unwrap_or_else(|error| panic!("{bare}: {error}"));
        let full_rule = lowered(&full).unwrap_or_else(|error| panic!("{full}: {error}"));
        assert_eq!(bare_rule, full_rule, "{label}");
        assert_eq!(bare_rule["ability"]["cost"]["manaCost"], "{1}{R}");
    }
    // Distinct labels yield distinct searched-card predicates.
    let mountain = lowered("Mountaincycling {2}").unwrap();
    let forest = lowered("Forestcycling {2}").unwrap();
    assert_ne!(mountain["ability"]["where"], forest["ability"]["where"]);
}

#[test]
fn contradicting_or_malformed_envelopes_fail_explicitly() {
    for text in [
        // Reminder names a different card kind than the label.
        format!("Mountaincycling {{2}} {}", reminder("{2}", "a", "Forest")),
        // Reminder cost differs from the printed cost.
        format!("Mountaincycling {{2}} {}", reminder("{3}", "a", "Mountain")),
        // Reminder searches without revealing.
        "Mountaincycling {2} ({2}, Discard this card: Search your library for a Mountain card, put it into your hand, then shuffle.)".to_string(),
        // Reminder sends the card to the battlefield.
        "Mountaincycling {2} ({2}, Discard this card: Search your library for a Mountain card, reveal it, put it onto the battlefield, then shuffle.)".to_string(),
        // Variable and nonmana typecycling costs have no runtime proof.
        "Mountaincycling {X}".to_string(),
        "Mountaincycling Pay 2 life.".to_string(),
        // Trailing text is never silently dropped.
        "Mountaincycling {2}. Draw a card.".to_string(),
        // The label must be a real card kind.
        "Zzzcycling {2} (garbled)".to_string(),
    ] {
        assert!(node(&text).is_err(), "{text}");
    }
}

#[test]
fn plain_cycling_has_no_type_label_and_is_not_claimed() {
    let tokens = recognize_oracle_entities("Cycling {2}");
    assert!(!typecycling_label_starts(&tokens));
}
