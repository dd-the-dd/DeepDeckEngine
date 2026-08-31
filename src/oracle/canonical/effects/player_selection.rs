use super::super::*;

struct PlayerSubject {
    players: Value,
    decisions: Vec<Value>,
    second_person: bool,
}

fn player_subject(instruction: &str) -> Option<(PlayerSubject, &str)> {
    let candidates = [
        (
            "A player chosen at random",
            json!({ "kind": "randomPlayer" }),
            None,
            false,
        ),
        (
            "An opponent chosen at random",
            json!({ "kind": "randomOpponentOf", "player": controller() }),
            None,
            false,
        ),
        (
            "Each opponent",
            json!({ "kind": "opponentsOf", "player": controller() }),
            None,
            false,
        ),
        ("Each player", json!({ "kind": "eachPlayer" }), None, false),
        (
            "Target opponent",
            chosen_target("choosingPlayer"),
            Some(json!({
                "kind": "players",
                "where": { "kind": "isOpponentOf", "player": controller() },
            })),
            false,
        ),
        (
            "Target player",
            chosen_target("choosingPlayer"),
            Some(json!({ "kind": "players" })),
            false,
        ),
        ("You", controller(), None, true),
    ];

    candidates
        .into_iter()
        .find_map(|(prefix, players, target_candidates, second_person)| {
            let rest = strip_prefix_ascii_case(instruction, prefix)?;
            if !rest.chars().next().is_some_and(char::is_whitespace) {
                return None;
            }
            let decisions = target_candidates
                .map(|candidates| target_decision("choosingPlayer", candidates, 1, 1))
                .into_iter()
                .collect();
            Some((
                PlayerSubject {
                    players,
                    decisions,
                    second_person,
                },
                rest.trim_start(),
            ))
        })
}

fn pool_criteria(description: &str, face_name: &str) -> Option<Value> {
    parse_permanent_criteria(description, face_name).or_else(|| {
        description
            .trim()
            .strip_suffix(" permanents")
            .or_else(|| description.trim().strip_suffix(" permanent"))
            .and_then(|criteria| parse_permanent_criteria(criteria, face_name))
    })
}

fn controlled_pool(description: &str, face_name: &str) -> Option<(Value, Value)> {
    let description =
        strip_prefix_ascii_case(description.trim(), "the ").unwrap_or(description.trim());
    for suffix in [" they control", " that player controls"] {
        if let Some(criteria) = strip_suffix_ascii_case(description, suffix) {
            return Some((
                json!({ "kind": "currentPlayer" }),
                pool_criteria(criteria, face_name)?,
            ));
        }
    }
    let criteria = strip_suffix_ascii_case(description, " you control")?;
    Some((controller(), pool_criteria(criteria, face_name)?))
}

fn parse_selection_item(item: &str, face_name: &str) -> Option<Value> {
    let item = item.trim().trim_start_matches("and ").trim();
    if let Some(quantity) = parse_numeric_expression_text(item) {
        return Some(json!({
            "quantity": quantity,
            "where": Value::Null,
        }));
    }
    let (quantity, description) = parse_quantity_prefix(item)?;
    Some(json!({
        "quantity": quantity,
        "where": parse_permanent_criteria(description, face_name)?,
    }))
}

fn split_selection_items<'a>(description: &'a str, face_name: &str) -> Option<Vec<&'a str>> {
    let comma_parts = description.split(',').map(str::trim).collect::<Vec<_>>();
    if comma_parts.len() > 1 {
        let parts = comma_parts
            .into_iter()
            .map(|part| part.strip_prefix("and ").unwrap_or(part).trim())
            .collect::<Vec<_>>();
        if parts
            .iter()
            .all(|part| parse_selection_item(part, face_name).is_some())
        {
            return Some(parts);
        }
    }

    if parse_selection_item(description, face_name).is_some() {
        return Some(vec![description.trim()]);
    }
    let lower = description.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(relative) = lower[offset..].find(" and ") {
        let index = offset + relative;
        let left = description[..index].trim();
        let right = description[index + " and ".len()..].trim();
        if parse_selection_item(left, face_name).is_some()
            && parse_selection_item(right, face_name).is_some()
        {
            return Some(vec![left, right]);
        }
        offset = index + " and ".len();
    }
    None
}

pub(in crate::oracle::canonical) fn parse_choose_permanents_then_sacrifice_rest(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let instruction = instruction.trim();
    let instruction = strip_suffix_ascii_case(instruction, ".").unwrap_or(instruction);
    let (subject, selection_and_remainder) = player_subject(instruction)?;
    let selection_and_remainder = strip_prefix_ascii_case(
        selection_and_remainder,
        if subject.second_person {
            "choose "
        } else {
            "chooses "
        },
    )?;
    let (selection, remainder) = split_once_ascii_case(selection_and_remainder, ", then ")?;
    if !matches!(
        remainder.to_ascii_lowercase().as_str(),
        "sacrifices the rest" | "sacrifice the rest" | "that player sacrifices the rest"
    ) {
        return None;
    }

    let (selection, random) = strip_prefix_ascii_case(selection.trim(), "at random ")
        .map(|selection| (selection, true))
        .or_else(|| {
            strip_suffix_ascii_case(selection.trim(), " at random")
                .map(|selection| (selection, true))
        })
        .unwrap_or((selection.trim(), false));
    let (selection, pool) = split_once_ascii_case(selection, " from among ")?;
    let (candidate_controller, among) = controlled_pool(pool, face_name)?;
    let selections = split_selection_items(selection, face_name)?
        .into_iter()
        .map(|item| parse_selection_item(item, face_name))
        .collect::<Option<Vec<_>>>()?;
    if selections.is_empty() {
        return None;
    }

    Some((
        vec![json!({
            "kind": "forEachPlayer",
            "players": subject.players,
            "effects": [
                {
                    "kind": "selectPermanents",
                    "id": "keptPermanents",
                    "player": { "kind": "currentPlayer" },
                    "candidates": {
                        "kind": "permanents",
                        "controller": candidate_controller.clone(),
                        "where": among.clone(),
                    },
                    "groups": selections,
                    "selection": if random {
                        json!({ "kind": "random" })
                    } else {
                        json!({ "kind": "choice" })
                    },
                },
                {
                    "kind": "sacrificePermanents",
                    "objects": {
                        "kind": "selectionRemainder",
                        "selectionId": "keptPermanents",
                        "candidates": {
                            "kind": "permanents",
                            "controller": candidate_controller,
                            "where": among,
                        },
                    },
                },
            ],
        })],
        subject.decisions,
    ))
}
