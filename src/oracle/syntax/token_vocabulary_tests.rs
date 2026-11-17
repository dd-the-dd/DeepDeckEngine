use super::*;

#[test]
fn finite_words_retain_their_first_matching_lexical_role() {
    for (raw, expected) in [
        ("at", ("trigger", "triggerWord")),
        ("AT", ("trigger", "triggerWord")),
        ("a", ("logic", "quantifierWord")),
        ("an", ("logic", "quantifierWord")),
        ("player's", ("entityReference", "possessivePlayerRoleWord")),
        ("player’s", ("entityReference", "possessivePlayerRoleWord")),
        ("spell’s", ("entityReference", "possessiveTypeWord")),
        ("nonbasic", ("entityReference", "negativeSupertypeWord")),
        ("card", ("entityReference", "typeWord")),
        ("cards", ("magicVocabulary", "objectWord")),
        ("only", ("magicVocabulary", "qualifierWord")),
    ] {
        assert_eq!(classify(raw), expected, "{raw}");
    }
}

#[test]
fn open_patterns_and_unknowns_preserve_the_existing_lexical_boundary() {
    for (raw, expected_type) in [
        ("{E}", "counterSymbol"),
        ("{T}", "tapSymbol"),
        ("{garbage}", "manaSymbol"),
        ("{}", "unknown"),
        ("first", "ordinalWord"),
        ("21st", "ordinalWord"),
        ("1th", "ordinalWord"),
        ("21stx", "unknown"),
        ("twenty-one", "quantity"),
        ("X", "quantity"),
        ("9223372036854775807", "quantity"),
        ("9223372036854775808", "unknown"),
        ("+1/+1", "counterValue"),
        ("1/-2", "counterValue"),
        ("+", "separator"),
        ("−", "separator"),
        ("artifact's", "possessiveTypeWord"),
        ("creature’s", "possessiveTypeWord"),
        ("noncreatures", "negativeTypeWord"),
        ("noncommander", "negativeCharacteristicWord"),
        ("noncommanders", "unknown"),
        ("kindred", "unknown"),
        ("nonkindred", "unknown"),
        (" when ", "unknown"),
        (" a ", "quantity"),
        (" first ", "ordinalWord"),
        ("UnregisteredCardName", "unknown"),
    ] {
        assert_eq!(classify(raw).1, expected_type, "{raw}");
    }
}

#[test]
fn repeated_and_concurrent_lookups_share_the_immutable_dictionary() {
    let initial = vocabulary() as *const TokenVocabulary as usize;
    let threads: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                assert_eq!(classify("deals"), ("magicVocabulary", "actionWord"));
                vocabulary() as *const TokenVocabulary as usize
            })
        })
        .collect();
    for thread in threads {
        assert_eq!(thread.join().expect("lookup thread"), initial);
    }
    assert_eq!(vocabulary() as *const TokenVocabulary as usize, initial);
}

#[test]
fn registered_possessives_share_ascii_and_unicode_apostrophe_roles() {
    for raw in ["CARD'S", "CARD’S", "creature’s", "ARTIFACT’S"] {
        assert_eq!(
            classify(raw),
            ("entityReference", "possessiveTypeWord"),
            "{raw}"
        );
    }
    for raw in [
        "unregisterednoun's",
        "unregisterednoun’s",
        "UNREGISTEREDNOUN’S",
    ] {
        assert_eq!(classify(raw), UNKNOWN, "{raw}");
    }
}

#[test]
fn unicode_possessive_classification_keeps_original_bytes_and_spans() {
    let text = "equal to THAT CARD’S POWER";
    let tokens = super::super::recognize_entities(text);
    let nominal = tokens
        .iter()
        .find(|token| token.raw == "CARD’S")
        .expect("raw nominal retained");
    assert_eq!(nominal.token_type, "possessiveTypeWord");
    assert_eq!(&text[nominal.span_start..nominal.span_end], nominal.raw);
    assert_eq!((nominal.span_start, nominal.span_end), (14, 22));
}
