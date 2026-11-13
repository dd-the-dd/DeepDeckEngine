use super::super::*;

pub(in crate::oracle::canonical) fn lower_object_controller_scope(
    scope: ObjectControllerScope,
) -> Value {
    match scope {
        ObjectControllerScope::AbilityController => controller(),
        ObjectControllerScope::NotAbilityController => json!({
            "kind": "playersExcept",
            "player": controller(),
        }),
        ObjectControllerScope::OpponentOfAbilityController => json!({
            "kind": "opponentsOf",
            "player": controller(),
        }),
    }
}

/// Lowers one target-selector branch into the engine's generic candidate-set
/// vocabulary. Cross-domain unions remain one target slot whose candidates
/// are the union of independently lowered sets.
pub(in crate::oracle::canonical) fn lower_target_selector_candidates(
    selector: &TargetSelector,
) -> Option<(&'static str, Value)> {
    Some(match selector {
        TargetSelector::Any => ("targetDamageable", json!({ "kind": "anyTarget" })),
        TargetSelector::Union(branches) => {
            if branches.is_empty() {
                return None;
            }
            let sets = branches
                .iter()
                .map(|branch| lower_target_selector_candidates(branch).map(|(_, set)| set))
                .collect::<Option<Vec<_>>>()?;
            ("targetDamageable", json!({ "kind": "union", "sets": sets }))
        }
        TargetSelector::Player(PlayerPredicate::Any) => {
            ("targetPlayer", json!({ "kind": "players" }))
        }
        TargetSelector::Player(PlayerPredicate::Opponent) => (
            "targetOpponent",
            json!({
                "kind": "players",
                "where": { "kind": "isOpponentOf", "player": controller() },
            }),
        ),
        TargetSelector::Object(predicate) => (
            "targetPermanent",
            json!({
                "kind": "permanents",
                "where": lower_permanent_object_predicate(predicate),
            }),
        ),
        TargetSelector::ControlledObject {
            predicate,
            controller: controller_scope,
        } => (
            "targetPermanent",
            json!({
                "kind": "permanents",
                "controller": lower_object_controller_scope(*controller_scope),
                "where": lower_permanent_object_predicate(predicate),
            }),
        ),
        TargetSelector::StackItem(_) => return None,
    })
}

/// Lowers the complete target slot, including restrictions carried by its
/// determiner. Parents must not drop `another` when delegating the predicate.
pub(in crate::oracle::canonical) fn lower_target_slot_candidates(
    slot: &TargetSlot,
) -> Option<(&'static str, Value)> {
    let (id, mut candidates) = lower_target_selector_candidates(&slot.selector)?;
    if slot.exclude_source {
        candidates["excludeSource"] = Value::Bool(true);
    }
    Some((id, candidates))
}

pub(in crate::oracle::canonical) fn target_decision(
    id: &str,
    candidates: Value,
    minimum: i64,
    maximum: i64,
) -> Value {
    json!({
        "id": id,
        "kind": "chooseTargets",
        "minimum": minimum,
        "maximum": maximum,
        "candidates": candidates,
    })
}

pub(in crate::oracle::canonical) fn sole_target_decision_id(decisions: &[Value]) -> Option<&str> {
    (decisions.len() == 1 && decisions[0]["kind"].as_str() == Some("chooseTargets"))
        .then(|| decisions[0]["id"].as_str())
        .flatten()
}

pub(in crate::oracle::canonical) fn chosen_target(id: &str) -> Value {
    json!({ "kind": "chosenTarget", "id": id })
}

pub(in crate::oracle::canonical) fn decision_result(id: &str) -> Value {
    json!({ "kind": "decisionResult", "decisionId": id })
}

pub(in crate::oracle::canonical) fn stable_rule_id(prefix: &str, text: &str) -> String {
    let hash = text.bytes().fold(0xCBF2_9CE4_8422_2325_u64, |value, byte| {
        (value ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
    });
    format!("{prefix}:{hash:016x}")
}

pub(in crate::oracle::canonical) fn bound_objects(binding: &str) -> Value {
    json!({ "kind": "boundObjects", "binding": binding })
}

pub(in crate::oracle::canonical) fn library(player: Value) -> Value {
    json!({ "kind": "library", "player": player })
}

pub(in crate::oracle::canonical) fn graveyard(player: Value) -> Value {
    json!({ "kind": "graveyard", "player": player })
}

pub(in crate::oracle::canonical) fn search_library_effects(
    filter: Value,
    maximum: i64,
    destination: &str,
    tapped: bool,
) -> Vec<Value> {
    search_library_effects_for(controller(), filter, maximum, destination, tapped)
}

pub(in crate::oracle::canonical) fn search_library_effects_for(
    player: Value,
    filter: Value,
    maximum: i64,
    destination: &str,
    tapped: bool,
) -> Vec<Value> {
    search_library_effects_for_binding(
        player,
        filter,
        maximum,
        destination,
        tapped,
        CollectionBinding(0),
    )
}

pub(in crate::oracle::canonical) fn lower_collection_binding(binding: CollectionBinding) -> String {
    match binding.0 {
        0 => "searchedCards".to_string(),
        index => format!("searchedCards:{index}"),
    }
}

pub(in crate::oracle::canonical) fn search_library_effects_for_binding(
    player: Value,
    filter: Value,
    maximum: i64,
    destination: &str,
    tapped: bool,
    binding: CollectionBinding,
) -> Vec<Value> {
    let searched = lower_collection_binding(binding);
    vec![
        json!({
            "kind": "chooseCards",
            "id": searched.clone(),
            "player": player.clone(),
            "minimum": 0,
            "maximum": maximum,
            "candidates": {
                "kind": "cards",
                "zone": library(player.clone()),
                "where": filter,
            },
        }),
        json!({
            "kind": "moveCards",
            "cards": decision_result(&searched),
            "to": {
                "kind": destination,
                "player": player.clone(),
                "tapped": tapped,
            },
        }),
        json!({
            "kind": "shuffleZone",
            "zone": library(player.clone()),
        }),
        json!({
            "kind": "recordLibrarySearch",
            "player": player,
            "cards": decision_result(&searched),
        }),
    ]
}

pub(in crate::oracle::canonical) fn search_library_then_put_on_top_effects(
    filter: Value,
    reveal: bool,
) -> Vec<Value> {
    let mut effects = vec![json!({
        "kind": "chooseCards",
        "id": "searchedCards",
        "player": controller(),
        "minimum": 0,
        "maximum": 1,
        "candidates": {
            "kind": "cards",
            "zone": library(controller()),
            "where": filter,
        },
    })];
    if reveal {
        effects.push(json!({
            "kind": "revealCards",
            "cards": decision_result("searchedCards"),
        }));
    }
    effects.extend([
        json!({
            "kind": "shuffleZone",
            "zone": library(controller()),
        }),
        json!({
            "kind": "moveCards",
            "cards": decision_result("searchedCards"),
            "to": {
                "kind": "library",
                "player": controller(),
                "position": "top",
            },
        }),
        json!({
            "kind": "recordLibrarySearch",
            "player": controller(),
            "cards": decision_result("searchedCards"),
        }),
    ]);
    effects
}

pub(in crate::oracle::canonical) fn split_library_search_between_battlefield_and_hand_effects(
    instruction: &str,
    face_name: &str,
) -> Option<Vec<Value>> {
    let split_search_re = Regex::new(&format!(
        r"(?i)^Search your library for up to ({}) (.+?) cards, reveal (?:those cards|them), put one onto the battlefield( tapped)? and the other into your hand, then shuffle\.$",
        count_word_pattern(),
    ))
    .expect("split library-search instruction regex compiles");
    let captures = split_search_re.captures(instruction)?;
    let maximum = parse_number_word(captures.get(1)?.as_str())?;
    if maximum != 2 {
        return None;
    }
    let filter = entry_search_filter(&format!("{} card", captures.get(2)?.as_str()))
        .or_else(|| parse_permanent_criteria(captures.get(2)?.as_str(), face_name))?;

    Some(vec![
        json!({
            "kind": "chooseCards",
            "id": "searchedCards",
            "player": controller(),
            "minimum": 0,
            "maximum": maximum,
            "candidates": {
                "kind": "cards",
                "zone": library(controller()),
                "where": filter,
            },
        }),
        json!({ "kind": "revealCards", "cards": decision_result("searchedCards") }),
        json!({
            "kind": "chooseCards",
            "id": "battlefieldCard",
            "player": controller(),
            "from": decision_result("searchedCards"),
            "count": integer(1),
        }),
        json!({
            "kind": "moveCards",
            "cards": decision_result("battlefieldCard"),
            "to": {
                "kind": "battlefield",
                "player": controller(),
                "tapped": captures.get(3).is_some(),
            },
        }),
        json!({
            "kind": "moveCards",
            "cards": {
                "kind": "setDifference",
                "left": decision_result("searchedCards"),
                "right": decision_result("battlefieldCard"),
            },
            "to": hand(controller()),
        }),
        json!({ "kind": "shuffleZone", "zone": library(controller()) }),
        json!({
            "kind": "recordLibrarySearch",
            "player": controller(),
            "cards": decision_result("searchedCards"),
        }),
    ])
}

pub(in crate::oracle::canonical) fn airbend_target_decision(text: &str) -> Option<Value> {
    let lower = text.to_ascii_lowercase();
    let minimum = if lower.contains("up to one") { 0 } else { 1 };
    let controlled = lower.contains("you control");
    let mut permanent_candidates = if lower.contains("nonland permanent") {
        json!({
            "kind": "permanents",
            "where": not(card_type("Land")),
        })
    } else {
        json!({
            "kind": "permanents",
            "where": card_type("Creature"),
        })
    };
    if controlled {
        permanent_candidates["controller"] = controller();
    }
    if lower.contains("another target") || lower.contains("other target") {
        permanent_candidates["excludeSource"] = Value::Bool(true);
    }
    let candidates = if lower.contains("creature or spell") {
        json!({
            "kind": "union",
            "sets": [permanent_candidates, { "kind": "spells" }],
        })
    } else {
        permanent_candidates
    };
    Some(target_decision("airbendTarget", candidates, minimum, 1))
}

#[cfg(test)]
#[path = "target_slot_lowering_tests.rs"]
mod target_slot_lowering_tests;
