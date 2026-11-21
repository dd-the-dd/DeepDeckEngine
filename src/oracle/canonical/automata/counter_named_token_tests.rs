use super::*;

fn catalog_oracle(token_name: &str) -> &'static str {
    named_token_printing(token_name)
        .expect("the local named-token catalog is available")
        .expect("the token has a catalog printing")
        .oracle_text
        .as_deref()
        .expect("the token has Oracle text")
}

fn embedded_range(tokens: &[RecognizedEntity]) -> (usize, usize) {
    let opening = tokens.iter().position(|token| token.raw == "\"").unwrap();
    let start = opening + 1;
    let end = start
        + tokens[start..]
            .iter()
            .position(|token| token.raw == "\"")
            .unwrap();
    (start, end)
}

#[test]
fn borrowed_token_semantics_match_catalog_without_rebasing_entity_indices() {
    for (name, oracle) in [
        (
            "Treasure",
            "{T}, Sacrifice this token: Add one mana of any color.",
        ),
        // This is a semantic sibling, not a literal catalog-sentence match.
        (
            "Food",
            "{2}, {T}, Sacrifice this token: You gain three life.",
        ),
    ] {
        let catalog = parse_named_token_semantics(catalog_oracle(name));
        assert!(catalog.is_some());
        let source = format!("Create a token. (It's an artifact with \"{oracle}\")");
        let tokens = recognize_entities(&source);
        let (start, end) = embedded_range(&tokens);
        let borrowed = &tokens[start..end];
        assert!(start > 0);
        assert_eq!(borrowed[0].index, start);
        assert_ne!(borrowed[0].index, 0);
        assert_eq!(
            &source[borrowed[0].span_start..borrowed[0].span_end],
            borrowed[0].raw
        );
        assert_eq!(parse_named_token_semantics_from_tokens(borrowed), catalog);
    }
}

#[test]
fn named_token_reminders_preserve_exact_treasure_and_semantic_food_projection() {
    for (name, quantity, text) in [
        (
            "Treasure",
            NumericAtom::Integer(2),
            "Draw two cards and create two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        ),
        (
            "Food",
            NumericAtom::Integer(1),
            "Create a Food token. (It's an artifact with \"{2}, {T}, Sacrifice this token: You gain three life.\")",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.named-token-reminder", &tokens);
        let opening = tokens.iter().position(|token| token.raw == "(").unwrap();
        cursor.advance_to(opening).unwrap();
        recognize_named_token_reminder(&mut cursor, &quantity, name)
            .expect("the reminder projects the catalog semantics");
        cursor.finish().expect("the complete reminder is consumed");
    }
}

#[test]
fn semantic_near_miss_keeps_the_containing_ability_diagnostic_span() {
    let text = "Create a Food token. (It's an artifact with \"{2}, {T}, Sacrifice this token: You gain 4 life.\") then";
    let tokens = recognize_entities(text);
    let opening = tokens.iter().position(|token| token.raw == "(").unwrap();
    let trailing = tokens.iter().position(|token| token.raw == "then").unwrap();
    let mut cursor = OracleTokenCursor::new("test.named-token-reminder", &tokens);
    cursor.advance_to(opening).unwrap();
    let failure = recognize_named_token_reminder(&mut cursor, &NumericAtom::Integer(1), "Food")
        .expect_err("a different life amount is not the catalog token");
    assert_eq!(failure.state, "reminder.named-token.semantic-match");
    assert_eq!(failure.token_index, trailing);
    assert_eq!(failure.span_start, tokens[trailing].span_start);
    assert_eq!(failure.span_end, tokens[trailing].span_end);
    assert_eq!(failure.actual.as_deref(), Some("then"));
    assert_eq!(
        failure.expected,
        "the catalog token type line and canonical rules"
    );
}

#[test]
fn reminder_honors_upstream_token_classes_instead_of_relexing_raw_text() {
    // Warm and validate the separate catalog boundary before mutating the
    // caller's lexical result. The raw sentence stays valid Oracle; only the
    // upstream classification is deliberately unsupported. Render-and-relex
    // would incorrectly erase this distinction and accept the reminder.
    assert!(parse_named_token_semantics(catalog_oracle("Food")).is_some());
    let text = "Create a Food token. (It's an artifact with \"{2}, {T}, Sacrifice this token: You gain 3 life.\")";
    let mut tokens = recognize_entities(text);
    let mana = tokens.iter_mut().find(|token| token.raw == "{2}").unwrap();
    assert_eq!(mana.token_type, "manaSymbol");
    mana.token_type = "unknown".to_string();
    let opening = tokens.iter().position(|token| token.raw == "(").unwrap();
    let mut cursor = OracleTokenCursor::new("test.named-token-reminder", &tokens);
    cursor.advance_to(opening).unwrap();
    let failure = recognize_named_token_reminder(&mut cursor, &NumericAtom::Integer(1), "Food")
        .expect_err("a child may not replace its caller's lexical classification");
    assert_eq!(failure.state, "reminder.named-token.semantic-match");
    assert_eq!(failure.token_index, tokens.len());
    assert_eq!(failure.span_start, text.len());
    assert_eq!(failure.span_end, text.len());
}
