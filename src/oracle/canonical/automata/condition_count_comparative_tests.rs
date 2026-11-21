use crate::oracle::syntax::recognize_entities;

use super::*;

#[test]
fn comparative_prefixes_share_the_existing_count_ast() {
    for (text, threshold, direction) in [
        (
            "you control at least two creatures,",
            2,
            ControlledCountDirection::OrMore,
        ),
        (
            "you control two or more creatures,",
            2,
            ControlledCountDirection::OrMore,
        ),
        (
            "you control at most 1 artifact,",
            1,
            ControlledCountDirection::OrFewer,
        ),
        (
            "you control one or fewer artifacts,",
            1,
            ControlledCountDirection::OrFewer,
        ),
        (
            "you control at least 0 artifacts,",
            0,
            ControlledCountDirection::OrMore,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.comparative-count", &tokens);
        let node = recognize_controlled_permanent_count_query(&mut cursor).expect(text);
        assert_eq!(node.threshold, threshold, "{text}");
        assert_eq!(node.direction, direction, "{text}");
        assert!(!node.exclude_source);
        assert_eq!(cursor.peek(), Some(","), "{text}");
    }
}

#[test]
fn mixed_unknown_missing_and_variable_comparisons_are_explicit_errors() {
    for text in [
        "you control at least X creatures,",
        "you control at roughly two creatures,",
        "you control at least creatures,",
        "you control at most two or more creatures,",
        "you control at two creatures,",
        "you control at least -1 creatures,",
        "you control at least 9223372036854775808 creatures,",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.comparative-count", &tokens);
        assert!(
            recognize_controlled_permanent_count_query(&mut cursor).is_err(),
            "{text}"
        );
    }
}
