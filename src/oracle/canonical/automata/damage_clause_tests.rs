//! The damage frame of the shared clause: active and passive voice, the
//! damage-as-subject passive, the `would` modality, the one damage-kind
//! modifier, and the claim boundary of the frame.
use super::super::super::OracleTokenCursor;
use super::super::{
    ClauseRecognition, ClauseSubject, DamageEventRecipient, DamagePlayerRecipient,
    DamageReplacementContext, DamageSourceAntecedent, recognize_clause,
};
use super::*;
use crate::oracle::syntax::recognize_entities;

fn read(text: &str, frame: &ClauseFrame<'_>) -> (ClauseRecognition, usize) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("damage-clause-test", &tokens);
    let result = recognize_clause(&mut cursor, frame);
    (result, cursor.position())
}

fn arguments(result: ClauseRecognition) -> DamageClauseArguments {
    match result {
        ClauseRecognition::Recognized(clause) => match clause.action {
            ClauseAction::DealDamage(arguments) => arguments,
            other => panic!("not a damage clause: {other:?}"),
        },
        ClauseRecognition::Failed(failure) => panic!("failed: {failure:?}"),
        ClauseRecognition::NotClaimed => panic!("not claimed"),
    }
}

const CONTEXT: DamageReplacementContext<'static> = DamageReplacementContext {
    name_links: &[],
    name_failure: None,
    attachment_word: None,
};

#[test]
fn the_passive_reads_the_auxiliary_in_the_subject_agreement() {
    let singular = supplied_damage_frame(
        VerbAgreement::ThirdPersonSingular,
        Voice::Passive,
        ClauseModality::Indicative,
        DamageArguments::PassiveKind,
    );
    assert_eq!(
        arguments(read("is dealt combat damage", &singular).0),
        DamageClauseArguments::Kind(DamageKind::Combat)
    );
    // The plural auxiliary after a singular subject is this frame's failure.
    assert!(matches!(
        read("are dealt damage", &singular).0,
        ClauseRecognition::Failed(_)
    ));
    // The same lemma in the active voice is another frame's clause.
    assert!(matches!(
        read("deals damage", &singular),
        (ClauseRecognition::NotClaimed, 0)
    ));
    let plural = supplied_damage_frame(
        VerbAgreement::Base,
        Voice::Passive,
        ClauseModality::Indicative,
        DamageArguments::PassiveKind,
    );
    assert_eq!(
        arguments(read("are dealt noncombat damage", &plural).0),
        DamageClauseArguments::Kind(DamageKind::Noncombat)
    );
}

#[test]
fn the_damage_can_be_the_subject_of_a_modal_passive() {
    let frame = ClauseFrame {
        subject: ClauseSubjectPolicy::DamageTheme,
        modality: ClauseModality::Would,
        tense: ClauseTense::Present,
        voice: Voice::Passive,
        arguments: vec![ClauseArguments::Damage(
            DamageArguments::ReplacementRecipients(&CONTEXT),
        )],
        lemmas: DEAL,
    };
    let (ClauseRecognition::Recognized(clause), _) =
        read("combat damage would be dealt to you", &frame)
    else {
        panic!("damage-subject passive");
    };
    assert_eq!(
        clause.subject,
        ClauseSubject::DamageTheme(DamageKind::Combat)
    );
    assert_eq!(
        clause.action,
        ClauseAction::DealDamage(DamageClauseArguments::Recipients(vec![
            DamageEventRecipient::Player(DamagePlayerRecipient::You)
        ]))
    );
    // Not the damage noun: no claim, nothing consumed.
    assert!(matches!(
        read("a creature would be dealt damage", &frame),
        (ClauseRecognition::NotClaimed, 0)
    ));
}

#[test]
fn the_active_frame_serves_the_effect_and_the_replacement_policies() {
    let effect = damage_effect_clause_frame(
        SubjectVerbAgreement::Finite,
        DamageSourceAntecedent::Unavailable,
    );
    assert!(matches!(
        arguments(read("deals 3 damage to any target", &effect).0),
        DamageClauseArguments::Effect(_)
    ));
    // After a causative only the bare verb agrees.
    let causative = damage_effect_clause_frame(
        SubjectVerbAgreement::BareInfinitive,
        DamageSourceAntecedent::Unavailable,
    );
    assert!(matches!(
        read("deals 3 damage to any target", &causative).0,
        ClauseRecognition::Failed(_)
    ));
    let replacement = supplied_damage_frame(
        VerbAgreement::Base,
        Voice::Active,
        ClauseModality::Would,
        DamageArguments::ReplacementEvent(&CONTEXT),
    );
    assert_eq!(
        arguments(read("would deal 3 or more combat damage to you", &replacement).0),
        DamageClauseArguments::ReplacementEvent {
            kind: DamageKind::Combat,
            minimum: Some(3),
            maximum: None,
            recipients: vec![DamageEventRecipient::Player(DamagePlayerRecipient::You)],
        }
    );
    // The lemma without its damage noun is no damage clause.
    assert!(matches!(
        read("deals with it", &effect),
        (ClauseRecognition::NotClaimed, 0)
    ));
}

fn card(name: &str, type_line: &str, text: &str) -> crate::oracle::OracleCardParseResult {
    crate::oracle::parse_oracle_card(crate::oracle::OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn real_cards_read_every_damage_role_through_the_shared_clause() {
    for (name, type_line, text) in [
        // Passive event, then the active effect of the same lemma.
        (
            "Boros Reckoner",
            "Creature — Minotaur Wizard",
            "Whenever this creature is dealt damage, it deals that much damage to any target.",
        ),
        // Active replacement with a typed source and coordinated recipients.
        (
            "Torbran, Thane of Red Fell",
            "Legendary Creature — Dwarf Noble",
            "If a red source you control would deal damage to an opponent or a permanent an opponent controls, it deals that much damage plus 2 instead.",
        ),
        // Quantified subject, active event complement.
        (
            "Edric Probe",
            "Enchantment",
            "Whenever a creature you control deals combat damage to a player, draw a card.",
        ),
        // Player actor, passive event with a damage kind.
        (
            "Player Kind Probe",
            "Enchantment",
            "Whenever an opponent is dealt noncombat damage, you gain 1 life.",
        ),
        // Relative clauses of the damage frame in a prevention.
        (
            "Fog Probe",
            "Instant",
            "Prevent all combat damage that would be dealt this turn.",
        ),
    ] {
        assert_eq!(card(name, type_line, text).status, "canonical", "{text}");
    }
    for (type_line, text) in [
        (
            "Enchantment",
            "Whenever a creature you control are dealt damage, draw a card.",
        ),
        (
            "Creature — Horror",
            "When this creature enters, you may have it deals 2 damage to any target.",
        ),
        (
            "Enchantment",
            "If noncombat damage would deal to you, prevent that damage.",
        ),
    ] {
        assert_ne!(
            card("Near Miss", type_line, text).status,
            "canonical",
            "{text}"
        );
    }
}
