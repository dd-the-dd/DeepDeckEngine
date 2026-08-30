use super::super::*;

pub(in crate::oracle::canonical) fn parse_avatar_activated_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text.contains("Put a +1/+1 counter on Jeong Jeong")
        && text.contains("When you next cast a Lesson spell this turn")
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payMana", "manaCost": "{3}" }],
                "activationLimit": { "kind": "oncePerGameObject", "id": "exhaust" },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "jeongJeongLessonCopy",
                }],
            }),
            &[
                "Pay the exhaust cost",
                "Add the counter",
                "Copy the next Lesson cast this turn",
            ],
        ));
    }
    if text.contains("Sacrifice a Desert: Exile target creature card with mana value X")
        && text.contains("except it's a 4/4 black Zombie")
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{X}{2}" },
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificedDesert"),
                    },
                ],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "xValue",
                        "kind": "chooseNumber",
                        "minimum": 0,
                    }, target_decision(
                        "sacrificedDesert",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": subtype("Desert"),
                        }),
                        1,
                        1,
                    )],
                },
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "lazotepQuarryZombie",
                }],
            }),
            &[
                "Declare X",
                "Pay mana, tap, and sacrifice a Desert",
                "Create the modified Zombie copy",
            ],
        ));
    }
    let (exhaust, normalized) = if let Some(rest) = text.strip_prefix("Exhaust — ") {
        (true, rest)
    } else if let Some(rest) = text.strip_prefix("Exhaust â€” ") {
        (true, rest)
    } else {
        (false, text)
    };
    let (cost_text, instruction) = normalized.split_once(':')?;
    let waterbend_cost_re =
        Regex::new(r"^Waterbend \{(\d+|X)\}$").expect("activated waterbend cost regex compiles");
    let (costs, cost_decisions) = if let Some(captures) = waterbend_cost_re.captures(cost_text) {
        let amount = if &captures[1] == "X" {
            decision_result("xValue")
        } else {
            integer(captures[1].parse::<i64>().ok()?)
        };
        (
            vec![json!({
                "kind": "payWaterbend",
                "amount": amount,
            })],
            Vec::new(),
        )
    } else {
        parse_activation_costs(cost_text)?
    };
    let earthbend_re = Regex::new(r"^Earthbend (\d+)(?:\.|\. Activate only .+)$")
        .expect("activated earthbend regex compiles");
    if let Some(captures) = earthbend_re.captures(instruction.trim()) {
        let mut decisions = cost_decisions;
        decisions.push(target_decision(
            "earthbendLand",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Land"),
            }),
            1,
            1,
        ));
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
            },
            "activationCondition": { "kind": "sorceryTiming" },
            "effects": [earthbend_effect(
                "earthbendLand",
                integer(captures[1].parse::<i64>().ok()?),
            )],
        });
        if exhaust {
            rule["activationLimit"] = json!({
                "kind": "oncePerGameObject",
                "id": "exhaust",
            });
        }
        return Some(draft(
            rule,
            &[
                "Partition activation costs",
                "Declare controlled land target",
                "Apply sorcery timing and exhaust limit",
                "Resolve earthbend",
            ],
        ));
    }

    let grant_firebending_re =
        Regex::new(r"^Target creature you control gains firebending (\d+) until end of turn\.")
            .expect("grant firebending regex compiles");
    if let Some(captures) = grant_firebending_re.captures(instruction.trim()) {
        let mut decisions = cost_decisions;
        decisions.push(target_decision(
            "firebendingTarget",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                },
                "effects": [{
                    "kind": "grantFirebending",
                    "object": chosen_target("firebendingTarget"),
                    "quantity": integer(captures[1].parse::<i64>().ok()?),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Partition activation costs",
                "Declare controlled creature target",
                "Grant temporary firebending",
            ],
        ));
    }
    let simple_effects = if instruction.trim().starts_with("Draw a card.") {
        Some(vec![json!({
            "kind": "drawCards",
            "player": controller(),
            "count": integer(1),
        })])
    } else if instruction.trim() == "Transform Aang." {
        Some(vec![json!({
            "kind": "transformPermanent",
            "object": { "kind": "abilitySource" },
        })])
    } else if instruction
        .trim()
        .starts_with("Sacrifice this enchantment. If you do, scry 2.")
    {
        Some(vec![
            json!({ "kind": "sacrificePermanent", "permanent": { "kind": "abilitySource" } }),
            json!({ "kind": "scry", "player": controller(), "count": integer(2) }),
        ])
    } else {
        None
    };
    if let Some(effects) = simple_effects {
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "effects": effects,
        });
        if !cost_decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": cost_decisions,
            });
        }
        if exhaust {
            rule["activationLimit"] = json!({
                "kind": "oncePerGameObject",
                "id": "exhaust",
            });
        }
        return Some(draft(
            rule,
            &[
                "Resolve activated waterbend payment",
                "Apply the simple activated instruction",
            ],
        ));
    }
    if instruction
        .trim()
        .starts_with("Creatures you control have base power and toughness X/X until end of turn.")
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "waterbendSetBaseX",
                }],
            }),
            &[
                "Choose a positive waterbend X value",
                "Tap artifacts and creatures before spending mana",
                "Set controlled creature base stats through end of turn",
            ],
        ));
    }
    if instruction
        .trim()
        .starts_with("Target creature can't be blocked this turn.")
    {
        let mut decisions = cost_decisions;
        decisions.push(target_decision(
            "targetCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        ));
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                },
                "effects": [{
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "cantBeBlocked",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Resolve activated waterbend payment",
                "Declare the creature target",
                "Prevent blocking this turn",
            ],
        ));
    }
    None
}
