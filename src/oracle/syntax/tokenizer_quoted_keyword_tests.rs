use super::*;

#[test]
fn quoted_grant_body_resolves_exact_catalogue_keyword_without_losing_spans() {
    let text = "Other creatures you control have \"Ward—Pay 2 life.\"";
    let tokens = recognize_entities(text);
    let word = tokens.iter().find(|token| token.raw == "Ward").unwrap();
    assert_eq!(word.token_type, "keywordWord");
    assert_eq!(&text[word.span_start..word.span_end], "Ward");
}

#[test]
fn quoted_card_name_or_free_quote_does_not_become_keyword_context() {
    for text in [
        "Create a token named \"Ward\".",
        "\"Ward\"",
        "Choose a card named \"Ward\".",
    ] {
        let tokens = recognize_entities(text);
        let word = tokens.iter().find(|token| token.raw == "Ward").unwrap();
        assert_ne!(word.token_type, "keywordWord", "{text}");
    }
}
