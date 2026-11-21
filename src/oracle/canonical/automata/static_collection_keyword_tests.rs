use crate::oracle::syntax::recognize_entities;

use super::super::GrantedKeyword;
use super::*;

#[test]
fn object_union_and_keyword_conjunction_remain_separate_structures() {
    let tokens = recognize_entities(
        "Artifacts, enchantments, and creatures you control have flying, vigilance, and haste.",
    );
    let mut cursor = OracleTokenCursor::new("test.static-collection-keyword", &tokens);
    let StaticCollectionKeywordRecognition::Parsed(node) =
        try_recognize_static_collection_keyword_grant(&mut cursor)
    else {
        panic!("a coordinated collection with a keyword list is supported");
    };
    assert_eq!(cursor.position(), tokens.len());
    assert!(matches!(
        node.objects.predicate,
        super::super::ObjectPredicate::Union(ref branches) if branches.len() == 3
    ));
    assert_eq!(
        node.keywords,
        [
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying),
            KeywordAtomNode::Parameterless(GrantedKeyword::Vigilance),
            KeywordAtomNode::Parameterless(GrantedKeyword::Haste)
        ]
    );
}

#[test]
fn unsupported_envelopes_restore_nonzero_caller_memory_and_trace() {
    for body in [
        "Creatures you control have haste until end of turn.",
        "Creatures you control have haste as long as you control an Island.",
        "Creatures you control have flying or vigilance.",
        "Creatures you control have haste. You gain 1 life.",
    ] {
        let tokens = recognize_entities(&format!("prefix {body}"));
        let mut cursor = OracleTokenCursor::new("test.static-collection-rollback", &tokens);
        cursor.advance_to(1).expect("caller consumed its prefix");
        cursor.enter("caller.memory-preserved");
        let initial_trace = cursor.transitions().to_vec();
        let StaticCollectionKeywordRecognition::UnsupportedEnvelope(failure) =
            try_recognize_static_collection_keyword_grant(&mut cursor)
        else {
            panic!("a suffix must not become an unconditional static grant: {body}");
        };
        assert!(!failure.expected.is_empty());
        assert_eq!(cursor.position(), 1);
        assert_eq!(cursor.transitions().len(), initial_trace.len() + 1);
        assert_eq!(&cursor.transitions()[..initial_trace.len()], initial_trace);
        assert_eq!(cursor.transitions().last().unwrap().state, "rollback");
        assert_eq!(cursor.transitions().last().unwrap().token_index, 1);
        assert_eq!(cursor.fail("observation").state, "caller.memory-preserved");
    }
}

#[test]
fn malformed_keyword_conjunction_keeps_the_rejected_token() {
    let tokens = recognize_entities("Creatures you control have haste and.");
    let mut cursor = OracleTokenCursor::new("test.static-collection-keyword", &tokens);
    let StaticCollectionKeywordRecognition::Failed(failure) =
        try_recognize_static_collection_keyword_grant(&mut cursor)
    else {
        panic!("a dangling keyword connector is a structured failure");
    };
    assert_eq!(failure.state, "keyword-list.item");
    assert_eq!(failure.actual.as_deref(), Some("."));
}
