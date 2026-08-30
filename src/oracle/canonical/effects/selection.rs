use super::super::*;

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
    vec![
        json!({
            "kind": "chooseCards",
            "id": "searchedCards",
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
            "cards": decision_result("searchedCards"),
            "to": {
                "kind": destination,
                "player": player.clone(),
                "tapped": tapped,
            },
        }),
        json!({
            "kind": "shuffleZone",
            "zone": library(player),
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
