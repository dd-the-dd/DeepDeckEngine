use super::super::*;

pub(in crate::oracle::canonical) fn parse_simple_damage(text: &str) -> Option<CanonicalRuleDraft> {
    let kicked_creature_re = Regex::new(
        r"^[^.]+ deals (\d+) damage to target creature\. If this spell was kicked, it deals (\d+) damage to that creature instead\.$",
    )
    .expect("kicked creature damage regex compiles");
    if let Some(captures) = kicked_creature_re.captures(text) {
        let normal_amount = captures[1].parse::<i64>().ok()?;
        let kicked_amount = captures[2].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": {
                        "kind": "conditionalValue",
                        "condition": {
                            "kind": "wasKicked",
                            "spell": self_ref(),
                        },
                        "ifTrue": integer(kicked_amount),
                        "ifFalse": integer(normal_amount),
                    },
                    "recipient": chosen_target("targetCreature"),
                }],
            }),
            &[
                "Extract required creature target",
                "Branch damage on kicked state",
                "Resolve damage vocabulary",
            ],
        ));
    }

    let lightning_re =
        Regex::new(r"^[^.]+ deals (\d+) damage to any target and you gain (\d+) life\.$")
            .expect("lightning regex compiles");
    if let Some(captures) = lightning_re.captures(text) {
        let damage = captures[1].parse::<i64>().ok()?;
        let life = captures[2].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetDamageable",
                            json!({ "kind": "anyTarget" }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "amount": integer(damage),
                        "recipient": chosen_target("targetDamageable"),
                    },
                    {
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": integer(life),
                    },
                ],
            }),
            &[
                "Extract required any-target declaration",
                "Resolve damage vocabulary",
                "Resolve linked life gain",
            ],
        ));
    }

    let permanent_target_re =
        Regex::new(r"^[^.]+ deals (\d+) damage to target creature or planeswalker\.$")
            .expect("permanent damage target regex compiles");
    if let Some(captures) = permanent_target_re.captures(text) {
        let amount = captures[1].parse::<i64>().ok()?;
        return Some(draft(
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
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(amount),
                    "recipient": chosen_target("targetDamageable"),
                }],
            }),
            &[
                "Extract creature-or-planeswalker target",
                "Resolve damage vocabulary",
            ],
        ));
    }

    let qualified_creature_re = Regex::new(
        r"^[^.]+ deals (\d+) damage to target(?:(?: (white|blue|black|red|green))|(?: (white|blue|black|red|green) or (white|blue|black|red|green)))? creature\.$",
    )
    .expect("qualified creature damage regex compiles");
    if let Some(captures) = qualified_creature_re.captures(text) {
        let amount = captures[1].parse::<i64>().ok()?;
        let colors = [captures.get(2), captures.get(3), captures.get(4)]
            .into_iter()
            .flatten()
            .map(|capture| {
                let mut color = capture.as_str().to_string();
                color.get_mut(0..1).map(str::make_ascii_uppercase);
                json!({ "kind": "colorContains", "value": color })
            })
            .collect::<Vec<_>>();
        let filter = if colors.is_empty() {
            card_type("Creature")
        } else {
            and(vec![card_type("Creature"), or(colors)])
        };
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({ "kind": "permanents", "where": filter }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(amount),
                    "recipient": chosen_target("targetCreature"),
                }],
            }),
            &[
                "Resolve creature color qualification",
                "Deal fixed damage to the target",
            ],
        ));
    }

    let each_creature_re = Regex::new(r"^[^.]+ deals (\d+) damage to each creature\.$")
        .expect("each creature damage regex compiles");
    if let Some(captures) = each_creature_re.captures(text) {
        let amount = captures[1].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(amount),
                    "recipient": {
                        "kind": "eachPermanent",
                        "where": card_type("Creature"),
                    },
                }],
            }),
            &[
                "Resolve untargeted creature set",
                "Resolve damage vocabulary",
            ],
        ));
    }

    None
}
