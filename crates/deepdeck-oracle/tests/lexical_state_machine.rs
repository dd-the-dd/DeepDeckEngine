use deepdeck_oracle::oracle::{OracleCardParseRequest, parse_oracle_card};
use serde_json::Value;

const LIGHTNING_HELIX_ORACLE: &str =
    "Lightning Helix deals 3 damage to any target and you gain 3 life.";

fn lexical_entities(oracle_text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Lightning Helix".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });

    serde_json::to_value(&parsed.abilities[0].entities)
        .expect("lexical entities serialize")
        .as_array()
        .expect("lexical entities are an array")
        .clone()
}

#[test]
fn lightning_helix_lexical_view_retains_unknown_name_tokens_and_exact_spans() {
    let entities = lexical_entities(LIGHTNING_HELIX_ORACLE);
    let observed = entities
        .iter()
        .map(|entity| {
            (
                entity["raw"].as_str().expect("raw token"),
                entity["category"].as_str().expect("token category"),
                entity["tokenType"].as_str().expect("token type"),
                entity["spanStart"].as_u64().expect("span start"),
                entity["spanEnd"].as_u64().expect("span end"),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        observed,
        [
            ("Lightning", "unknown", "unknown", 0, 9),
            ("Helix", "unknown", "unknown", 10, 15),
            ("deals", "magicVocabulary", "actionWord", 16, 21),
            ("3", "logic", "quantity", 22, 23),
            ("damage", "magicVocabulary", "resourceWord", 24, 30),
            ("to", "syntax", "relationWord", 31, 33),
            ("any", "logic", "quantifierWord", 34, 37),
            ("target", "entityReference", "targetMarker", 38, 44),
            ("and", "logic", "logicWord", 45, 48),
            ("you", "entityReference", "referenceWord", 49, 52),
            ("gain", "magicVocabulary", "actionWord", 53, 57),
            ("3", "logic", "quantity", 58, 59),
            ("life", "magicVocabulary", "resourceWord", 60, 64),
            (".", "boundary", "separator", 64, 65),
        ]
    );
}

#[test]
fn lexical_view_emits_an_unknown_token_instead_of_dropping_an_unknown_character() {
    let entities = lexical_entities("Lightning Helix deals 3 damage to any target?");
    let unknown = entities.last().expect("unknown character is retained");

    assert_eq!(unknown["raw"], "?");
    assert_eq!(unknown["category"], "unknown");
    assert_eq!(unknown["tokenType"], "unknown");
    assert_eq!(unknown["spanStart"], 44);
    assert_eq!(unknown["spanEnd"], 45);
}

#[test]
fn lexical_view_preserves_symbols_boundaries_and_power_toughness() {
    let entities = lexical_entities("{2}{W}, {T}: Target creature gets +1/+1.");
    let raw = entities
        .iter()
        .map(|entity| entity["raw"].as_str().expect("raw token"))
        .collect::<Vec<_>>();

    assert_eq!(
        raw,
        [
            "{2}", "{W}", ",", "{T}", ":", "Target", "creature", "gets", "+1/+1", ".",
        ]
    );
}

#[test]
fn lexical_view_keeps_ascii_and_curly_apostrophes_inside_words() {
    let entities = lexical_entities("It’s that player's card.");
    let raw = entities
        .iter()
        .map(|entity| entity["raw"].as_str().expect("raw token"))
        .collect::<Vec<_>>();

    assert_eq!(raw, ["It’s", "that", "player's", "card", "."]);
}

#[test]
fn lexical_view_emits_an_unclosed_mana_delimiter_as_unknown() {
    let entities = lexical_entities("Add {G");
    assert_eq!(entities[1]["raw"], "{");
    assert_eq!(entities[1]["tokenType"], "unknown");
    assert_eq!(entities[1]["spanStart"], 4);
    assert_eq!(entities[1]["spanEnd"], 5);
}
