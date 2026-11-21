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

#[test]
fn reminder_subjects_use_exact_catalog_names_across_quoted_descriptions() {
    let index = ExactNameTokenIndex::from_names(["Map", "Ancient Map", "Treasure"]);
    let mut tokens = recognize_entities(
        "Create an Ancient Map token or a Treasure token. (Ancient Map is an artifact with \"Draw a card.\" Treasure is an artifact with \"Add one mana of any color.\")",
    );
    resolve_named_token_spans(&mut tokens, &index);
    for name in ["Ancient", "Map", "Treasure"] {
        assert!(
            tokens
                .iter()
                .filter(|token| token.raw == name)
                .all(|token| token.token_type == "namedTokenWord")
        );
    }
}

#[test]
fn reminder_subject_linking_preserves_source_entities_and_scopes() {
    let index = ExactNameTokenIndex::from_names(["Relic"]);
    let mut tokens = recognize_entities(
        "Relic is ordinary text. (Relic is an artifact with \"Relic is quoted.\")",
    );
    let original = tokens.clone();
    resolve_named_token_spans(&mut tokens, &index);
    let locations = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.raw == "Relic")
        .map(|(position, _)| position)
        .collect::<Vec<_>>();
    assert_eq!(locations.len(), 3);
    assert_eq!(
        serde_json::to_value(&tokens[locations[0]]).unwrap(),
        serde_json::to_value(&original[locations[0]]).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&tokens[locations[2]]).unwrap(),
        serde_json::to_value(&original[locations[2]]).unwrap()
    );
    assert_eq!(tokens[locations[1]].token_type, "namedTokenWord");
    let mut expected = original;
    expected[locations[1]].category = "magicVocabulary".to_string();
    expected[locations[1]].token_type = "namedTokenWord".to_string();
    assert_eq!(
        serde_json::to_value(&tokens).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[test]
fn reminder_subject_requires_exact_complete_name_and_copula() {
    let index = ExactNameTokenIndex::from_names(["Ancient Map", "Relic"]);
    for text in [
        "(Map is an artifact.)",
        "(Ancient Mapmaker is an artifact.)",
        "(Relic becomes an artifact.)",
        "(Other Relic is an artifact.)",
        "(Mystery is an artifact.)",
    ] {
        let mut tokens = recognize_entities(text);
        let original = tokens.clone();
        resolve_named_token_spans(&mut tokens, &index);
        assert_eq!(
            serde_json::to_value(&tokens).unwrap(),
            serde_json::to_value(&original).unwrap(),
            "{text}"
        );
    }
}

#[test]
fn reminder_subject_articles_and_token_nouns_keep_the_same_identity() {
    let index = ExactNameTokenIndex::from_names(["Relic"]);
    for text in [
        "(Relic is an artifact.)",
        "(A Relic is an artifact.)",
        "(A Relic token is an artifact.)",
    ] {
        let mut tokens = recognize_entities(text);
        resolve_named_token_spans(&mut tokens, &index);
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.raw == "Relic")
                .unwrap()
                .token_type,
            "namedTokenWord"
        );
    }
}

#[test]
fn quoted_parentheses_outside_reminders_do_not_promote_bare_subjects() {
    let index = ExactNameTokenIndex::from_names(["Relic"]);
    let mut tokens = recognize_entities(
        "Create a token with \"(Relic is an artifact.)\". Relic is ordinary text.",
    );
    let original = serde_json::to_value(&tokens).unwrap();
    resolve_named_token_spans(&mut tokens, &index);
    assert_eq!(serde_json::to_value(&tokens).unwrap(), original);

    // The preexisting token-noun pass has always linked this explicit name,
    // even inside a quoted rule. Global quote tracking changes only subjects.
    let mut tokens = recognize_entities("Create a token with \"Create a Relic token.\"");
    resolve_named_token_spans(&mut tokens, &index);
    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw == "Relic")
            .unwrap()
            .token_type,
        "namedTokenWord"
    );
}
