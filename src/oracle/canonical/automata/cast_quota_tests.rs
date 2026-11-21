use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn typed_quota_reuses_numeric_and_boolean_spell_children() {
    for text in [
        "Each player can't cast more than one noncreature spell each turn.",
        "Each player cannot cast more than 2 instant or sorcery spells each turn.",
        "Each player can't cast more than 1 artifact creature spell each turn.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("quota-child", &tokens);
        let CastQuotaRecognition::Parsed(node) = try_recognize_cast_quota(&mut cursor) else {
            panic!("complete quota must parse: {text}");
        };
        assert_eq!(node.players, PlayerSetReferenceNode::EachPlayer);
        assert!(matches!(
            node.maximum,
            NumericExpression::Atom(super::super::NumericAtom::Integer(_))
        ));
        assert!(cursor.peek().is_none());
    }
}

#[test]
fn quota_does_not_claim_other_prohibitions_or_broader_envelopes() {
    for text in [
        "Your opponents can't cast spells during your turn.",
        "Each player can't cast more than one noncreature spell each turn. You gain 1 life.",
        "Each player can't cast more than one noncreature spell each turn unless they pay {1}.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("quota-envelope", &tokens);
        assert!(matches!(
            try_recognize_cast_quota(&mut cursor),
            CastQuotaRecognition::NotClaimed | CastQuotaRecognition::UnsupportedEnvelope(_)
        ));
        assert_eq!(
            cursor.peek(),
            tokens.first().map(|token| token.raw.as_str())
        );
    }
}

#[test]
fn malformed_committed_operator_reports_local_failure() {
    let tokens =
        recognize_entities("Each player can't cast more under one noncreature spell each turn.");
    let mut cursor = OracleTokenCursor::new("quota-near-miss", &tokens);
    let CastQuotaRecognition::Failed(failure) = try_recognize_cast_quota(&mut cursor) else {
        panic!("committed malformed operator must fail")
    };
    assert_eq!(failure.state, "static.cast-quota.comparison");
}
