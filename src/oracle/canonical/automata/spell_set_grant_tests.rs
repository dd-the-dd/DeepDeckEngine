use super::super::*;
use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

fn attempt(text: &str) -> SpellSetGrantAttempt {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("static-spell-grant-test", &tokens);
    try_recognize_spell_set_grant(&mut cursor)
}

fn parsed(text: &str) -> SpellSetGrantNode {
    match attempt(text) {
        SpellSetGrantAttempt::Parsed(node) => node,
        SpellSetGrantAttempt::Failed(failure) => panic!("{text}: {failure}"),
        SpellSetGrantAttempt::NotClaimed => panic!("{text}: not claimed"),
    }
}

#[test]
fn chief_engineer_grants_convoke_to_artifact_spells_you_cast() {
    let node = parsed(
        "Artifact spells you cast have convoke. (Your creatures can help cast those spells. Each creature you tap while casting an artifact spell pays for {1} or one mana of that creature's color.)",
    );
    assert_eq!(
        node.keyword,
        SpellGrantedKeyword::Parameterless(GrantedKeyword::Convoke)
    );
    assert_eq!(
        node.spells.caster,
        Some(SpellCasterScope::AbilityController)
    );
    assert!(node.spells.predicate.is_in_domain(ObjectKind::Spell));
    assert!(
        node.spells
            .predicate
            .guarantees_card_type(ObjectCardType::Artifact)
    );
}

#[test]
fn each_singular_subject_agrees_with_has_and_reads_conspire_and_replicate() {
    let conspire = parsed("Each red or green instant or sorcery spell you cast has conspire.");
    assert_eq!(conspire.keyword, SpellGrantedKeyword::Conspire);
    let replicate = parsed("Artifact spells you cast have replicate {2}{U}.");
    assert!(matches!(
        &replicate.keyword,
        SpellGrantedKeyword::Replicate(cost) if cost.as_oracle_cost() == "{2}{U}"
    ));
}

#[test]
fn verb_agreement_and_unread_cost_sentences_fail_explicitly() {
    // `each ... have` does not agree: the frame is never claimed.
    assert!(matches!(
        attempt("Each artifact spell you cast have convoke."),
        SpellSetGrantAttempt::NotClaimed
    ));
    // A permanent subject belongs to the permanent keyword grants.
    assert!(matches!(
        attempt("Creatures you control have flying."),
        SpellSetGrantAttempt::NotClaimed
    ));
    // `The replicate cost is equal to its mana cost.` is not a mana cost.
    match attempt(
        "Each instant and sorcery spell you cast has replicate. The replicate cost is equal to its mana cost.",
    ) {
        SpellSetGrantAttempt::Failed(failure) => {
            assert_eq!(failure.state, "cost.mana.start", "{failure}")
        }
        _ => panic!("an unread replicate cost fails after the claim"),
    }
    // A cast origin is a record this grant cannot carry: it fails at `from`.
    match attempt("Spells you cast from exile have convoke.") {
        SpellSetGrantAttempt::NotClaimed => {}
        SpellSetGrantAttempt::Failed(failure) => {
            assert_eq!(failure.state, "selector.cast-origin.consumer", "{failure}")
        }
        SpellSetGrantAttempt::Parsed(node) => panic!("cast origin dropped: {node:?}"),
    }
}
