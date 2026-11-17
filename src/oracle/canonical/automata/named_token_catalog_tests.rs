use crate::card_catalog::named_token_printing;
use crate::oracle::{OracleCardParseRequest, ParsedOracleAbility, parse_oracle_card};
use serde_json::{Value, json};

const LANDER: &str = "{2}, {T}, Sacrifice this token: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.";
const MUTAGEN: &str = "{1}, {T}, Sacrifice this token: Put a +1/+1 counter on target creature. Activate only as a sorcery.";
const HEARTWOOD: &str = "{T}: Add {R} or {G}.";

fn parse(type_line: &str, text: &str) -> Vec<ParsedOracleAbility> {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Probe".into(),
        type_line: type_line.into(),
        mana_cost: None,
        oracle_text: Some(text.into()),
        layout: Some("normal".into()),
        faces: Vec::new(),
    })
    .abilities
}

fn canonical_rule(type_line: &str, text: &str) -> Value {
    let abilities = parse(type_line, text);
    assert_eq!(abilities.len(), 1, "{abilities:?}");
    assert_eq!(
        abilities[0].status, "canonical",
        "{:?}",
        abilities[0].diagnostics
    );
    abilities[0].rule.clone().unwrap()
}

fn failure_state(type_line: &str, text: &str) -> String {
    let abilities = parse(type_line, text);
    let ability = abilities.last().unwrap();
    assert_ne!(ability.status, "canonical", "{text}");
    assert!(ability.rule.is_none());
    ability
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.state.clone())
        .last()
        .unwrap_or_default()
}

fn token_of(rule: &Value) -> Value {
    fn find(value: &Value) -> Option<Value> {
        match value {
            Value::Object(map) => {
                if map.get("kind").and_then(Value::as_str) == Some("createTokens") {
                    return Some(value.clone());
                }
                map.values().find_map(find)
            }
            Value::Array(items) => items.iter().find_map(find),
            _ => None,
        }
    }
    find(rule).expect("createTokens effect")
}

#[test]
fn catalog_tokens_compile_with_the_shared_ability_grammar() {
    for (name, oracle) in [
        ("lander", LANDER),
        ("mutagen", MUTAGEN),
        ("heartwood", HEARTWOOD),
    ] {
        let printing = named_token_printing(name).unwrap().expect(name);
        assert_eq!(printing.oracle_text.as_deref(), Some(oracle));
        let rules = crate::oracle::compile_named_token_catalog_rules(
            name,
            printing.type_line.as_deref().unwrap(),
            oracle,
        )
        .expect(name);
        assert_eq!(rules.len(), 1);
    }
    assert_eq!(
        named_token_printing("heartwood")
            .unwrap()
            .unwrap()
            .color_names(),
        ["red", "green"]
    );
}

#[test]
fn real_cards_create_lander_mutagen_and_heartwood_with_validated_reminders() {
    for (text, name, quantity) in [
        (
            "When this creature enters, create a Lander token. (It's an artifact with \"{2}, {T}, Sacrifice this token: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\")",
            "Lander",
            json!({"kind":"integer","value":1}),
        ),
        (
            "When this creature enters, create a Mutagen token. (It's an artifact with \"{1}, {T}, Sacrifice this token: Put a +1/+1 counter on target creature. Activate only as a sorcery.\")",
            "Mutagen",
            json!({"kind":"integer","value":1}),
        ),
        (
            "When this creature enters, you may sacrifice a land. If you do, create two tapped Heartwood tokens. (They're red and green artifacts with \"{T}: Add {R} or {G}.\")",
            "Heartwood",
            json!({"kind":"integer","value":2}),
        ),
    ] {
        let effect = token_of(&canonical_rule("Creature - Elf", text));
        assert_eq!(effect["token"]["name"], json!(name));
        assert_eq!(effect["quantity"], quantity);
    }
}

#[test]
fn catalog_tokens_compose_with_unused_combinations() {
    // Plural quantity, a tapped entry state and a different controller are
    // independent of the token identity.
    let rule = canonical_rule("Sorcery", "Create two tapped Mutagen tokens.");
    let effect = token_of(&rule);
    assert_eq!(effect["token"]["name"], json!("Mutagen"));
    assert_eq!(effect["tapped"], json!(true));
    assert_eq!(effect["quantity"], json!({"kind":"integer","value":2}));
    let rule = canonical_rule(
        "Instant",
        "Destroy target nonland permanent. Its controller creates a Lander token.",
    );
    assert_eq!(token_of(&rule)["token"]["name"], json!("Lander"));
    assert_eq!(
        token_of(&canonical_rule(
            "Sorcery",
            "Create a Heartwood token. Create a Lander token."
        ))["token"]["name"],
        json!("Heartwood")
    );
}

#[test]
fn mismatched_reminders_and_unmodeled_tokens_fail_explicitly() {
    // Wrong colors for a colored catalog token.
    assert_ne!(
        failure_state(
            "Sorcery",
            "Create a Heartwood token. (It's a red artifact with \"{T}: Add {R} or {G}.\")"
        ),
        ""
    );
    // Colorless description for a colored token.
    assert_ne!(
        failure_state(
            "Sorcery",
            "Create a Heartwood token. (It's a colorless artifact with \"{T}: Add {R} or {G}.\")"
        ),
        ""
    );
    // Embedded rules differing from the catalog definition.
    assert_ne!(
        failure_state(
            "Sorcery",
            "Create a Lander token. (It's an artifact with \"{2}, {T}, Sacrifice this token: Draw a card.\")"
        ),
        ""
    );
    // Mutavault's animation ability is not representable yet, so it is not
    // a catalog token and its creation stays an explicit failure.
    assert_eq!(
        failure_state("Sorcery", "Create a tapped Mutavault token."),
        "token.named.catalog-identity"
    );
    assert!(named_token_printing("mutavault").unwrap().is_none());
}

#[test]
fn heartwood_definition_carries_its_printed_colors() {
    let printing = named_token_printing("heartwood").unwrap().unwrap();
    let marker = printing.color_indicator_rule().unwrap();
    assert_eq!(marker["colorIndicator"], json!(["R", "G"]));
    assert!(
        named_token_printing("lander")
            .unwrap()
            .unwrap()
            .color_indicator_rule()
            .is_none()
    );
}
