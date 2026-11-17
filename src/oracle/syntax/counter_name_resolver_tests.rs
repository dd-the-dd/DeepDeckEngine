use super::{recognize_entities, resolve_counter_name_spans};

fn resolve(text: &str) -> Vec<super::super::counter_name::CounterName> {
    let mut tokens = recognize_entities(text);
    let names = resolve_counter_name_spans(&mut tokens);
    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "contextual counter names must consume only their own UNKNOWN tokens: {tokens:#?}"
    );
    names
}

#[test]
fn resolves_open_single_and_multi_token_counter_names() {
    for (text, expected) in [
        ("Put a +1/+1 counter on this creature.", "+1/+1"),
        ("Put a flying counter on this creature.", "flying"),
        ("Put two charge counters on this artifact.", "charge"),
        ("Put three oil counters on this artifact.", "oil"),
        (
            "Put a flux capacitor calibration counter on this artifact.",
            "flux capacitor calibration",
        ),
        (
            "Put a Flux-Capacitor Calibration counter on this artifact.",
            "flux-capacitor calibration",
        ),
    ] {
        let names = resolve(text);
        assert_eq!(names.len(), 1, "{text}");
        assert_eq!(names[0].as_str(), expected, "{text}");
    }
}

#[test]
fn preserves_typed_power_toughness_counter_values_while_resolving_their_names() {
    for value in ["+1/+1", "-1/-1", "-2/-2"] {
        let text = format!("Put a {value} counter on this creature.");
        let mut tokens = recognize_entities(&text);

        let names = resolve_counter_name_spans(&mut tokens);

        assert_eq!(names.len(), 1, "{text}");
        assert_eq!(names[0].as_str(), value, "{text}");
        let value_token = tokens
            .iter()
            .find(|token| token.raw == value)
            .expect("the power/toughness counter value remains one token");
        assert_eq!(value_token.category, "magicVocabulary", "{text}");
        assert_eq!(value_token.token_type, "counterValue", "{text}");
    }
}

#[test]
fn still_normalizes_named_counter_words_for_the_shared_counter_automaton() {
    for (text, raw, expected) in [
        ("Put a flying counter on this creature.", "flying", "flying"),
        (
            "Put two charge counters on this artifact.",
            "charge",
            "charge",
        ),
    ] {
        let mut tokens = recognize_entities(text);

        let names = resolve_counter_name_spans(&mut tokens);

        assert_eq!(names.len(), 1, "{text}");
        assert_eq!(names[0].as_str(), expected, "{text}");
        let name_token = tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case(raw))
            .expect("the named counter word remains present");
        assert_eq!(name_token.token_type, "counterNamePart", "{text}");
    }
}

#[test]
fn does_not_hide_unknowns_outside_a_counter_name_span() {
    let mut tokens = recognize_entities("Put a charge counter on Mystery Widget.");
    let names = resolve_counter_name_spans(&mut tokens);

    assert_eq!(names[0].as_str(), "charge");
    let unresolved = tokens
        .iter()
        .filter(|token| token.token_type == "unknown")
        .map(|token| token.raw.as_str())
        .collect::<Vec<_>>();
    assert_eq!(unresolved, ["Mystery", "Widget"]);
}

#[test]
fn unnamed_counter_phrases_are_not_invented() {
    let mut tokens = recognize_entities("Put two counters on this creature.");
    let names = resolve_counter_name_spans(&mut tokens);

    assert!(names.is_empty());
    assert_eq!(tokens[1].token_type, "quantity");
}

#[test]
fn preserves_semantic_counter_types_for_specialized_automata() {
    let mut tokens = recognize_entities("You get three energy counters.");
    let names = resolve_counter_name_spans(&mut tokens);

    assert!(names.is_empty());
    let energy = tokens
        .iter()
        .find(|token| token.raw.eq_ignore_ascii_case("energy"))
        .expect("energy token");
    assert_eq!(energy.token_type, "counterTypeWord");
}
