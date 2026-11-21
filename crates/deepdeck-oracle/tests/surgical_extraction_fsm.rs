use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const SURGICAL_ORACLE: &str = "({B/P} can be paid with either {B} or 2 life.)\nChoose target card in a graveyard other than a basic land card. Search its owner's graveyard, hand, and library for any number of cards with the same name as that card and exile them. Then that player shuffles.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Grammar Probe".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B/P}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn same_name_rule(excluded: Value, all: bool) -> Value {
    json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetNamedCard",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "cards",
                    "zone": { "kind": "anyGraveyard" },
                    "where": { "kind": "not", "operand": excluded },
                },
            }],
        },
        "effects": [{
            "kind": "exileNamedCardsFromZones",
            "player": { "kind": "chosenTarget", "id": "targetNamedCard" },
            "chooser": { "kind": "controllerOf", "object": { "kind": "self" } },
            "nameFromCard": { "kind": "chosenTarget", "id": "targetNamedCard" },
            "zones": ["graveyard", "hand", "library"],
            "all": all,
            "shuffleLibrary": true,
        }],
    })
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Surgical Extraction; see docs/coverage-targets.md"
)]
#[test]
fn surgical_extraction_is_two_native_primitive_rules() {
    let parsed = parse(SURGICAL_ORACLE);
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["kind"],
        "rulesMarker"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&same_name_rule(
            json!({
                "kind": "and",
                "operands": [
                    { "kind": "typeLineContains", "value": "Basic" },
                    { "kind": "cardTypeContains", "value": "Land" },
                ],
            }),
            false,
        ))
    );
    let unknown = parsed
        .abilities
        .iter()
        .flat_map(|ability| &ability.entities)
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert!(unknown.is_empty(), "unknown Surgical tokens: {unknown:?}");
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Surgical Extraction; see docs/coverage-targets.md"
)]
#[test]
fn phyrexian_colors_and_excluded_card_predicates_generalize_independently() {
    let parsed = parse(
        "({U/P} can be paid with either {U} or 2 life.)\nChoose target card in a graveyard other than an instant card. Search its owner's graveyard, hand, and library for all cards with the same name as that card and exile them. Then that player shuffles.",
    );
    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&same_name_rule(
            json!({ "kind": "cardTypeContains", "value": "Instant" }),
            true,
        ))
    );
}

#[test]
fn malformed_family_members_fail_in_the_committed_child() {
    // A matching glossary is retained as a validated rules marker.
    let valid = parse("({B/P} can be paid with either {B} or 2 life.)");
    assert_eq!(valid.status, "canonical");
    assert_eq!(valid.abilities.len(), 1);
    assert_eq!(valid.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        valid.abilities[0].rule.as_ref().unwrap()["kind"],
        "rulesMarker"
    );
    // CR 207.2: a reminder-only line has no rules meaning. When the committed
    // child rejects the color mismatch, the line is dropped as reminder text
    // instead of being retained as a validated marker.
    let mismatch = parse("({B/P} can be paid with either {U} or 2 life.)");
    assert!(mismatch.abilities.is_empty(), "{:#?}", mismatch.abilities);

    let bad_reference = parse(
        "Choose target card in a graveyard other than a basic land card. Search its owner's graveyard, hand, and library for any number of cards with the same name as this card and exile them. Then that player shuffles.",
    );
    assert_eq!(bad_reference.status, "unsupported");
    assert_eq!(
        bad_reference.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        bad_reference.abilities[0].diagnostics[0].state.as_deref(),
        Some("same-name-search.target-reference")
    );
}
