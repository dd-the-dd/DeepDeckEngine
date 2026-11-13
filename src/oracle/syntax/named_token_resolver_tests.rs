use super::{ExactNameTokenIndex, recognize_entities, resolve_named_token_spans};

#[test]
fn named_token_resolution_commits_the_longest_exact_catalog_span() {
    let index = ExactNameTokenIndex::from_names(["Map", "Ancient Map"]);
    let mut tokens = recognize_entities("You create two Ancient Map tokens");

    resolve_named_token_spans(&mut tokens, &index);

    assert_eq!(tokens[3].raw, "Ancient");
    assert_eq!(tokens[4].raw, "Map");
    assert_eq!(tokens[3].token_type, "namedTokenWord");
    assert_eq!(tokens[4].token_type, "namedTokenWord");
}

#[test]
fn unknown_token_name_remains_unknown_when_the_catalog_has_no_exact_match() {
    let index = ExactNameTokenIndex::from_names(["Map"]);
    let mut tokens = recognize_entities("You create two Mystery tokens");

    resolve_named_token_spans(&mut tokens, &index);

    let mystery = tokens
        .iter()
        .find(|token| token.raw == "Mystery")
        .expect("Mystery token");
    assert_eq!(mystery.token_type, "unknown");
}
