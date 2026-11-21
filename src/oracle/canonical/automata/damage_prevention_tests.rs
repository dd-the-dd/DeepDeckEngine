use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};
use serde_json::{Value, json};

use crate::oracle::canonical::dispatch::{CanonicalParseOutcome, parse_canonical_rule};

fn parse_as(text: &str, type_line: &str, kind: &str) -> Result<Value, String> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Generic Shield",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule(&input, kind) {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft.rule),
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => {
            Err(format!("{}: {}", failure.state, failure.expected))
        }
        CanonicalParseOutcome::Unsupported => Err("unsupported".to_string()),
    }
}

fn spell(text: &str) -> Result<Value, String> {
    parse_as(text, "Instant", "spellAbility")
}

fn shield(rule: &Value, index: usize) -> &Value {
    let effect = &rule["effects"][index];
    assert_eq!(effect["kind"], "preventDamage", "{effect}");
    assert_eq!(effect["duration"], json!({"kind": "untilEndOfCurrentTurn"}));
    effect
}

/// A real card shape: all combat damage, no recipient and no source.
#[test]
fn all_combat_damage_has_no_recipient_or_source() {
    let rule = spell("Prevent all combat damage that would be dealt this turn.").expect("parses");
    let effect = shield(&rule, 0);
    assert_eq!(effect["amount"], json!({"kind": "all"}));
    assert_eq!(effect["combatOnly"], json!(true));
    assert!(effect.get("recipient").is_none() && effect.get("source").is_none());
}

/// A real card shape: a finite amount over any target, declared as one target.
#[test]
fn next_n_damage_to_any_target_declares_one_target() {
    let rule = spell("Prevent the next 3 damage that would be dealt to any target this turn.")
        .expect("parses");
    let effect = shield(&rule, 0);
    assert_eq!(effect["amount"], json!({"kind": "integer", "value": 3}));
    assert_eq!(effect["combatOnly"], json!(false));
    let decisions = rule["declaration"]["decisions"]
        .as_array()
        .expect("decisions");
    assert_eq!(decisions.len(), 1, "{decisions:?}");
    assert_eq!(decisions[0]["kind"], "chooseTargets");
    assert_eq!(effect["recipient"]["id"], decisions[0]["id"]);
}

/// The same action in an activated ability, with the recipient slot filled by
/// a different shared child (the controller player reference).
#[test]
fn activated_shield_over_the_controller() {
    let rule = parse_as(
        "{T}: Prevent the next 1 combat damage that would be dealt to you this turn.",
        "Creature — Cleric",
        "activatedAbility",
    )
    .expect("parses");
    let effect = shield(&rule, 0);
    assert_eq!(effect["amount"], json!({"kind": "integer", "value": 1}));
    assert_eq!(effect["combatOnly"], json!(true));
    assert_eq!(
        effect["recipient"],
        json!({"kind": "controllerOf", "object": {"kind": "self"}})
    );
    assert_eq!(rule["costs"][0]["kind"], "tap");
}

/// Previously unused combination: source-first frame, a targeted creature as
/// the source, combat restriction and an `all` amount.
#[test]
fn targeted_source_in_the_would_deal_frame() {
    let rule =
        spell("Prevent all combat damage target creature would deal this turn.").expect("parses");
    let effect = shield(&rule, 0);
    assert_eq!(effect["amount"], json!({"kind": "all"}));
    assert_eq!(effect["combatOnly"], json!(true));
    assert_eq!(effect["source"]["kind"], "chosenTarget");
    assert!(effect.get("recipient").is_none());
}

/// Previously unused combination: a recipient and a distinct source in the
/// passive frame, the duration between them.
#[test]
fn passive_frame_with_recipient_then_duration_then_source() {
    let rule = spell("Prevent all damage that would be dealt to you this turn by target creature.")
        .expect("parses");
    let effect = shield(&rule, 0);
    assert_eq!(
        effect["recipient"],
        json!({"kind": "controllerOf", "object": {"kind": "self"}})
    );
    assert_eq!(effect["source"]["kind"], "chosenTarget");
}

#[test]
fn shield_composes_with_the_following_action() {
    let rule = spell(
        "Prevent the next 2 damage that would be dealt to target creature this turn. Draw a card.",
    )
    .expect("parses");
    shield(&rule, 0);
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
}

/// `where X is` binds the shared numeric child like any other action.
#[test]
fn where_x_binds_the_prevented_amount() {
    let rule = spell(
        "Prevent the next X damage that would be dealt to target creature this turn, where X is the number of creatures you control.",
    );
    assert!(rule.is_ok(), "{rule:?}");
}

/// Near misses fail explicitly instead of dropping the unsupported part.
#[test]
fn near_misses_fail_at_a_precise_state() {
    for (text, state) in [
        // A collection source is a continuous scope, not one object.
        (
            "Prevent all damage that would be dealt to you this turn by attacking creatures without flying.",
            "reference.source.start",
        ),
        // A collection recipient.
        (
            "Prevent all damage that would be dealt to creatures this turn.",
            "effect.prevent.party.reference",
        ),
        // The engine shield lasts for one turn only.
        (
            "Prevent all damage that would be dealt to you until your next turn.",
            "effect.prevent.duration",
        ),
        // A duration is required.
        (
            "Prevent all damage that would be dealt to you.",
            "effect.prevent.duration",
        ),
        // Two target declarations would need a distinctness relation.
        (
            "Prevent all damage that would be dealt to target creature this turn by target creature.",
            "effect.prevent.targets",
        ),
        // A divided prevention names several recipients.
        (
            "Prevent the next 3 damage that would be dealt this turn to any number of targets, divided as you choose.",
            "selector.predicate.start",
        ),
    ] {
        let failure = spell(text).expect_err(text);
        assert!(
            failure.contains(state),
            "{text}: expected state {state}, got {failure}"
        );
    }
}
