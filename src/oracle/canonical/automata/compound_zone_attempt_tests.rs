use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn atomic_targeted_zone_change_retains_one_typed_action_and_all_costs() {
    let tokens = recognize_entities("{T}, Sacrifice this artifact: Exile target creature.");
    let mut cursor = OracleTokenCursor::new("compound-zone-attempt-test", &tokens);
    let CompoundActivatedZoneChangeParseAttempt::Parsed(node) =
        try_recognize_compound_activated_zone_change(&mut cursor)
    else {
        panic!("complete atomic family must parse");
    };
    assert_eq!(node.costs.costs.len(), 2);
    cursor
        .finish()
        .expect("atomic parent owns full consumption");
}

#[test]
fn later_effects_reject_the_atomic_envelope_transactionally() {
    for text in [
        "{T}: Exile target creature. Draw a card.",
        "{T}: Exile target creature and draw a card.",
        "{T}: Exile target creature and manifest dread.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("compound-zone-attempt-test", &tokens);
        assert!(
            matches!(
                try_recognize_compound_activated_zone_change(&mut cursor),
                CompoundActivatedZoneChangeParseAttempt::RejectedCandidate(_)
            ),
            "{text}"
        );
        assert_eq!(cursor.position(), 0, "{text}");
    }
}

#[test]
fn malformed_members_preserve_the_original_cost_and_selector_diagnostics() {
    for (text, state, found) in [
        (
            "{T}, Sacrifice this land Destroy target nonbasic land.",
            "activation.compound.effect-separator",
            "Destroy",
        ),
        (
            "{T}, Sacrifice this land: Destroy target nonbasic.",
            "effect.zone-change.target.validation",
            ".",
        ),
        (
            "{T}, Sacrifice this spell: Destroy target nonbasic land.",
            "cost.sacrifice-self.source-type",
            "spell",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("compound-zone-attempt-test", &tokens);
        let CompoundActivatedZoneChangeParseAttempt::FailedCommitted(failure) =
            try_recognize_compound_activated_zone_change(&mut cursor)
        else {
            panic!("malformed known family must retain its failure: {text}");
        };
        assert_eq!(failure.state, state);
        assert_eq!(failure.actual.as_deref(), Some(found));
    }
}
