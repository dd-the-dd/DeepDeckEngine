use super::{requires_checkpoint, stack_requires_checkpoint};
use crate::engine::*;
use serde_json::json;

fn object(physical: bool) -> StackObject {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"preflight-source", "name":"Preflight source", "typeLine":"Instant",
        "manaCost":"{0}", "rules":[]
    }))
    .unwrap();
    StackObject {
        id: "stack-preflight".into(),
        controller: "p1".into(),
        card: card_instance("p1", &definition, 1),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: (!physical).then(|| "triggeredAbility".into()),
        ability_rule: (!physical).then(|| json!({"kind":"triggeredAbility", "effects":[]})),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

#[test]
fn physical_spell_exit_is_checkpointed_before_pop_even_without_effects() {
    assert!(stack_requires_checkpoint(&[object(true)]));
}

#[test]
fn underlying_physical_spell_does_not_force_snapshot_for_literal_ability() {
    assert!(!stack_requires_checkpoint(&[object(true), object(false)]));
    assert!(!stack_requires_checkpoint(&[]));
}

#[test]
fn spell_copy_has_no_physical_destination_but_its_program_remains_checked() {
    let mut copy = object(true);
    copy.card.flags.insert("isSpellCopy".into(), true);
    assert!(!stack_requires_checkpoint(&[copy.clone()]));
    copy.card
        .definition
        .rules
        .push(json!({"kind":"spellAbility", "effects":[
            {"kind":"drawCards", "count":1}
        ]}));
    assert!(stack_requires_checkpoint(&[copy]));
}

#[test]
fn pending_declaration_under_literal_top_keeps_rewritten_top_guard() {
    let mut pending = object(false);
    pending.ability_rule.as_mut().unwrap()["declaration"] = json!({
        "decisions":[{"kind":"chooseTargets", "id":"target"}]
    });
    assert!(stack_requires_checkpoint(&[pending, object(false)]));
}

#[test]
fn zone_writer_hooks_share_recursive_walk_without_interpreting_names() {
    for kind in [
        "drawCards",
        "mill",
        "counterStackObject",
        "returnCardsFromGraveyard",
        "putCardsFromHandOntoBattlefield",
        "blinkPermanents",
    ] {
        assert!(
            requires_checkpoint(&json!({"effects":[{"kind":kind}]})),
            "{kind}"
        );
    }
    assert!(!requires_checkpoint(
        &json!({"kind":"gainLife", "name":"blinkPermanents", "player":{"kind":"abilityController"}, "amount":{"kind":"integer","value":2}})
    ));
}
