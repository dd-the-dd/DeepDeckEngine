use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_entities, resolve_counter_name_spans},
};

const REPLACEMENT: &str = "If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead.";

fn with_input<T>(text: &str, type_line: &str, operation: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    operation(&AbilityInput {
        face_name: "Synthetic quantity replacement",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn battlefield_replacement_requires_a_permanent_source_face() {
    for type_line in ["Enchantment", "Artifact", "Creature — Human"] {
        with_input(REPLACEMENT, type_line, |input| {
            assert!(matches!(
                try_parse_promoted_quantity_replacement(input),
                QuantityReplacementParseAttempt::Parsed(_)
            ));
        });
    }
    for type_line in ["Instant", "Sorcery"] {
        with_input(REPLACEMENT, type_line, |input| {
            let QuantityReplacementParseAttempt::Failed(failure) =
                try_parse_promoted_quantity_replacement(input)
            else {
                panic!("a nonpermanent face cannot own this battlefield modifier");
            };
            assert_eq!(failure.state, "quantity-replacement.source-domain");
        });
    }
}

#[test]
fn unrelated_and_conditional_envelopes_keep_their_parent() {
    for text in [
        "If you control a creature, draw a card.",
        "If one or more creatures you control die, draw a card.",
        "If an effect would create one or more tokens under your control, if you control a creature, it creates twice that many of those tokens instead.",
        "If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead. Draw a card.",
    ] {
        with_input(text, "Enchantment", |input| {
            assert!(
                matches!(
                    try_parse_promoted_quantity_replacement(input),
                    QuantityReplacementParseAttempt::NotClaimed
                ),
                "must preserve the enclosing family: {text}"
            );
        });
    }
}

#[test]
fn event_domain_commit_retains_its_structured_failure() {
    for (text, state) in [
        (
            "If one or more +1/+1 counters would be put on a creature you control, that many plus one -1/-1 counters are put on it instead.",
            "quantity-replacement.counter-identity",
        ),
        (
            "If one or more ice counters would be put on a creature you control, that many plus X ice counters are put on it instead.",
            "quantity-replacement.numeric-binding",
        ),
    ] {
        with_input(text, "Enchantment", |input| {
            let QuantityReplacementParseAttempt::Failed(failure) =
                try_parse_promoted_quantity_replacement(input)
            else {
                panic!("a committed replacement must retain its diagnostic");
            };
            assert_eq!(failure.state, state);
        });
    }
}

#[test]
fn soft_mismatch_restores_a_nonzero_caller_checkpoint() {
    let mut tokens = recognize_entities("prefix If one or more creatures die, draw a card.");
    resolve_counter_name_spans(&mut tokens);
    let mut cursor = OracleTokenCursor::new("quantity-transaction-test", &tokens);
    cursor.consume("prefix").unwrap();
    cursor.enter("caller-retained-frame");
    let position = cursor.position();
    let trace = cursor.transitions().to_vec();
    assert!(matches!(
        try_recognize_quantity_replacement(&mut cursor),
        QuantityReplacementRecognition::NotClaimed
    ));
    assert_eq!(cursor.position(), position);
    assert_eq!(&cursor.transitions()[..trace.len()], trace.as_slice());
    assert_eq!(cursor.transitions().len(), trace.len() + 1);
    assert_eq!(cursor.transitions().last().unwrap().state, "rollback");
    assert_eq!(cursor.peek(), Some("If"));
}
