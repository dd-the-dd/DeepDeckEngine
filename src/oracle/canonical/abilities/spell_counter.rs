use super::super::*;

pub(in crate::oracle::canonical) fn parse_global_destruction(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let mana_value_re =
        Regex::new(r"(?i)^Destroy (?:all|each) (.+?) with mana value (X|\d+) or less\.$")
            .expect("global mana-value destruction regex compiles");
    if let Some(captures) = mana_value_re.captures(text) {
        let maximum = if captures[2].eq_ignore_ascii_case("X") {
            decision_result("xValue")
        } else {
            integer(captures[2].parse::<i64>().ok()?)
        };
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "destroyPermanentsByManaValue",
                    "where": parse_permanent_criteria(&captures[1], "")?,
                    "maximum": maximum,
                }],
            }),
            &[
                "Resolve an untargeted permanent set",
                "Evaluate mana-value ceiling",
                "Destroy every matching permanent",
            ],
        ));
    }
    let where_filter = match text {
        "Destroy all creatures." => card_type("Creature"),
        _ => return None,
    };
    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [{
                "kind": "destroyPermanent",
                "permanent": {
                    "kind": "eachPermanent",
                    "where": where_filter,
                },
            }],
        }),
        &[
            "Resolve untargeted creature set",
            "Destroy every matching permanent",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_counter_unless_paid(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let re = Regex::new(
        r"^Counter target spell unless its controller pays ((?:\{[^}]+\})+)\.(?: If that spell is countered this way, exile it instead of putting it into its owner's graveyard\.)?$",
    )
    .expect("counter-unless-paid regex compiles");
    let captures = re.captures(text)?;
    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [
                    target_decision(
                        "targetSpell",
                        json!({ "kind": "spells" }),
                        1,
                        1,
                    ),
                ],
            },
            "effects": [{
                "kind": "counterStackObjectUnlessPays",
                "spell": chosen_target("targetSpell"),
                "manaCost": &captures[1],
                "exileInstead": text.contains("exile it instead"),
            }],
        }),
        &[
            "Extract required spell target",
            "Resolve optional controller payment",
            "Apply the optional destination replacement",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_impractical_joke(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let re = Regex::new(
        r"^Damage can't be prevented this turn\. [^.]+ deals (\d+) damage to up to one target creature or planeswalker\.$",
    )
    .expect("damage prevention regex compiles");
    let captures = re.captures(text)?;
    let amount = captures[1].parse::<i64>().ok()?;
    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [
                    target_decision(
                        "targetDamageable",
                        json!({
                            "kind": "permanents",
                            "where": or(vec![
                                card_type("Creature"),
                                card_type("Planeswalker"),
                            ]),
                        }),
                        0,
                        1,
                    ),
                ],
            },
            "effects": [
                {
                    "kind": "installRuleModifier",
                    "modifier": { "kind": "damageCantBePrevented" },
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                },
                {
                    "kind": "conditional",
                    "condition": {
                        "kind": "selectionNotEmpty",
                        "selection": {
                            "kind": "decisionResult",
                            "decisionId": "targetDamageable",
                        },
                    },
                    "then": [{
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "amount": integer(amount),
                        "recipient": chosen_target("targetDamageable"),
                    }],
                },
            ],
        }),
        &[
            "Install turn damage-prevention modifier",
            "Extract optional target declaration",
            "Guard damage with target selection",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_counter_spell(text: &str) -> Option<CanonicalRuleDraft> {
    let normalized;
    let text = if let Some((main, _)) = text.split_once(". (") {
        normalized = format!("{main}.");
        normalized.as_str()
    } else {
        text
    };
    let counter_rule = |criteria: Option<&str>, mut effect: Value| {
        let mut candidates = json!({ "kind": "spells" });
        if let Some(criteria) = criteria {
            candidates["where"] = if let Some(positive) = criteria.strip_prefix("non") {
                not(known_card_type_filter(positive.trim())?)
            } else {
                parse_permanent_criteria(criteria, "")?
            };
        }
        effect["kind"] = Value::String("counterSpell".to_string());
        effect["spell"] = chosen_target("targetSpell");
        Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision("targetSpell", candidates, 1, 1)],
                },
                "effects": [effect],
            }),
            &[
                "Extract the reusable spell criterion",
                "Counter the chosen spell",
                "Apply the counter result to its controller or destination",
            ],
        ))
    };
    let controller_tokens_re = Regex::new(&format!(
        r"(?i)^Counter target (.+?) spell\. Its controller creates ({}) ([A-Za-z][A-Za-z '-]+) tokens?\.$",
        count_word_pattern(),
    ))
    .expect("counter-and-controller-tokens regex compiles");
    if let Some(captures) = controller_tokens_re.captures(text) {
        return counter_rule(
            Some(&captures[1]),
            json!({
                "controllerCreatesTokens": {
                    "quantity": integer(parse_number_word(&captures[2])?),
                    "name": captures[3].trim(),
                },
            }),
        );
    }
    let counter_then_venture_re =
        Regex::new(r"(?i)^Counter target (.+?) spell\. Venture into the dungeon\.$")
            .expect("counter-then-venture regex compiles");
    if let Some(captures) = counter_then_venture_re.captures(text) {
        let mut parsed = counter_rule(Some(&captures[1]), json!({}))?;
        parsed.rule["effects"]
            .as_array_mut()?
            .push(json!({ "kind": "ventureDungeon", "player": controller() }));
        return Some(parsed);
    }
    if text == "Counter target spell. Its controller can't cast spells this turn." {
        return counter_rule(None, json!({ "prohibitControllerSpellsThisTurn": true }));
    }
    if text
        == "Counter target spell. At the beginning of your next main phase, add an amount of {C} equal to that spell's mana value."
    {
        return counter_rule(None, json!({ "addManaAtNextMainPhase": "C" }));
    }
    if text
        == "Counter target spell. At the beginning of your next first main phase, add X mana in any combination of colors, where X is that spell's mana value."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("targetSpell", json!({ "kind": "spells" }), 1, 1),
                    ],
                },
                "effects": [{
                    "kind": "resolveSpellInstruction",
                    "operation": "plasmCapture",
                }],
            }),
            &[
                "Declare the spell target",
                "Counter it while preserving its mana value",
                "Install colored mana for the controller's next first main phase",
            ],
        ));
    }
    let counter_exile_re = Regex::new(
        r"(?i)^Counter target (.+?) spell\. If that spell is countered this way, exile it instead of putting it into its owner's graveyard\.$",
    )
    .expect("counter-with-exile-replacement regex compiles");
    if let Some(captures) = counter_exile_re.captures(text) {
        return counter_rule(Some(&captures[1]), json!({ "exileInstead": true }));
    }
    let post_spell_qualified_re = Regex::new(r"(?i)^Counter target (.+?) spell with (.+?)\.$")
        .expect("post-spell qualified counterspell target regex compiles");
    if let Some(captures) = post_spell_qualified_re.captures(text) {
        let criteria = format!("{} with {}", &captures[1], &captures[2]);
        return counter_rule(Some(&criteria), json!({}));
    }
    let qualified_spell_re = Regex::new(r"(?i)^Counter target (.+? with .+?) spell\.$")
        .expect("qualified counterspell target regex compiles");
    if let Some(captures) = qualified_spell_re.captures(text) {
        return counter_rule(Some(captures.get(1)?.as_str()), json!({}));
    }
    let type_re = Regex::new(r"^Counter target (non)?(creature) spell\.$")
        .expect("counter spell-type regex compiles");
    if text == "Counter target spell." || type_re.is_match(text) {
        let where_filter = type_re.captures(text).map(|captures| {
            let filter = card_type(&captures[2]);
            if captures.get(1).is_some() {
                not(filter)
            } else {
                filter
            }
        });
        let mut candidates = json!({ "kind": "spells" });
        if let Some(where_filter) = where_filter {
            candidates["where"] = where_filter;
        }
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("targetSpell", candidates, 1, 1),
                    ],
                },
                "effects": [{
                    "kind": "counterSpell",
                    "spell": chosen_target("targetSpell"),
                }],
            }),
            &[
                "Extract required spell target",
                "Reduce spell-type predicate",
                "Resolve counter vocabulary",
            ],
        ));
    }

    let mana_re = Regex::new(r"^Counter target spell with mana value (\d+)( or greater)?\.$")
        .expect("counter mana value regex compiles");
    if let Some(captures) = mana_re.captures(text) {
        let value = captures[1].parse::<i64>().ok()?;
        let operator = if captures.get(2).is_some() {
            ">="
        } else {
            "=="
        };
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetSpell",
                            json!({
                                "kind": "spells",
                                "where": compare(
                                    operator,
                                    json!({
                                        "kind": "manaValueOf",
                                        "object": { "kind": "candidate" },
                                    }),
                                    integer(value),
                                ),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "counterSpell",
                    "spell": chosen_target("targetSpell"),
                }],
            }),
            &[
                "Extract required spell target",
                "Reduce mana-value predicate",
                "Resolve counter vocabulary",
            ],
        ));
    }

    let color_re = Regex::new(
        r"^Counter target (red|white|blue|black|green) or (red|white|blue|black|green) spell\.$",
    )
    .expect("counter color regex compiles");
    if let Some(captures) = color_re.captures(text) {
        let capitalize = |value: &str| {
            let mut chars = value.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        };
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetSpell",
                            json!({
                                "kind": "spells",
                                "where": or(vec![
                                    json!({
                                        "kind": "colorContains",
                                        "value": capitalize(&captures[1]),
                                    }),
                                    json!({
                                        "kind": "colorContains",
                                        "value": capitalize(&captures[2]),
                                    }),
                                ]),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "counterSpell",
                    "spell": chosen_target("targetSpell"),
                }],
            }),
            &[
                "Extract required spell target",
                "Reduce color alternatives",
                "Resolve counter vocabulary",
            ],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_spell_cant_be_countered(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text != "This spell can't be countered." {
        return None;
    }
    Some(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": {
                "kind": "inZone",
                "object": self_ref(),
                "zone": { "kind": "stack" },
            },
            "modifiers": [{
                "kind": "cantBeCountered",
                "object": self_ref(),
            }],
        }),
        &[
            "Resolve this spell as self",
            "Restrict static rule to stack",
            "Install counter prohibition",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_target_player_draw(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let re = Regex::new(&format!(
        r"^Target player draws ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("target player draw regex compiles");
    let captures = re.captures(text)?;
    let count = parse_number_word(&captures[1])?;
    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [
                    target_decision(
                        "targetPlayer",
                        json!({ "kind": "players" }),
                        1,
                        1,
                    ),
                ],
            },
            "effects": [{
                "kind": "drawCards",
                "player": chosen_target("targetPlayer"),
                "count": count,
            }],
        }),
        &["Extract required player target", "Resolve draw vocabulary"],
    ))
}
