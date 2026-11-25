//! One typed antecedent environment across zones (batch 9): a copy reads a
//! card or a departed event object for its copiable values, an `unless`
//! payer is `that player`, `them` names the preceding object set, `transform
//! it` names the source through the environment, `also` is no clause
//! boundary and an attached object is a zone action's bound operand. Real
//! cards, an unused recombination and near misses; the runtime proof of the
//! departed copy lives in `engine/source_lki_contexts_tests.rs`.
use serde_json::Value;

use super::super::{
    ActionResultReference, ActionResultVerb, GrammaticalNumber, ObjectKind, ObjectPredicate,
    PermanentEffectReference, TargetBinding, TargetSelector,
};
use super::{
    ActionResultObject, BindingEnvironment, COPIED_CARD_AMBIGUOUS, COPIED_CARD_MISSING,
    COPIED_DEPARTED_UNRECORDED, COPIED_LINKED_EXILE, DeclaredTargetAntecedent,
    resolve_copied_card_antecedent,
};
use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

fn parse(type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>) {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let states = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.iter())
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect();
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    (parsed.status, rules, states)
}

/// Every lowered object of `rules` (at any depth) whose `kind` is `kind`.
fn effects_of_kind(rules: &[Value], kind: &str) -> Vec<Value> {
    fn collect(value: &Value, kind: &str, found: &mut Vec<Value>) {
        match value {
            Value::Object(map) => {
                if map.get("kind").and_then(Value::as_str) == Some(kind) {
                    found.push(value.clone());
                }
                map.values().for_each(|child| collect(child, kind, found));
            }
            Value::Array(items) => items.iter().for_each(|child| collect(child, kind, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    rules
        .iter()
        .for_each(|rule| collect(rule, kind, &mut found));
    found
}

fn canonical(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
}

fn failure_states(type_line: &str, oracle: &str) -> Vec<String> {
    let (status, _, states) = parse(type_line, oracle);
    assert_ne!(status, "canonical", "{oracle} must not compile");
    states
}

const CREATURE: &str = "Creature \u{2014} Demon";
const INSTANT: &str = "Instant";
const AURA: &str = "Enchantment \u{2014} Aura";

fn card_result(effect_index: usize, verb: Option<ActionResultVerb>) -> ActionResultObject {
    ActionResultObject {
        reference: ActionResultReference {
            effect_index,
            permanent: false,
        },
        selector: TargetSelector::Object(ObjectPredicate::ObjectKind(ObjectKind::Card)),
        number: GrammaticalNumber::Singular,
        verb,
    }
}

fn card_target(effect_index: usize) -> DeclaredTargetAntecedent {
    DeclaredTargetAntecedent {
        binding: TargetBinding {
            effect_index,
            slot_index: 0,
        },
        selector: TargetSelector::Object(ObjectPredicate::ObjectKind(ObjectKind::Card)),
    }
}

/// The copied-card resolver: the nearest of the declared card target and
/// the moved card wins; two antecedents of one action, none at all, a
/// linked exile and a sacrificed permanent keep distinct diagnostics.
#[test]
fn a_copied_card_names_the_nearest_producer_with_distinct_failures() {
    let mut environment = BindingEnvironment::default();
    assert_eq!(
        resolve_copied_card_antecedent(&environment, None),
        Err(COPIED_CARD_MISSING)
    );
    assert_eq!(
        resolve_copied_card_antecedent(&environment, Some(ActionResultVerb::Exiled)),
        Err(COPIED_LINKED_EXILE)
    );
    environment.card_target = Some(card_target(0));
    environment.result_object = Some(card_result(1, Some(ActionResultVerb::Exiled)));
    let result = PermanentEffectReference::ActionResult(ActionResultReference {
        effect_index: 1,
        permanent: false,
    });
    assert_eq!(
        resolve_copied_card_antecedent(&environment, None),
        Ok(result)
    );
    environment.card_target = Some(card_target(2));
    assert_eq!(
        resolve_copied_card_antecedent(&environment, None),
        Ok(PermanentEffectReference::Target(TargetBinding {
            effect_index: 2,
            slot_index: 0,
        }))
    );
    // `the exiled card` names only the card an exile of the program moved.
    assert_eq!(
        resolve_copied_card_antecedent(&environment, Some(ActionResultVerb::Exiled)),
        Ok(result)
    );
    environment.card_target = Some(card_target(1));
    assert_eq!(
        resolve_copied_card_antecedent(&environment, None),
        Err(COPIED_CARD_AMBIGUOUS)
    );
    environment.card_target = None;
    environment.result_object = Some(card_result(1, Some(ActionResultVerb::Sacrificed)));
    assert_eq!(
        resolve_copied_card_antecedent(&environment, None),
        Err(COPIED_DEPARTED_UNRECORDED)
    );
}

#[test]
fn a_token_copy_reads_a_departed_event_object_as_it_last_existed() {
    // Real cards: Ratadrabik of Urborg's and Genestealer Patriarch's copy
    // of a creature that died. Unused recombination: the copy of the dead
    // creature is created tapped.
    for oracle in [
        "Whenever another nontoken creature you control dies, create a token that's a copy of that creature.",
        "Whenever a creature with an infection counter on it dies, you create a token that's a copy of that creature, except it's a Tyranid in addition to its other types.",
        "Whenever another creature you control dies, create a tapped token that's a copy of that creature.",
    ] {
        let rules = canonical(CREATURE, oracle);
        let copies = effects_of_kind(&rules, "copyOf");
        assert_eq!(copies.len(), 1, "{oracle}: {rules:?}");
        assert_eq!(
            copies[0]["object"]["kind"], "triggeringPermanent",
            "{oracle}"
        );
    }
}

#[test]
fn a_token_copy_reads_a_card_the_program_declared_or_moved() {
    // Real card: Soul Separator's declared card target (first sentence of
    // its activated ability). Unused recombination: the card an exile of the
    // top card of the library moved.
    let rules = canonical(
        "Artifact",
        "{5}, {T}, Sacrifice this artifact: Exile target creature card from your graveyard. Create a token that's a copy of that card, except it's 1/1, it's a Spirit in addition to its other types, and it has flying.",
    );
    let copies = effects_of_kind(&rules, "copyOf");
    assert_eq!(copies[0]["object"]["kind"], "chosenTarget", "{rules:?}");
    let rules = canonical(
        "Artifact",
        "{4}, {T}: Exile the top card of your library. Create a token that's a copy of the exiled card.",
    );
    let copies = effects_of_kind(&rules, "copyOf");
    assert_eq!(copies.len(), 1, "{rules:?}");
    // The exile's recorded object, read through the shared action-result
    // binding (a bound object or the decision that chose it).
    assert!(
        matches!(
            copies[0]["object"]["kind"].as_str(),
            Some("singleBoundObject" | "decisionResult")
        ),
        "{rules:?}"
    );
}

#[test]
fn a_copied_card_without_a_producer_is_a_missing_antecedent() {
    // Near miss: no card of the program; Prototype Portal's linked exile.
    let states = failure_states(
        "Artifact",
        "{2}, {T}: Draw a card. Create a token that's a copy of that card.",
    );
    assert!(
        states
            .iter()
            .any(|state| state == "copy.object.card-antecedent"),
        "{states:?}"
    );
    let states = failure_states(
        "Artifact",
        "{X}, {T}: Create a token that's a copy of the exiled card.",
    );
    assert!(
        states.iter().any(|state| state == "copy.object.participle"),
        "{states:?}"
    );
}

#[test]
fn that_player_names_the_payer_of_an_unless_frame() {
    // Real card: Frightful Delusion. The payer is the countered spell's
    // controller whether or not the cost was paid (CR 118.12).
    let rules = canonical(
        INSTANT,
        "Counter target spell unless its controller pays {1}. That player discards a card.",
    );
    let payers = effects_of_kind(&rules, "tryPayCost");
    assert_eq!(payers.len(), 1, "{rules:?}");
    let serialized = serde_json::to_string(&rules).expect("rules serialize");
    assert!(
        serialized.matches("\"controllerOf\"").count() >= 2,
        "the payer and the discarding player are the same relation: {serialized}"
    );
}

#[test]
fn that_player_without_a_player_producer_is_a_missing_antecedent() {
    // Near miss: no player and no payer before `that player`.
    let states = failure_states(INSTANT, "Draw a card. That player discards a card.");
    assert!(
        states
            .iter()
            .any(|state| state == "player-subject.bound.resolve" || state == "effect.start"),
        "{states:?}"
    );
}

#[test]
fn them_names_the_preceding_object_set() {
    // Real cards: Last Night Together (declared set), Great Oak Guardian
    // (collection read again). Unused recombination: a tapped set.
    for oracle in [
        "Choose two target creatures. Untap them.",
        "Creatures target player controls get +2/+2 until end of turn. Untap them.",
        "Up to two target creatures each get +1/+1 until end of turn. Untap them.",
    ] {
        let rules = canonical(INSTANT, oracle);
        assert!(
            !effects_of_kind(&rules, "untapPermanents").is_empty(),
            "{oracle}: {rules:?}"
        );
    }
    // Near miss: no object set before `them`.
    let states = failure_states(INSTANT, "Draw a card. Untap them.");
    assert!(!states.is_empty(), "{states:?}");
}

#[test]
fn an_additive_adverb_is_read_before_the_verb() {
    // Real cards: Kami's Flare, Misthios's Fury. Unused recombination: a
    // creature subject's additional life loss.
    for oracle in [
        "Probe deals 3 damage to target creature. If you control an Equipment, Probe also deals 2 damage to that creature's controller.",
        "Probe deals 3 damage to target creature or planeswalker. Probe also deals 2 damage to that permanent's controller.",
    ] {
        let rules = canonical(INSTANT, oracle);
        assert_eq!(effects_of_kind(&rules, "dealDamage").len(), 2, "{oracle}");
    }
}

#[test]
fn an_attached_object_is_the_bound_operand_of_a_zone_action() {
    // Real cards: Dreadful Apathy (exile), Pooling Venom (destroy). Unused
    // recombination: an upkeep trigger exiling the enchanted creature.
    for (oracle, kind) in [
        (
            "Enchant creature\n{2}{W}: Exile enchanted creature.",
            "exilePermanent",
        ),
        (
            "Enchant land\n{3}{B}: Destroy enchanted land.",
            "destroyPermanent",
        ),
        (
            "Enchant creature\nAt the beginning of your upkeep, exile enchanted creature.",
            "exilePermanent",
        ),
    ] {
        let rules = canonical(AURA, oracle);
        let actions = effects_of_kind(&rules, kind);
        assert_eq!(actions.len(), 1, "{oracle}: {rules:?}");
        assert_eq!(
            actions[0]["permanent"]["kind"], "attachedPermanent",
            "{oracle}"
        );
    }
    // Near miss: an Equipment has no `enchanted` object.
    let states = failure_states(
        "Artifact \u{2014} Equipment",
        "Equip {2}\n{2}: Exile enchanted creature.",
    );
    assert!(!states.is_empty(), "{states:?}");
}
