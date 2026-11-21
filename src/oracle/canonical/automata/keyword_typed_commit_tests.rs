use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_entities},
};

fn claim(text: &str) -> bool {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    super::super::super::claims_promoted_simple_keyword_family(&AbilityInput {
        face_name: "Resource keyword",
        face_type_line: "Creature",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn resource_failure_is_typed_from_actual_child_and_through_a_keyword_list() {
    for text in [
        "Ward—Pay X life.",
        "Ward—Sacrifice this creature.",
        "Flying and ward—Pay X life.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.typed-ward-resource", &tokens);
        assert!(
            matches!(
                try_recognize_standalone_keyword_ability(&mut cursor),
                StandaloneKeywordRecognition::FailedParameterizedResource(_)
            ),
            "{text}"
        );
        assert!(claim(text), "{text}");
    }
}

#[test]
fn parameterless_failure_does_not_inherit_a_resource_commitment() {
    let tokens = recognize_entities("Flying and.");
    let mut cursor = OracleTokenCursor::new("test.no-resource-commitment", &tokens);
    assert!(matches!(
        try_recognize_standalone_keyword_ability(&mut cursor),
        StandaloneKeywordRecognition::Failed(_)
    ));
}

#[test]
fn successful_resource_then_conditional_or_multisentence_tail_yields_the_family() {
    for text in [
        "Ward—Pay 2 life. You gain 1 life.",
        "Ward—Pay 2 life as long as you control an Island.",
        "Ward {2} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.) If you control an Island, draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.typed-ward-envelope", &tokens);
        assert!(
            matches!(
                try_recognize_standalone_keyword_ability(&mut cursor),
                StandaloneKeywordRecognition::RejectedEnvelope {
                    parameterized_resource: true,
                    ..
                }
            ),
            "{text}"
        );
        assert!(!claim(text), "{text}");
    }
}

#[test]
fn complete_success_does_not_change_the_ordinary_keyword_contract() {
    for text in ["Ward {2}", "Ward—Pay 2 life.", "Flying, ward {2}"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.typed-ward-success", &tokens);
        assert!(matches!(
            try_recognize_standalone_keyword_ability(&mut cursor),
            StandaloneKeywordRecognition::Parsed(_)
        ));
        assert!(claim(text));
    }
}
