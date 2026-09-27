use super::super::*;

pub(in crate::oracle::canonical) fn parse_hand_put_haste_delayed_sacrifice(
    instruction: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let criteria = parse_optional_hand_permanent_with_haste_and_delayed_sacrifice(instruction)?;
    let criteria = strip_prefix_ascii_case(criteria, "a ").unwrap_or(criteria);
    let total_stats_re =
        Regex::new(r"(?i)^creature(?: card)? with total power and toughness (\d+) or less$")
            .expect("hand creature total-stat filter regex compiles");
    let where_filter = if let Some(captures) = total_stats_re.captures(criteria) {
        and(vec![
            card_type("Creature"),
            json!({
                "kind": "totalPowerAndToughnessAtMost",
                "value": integer(captures.get(1)?.as_str().parse::<i64>().ok()?),
            }),
        ])
    } else {
        parse_permanent_criteria(criteria, "")?
    };
    let decisions = vec![target_decision(
        "handCard",
        json!({
            "kind": "cards",
            "zone": hand(controller()),
            "where": where_filter,
        }),
        0,
        1,
    )];
    let effects = vec![
        json!({
            "kind": "moveTargetCard",
            "card": chosen_target("handCard"),
            "to": "battlefield",
            "controller": controller(),
            "tapped": false,
        }),
        json!({
            "kind": "grantKeyword",
            "object": chosen_target("handCard"),
            "keyword": "haste",
            "duration": { "kind": "permanent" },
        }),
        json!({
            "kind": "installDelayedStepTrigger",
            "controller": controller(),
            "step": "endStep",
            "trackedObject": chosen_target("handCard"),
            "effects": [{
                "kind": "sacrificePermanent",
                "permanent": { "kind": "triggeringPermanent" },
            }],
        }),
    ];
    Some((effects, decisions))
}

pub(in crate::oracle::canonical) fn parse_simple_activated_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    parse_simple_activated_ability_for_face(text, "")
}

pub(in crate::oracle::canonical) fn parse_simple_activated_ability_for_face(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let text = if text.starts_with("Sokratic Dialogue") {
        text.find('{').map(|index| &text[index..]).unwrap_or(text)
    } else {
        text
    };
    let (exhaust, text) = if let Some(rest) = text
        .strip_prefix("Exhaust — ")
        .or_else(|| text.strip_prefix("Exhaust â€” "))
    {
        (true, rest)
    } else {
        (false, text)
    };
    let normalized = text
        .strip_prefix("Crown of Madness — ")
        .or_else(|| text.strip_prefix("Crown of Madness â€” "))
        .unwrap_or(text);
    let normalized = strip_short_oracle_label(normalized);

    if normalized
        == "Tap two untapped creatures you control: Copy target triggered ability you control. You may choose new targets for the copy. Activate only once each turn."
    {
        let (costs, mut decisions) =
            parse_activation_costs("Tap two untapped creatures you control")?;
        decisions.push(target_decision(
            "kirolTriggeredAbility",
            json!({
                "kind": "stackItems",
                "controller": controller(),
                "where": { "kind": "isTriggeredAbility" },
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
                "activationLimit": { "kind": "oncePerTurn", "id": "kirolCopyTrigger" },
                "effects": [{
                    "kind": "copyStackItem",
                    "object": chosen_target("kirolTriggeredAbility"),
                    "controller": controller(),
                    "mayChooseNewTargets": true,
                }],
            }),
            &["Tap two distinct controlled creatures and copy a controlled triggered ability"],
        ));
    }

    if normalized
        == "{5}, {T}, Sacrifice this artifact: Create X tapped 2/2 colorless Scarecrow artifact creature tokens, where X is the number of charge counters on this artifact."
    {
        let (costs, decisions) = parse_activation_costs("{5}, {T}, Sacrifice this artifact")?;
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
                    "kind": "resolveTriggeredInstruction",
                    "operation": "wickersmithCreateScarecrows",
                }],
            }),
            &["Create tapped Scarecrows equal to the sacrificed source's charge counters"],
        ));
    }

    if normalized == "{1}{B}{R}: Put a -1/-1 counter on another target creature." {
        let (costs, mut decisions) = parse_activation_costs("{1}{B}{R}")?;
        decisions.push(target_decision(
            "scorpionGodCreature",
            json!({
                "kind": "permanents",
                "where": card_type("Creature"),
                "excludeSource": true,
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
                    "kind": "putCounters",
                    "permanent": chosen_target("scorpionGodCreature"),
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &["Put a -1/-1 counter on another targeted creature"],
        ));
    }

    if normalized == "{T}: Double the number of each kind of counter on target creature." {
        let (costs, mut decisions) = parse_activation_costs("{T}")?;
        decisions.push(target_decision(
            "ferraforCreature",
            permanent_target_candidates("creature", face_name)?,
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
                    "kind": "resolveTriggeredInstruction",
                    "operation": "ferraforDoubleCounters",
                }],
            }),
            &["Double every kind of counter on the targeted creature"],
        ));
    }

    if normalized == "{T}: Exchange target opponent's life total with this creature's toughness." {
        let (costs, mut decisions) = parse_activation_costs("{T}")?;
        decisions.push(target_decision(
            "treeOpponent",
            json!({
                "kind": "players",
                "where": { "kind": "isOpponentOf", "player": controller() },
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
                    "kind": "resolveTriggeredInstruction",
                    "operation": "treeOfPerditionExchange",
                }],
            }),
            &["Exchange the opponent's life total with the source's toughness"],
        ));
    }

    if normalized
        == "{B}, Remove a -1/-1 counter from this creature: Put a -1/-1 counter on each other creature."
    {
        let (costs, decisions) =
            parse_activation_costs("{B}, Remove a -1/-1 counter from this creature")?;
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
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": card_type("Creature"),
                        "excludeSource": true,
                    },
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &["Move a -1/-1 counter from the source to every other creature"],
        ));
    }

    if normalized == "Put a -1/-1 counter on this creature: Untap this creature." {
        let costs = vec![json!({
            "kind": "putCountersOnSource",
            "counter": "-1/-1",
            "count": integer(1),
        })];
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "devotedDruidUntap",
                }],
            }),
            &["Put a -1/-1 counter on the source and untap it"],
        ));
    }

    if normalized
        == "Remove a -1/-1 counter from this creature: Put a -1/-1 counter on another target creature."
    {
        let (costs, mut decisions) =
            parse_activation_costs("Remove a -1/-1 counter from this creature")?;
        decisions.push(target_decision(
            "grimPoppetCreature",
            json!({
                "kind": "permanents",
                "where": card_type("Creature"),
                "excludeSource": true,
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
                    "kind": "putCounters",
                    "permanent": chosen_target("grimPoppetCreature"),
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &["Move a -1/-1 counter from the source to another creature"],
        ));
    }

    if normalized
        == "{R}: Target creature you control gains trample until end of turn. If this is the third time this ability has resolved this turn, add {R}{R}{R}{R}."
    {
        let (costs, mut decisions) = parse_activation_costs("{R}")?;
        decisions.push(target_decision(
            "targetCreature",
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
                "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                "effects": [
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("targetCreature"),
                        "keyword": "trample",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "resolveOrdinalTriggeredAbility",
                        "id": stable_rule_id("soulbrightSeeker", normalized),
                        "branches": [{
                            "ordinal": integer(3),
                            "effects": [{
                                "kind": "addMana",
                                "player": controller(),
                                "mana": "{R}{R}{R}{R}",
                            }],
                        }],
                    },
                ],
            }),
            &[
                "Pay red mana",
                "Give a controlled creature trample",
                "Add four red mana on the third resolution",
            ],
        ));
    }

    if normalized == "{4}: This artifact becomes a 4/4 artifact creature until end of turn." {
        let (costs, decisions) = parse_activation_costs("{4}")?;
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                "effects": [{
                    "kind": "becomeCreature",
                    "object": self_ref(),
                    "addTypes": ["Creature"],
                    "addSubtypes": [],
                    "basePower": integer(4),
                    "baseToughness": integer(4),
                    "retainExistingTypes": true,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Pay the animation cost",
                "Animate the artifact as a 4/4 until end of turn",
            ],
        ));
    }

    if normalized.starts_with(
        "Discard a card: This creature gains indestructible until end of turn. Tap it.",
    ) {
        let (costs, decisions) = parse_activation_costs("Discard a card")?;
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                "effects": [
                    {
                        "kind": "grantKeyword",
                        "object": self_ref(),
                        "keyword": "indestructible",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "tapPermanent",
                        "permanent": self_ref(),
                    },
                ],
            }),
            &[
                "Discard a card as the activation cost",
                "Grant indestructible until end of turn",
                "Tap the source",
            ],
        ));
    }

    if normalized
        == "{U}{R}: Until end of turn, this land becomes a 2/1 blue and red Elemental creature with \"During your turn, this creature has first strike.\" It's still a land."
    {
        let (costs, decisions) = parse_activation_costs("{U}{R}")?;
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                "effects": [
                    {
                        "kind": "becomeCreature",
                        "object": self_ref(),
                        "addTypes": ["Creature"],
                        "addSubtypes": ["Elemental"],
                        "addColors": ["blue", "red"],
                        "basePower": 2,
                        "baseToughness": 1,
                        "retainExistingTypes": true,
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": self_ref(),
                        "keyword": "firstStrike",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Pay the animation cost",
                "Animate Restless Spire until end of turn",
                "Grant its controller-turn first strike ability",
            ],
        ));
    }
    if normalized.starts_with("{2}{U}, {T}: Put target creature on the bottom of its owner's library. That creature's controller reveals cards from the top of their library until they reveal a creature card.") {
        let (costs, mut decisions) = parse_activation_costs("{2}{U}, {T}")?;
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
                "activationCondition": { "kind": "sorceryTiming" },
                "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "proteusStaff",
                }],
            }),
            &[
                "Pay and tap Proteus Staff at sorcery speed",
                "Put the target creature on the bottom of its owner's library",
                "Reveal and replace it with the next creature",
            ],
        ));
    }
    if normalized
        == "{T}: Put a card exiled with this artifact into its owner's graveyard. If it's a land card, create a Treasure token. If it's a nonland card, create a 2/2 black Rogue creature token."
    {
        let (costs, decisions) = parse_activation_costs("{T}")?;
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "currencyConverterCashIn",
                }],
            }),
            &[
                "Tap Currency Converter",
                "Move one linked exiled card to its owner's graveyard",
                "Create Treasure for a land or Rogue for a nonland",
            ],
        ));
    }
    if normalized
        == "{3}: Put a shadow counter on another target creature. Activate only as a sorcery."
    {
        let (costs, mut decisions) = parse_activation_costs("{3}")?;
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "where": card_type("Creature"),
                "excludeSource": true,
            }),
            1,
            1,
        ));
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "activationCondition": { "kind": "sorceryTiming" },
                "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": chosen_target("targetCreature"),
                    "counter": "shadow",
                    "count": integer(1),
                }],
            }),
            &["Put a shadow counter on another target creature at sorcery speed"],
        ));
    }

    if let Some((cost_text, raw_instruction)) = normalized.split_once(':') {
        let instruction = raw_instruction
            .trim()
            .split_once(" (")
            .map(|(instruction, _)| instruction)
            .unwrap_or(raw_instruction.trim());
        if instruction
            == "Destroy target artifact or enchantment. If that permanent was a legendary enchantment, draw a card. Activate only as a sorcery."
        {
            let (costs, mut decisions) = parse_activation_costs(cost_text)?;
            decisions.push(target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
                }),
                1,
                1,
            ));
            let target = chosen_target("targetPermanent");
            return Some(draft(
                json!({
                    "kind": "activatedAbility",
                    "source": self_ref(),
                    "costs": costs,
                    "activationCondition": { "kind": "sorceryTiming" },
                    "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                    "effects": [
                        { "kind": "destroyPermanent", "permanent": target.clone() },
                        {
                            "kind": "conditionalEffect",
                            "condition": {
                                "kind": "objectMatchesFilter",
                                "object": target,
                                "where": and(vec![
                                    json!({ "kind": "isLegendary" }),
                                    card_type("Enchantment"),
                                ]),
                            },
                            "then": [{
                                "kind": "drawCards",
                                "player": controller(),
                                "count": integer(1),
                            }],
                            "else": [],
                        },
                    ],
                }),
                &[
                    "Pay the sacrifice cost",
                    "Destroy the chosen artifact or enchantment",
                    "Inspect its last known card characteristics",
                    "Draw for a legendary enchantment",
                ],
            ));
        }
        if instruction
            == "Until end of turn, whenever Lyra deals combat damage to a player, draw two cards."
        {
            let (costs, decisions) = parse_activation_costs(cost_text)?;
            return Some(draft(
                json!({
                    "kind": "activatedAbility",
                    "source": self_ref(),
                    "costs": costs,
                    "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                    "effects": [{
                        "kind": "installCombatDamageTrigger",
                        "object": self_ref(),
                        "duration": { "kind": "currentCombat" },
                        "effects": [{
                            "kind": "drawCards",
                            "player": controller(),
                            "count": integer(2),
                        }],
                    }],
                }),
                &[
                    "Pay the activation cost",
                    "Install Lyra's combat-damage draw trigger",
                ],
            ));
        }
        if instruction
            == "Whenever a creature you control deals combat damage to a player or planeswalker this turn, draw a card."
        {
            let (costs, decisions) = parse_activation_costs(cost_text)?;
            return Some(draft(
                json!({
                    "kind": "activatedAbility",
                    "source": self_ref(),
                    "costs": costs,
                    "declaration": { "kind": "castingDeclaration", "decisions": decisions },
                    "effects": [{
                        "kind": "installControlledCombatDamageDrawUntilEndOfTurn",
                        "player": controller(),
                        "count": integer(1),
                    }],
                }),
                &[
                    "Pay the mana and counter-removal costs",
                    "Install the controlled-creature combat-damage trigger for this turn",
                    "Draw a card for each qualifying damage event",
                ],
            ));
        }
        if instruction
            == "Target creature with a +1/+1 counter on it gains flying until end of turn."
        {
            let (costs, cost_decisions) = parse_activation_costs(cost_text)?;
            let mut rule = json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "where": and(vec![
                                card_type("Creature"),
                                json!({ "kind": "hasCounter", "counter": "+1/+1" }),
                            ]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "flying",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            });
            if !cost_decisions.is_empty() {
                rule["declaration"]["decisions"]
                    .as_array_mut()?
                    .extend(cost_decisions);
            }
            return Some(draft(
                rule,
                &[
                    "Pay the activation cost",
                    "Target a countered creature",
                    "Grant flying",
                ],
            ));
        }
        if let Some(behold_criteria) = parse_search_then_optional_behold_untap(instruction) {
            let (costs, decisions) = parse_activation_costs(cost_text)?;
            let mut effects = search_library_effects(
                json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                1,
                "battlefield",
                true,
            );
            effects.push(json!({
                "kind": "optionalBehold",
                "player": controller(),
                "where": parse_permanent_criteria(behold_criteria, "")?,
                "untap": decision_result("searchedCards"),
            }));
            let mut rule = json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "effects": effects,
            });
            if !decisions.is_empty() {
                rule["declaration"] = json!({
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                });
            }
            return Some(draft(
                rule,
                &[
                    "Parse the reusable activation costs",
                    "Search for the basic land through shared zone effects",
                    "Optionally behold a matching card and untap the searched land",
                ],
            ));
        }
    }

    let (cost_text, raw_instruction) = normalized.split_once(':')?;
    let (mut costs, mut decisions) = parse_activation_costs(cost_text)?;
    let activation_instruction = raw_instruction
        .trim()
        .split_once(". (")
        .map(|(instruction, _)| format!("{}.", instruction.trim_end_matches('.')))
        .unwrap_or_else(|| raw_instruction.trim().to_string());
    let raw_instruction = activation_instruction
        .split_once(" Activate only ")
        .map(|(instruction, _)| instruction.trim())
        .unwrap_or(activation_instruction.as_str());
    let (raw_instruction, mana_cost_reduction) = match parse_activation_reduction(raw_instruction) {
        Some(ActivationReduction::Conditional {
            instruction,
            amount,
            condition,
        }) => (
            instruction.to_string(),
            Some(json!({
                "kind": "conditionalValue",
                "condition": parse_condition_text(condition)
                    .or_else(|| parse_controlled_permanent_condition(condition, ""))?,
                "ifTrue": integer(amount),
                "ifFalse": integer(0),
            })),
        ),
        Some(ActivationReduction::PerControlledPermanent {
            instruction,
            amount,
            criteria,
        }) => {
            let count = json!({
                "kind": "countPermanents",
                "player": controller(),
                "where": parse_permanent_criteria(criteria, "")?,
            });
            (
                instruction.to_string(),
                Some(if amount == 1 {
                    count
                } else {
                    json!({
                        "kind": "multiply",
                        "left": count,
                        "right": integer(amount),
                    })
                }),
            )
        }
        None => (raw_instruction.to_string(), None),
    };
    let instruction = raw_instruction
        .rsplit_once(" (")
        .filter(|(_, reminder)| reminder.ends_with(')'))
        .map(|(instruction, _)| instruction.to_string())
        .unwrap_or_else(|| raw_instruction.to_string());
    let (instruction, activation_x_value) = instruction
        .strip_suffix(" X is the mana value of the exiled card.")
        .map(|instruction| {
            (
                instruction.to_string(),
                Some(json!({
                    "kind": "manaValueOf",
                    "object": { "kind": "cardExiledWithSource" },
                })),
            )
        })
        .unwrap_or((instruction, None));

    if instruction.lines().next().is_some_and(|header| {
        header
            .trim_end_matches([' ', '-', '\u{2014}'])
            .eq_ignore_ascii_case("Choose one")
    }) {
        let mut modal = parse_general_modal_spell(&instruction)?;
        modal.rule["kind"] = Value::String("activatedAbility".to_string());
        modal.rule["costs"] = Value::Array(costs);
        if !decisions.is_empty() {
            let modal_decisions = modal.rule["declaration"]["decisions"]
                .as_array_mut()
                .expect("general modal declaration decisions");
            decisions.append(modal_decisions);
            modal.rule["declaration"]["decisions"] = Value::Array(decisions);
        }
        modal.operations.insert(
            0,
            "Partition reusable activation costs before parsing the modal instruction".to_string(),
        );
        return Some(modal);
    }
    let mut effects = Vec::new();

    if instruction
        == "Target land gains \"{T}: Add {C}{C}\" until this card is cast from exile. You may cast this card for as long as it remains exiled."
    {
        decisions.push(target_decision(
            "targetLand",
            json!({
                "kind": "permanents",
                "where": card_type("Land"),
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "resolveTriggeredInstruction",
            "operation": "emrakulExigentExile",
        }));
    }

    if let Some((selection_effects, selection_decisions)) =
        parse_choose_permanents_then_sacrifice_rest(&instruction, face_name)
    {
        effects.extend(selection_effects);
        decisions.extend(selection_decisions);
    }

    if effects.is_empty()
        && let Some(parts) = parse_token_reflexive_count_damage(&instruction)
    {
        effects.extend([
            create_token_effect(&format!("{}.", parts.token_instruction))?,
            json!({
                "kind": "createReflexiveTrigger",
                "source": self_ref(),
                "controller": controller(),
                "ability": {
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                    "condition": {
                        "kind": "controlsPermanent",
                        "where": parse_permanent_criteria(parts.required_permanent, "")?,
                        "excludeName": parts.excluded_name,
                    },
                    "declaration": {
                        "kind": "castingDeclaration",
                        "decisions": [target_decision(
                            "targetDamageable",
                            json!({ "kind": "anyTarget" }),
                            1,
                            1,
                        )],
                    },
                    "effects": [{
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "amount": {
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": parse_permanent_criteria(parts.counted_permanents, "")?,
                        },
                        "recipient": chosen_target("targetDamageable"),
                    }],
                },
            }),
        ]);
    }

    let temporary_opponent_attack_bonus_re = Regex::new(
        r"(?i)^Until your next turn, whenever one or more creatures attack one of your opponents, those creatures get ([+-]\d+)/([+-]\d+) and gain (.+?) until end of turn\.$",
    )
    .expect("temporary opponent-attack bonus regex compiles");
    if effects.is_empty()
        && let Some(captures) = temporary_opponent_attack_bonus_re.captures(&instruction)
    {
        effects.push(json!({
            "kind": "installOpponentAttackTrigger",
            "controller": controller(),
            "duration": { "kind": "untilNextTurn", "player": controller() },
            "effects": [
                {
                    "kind": "modifyTriggeringAttackers",
                    "power": integer(captures[1].parse::<i64>().ok()?),
                    "toughness": integer(captures[2].parse::<i64>().ok()?),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                },
                {
                    "kind": "grantKeywordToTriggeringAttackers",
                    "keyword": oracle_keyword_kind(captures.get(3)?.as_str())?,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                },
            ],
        }));
    }

    if effects.is_empty()
        && let Some((power, toughness)) = parse_protected_attack_stat_change(&instruction)
    {
        effects.push(json!({
            "kind": "installDelayedTriggeredAbility",
            "controller": controller(),
            "event": {
                "kind": "permanentAttacksProtectedPlayerOrPlaneswalker",
                "protectedPlayer": controller(),
            },
            "effects": [{
                "kind": "modifyPowerToughness",
                "object": { "kind": "triggeringPermanent" },
                "power": integer(power),
                "toughness": integer(toughness),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            }],
            "duration": { "kind": "untilNextTurn", "player": controller() },
        }));
    }

    if effects.is_empty()
        && let Some((parsed_effects, parsed_decisions)) =
            parse_hand_put_haste_delayed_sacrifice(&instruction)
    {
        decisions.extend(parsed_decisions);
        effects.extend(parsed_effects);
    }
    if !effects.is_empty() {
        // The generic activation assembly below preserves costs and the optional hand choice.
    } else {
        if let Some(criteria) = parse_sacrificed_power_draw_then_discard(&instruction) {
            let sacrifice = costs
                .iter_mut()
                .find(|cost| cost["kind"].as_str() == Some("sacrificePermanent"))?;
            let sacrificed_where = sacrifice.get("where").cloned().or_else(|| {
                let target_id = sacrifice["permanent"]["id"].as_str()?;
                decisions
                    .iter()
                    .find(|decision| decision["id"].as_str() == Some(target_id))
                    .map(|decision| decision["candidates"]["where"].clone())
            })?;
            if sacrificed_where != parse_permanent_criteria(criteria, "")? {
                return None;
            }
            sacrifice["bindPowerAs"] = Value::String("sacrificedPower".to_string());
            effects.extend([
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": decision_result("sacrificedPower"),
                }),
                json!({
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(1),
                }),
            ]);
        } else {
            if let Some(criteria) = parse_target_player_mill_sacrificed_power(&instruction) {
                let sacrifice = costs
                    .iter_mut()
                    .find(|cost| cost["kind"].as_str() == Some("sacrificePermanent"))?;
                let sacrificed_where = sacrifice.get("where").cloned().or_else(|| {
                    let target_id = sacrifice["permanent"]["id"].as_str()?;
                    decisions
                        .iter()
                        .find(|decision| decision["id"].as_str() == Some(target_id))
                        .map(|decision| decision["candidates"]["where"].clone())
                })?;
                if sacrificed_where != parse_permanent_criteria(criteria, "")? {
                    return None;
                }
                sacrifice["bindPowerAs"] = Value::String("sacrificedPower".to_string());
                decisions.push(target_decision(
                    "targetPlayer",
                    json!({ "kind": "players" }),
                    1,
                    1,
                ));
                effects.push(json!({
                    "kind": "mill",
                    "player": chosen_target("targetPlayer"),
                    "count": decision_result("sacrificedPower"),
                }));
            } else {
                if let Some(criteria) = parse_single_graveyard_cast_permission(&instruction) {
                    let mut where_filter = parse_permanent_criteria(criteria, "")?;
                    if criteria.to_ascii_lowercase().contains("permanent") {
                        where_filter = and(vec![
                            where_filter,
                            or(vec![
                                card_type("Artifact"),
                                card_type("Battle"),
                                card_type("Creature"),
                                card_type("Enchantment"),
                                card_type("Planeswalker"),
                            ]),
                        ]);
                    }
                    decisions.push(target_decision(
                        "graveyardCard",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": where_filter,
                        }),
                        1,
                        1,
                    ));
                    effects.push(json!({
                        "kind": "grantSingleCastPermissionIfNoSpellCast",
                        "player": controller(),
                        "card": chosen_target("graveyardCard"),
                        "expiresAfterTurn": { "kind": "currentTurn" },
                        "prohibitAdditionalSpells": true,
                    }));
                } else if let Some(exchange) = parse_linked_permanent_exchange(&instruction) {
        let battlefield_criteria = parse_permanent_criteria(exchange.battlefield_criteria, "")?;
        let graveyard_criteria = parse_permanent_criteria(exchange.graveyard_criteria, "")?;
        if battlefield_criteria != parse_permanent_criteria(exchange.sacrificed_criteria, "")?
            || graveyard_criteria != parse_permanent_criteria(exchange.returned_criteria, "")?
        {
            return None;
        }
        decisions.push(target_decision(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "where": battlefield_criteria,
            }),
            1,
            1,
        ));
        let mut graveyard_decision = target_decision(
            "targetGraveyardCard",
            json!({
                "kind": "cards",
                "zone": { "kind": "anyGraveyard" },
                "where": graveyard_criteria,
            }),
            1,
            1,
        );
        graveyard_decision["selectionConstraint"] = json!({
            "kind": "zoneOwnerMatchesTargetController",
            "zone": "graveyard",
            "targetId": "targetPermanent",
        });
        decisions.push(graveyard_decision);
        effects.push(json!({
            "kind": "exchangePermanentWithGraveyardCard",
            "permanent": chosen_target("targetPermanent"),
            "card": chosen_target("targetGraveyardCard"),
        }));
    } else if let Some((generic_effects, generic_decisions)) =
        parse_general_effect_sequence(&instruction, face_name)
            .or_else(|| parse_general_effect_instruction(&instruction, face_name))
    {
        effects = generic_effects;
        decisions.extend(generic_decisions);
    } else if instruction
        == "Untap all creatures you control. After this main phase, there is an additional combat phase followed by an additional main phase."
    {
        effects.push(json!({
            "kind": "resolveTriggeredInstruction",
            "operation": "aggravatedAssault",
        }));
    } else if instruction
        == "Destroy target creature of your choice, then destroy target creature of an opponent's choice."
    {
        decisions.push(target_decision(
            "firstCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "resolveTriggeredInstruction",
            "operation": "azulaFlameDestroyTwo",
        }));
    } else if instruction
        == "Exile the top card of each opponent's library. Until end of turn, you may play one of those cards without paying its mana cost."
    {
        effects.push(json!({
            "kind": "resolveTriggeredInstruction",
            "operation": "fireLordOzaiExileTop",
        }));
    } else if instruction
        == "Return target instant or sorcery card from your graveyard to your hand."
    {
        decisions.push(target_decision(
            "targetSpellCard",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "moveTargetCard",
            "card": chosen_target("targetSpellCard"),
            "to": "hand",
            "tapped": false,
        }));
    } else if instruction == "Exile target card from a graveyard." {
        decisions.push(target_decision(
            "targetGraveyardCard",
            json!({
                "kind": "cards",
                "zone": { "kind": "anyGraveyard" },
                "where": Value::Null,
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "exileTargetCardWithSource",
            "card": chosen_target("targetGraveyardCard"),
            "source": self_ref(),
        }));
    } else if instruction == "Draw a card, then discard a card. Put a quest counter on Arcade Gannon." {
        effects.extend([
            json!({
                "kind": "drawThenDiscard",
                "player": controller(),
                "drawCount": integer(1),
                "discardCount": integer(1),
            }),
            json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "quest",
                "count": integer(1),
            }),
        ]);
    } else if instruction == "Each player draws a card." {
        effects.push(json!({
            "kind": "drawEachPlayer",
            "count": integer(1),
        }));
    } else if instruction == "Draw a card." {
        effects.push(json!({
            "kind": "drawCards",
            "player": controller(),
            "count": integer(1),
        }));
    } else if instruction == "Draw a card, then discard a card." {
        effects.push(json!({
            "kind": "drawThenDiscard",
            "player": controller(),
            "drawCount": integer(1),
            "discardCount": integer(1),
        }));
    } else if let Some(count) = parse_counted_draw(&instruction) {
        effects.push(json!({
            "kind": "drawCards",
            "player": controller(),
            "count": count,
        }));
    } else if instruction.starts_with("Return ")
        && instruction.ends_with("to its owner's hand.")
        && !instruction.to_ascii_lowercase().contains("target")
    {
        effects.push(json!({
            "kind": "returnToOwnersHand",
            "object": self_ref(),
        }));
    } else if let Some(counter) = parse_put_counter(&instruction)
        && matches!(counter.recipient, CounterRecipient::Source)
    {
        effects.push(json!({
            "kind": "putCounters",
            "permanent": self_ref(),
            "counter": counter.counter,
            "count": counter.count,
        }));
    } else if instruction == "Untap this creature." || instruction == "Untap this permanent." {
        effects.push(json!({
            "kind": "untapPermanent",
            "permanent": self_ref(),
        }));
    } else if instruction == "Untap another target permanent." {
        decisions.push(target_decision(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "excludeSource": true,
                "where": Value::Null,
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "untapPermanent",
            "permanent": chosen_target("targetPermanent"),
        }));
    } else if instruction == "Untap two other target legendary creatures." {
        decisions.push(target_decision(
            "targetCreatures",
            json!({
                "kind": "permanents",
                "excludeSource": true,
                "where": and(vec![card_type("Creature"), json!({ "kind": "isLegendary" })]),
            }),
            2,
            2,
        ));
        effects.push(json!({
            "kind": "untapPermanents",
            "objects": { "kind": "chosenTargets", "id": "targetCreatures" },
        }));
    } else if instruction == "Put a stun counter on up to one target tapped creature." {
        decisions.push(target_decision(
            "targetTappedCreature",
            json!({
                "kind": "permanents",
                "where": and(vec![card_type("Creature"), json!({ "kind": "isTapped" })]),
            }),
            0,
            1,
        ));
        effects.push(json!({
            "kind": "putCounters",
            "permanent": chosen_target("targetTappedCreature"),
            "counter": "stun",
            "count": integer(1),
        }));
    } else if instruction
        == "Create X 1/1 white Human Soldier creature tokens, where X is the number of Humans you control."
    {
        effects.push(json!({
            "kind": "createTokens",
            "controller": controller(),
            "quantity": {
                "kind": "countPermanents",
                "player": controller(),
                "where": subtype("Human"),
            },
            "token": {
                "name": "Human Soldier Token",
                "colors": ["white"],
                "types": ["Creature"],
                "subtypes": ["Human", "Soldier"],
                "power": 1,
                "toughness": 1,
            },
        }));
    } else if instruction == "You gain 1 life for each colorless creature you control." {
        effects.push(json!({
            "kind": "gainLife",
            "player": controller(),
            "amount": {
                "kind": "countPermanents",
                "player": controller(),
                "where": and(vec![
                    card_type("Creature"),
                    compare(
                        "==",
                        json!({ "kind": "colorCountOf", "object": { "kind": "candidate" } }),
                        integer(0),
                    ),
                ]),
            },
        }));
    } else if instruction == "Attach this Equipment to target creature you control." {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "attachPermanent",
            "attachment": self_ref(),
            "to": chosen_target("targetCreature"),
        }));
    } else if instruction
        == "You may put a historic permanent card from your hand onto the battlefield."
    {
        decisions.push(target_decision(
            "historicPermanent",
            json!({
                "kind": "cards",
                "zone": { "kind": "hand", "player": controller() },
                "where": and(vec![
                    json!({ "kind": "historic" }),
                    not(or(vec![card_type("Instant"), card_type("Sorcery")])),
                ]),
            }),
            0,
            1,
        ));
        effects.push(json!({
            "kind": "moveTargetCard",
            "card": chosen_target("historicPermanent"),
            "to": "battlefield",
            "tapped": false,
            "controller": controller(),
        }));
    } else if instruction
        == "Create a token that's a copy of target artifact. That token gains haste. Exile it at the beginning of the next end step."
    {
        decisions.push(target_decision(
            "targetArtifact",
            json!({ "kind": "permanents", "where": card_type("Artifact") }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "createTokenCopyOfPermanent",
            "object": chosen_target("targetArtifact"),
            "grantKeywords": ["haste"],
            "exileAtNextEndStep": true,
        }));
    } else if instruction
        == "Look at the top five cards of your library. You may reveal a historic card from among them and put it into your hand. Put the rest on the bottom of your library in a random order."
    {
        effects.extend([
            json!({
                "kind": "lookAtTopCards",
                "zone": library(controller()),
                "count": integer(5),
                "bind": "lookedCards",
            }),
            json!({
                "kind": "chooseCards",
                "id": "historicCard",
                "player": controller(),
                "from": bound_objects("lookedCards"),
                "where": { "kind": "historic" },
                "minimum": 0,
                "maximum": 1,
            }),
            json!({
                "kind": "revealCards",
                "cards": decision_result("historicCard"),
            }),
            json!({
                "kind": "moveCards",
                "cards": decision_result("historicCard"),
                "to": hand(controller()),
            }),
            json!({
                "kind": "moveCards",
                "cards": {
                    "kind": "setDifference",
                    "left": bound_objects("lookedCards"),
                    "right": decision_result("historicCard"),
                },
                "to": {
                    "kind": "library",
                    "player": controller(),
                    "position": "bottom",
                },
                "order": { "kind": "random" },
            }),
        ]);
    } else if instruction
        == "Destroy each nonland permanent with mana value equal to the number of charge counters on this artifact."
    {
        effects.push(json!({
            "kind": "destroyPermanentsMatchingSourceCounterManaValue",
            "counter": "charge",
            "excludeLands": true,
        }));
    } else if instruction == "Exchange your life total with Evra's power." {
        effects.push(json!({
            "kind": "exchangeLifeWithSourcePower",
            "player": controller(),
        }));
    } else if instruction
        == "Creatures you control with power less than Lena's power gain indestructible until end of turn."
    {
        effects.push(json!({
            "kind": "grantKeywordBelowSourcePower",
            "player": controller(),
            "where": card_type("Creature"),
            "keyword": "indestructible",
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
    } else if instruction
        == "Exile Stenn. Return it to the battlefield under its owner's control at the beginning of the next end step."
    {
        effects.push(json!({
            "kind": "exileUntilNextEndStep",
            "objects": self_ref(),
            "returnUnderOwnerControl": true,
            "creatureCounter": "",
            "planeswalkerCounter": "",
        }));
    } else if instruction.starts_with(
        "Until end of turn, target creature gains \"If this creature would deal combat damage to a player, prevent that damage.",
    ) {
        decisions.push(target_decision(
            "targetCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "installSokratesDialogue",
            "permanent": chosen_target("targetCreature"),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
    } else if instruction == "Destroy target nonbasic land." {
        decisions.push(target_decision(
            "targetLand",
            json!({
                "kind": "permanents",
                "where": and(vec![
                    card_type("Land"),
                    not(json!({ "kind": "typeLineContains", "value": "Basic" })),
                ]),
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "destroyPermanent",
            "permanent": chosen_target("targetLand"),
        }));
    } else if instruction == "Exile target player's graveyard." {
        decisions.push(target_decision(
            "targetPlayer",
            json!({ "kind": "players" }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "resolveTriggeredInstruction",
            "operation": "exileTargetGraveyard",
        }));
    } else if let Some(amount) = parse_you_gain_life(&instruction) {
        effects.push(json!({
            "kind": "gainLife",
            "player": controller(),
            "amount": amount,
        }));
    } else if let Some(effect) = create_token_effect(&instruction) {
        effects.push(effect);
    } else if let Some((count, counter)) = parse_remove_counter(&instruction) {
        effects.push(json!({
            "kind": "removeCounters",
            "permanent": self_ref(),
            "counter": counter,
            "count": count,
        }));
    } else if let Some((amount, recipient, followup)) = parse_source_damage(&instruction)
        && recipient.eq_ignore_ascii_case("each opponent")
    {
        effects.push(json!({
            "kind": "dealDamageToEachOpponent",
            "amount": amount,
        }));
        if let Some(token_instruction) = followup {
            effects.push(create_token_effect(token_instruction)?);
        }
    } else if let Some((amount, recipient, None)) = parse_source_damage(&instruction)
        && recipient.eq_ignore_ascii_case("any target")
    {
        decisions.push(target_decision(
            "damageTarget",
            json!({ "kind": "anyTarget" }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "dealDamage",
            "source": self_ref(),
            "amount": amount,
            "recipient": chosen_target("damageTarget"),
        }));
    } else if let Some(counter) = parse_put_counter(&instruction)
        && let CounterRecipient::NamedSource(recipient) = counter.recipient
        && source_reference_matches(recipient, face_name)
    {
        effects.push(json!({
            "kind": "putCounters",
            "permanent": self_ref(),
            "counter": counter.counter,
            "count": counter.count,
        }));
    } else if instruction == "Target creature you control explores." {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "explore",
            "object": chosen_target("targetCreature"),
        }));
    } else if let Some(criteria) = parse_temporary_unblockable_target(&instruction) {
        decisions.push(target_decision(
            "targetCreature",
            permanent_target_candidates(criteria, "")?,
            1,
            1,
        ));
        effects.push(json!({
            "kind": "grantKeyword",
            "object": chosen_target("targetCreature"),
            "keyword": "cantBeBlocked",
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
    } else if let Some(counter) = parse_put_counter(&instruction)
        && let CounterRecipient::EachControlled(criteria) = counter.recipient
    {
        effects.push(json!({
            "kind": "putCounters",
            "permanent": {
                "kind": "eachPermanent",
                "player": controller(),
                "where": parse_permanent_criteria(criteria, "")?,
            },
            "counter": counter.counter,
            "count": counter.count,
        }));
    } else if let Some(criteria) = parse_destroy_target(&instruction) {
        decisions.push(target_decision(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "where": parse_permanent_criteria(criteria, "")?,
            }),
            1,
            1,
        ));
        effects.push(json!({
            "kind": "destroyPermanent",
            "permanent": chosen_target("targetPermanent"),
        }));
    } else if let Some((general_effects, general_decisions)) =
        parse_general_effect_sequence(&instruction, face_name)
            .or_else(|| parse_general_effect_instruction(&instruction, face_name))
    {
        effects.extend(general_effects);
        decisions.extend(general_decisions);
    } else {
        return None;
    }
            }
        }
    }

    let activates_from_hand = costs.iter().any(|cost| {
        cost["kind"] == "discardCard" && cost["card"]["kind"] == "self"
            || cost["kind"] == "exileSource" && cost["zone"] == "hand"
    });
    let activates_from_graveyard = costs
        .iter()
        .any(|cost| cost["kind"] == "exileSource" && cost["zone"] == "graveyard");
    let loyalty_ability = costs
        .iter()
        .any(|cost| cost["kind"].as_str() == Some("payLoyalty"));
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": costs,
        "effects": effects,
    });
    if let Some(value) = activation_x_value {
        rule["activationXValue"] = value;
    }
    if let Some(reduction) = mana_cost_reduction {
        rule["manaCostReduction"] = reduction;
    }
    if activates_from_hand {
        rule["activationZone"] = Value::String("hand".to_string());
    } else if activates_from_graveyard {
        rule["activationZone"] = Value::String("graveyard".to_string());
    } else if rule["effects"].as_array().is_some_and(|effects| {
        effects.iter().any(|effect| {
            effect["kind"].as_str() == Some("moveAbilitySourceToHand")
                || (effect["kind"].as_str() == Some("moveAbilitySourceToBattlefield")
                    && effect["from"].as_str() == Some("graveyard"))
        })
    }) {
        rule["activationZone"] = Value::String("graveyard".to_string());
    }
    if !decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        });
    }
    if loyalty_ability {
        let sorcery_timing = json!({ "kind": "sorceryTiming" });
        rule["activationCondition"] = if let Some((_, condition_text)) =
            activation_instruction.split_once("Activate only if ")
        {
            let condition_text = condition_text.trim_end_matches('.');
            and(vec![
                sorcery_timing,
                parse_condition_text(condition_text)
                    .or_else(|| parse_controlled_permanent_condition(condition_text, ""))?,
            ])
        } else {
            sorcery_timing
        };
        rule["activationLimit"] = json!({
            "kind": "oncePerTurn",
            "id": "loyaltyAbility",
        });
    } else if activation_instruction.contains("Activate only as a sorcery") {
        rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
    } else if activation_instruction.contains("Activate only during your turn") {
        rule["activationCondition"] = json!({
            "kind": "duringControllerTurn",
            "player": controller(),
        });
    } else if let Some((_, condition_text)) = activation_instruction.split_once("Activate only if ")
    {
        let condition_text = condition_text
            .split(" and only once each turn")
            .next()
            .unwrap_or(condition_text)
            .trim_end_matches('.');
        rule["activationCondition"] = parse_condition_text(condition_text)
            .or_else(|| parse_controlled_permanent_condition(condition_text, ""))?;
    }
    if !loyalty_ability
        && (activation_instruction.contains("Activate only once each turn")
            || activation_instruction.contains("and only once each turn"))
    {
        rule["activationLimit"] = json!({
            "kind": "oncePerTurn",
            "id": "oracleActivation",
        });
    }
    if exhaust {
        rule["activationLimit"] = json!({
            "kind": "oncePerGameObject",
            "id": "exhaust",
        });
    }
    Some(draft(
        rule,
        &[
            "Partition activation costs",
            "Declare activation targets",
            "Resolve simple activated instruction",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_common_activated_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let normalized = if text.starts_with("Crown of Madness") {
        text.find('{').map(|index| &text[index..]).unwrap_or(text)
    } else {
        text
    };
    let normalized = normalized
        .strip_prefix("Exhaust â€” ")
        .or_else(|| normalized.strip_prefix("Exhaust Ã¢â‚¬â€ "))
        .unwrap_or(normalized);
    let normalized = if normalized.starts_with("Boast ") {
        normalized
            .find('{')
            .map(|index| &normalized[index..])
            .unwrap_or(normalized)
    } else {
        normalized
    };
    let (cost_text, instruction) = normalized.split_once(':')?;
    let (costs, mut decisions) = parse_activation_costs(cost_text)?;
    let activated = |costs: Vec<Value>, decisions: Vec<Value>, effects: Vec<Value>| {
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        draft(
            rule,
            &[
                "Partition reusable activation costs",
                "Declare legal activation targets",
                "Resolve composed activation effects",
            ],
        )
    };
    let raw_instruction = instruction.trim();
    let instruction = raw_instruction
        .split_once(" (")
        .map(|(instruction, _)| instruction)
        .unwrap_or(raw_instruction);

    if let Some(criteria) = parse_copy_target_retaining_ability(instruction) {
        decisions.push(target_decision(
            "copyTarget",
            permanent_target_candidates(criteria, "this permanent")?,
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "becomeCopyOfPermanent",
                "object": self_ref(),
                "copy": chosen_target("copyTarget"),
                "retainResolvingAbility": true,
            })],
        ));
    }

    if instruction
        == "Create a token that's a copy of a card exiled with this artifact. It gains haste. Exile it at the beginning of the next end step."
    {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "mimicVatCreateToken",
            })],
        ));
    }
    if instruction == "Destroy target creature that dealt damage to you this turn." {
        decisions.push(target_decision(
            "damagingCreature",
            json!({
                "kind": "permanents",
                "where": and(vec![
                    card_type("Creature"),
                    json!({ "kind": "dealtDamageToControllerThisTurn" }),
                ]),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": chosen_target("damagingCreature"),
            })],
        ));
    }

    if instruction == "You may put a land card from your hand onto the battlefield." {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "mayPutLandFromHand",
            })],
        ));
    }
    if instruction == "Return this card from your graveyard to your hand." {
        let mut result = activated(
            costs,
            decisions,
            vec![json!({ "kind": "moveAbilitySourceToHand" })],
        );
        result.rule["activationZone"] = Value::String("graveyard".to_string());
        return Some(result);
    }
    if instruction == "Create an 8/8 green and white Elemental creature token with vigilance." {
        let mut result = activated(
            costs,
            decisions,
            vec![json!({
                "kind": "createTokens",
                "quantity": integer(1),
                "token": {
                    "name": "Elemental Token",
                    "colors": ["green", "white"],
                    "types": ["Creature"],
                    "subtypes": ["Elemental"],
                    "power": 8,
                    "toughness": 8,
                    "abilities": [{
                        "kind": "keywordAbility",
                        "source": self_ref(),
                        "ability": { "kind": "vigilance" },
                    }],
                },
            })],
        );
        result.rule["distinctTargets"] = json!([["tapCreatureCostOne", "tapCreatureCostTwo"]]);
        return Some(result);
    }
    if instruction
        == "Look at the top card of your library. If it's a land card, you may reveal it and put it into your hand."
    {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "revealTopLandIntoHand",
            })],
        ));
    }
    if instruction == "Proliferate twice" || instruction == "Proliferate twice." {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "proliferateTwice",
            })],
        ));
    }
    if matches!(
        instruction.to_ascii_lowercase().as_str(),
        "regenerate it"
            | "regenerate it."
            | "regenerate this creature"
            | "regenerate this creature."
            | "regenerate this permanent"
            | "regenerate this permanent."
    ) {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "installRegenerationShield",
                "object": self_ref(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if instruction == "Target creature you control phases out"
        || instruction == "Target creature you control phases out."
    {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "phaseOutPermanent",
                "permanent": chosen_target("targetCreature"),
            })],
        ));
    }
    if instruction
        == "Create a 0/0 green and blue Fractal creature token. Put X +1/+1 counters on it, where X is the number of differently named lands you control."
    {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "createTokens",
                "quantity": integer(1),
                "token": {
                    "name": "Fractal Token",
                    "colors": ["green", "blue"],
                    "types": ["Creature"],
                    "subtypes": ["Fractal"],
                    "power": 0,
                    "toughness": 0,
                },
                "counters": [{
                    "counter": "+1/+1",
                    "count": {
                        "kind": "countDistinctPermanentNames",
                        "player": controller(),
                        "where": card_type("Land"),
                    },
                }],
            })],
        ));
    }

    if instruction == "Untap target Forest." {
        decisions.push(target_decision(
            "targetForest",
            json!({ "kind": "permanents", "where": subtype("Forest") }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "untapPermanent",
                "permanent": chosen_target("targetForest"),
            })],
        ));
    }
    if instruction == "This creature gets +4/+4 until end of turn." {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": self_ref(),
                "power": integer(4),
                "toughness": integer(4),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if instruction == "Put a lore counter on this enchantment." {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "lore",
                "count": integer(1),
            })],
        ));
    }
    if instruction == "Destroy target artifact or enchantment." {
        decisions.push(target_decision(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": chosen_target("targetPermanent"),
            })],
        ));
    }
    if instruction
        == "Add X mana of any one color, where X is the number of enchantments you control."
    {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "addMana",
                "player": controller(),
                "mana": {
                    "kind": "chooseColor",
                    "amount": x_variable_expression("the number of enchantments you control")?,
                },
            })],
        ));
    }

    if instruction
        == "Shuffle your library, then exile the top X cards, where X is one plus the number of spells cast this turn. Until end of turn, you may play lands and cast spells from among cards exiled this way without paying their mana costs."
    {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "magusMindExileStormCountFree",
            })],
        ));
    }

    if instruction
        == "It deals damage equal to the number of creatures you control to target creature."
    {
        decisions.push(target_decision(
            "damageTarget",
            json!({
                "kind": "permanents",
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "dealDamage",
                "recipient": chosen_target("damageTarget"),
                "amount": {
                    "kind": "countPermanents",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
            })],
        ));
    }

    if instruction
        == "Create a token that's a copy of another target creature you control. It gains haste and \"When this token dies, draw a card.\" Sacrifice it at the beginning of the next end step. Activate only as a sorcery."
    {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "excludeSource": true,
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        let mut result = activated(
            costs,
            decisions,
            vec![json!({
                "kind": "createModifiedTokenCopy",
                "object": chosen_target("targetCreature"),
                "grantKeywords": ["haste"],
                "sacrificeAtNextEndStep": true,
                "diesDrawCard": true,
            })],
        );
        result.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(result);
    }
    if matches!(
        instruction,
        "Choose target enchantment you control that doesn't have the same name as another permanent you control. Create a token that's a copy of it, except it isn't legendary. If the token is an Aura, untap Yenna, Redtooth Regent, then scry 2. Activate only as a sorcery."
            | "Choose target enchantment you control that doesn't have the same name as another permanent you control. Create a token that's a copy of it, except it isn't legendary. If the token is an Aura, untap Yenna, then scry 2. Activate only as a sorcery."
    ) {
        decisions.push(target_decision(
            "targetEnchantment",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Enchantment"),
                "uniqueNameAmongController": true,
            }),
            1,
            1,
        ));
        let mut result = activated(
            costs,
            decisions,
            vec![
                json!({
                    "kind": "createModifiedTokenCopy",
                    "object": chosen_target("targetEnchantment"),
                    "removeLegendary": true,
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "objectMatchesFilter",
                        "object": chosen_target("targetEnchantment"),
                        "where": { "kind": "subtypeContains", "value": "Aura" },
                    },
                    "then": [
                        {
                            "kind": "untapPermanent",
                            "permanent": self_ref(),
                        },
                        {
                            "kind": "scry",
                            "player": controller(),
                            "count": { "kind": "integer", "value": 2 },
                        },
                    ],
                    "else": [],
                }),
            ],
        );
        result.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(result);
    }

    if instruction
        == "Search your library for a land card, put it onto the battlefield tapped, then shuffle."
    {
        return Some(activated(
            costs,
            decisions,
            search_library_effects(card_type("Land"), 1, "battlefield", true),
        ));
    }
    if instruction
        == "Search your library for up to two basic land cards that share a land type, put them onto the battlefield tapped, then shuffle."
    {
        let mut effects = search_library_effects(
            json!({ "kind": "typeLineContains", "value": "Basic Land" }),
            2,
            "battlefield",
            true,
        );
        effects[0]["sameLandType"] = Value::Bool(true);
        return Some(activated(costs, decisions, effects));
    }
    if let Some(search) = parse_basic_land_search(instruction) {
        let maximum = search.maximum;
        let description = search.description;
        let filter = if description == "basic land" {
            json!({ "kind": "typeLineContains", "value": "Basic Land" })
        } else {
            let normalized_types = description
                .trim_start_matches("basic ")
                .replace(", or ", ", ")
                .replace(" or ", ", ");
            let land_types = normalized_types
                .split(", ")
                .map(|value| subtype(value.trim()))
                .collect::<Vec<_>>();
            and(vec![
                json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                or(land_types),
            ])
        };
        let destination = search.destination;
        let mut result = activated(
            costs,
            decisions,
            search_library_effects(filter, maximum, destination, search.tapped),
        );
        if text.starts_with("Boast ") {
            result.rule["activationCondition"] = json!({ "kind": "sourceAttackedThisTurn" });
            result.rule["activationLimit"] = json!({
                "kind": "oncePerTurn",
                "id": "boast",
            });
        }
        return Some(result);
    }
    if instruction
        == "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four or more lands, untap that land."
    {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "resolveFabledPassage",
            })],
        ));
    }
    if instruction == "Put your commander into your hand from the command zone." {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "returnCommanderToHand",
            })],
        ));
    }
    if instruction == "Return target artifact card from your graveyard to your hand." {
        decisions.push(target_decision(
            "targetArtifactCard",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": card_type("Artifact"),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetArtifactCard"),
                "to": "hand",
                "tapped": false,
            })],
        ));
    }
    if instruction == "Return target Ally you control to its owner's hand." {
        decisions.push(target_decision(
            "targetAlly",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": subtype("Ally"),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": chosen_target("targetAlly"),
            })],
        ));
    }
    if instruction == "Return target creature you control to its owner's hand." {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": chosen_target("targetCreature"),
            })],
        ));
    }
    if instruction.starts_with("Target creature you control gains shroud until end of turn") {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "grantKeyword",
                "object": chosen_target("targetCreature"),
                "keyword": "shroud",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if instruction.starts_with(
        "Until end of turn, this enchantment becomes a Monk Avatar creature in addition to its other types",
    ) {
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "animateSourceFromLoreCounters",
            })],
        ));
    }
    if let Some((amount, recipient, None)) = parse_source_damage(instruction)
        && recipient.eq_ignore_ascii_case("target creature with flying")
    {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "where": and(vec![
                    card_type("Creature"),
                    json!({ "kind": "hasKeyword", "value": "flying" }),
                ]),
            }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "dealDamage",
                "recipient": chosen_target("targetCreature"),
                "amount": amount,
            })],
        ));
    }
    if instruction
        == "This creature deals 1 damage to target creature. That creature can't block this turn."
    {
        decisions.push(target_decision(
            "targetCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![
                json!({
                    "kind": "dealDamage",
                    "recipient": chosen_target("targetCreature"),
                    "amount": integer(1),
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "cantBlock",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
        ));
    }
    if instruction == "Attach target Equipment you control to target creature you control." {
        decisions.extend([
            target_decision(
                "targetEquipment",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": subtype("Equipment"),
                }),
                1,
                1,
            ),
            target_decision(
                "targetCreature",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Creature"),
                }),
                1,
                1,
            ),
        ]);
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "attachPermanent",
                "attachment": chosen_target("targetEquipment"),
                "to": chosen_target("targetCreature"),
            })],
        ));
    }
    if instruction == "Exile target player's graveyard. Draw a card." {
        decisions.push(target_decision(
            "targetPlayer",
            json!({ "kind": "players" }),
            1,
            1,
        ));
        return Some(activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "exileTargetGraveyardThenDraw",
            })],
        ));
    }

    let operation = |costs: Vec<Value>, decisions: Vec<Value>, operation: &str| {
        activated(
            costs,
            decisions,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": operation,
            })],
        )
    };
    if instruction == "Matoc deals 2 damage to any other target." {
        decisions.push(target_decision(
            "damageTarget",
            json!({ "kind": "anyTarget", "excludeSource": true }),
            1,
            1,
        ));
        return Some(operation(costs, decisions, "matocDealTwo"));
    }
    if instruction.starts_with("Manifest the top card of your library. Any player may activate this ability but only as a sorcery.") {
        let mut parsed = operation(costs, decisions, "sashManifestTop");
        parsed.rule["anyPlayerMayActivate"] = Value::Bool(true);
        parsed.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(parsed);
    }
    if instruction.starts_with("Double the number of +1/+1 counters on each creature you control.")
    {
        return Some(operation(
            costs,
            decisions,
            "doubleControlledCreatureCounters",
        ));
    }
    if instruction.starts_with("Double the amount of each type of unspent mana you have.") {
        return Some(operation(costs, decisions, "doubleManaPool"));
    }
    if instruction.starts_with("You may cast spells this turn as though they had flash.") {
        return Some(operation(costs, decisions, "grantFlashUntilEndOfTurn"));
    }
    if instruction
        .starts_with("You may put a land card from your hand onto the battlefield tapped.")
    {
        return Some(operation(
            costs,
            decisions,
            "putHandLandOntoBattlefieldTapped",
        ));
    }
    if instruction.starts_with("Target player mills X cards.") {
        decisions.push(target_decision(
            "targetPlayer",
            json!({ "kind": "players" }),
            1,
            1,
        ));
        return Some(operation(costs, decisions, "millTargetPlayerX"));
    }
    if instruction.starts_with("Target nonland permanent becomes an artifact in addition to its other types until end of turn.") {
        decisions.push(target_decision(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "where": not(card_type("Land")),
            }),
            1,
            1,
        ));
        return Some(operation(costs, decisions, "makeTargetArtifactUntilEndOfTurn"));
    }
    if instruction.starts_with("Goad target creature") {
        decisions.push(target_decision(
            "targetCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        ));
        return Some(operation(costs, decisions, "goadTargetCreature"));
    }
    if instruction.starts_with("Create X Treasure tokens.") {
        return Some(operation(costs, decisions, "createXTreasures"));
    }
    if instruction.starts_with("Search your library for any number of God cards, put them onto the battlefield, then shuffle.") {
        return Some(operation(costs, decisions, "putAllGodsOntoBattlefield"));
    }
    if instruction
        .starts_with("Reveal cards from the top of your library until you reveal an artifact card.")
    {
        return Some(operation(costs, decisions, "audaciousReshapers"));
    }

    None
}

pub(in crate::oracle::canonical) fn active_while_battlefield() -> Value {
    json!({
        "kind": "inZone",
        "object": self_ref(),
        "zone": { "kind": "battlefield" },
    })
}

pub(in crate::oracle::canonical) fn parse_special_activated_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let staged_creature_form_re = Regex::new(
        r"(?i)^(.+?): (?:If this creature is (?:a|an) ([A-Za-z][A-Za-z '-]+), )?(?:this creature|it) becomes (?:a|an) ([A-Za-z][A-Za-z '-]+) with base power and toughness (\d+)/(\d+)( and protection from each of your opponents)?\.$",
    )
    .expect("staged creature form activation regex compiles");
    if let Some(captures) = staged_creature_form_re.captures(text) {
        let (costs, decisions) = parse_activation_costs(captures.get(1)?.as_str())?;
        let subtypes = captures
            .get(3)?
            .as_str()
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut effects = vec![json!({
            "kind": "becomeCreature",
            "object": self_ref(),
            "addTypes": ["Creature"],
            "addSubtypes": subtypes,
            "basePower": integer(captures[4].parse::<i64>().ok()?),
            "baseToughness": integer(captures[5].parse::<i64>().ok()?),
            "retainExistingTypes": true,
            "replaceSubtypes": true,
            "duration": { "kind": "permanent" },
        })];
        if captures.get(6).is_some() {
            effects.push(json!({
                "kind": "grantKeyword",
                "object": self_ref(),
                "keyword": "protectionFromOpponents",
                "duration": { "kind": "permanent" },
            }));
        }
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "effects": effects,
        });
        if let Some(required_type) = captures.get(2) {
            rule["activationCondition"] = json!({
                "kind": "objectMatchesFilter",
                "object": self_ref(),
                "where": subtype(required_type.as_str()),
            });
        }
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Parse the staged activation cost",
                "Require the current creature form when specified",
                "Replace creature subtypes and base statistics permanently",
            ],
        ));
    }
    if let Some((cost_text, instruction)) = text.split_once(':')
        && let Some((effects, mut effect_decisions)) =
            parse_hand_put_haste_delayed_sacrifice(instruction.trim())
    {
        let (costs, mut cost_decisions) = parse_activation_costs(cost_text.trim())?;
        cost_decisions.append(&mut effect_decisions);
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
        return Some(draft(
            rule,
            &[
                "Parse the activation costs",
                "Choose an eligible permanent card from hand",
                "Grant haste and install the delayed sacrifice trigger",
            ],
        ));
    }
    if let Some((threshold, ability)) = parse_station_threshold(text) {
        let mut parsed = parse_simple_activated_ability(ability)
            .or_else(|| parse_common_activated_ability(ability))?;
        let station_condition = compare(
            ">=",
            json!({
                "kind": "countCounters",
                "object": self_ref(),
                "counter": "charge",
            }),
            integer(threshold),
        );
        parsed.rule["activationCondition"] =
            if let Some(existing) = parsed.rule.get("activationCondition") {
                and(vec![station_condition, existing.clone()])
            } else {
                station_condition
            };
        parsed
            .operations
            .push("Gate the reusable activation by the Spacecraft charge threshold".to_string());
        return Some(parsed);
    }
    let activated_rule = |costs: Vec<Value>, declaration: Option<Value>, effects: Vec<Value>| {
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "effects": effects,
        });
        if let Some(declaration) = declaration {
            rule["declaration"] = declaration;
        }
        draft(
            rule,
            &[
                "Partition activation costs",
                "Declare activation choices",
                "Resolve activated effects",
            ],
        )
    };
    let loyalty_rule = |costs: Vec<Value>, declaration: Option<Value>, effects: Vec<Value>| {
        let mut parsed = activated_rule(costs, declaration, effects);
        parsed.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        parsed.rule["activationLimit"] = json!({
            "kind": "oncePerTurn",
            "id": "loyaltyAbility",
        });
        parsed
    };
    let custom_loyalty =
        |amount: Value, starting_loyalty: i64, operation: &str, declaration: Option<Value>| {
            let mut parsed = loyalty_rule(
                vec![json!({
                    "kind": "payLoyalty",
                    "object": self_ref(),
                    "amount": amount,
                })],
                declaration,
                vec![json!({
                    "kind": "resolveTriggeredInstruction",
                    "operation": operation,
                })],
            );
            parsed.rule["startingLoyalty"] = integer(starting_loyalty);
            parsed
        };
    if text
        == "{1}, {T}: Another target creature gets +X/+X until end of turn, where X is Picard's power. Activate only as a sorcery."
    {
        let (costs, _) = parse_activation_costs("{1}, {T}")?;
        let mut parsed = activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "picardLeadingPump",
            })],
        );
        parsed.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(parsed);
    }
    if text
        == "{6}: Create a tapped Planet land token named New Planet with \"{T}: Add one mana of any color.\" Activate only as a sorcery."
    {
        let (costs, _) = parse_activation_costs("{6}")?;
        let mut parsed = activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "saurianExplorerPlanet",
            })],
        );
        parsed.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(parsed);
    }
    let hoc_activated = |cost_text: &str, operation_name: &str| {
        let (costs, _) = parse_activation_costs(cost_text)?;
        Some(activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction", "operation": operation_name,
            })],
        ))
    };
    if text
        == "{T}: Put a burden counter on The One Ring, then draw a card for each burden counter on The One Ring."
    {
        return hoc_activated("{T}", "oneRingBurden");
    }
    if text
        == "{1}{B}, {T}: Choose a player with the most life or tied for most life. Target creature can't be blocked by creatures that player controls this turn."
    {
        return hoc_activated("{1}{B}, {T}", "blackGateUnblockable");
    }
    if text
        == "{5}{B}{R}, {T}, Sacrifice Mount Doom and a legendary artifact: Choose up to two creatures, then destroy the rest. Activate only as a sorcery."
    {
        let mut parsed = hoc_activated(
            "{5}{B}{R}, {T}, Sacrifice this permanent",
            "mountDoomDestroyRest",
        )?;
        parsed.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(parsed);
    }
    if text == "{1}{G}, {T}, Tap an untapped creature you control: Create a Food token." {
        return hoc_activated("{1}{G}, {T}", "shireCreateFood");
    }
    if text
        == "{1}, {T}: Put a charge counter on this artifact. Note the type of mana spent to pay this activation cost. Activate only if there are no charge counters on this artifact."
    {
        let (costs, _) = parse_activation_costs("{1}, {T}")?;
        let mut parsed = activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "jeweledAmuletCharge",
            })],
        );
        parsed.rule["activationCondition"] = compare(
            "==",
            json!({
                "kind": "countCounters",
                "object": self_ref(),
                "counter": "charge",
            }),
            integer(0),
        );
        return Some(parsed);
    }
    if text
        == "{T}, Remove a charge counter from this artifact: Add one mana of this artifact's last noted type."
    {
        let (costs, _) = parse_activation_costs("{T}, Remove a charge counter from this artifact")?;
        return Some(activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "jeweledAmuletMana",
            })],
        ));
    }
    if text
        == "{5}{W}{W}{W}, Sacrifice Nivea: Create an Akroma, Angel of Wrath token. (She's a {5}{W}{W}{W} legendary 6/6 Angel creature with flying, first strike, vigilance, trample, haste, and protection from black and from red.)"
    {
        let (costs, _) = parse_activation_costs("{5}{W}{W}{W}, Sacrifice this permanent")?;
        return Some(activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "niveaCreateAkroma",
            })],
        ));
    }
    if text == "{1}{R}: Creatures you control get +1/+0 until end of turn." {
        let (costs, _) = parse_activation_costs("{1}{R}")?;
        return Some(activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
                "power": integer(1),
                "toughness": integer(0),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if text == "{2}{G}: This creature has base power and toughness 4/4 until end of turn." {
        let (costs, _) = parse_activation_costs("{2}{G}")?;
        return Some(activated_rule(
            costs,
            None,
            vec![json!({
                "kind": "setBasePowerToughness",
                "object": self_ref(),
                "power": integer(4),
                "toughness": integer(4),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if text == "{2}{W}: Another target creature perpetually gains lifelink." {
        let (costs, _) = parse_activation_costs("{2}{W}")?;
        return Some(activated_rule(
            costs,
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({
                        "kind": "permanents",
                        "where": card_type("Creature"),
                        "excludeSource": true,
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "grantKeyword",
                "object": chosen_target("targetCreature"),
                "keyword": "lifelink",
                "duration": { "kind": "permanent" },
            })],
        ));
    }
    if text
        == "{2}{U}: Target instant or sorcery card in your graveyard gains flashback until end of turn. The flashback cost is equal to its mana cost."
    {
        let (costs, _) = parse_activation_costs("{2}{U}")?;
        return Some(activated_rule(
            costs,
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCard",
                    json!({
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "grantAbility",
                "object": chosen_target("targetCard"),
                "ability": {
                    "kind": "flashback",
                    "cost": { "kind": "manaCostOf", "card": { "kind": "abilitySource" } },
                },
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if text
        == "{X}{B}: Return target creature card with mana value X from your graveyard to the battlefield with a finality counter on it. Activate only as a sorcery."
    {
        let (costs, decisions) = parse_activation_costs("{X}{B}")?;
        let mut parsed = activated_rule(
            costs,
            Some(json!({ "kind": "castingDeclaration", "decisions": decisions })),
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "jetCollectorReanimate",
            })],
        );
        parsed.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(parsed);
    }

    if text
        == "{3}, {T}: You draw a card and gain 1 life. This ability costs {3} less to activate if you had 200 or more cards in your starting deck."
    {
        let (costs, _) = parse_activation_costs("{3}, {T}")?;
        let mut parsed = activated_rule(
            costs,
            None,
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }),
                json!({
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(1),
                }),
            ],
        );
        parsed.rule["manaCostReduction"] = json!({
            "kind": "startingDeckSizeAtLeast",
            "threshold": integer(200),
            "amount": integer(3),
        });
        return Some(parsed);
    }

    let permanent_choice = |id: &str, filter: Value, exclude_source: bool| {
        let mut candidates = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": filter,
        });
        if exclude_source {
            candidates["excludeSource"] = Value::Bool(true);
        }
        target_decision(id, candidates, 1, 1)
    };

    let mbc_loyalty_text = text.replace("âˆ’", "−");
    if mbc_loyalty_text.contains("Create a Feroz's Ban token.") {
        return Some(custom_loyalty(integer(-7), 5, "ferozMinusSeven", None));
    }
    if mbc_loyalty_text.starts_with("+1: Add one mana of any color.")
        && mbc_loyalty_text.contains("Aura spell")
    {
        return Some(custom_loyalty(integer(1), 4, "unluckiestPlusOne", None));
    }
    if mbc_loyalty_text.contains("Discard your hand")
        && mbc_loyalty_text.contains("twice the number of Auras")
    {
        return Some(custom_loyalty(integer(-3), 4, "unluckiestMinusThree", None));
    }
    if mbc_loyalty_text.starts_with("+1: Exile up to one other target permanent you control.") {
        return Some(custom_loyalty(integer(1), 4, "venserPlusOne", None));
    }
    if mbc_loyalty_text.contains("For each opponent, return up to one target nonland permanent") {
        return Some(custom_loyalty(integer(-2), 4, "venserMinusTwo", None));
    }
    match mbc_loyalty_text.as_str() {
        "+2: Create two 1/1 blue Bird creature tokens with flying." => {
            return Some(custom_loyalty(integer(2), 5, "ferozPlusTwo", None));
        }
        "0: Draw a card. You may put a permanent card with mana value 4 or less from your hand onto the battlefield." =>
        {
            return Some(custom_loyalty(integer(0), 5, "ferozZero", None));
        }
        "−1: Look at the top six cards of your library. You may reveal a planeswalker or basic Plains card from among them and put it into your hand. Put the rest on the bottom of your library in a random order." =>
        {
            return Some(custom_loyalty(integer(-1), 4, "worzelMinusOne", None));
        }
        "−8: Create ten Scryb Sprites tokens. (They're {G} 1/1 Faerie creatures with flying.)" => {
            return Some(custom_loyalty(integer(-8), 4, "worzelMinusEight", None));
        }
        "+1: Create two tapped Powerstone tokens. (They're artifacts with \"{T}: Add {C}. This mana can't be spent to cast a nonartifact spell.\")" =>
        {
            return Some(custom_loyalty(integer(1), 4, "dyfedPlusOne", None));
        }
        "−6: Search your library for an artifact card, put it onto the battlefield, then shuffle." =>
        {
            return Some(custom_loyalty(integer(-6), 4, "dyfedMinusSix", None));
        }
        "−5: Create a Lord of the Pit token. (It's a {4}{B}{B}{B} 7/7 Demon creature with flying, trample, and \"At the beginning of your upkeep, sacrifice another creature. If you can't, this token deals 7 damage to you.\")" =>
        {
            return Some(custom_loyalty(integer(-5), 4, "thomilMinusFive", None));
        }
        "+2: Create a Giant Badger token. (It's a {1}{G}{G} 2/2 Badger creature with \"Whenever this token blocks, it gets +2/+2 until end of turn.\")" =>
        {
            return Some(custom_loyalty(integer(2), 5, "greensleevesPlusTwo", None));
        }
        "−3: Mill three cards. Put all permanent cards from among them into your hand." => {
            return Some(custom_loyalty(
                integer(-3),
                5,
                "greensleevesMinusThree",
                None,
            ));
        }
        "−8: Until end of turn, creatures you control have base power and toughness 8/8 and gain trample." =>
        {
            return Some(custom_loyalty(
                integer(-8),
                5,
                "greensleevesMinusEight",
                None,
            ));
        }
        "−3: Create a Black Lotus token. (It's a {0} artifact with \"{T}, Sacrifice this token: Add three mana of any one color.\")" =>
        {
            return Some(custom_loyalty(integer(-3), 4, "arzakonMinusThree", None));
        }
        "−6: Each opponent exiles the top two cards of their library. Until end of turn, you may play those cards without paying their mana costs." =>
        {
            return Some(custom_loyalty(integer(-6), 4, "sifaMinusSix", None));
        }
        _ => {}
    }
    if mbc_loyalty_text == "−X: Untap X target artifacts." {
        let x = json!({ "kind": "decisionResult", "decisionId": "xValue" });
        let mut target = target_decision(
            "targetArtifacts",
            json!({ "kind": "permanents", "where": card_type("Artifact") }),
            0,
            0,
        );
        target["minimum"] = x.clone();
        target["maximum"] = x.clone();
        return Some(custom_loyalty(
            json!({ "kind": "negate", "operand": x }),
            4,
            "dyfedMinusX",
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    { "id": "xValue", "kind": "chooseNumber", "minimum": 0 },
                    target,
                ],
            })),
        ));
    }
    if mbc_loyalty_text == "+2: Arzakon deals 3 damage to any other target." {
        return Some(custom_loyalty(
            integer(2),
            4,
            "arzakonPlusTwo",
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "damageTarget",
                    json!({ "kind": "anyTarget", "excludeSource": true }),
                    1,
                    1,
                )],
            })),
        ));
    }
    if mbc_loyalty_text == "+1: Goad up to two target creatures." {
        return Some(custom_loyalty(
            integer(1),
            4,
            "sifaPlusOne",
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreatures",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    0,
                    2,
                )],
            })),
        ));
    }

    if text
        == "{5}: This land becomes a copy of target creature you control until end of turn. The \"legend rule\" doesn't apply to permanents you control this turn."
    {
        return Some(activated_rule(
            vec![json!({ "kind": "payMana", "manaCost": "{5}" })],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "copyTarget",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Creature"),
                    }),
                    1,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "becomeCopyOfPermanent",
                    "object": self_ref(),
                    "copy": chosen_target("copyTarget"),
                    "retainResolvingAbility": true,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "ignoreLegendRuleUntilEndOfTurn",
                    "player": controller(),
                }),
            ],
        ));
    }
    if text
        == "{W}{U}{B}{R}{G}, {T}: Each creature you control becomes prepared. (Only creatures with prepare spells can become prepared.)"
    {
        return Some(activated_rule(
            vec![
                json!({ "kind": "payMana", "manaCost": "{W}{U}{B}{R}{G}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            None,
            vec![json!({
                "kind": "prepareControlledCreatures",
                "player": controller(),
            })],
        ));
    }
    if text
        == "−2: For each opponent, return up to one target artifact or creature that player controls to its owner's hand."
    {
        let mut decision = target_decision(
            "targetPermanents",
            json!({
                "kind": "permanents",
                "controller": { "kind": "opponentsOf", "player": controller() },
                "where": or(vec![card_type("Artifact"), card_type("Creature")]),
            }),
            0,
            0,
        );
        decision["maximum"] = json!({ "kind": "countOpponents", "player": controller() });
        decision["selectionConstraint"] = json!({ "kind": "distinctPermanentControllers" });
        return Some(loyalty_rule(
            vec![json!({
                "kind": "payLoyalty",
                "object": self_ref(),
                "amount": integer(-2),
            })],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [decision],
            })),
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": { "kind": "chosenTargets", "id": "targetPermanents" },
            })],
        ));
    }
    if text
        == "−6: Draw three cards. Then put X +1/+1 counters on each creature you control, where X is the number of cards in your hand."
    {
        return Some(loyalty_rule(
            vec![json!({
                "kind": "payLoyalty",
                "object": self_ref(),
                "amount": integer(-6),
            })],
            None,
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(3),
                }),
                json!({
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                    "counter": "+1/+1",
                    "count": {
                        "kind": "countCards",
                        "zone": { "kind": "hand", "player": controller() },
                        "where": Value::Null,
                    },
                }),
            ],
        ));
    }
    if text
        == "−2: Each player sacrifices a creature of their choice. If you sacrificed a creature this way, create a 4/4 green Beast creature token with trample."
    {
        return Some(loyalty_rule(
            vec![json!({
                "kind": "payLoyalty",
                "object": self_ref(),
                "amount": integer(-2),
            })],
            None,
            vec![json!({
                "kind": "eachPlayerSacrificesCreatureGarrukReward",
                "player": controller(),
            })],
        ));
    }
    if text
        == "−3: Each opponent discards two cards. For each opponent who didn't discard two nonland cards this way, you draw a card."
    {
        return Some(loyalty_rule(
            vec![json!({
                "kind": "payLoyalty",
                "object": self_ref(),
                "amount": integer(-3),
            })],
            None,
            vec![json!({
                "kind": "opponentsDiscardTwoDrawForShortfall",
                "player": controller(),
            })],
        ));
    }
    if text
        == "{3}{R}: Exile target creature or planeswalker you control. Reveal cards from the top of your library until you reveal a creature or planeswalker card. Put that card onto the battlefield and the rest on the bottom of your library in a random order. Activate only as a sorcery."
    {
        let mut parsed = activated_rule(
            vec![json!({ "kind": "payMana", "manaCost": "{3}{R}" })],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPermanent",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "resolveIdentityEcho",
                "player": controller(),
                "permanent": chosen_target("targetPermanent"),
            })],
        );
        parsed.rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
        return Some(parsed);
    }
    if text
        == "−3: Exile another target planeswalker or creature you control. Reveal cards from the top of your library until you reveal a creature or planeswalker card. Put that card onto the battlefield and the rest on the bottom of your library in a random order."
    {
        return Some(loyalty_rule(
            vec![json!({
                "kind": "payLoyalty",
                "object": self_ref(),
                "amount": integer(-3),
            })],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [permanent_choice(
                    "targetPermanent",
                    or(vec![card_type("Planeswalker"), card_type("Creature")]),
                    true,
                )],
            })),
            vec![json!({
                "kind": "resolveIdentityEcho",
                "player": controller(),
                "permanent": chosen_target("targetPermanent"),
            })],
        ));
    }

    if text.starts_with("Exile any number of historic cards from your graveyard")
        && text.contains("total mana value 30 or greater")
    {
        return Some(activated_rule(
            vec![json!({
                "kind": "exileHistoricManaValue",
                "minimum": integer(30),
            })],
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "capitolineTriadEmblem",
            })],
        ));
    }

    if text.starts_with("{1}, {T}: Any number of target players each mill two cards.") {
        return Some(activated_rule(
            vec![
                json!({ "kind": "payMana", "manaCost": "{1}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            None,
            vec![
                json!({
                    "kind": "choosePlayers",
                    "id": "playersToMill",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": { "kind": "countPlayers" },
                }),
                json!({
                    "kind": "millEachPlayer",
                    "players": decision_result("playersToMill"),
                    "count": integer(2),
                }),
            ],
        ));
    }

    if text.starts_with("Exhaust — {2}{U}{U}, {T}: Any number of target players each mill cards equal to the number of cards in their graveyard.") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{2}{U}{U}" },
                    { "kind": "tap", "object": self_ref() },
                ],
                "activationLimit": {
                    "kind": "oncePerGameObject",
                    "id": "exhaust",
                },
                "effects": [
                    {
                        "kind": "choosePlayers",
                        "id": "playersToMill",
                        "player": controller(),
                        "minimum": integer(0),
                        "maximum": { "kind": "countPlayers" },
                    },
                    {
                        "kind": "millEachPlayer",
                        "players": decision_result("playersToMill"),
                        "count": { "kind": "thatPlayersGraveyardCount" },
                    },
                ],
            }),
            &[
                "Partition exhaust activation costs",
                "Apply once-per-object activation limit",
                "Choose any number of players",
                "Mill each player by their graveyard size",
            ],
        ));
    }
    let target_permanent_choice = |id: &str, filter: Value| {
        target_decision(
            id,
            json!({
                "kind": "permanents",
                "where": filter,
            }),
            1,
            1,
        )
    };

    if text == "{3}{W}, {T}: Put a +1/+1 counter on each creature you control." {
        return Some(activated_rule(
            vec![
                json!({ "kind": "payMana", "manaCost": "{3}{W}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }

    if text == "{T}: Destroy target tapped creature." {
        return Some(activated_rule(
            vec![json!({ "kind": "tap", "object": self_ref() })],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    target_permanent_choice(
                        "targetPermanent",
                        and(vec![json!({ "kind": "isTapped" }), card_type("Creature")]),
                    ),
                ],
            })),
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": chosen_target("targetPermanent"),
            })],
        ));
    }

    if text == "{B}, Sacrifice a creature: Destroy target nonblack creature." {
        let (costs, mut decisions) = parse_activation_costs("{B}, Sacrifice a creature")?;
        decisions.push(target_permanent_choice(
            "targetPermanent",
            and(vec![not(color_filter("black")?), card_type("Creature")]),
        ));
        return Some(activated_rule(
            costs,
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            })),
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": chosen_target("targetPermanent"),
            })],
        ));
    }

    if text == "{1}{B}, Sacrifice another creature: Target creature gets -2/-1 until end of turn." {
        return Some(activated_rule(
            vec![
                json!({ "kind": "payMana", "manaCost": "{1}{B}" }),
                json!({
                    "kind": "sacrificePermanent",
                    "permanent": chosen_target("sacrificeCreature"),
                }),
            ],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    permanent_choice("sacrificeCreature", card_type("Creature"), true),
                    target_permanent_choice("targetCreature", card_type("Creature")),
                ],
            })),
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": chosen_target("targetCreature"),
                "power": integer(-2),
                "toughness": integer(-1),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }

    let sacrifice_creature_effect = match text {
        "Sacrifice a creature: Put a +1/+1 counter on this creature." => Some(json!({
            "kind": "putCounters",
            "permanent": self_ref(),
            "counter": "+1/+1",
            "count": integer(1),
        })),
        "Sacrifice a creature: Scry 1. (Look at the top card of your library. You may put that card on the bottom.)" => {
            Some(json!({
                "kind": "scry",
                "player": controller(),
                "count": integer(1),
            }))
        }
        "Sacrifice a creature: Add {C}{C}." => Some(json!({
            "kind": "addMana",
            "player": controller(),
            "mana": "{C}{C}",
        })),
        "Sacrifice a creature: Add one mana of any color." => Some(json!({
            "kind": "addMana",
            "player": controller(),
            "mana": {
                "kind": "chooseColor",
                "amount": 1,
            },
        })),
        _ => None,
    };
    if let Some(effect) = sacrifice_creature_effect {
        return Some(activated_rule(
            vec![json!({
                "kind": "sacrificePermanent",
                "permanent": chosen_target("sacrificeCreature"),
            })],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    permanent_choice("sacrificeCreature", card_type("Creature"), false),
                ],
            })),
            vec![effect],
        ));
    }

    if text
        == "Sacrifice another creature or artifact: Surveil 1. (Look at the top card of your library. You may put it into your graveyard.)"
    {
        return Some(activated_rule(
            vec![json!({
                "kind": "sacrificePermanent",
                "permanent": chosen_target("sacrificePermanent"),
            })],
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    permanent_choice(
                        "sacrificePermanent",
                        or(vec![card_type("Creature"), card_type("Artifact")]),
                        true,
                    ),
                ],
            })),
            vec![json!({
                "kind": "surveil",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }

    if let Some((cost_text, instruction)) = text.split_once(':')
        && let Some((costs, decisions)) = parse_activation_costs(cost_text)
        && costs.iter().any(|cost| cost["kind"] == "payMana")
        && costs.iter().any(|cost| cost["kind"] == "tap")
        && costs
            .iter()
            .any(|cost| cost["kind"] == "sacrificePermanent")
    {
        let instruction = instruction.trim();
        let effects = if let Some(count) = parse_counted_draw(instruction) {
            Some(vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": count,
            })])
        } else if let Some(amount) = parse_you_gain_life(instruction) {
            Some(vec![json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": amount,
            })])
        } else if let Some(criteria) = parse_random_graveyard_card_return(instruction) {
            Some(vec![json!({
                "kind": "moveRandomCard",
                "from": graveyard(controller()),
                "where": parse_permanent_criteria(criteria, "")?,
                "to": { "kind": "hand", "player": controller() },
            })])
        } else {
            None
        };
        if let Some(effects) = effects {
            let declaration = (!decisions.is_empty()).then(|| {
                json!({
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                })
            });
            return Some(activated_rule(costs, declaration, effects));
        }
    }

    if text == "{3}{B}{B}: Creatures you control gain lifelink until end of turn." {
        return Some(activated_rule(
            vec![json!({ "kind": "payMana", "manaCost": "{3}{B}{B}" })],
            None,
            vec![json!({
                "kind": "grantKeyword",
                "object": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
                "keyword": "lifelink",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }

    if text
        == "{3}{B}, {T}: Create a 1/1 colorless Spirit creature token with \"This token can't block or be blocked by non-Spirit creatures.\""
    {
        return Some(activated_rule(
            vec![
                json!({ "kind": "payMana", "manaCost": "{3}{B}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": {
                    "types": ["Creature"],
                    "subtypes": ["Spirit"],
                    "power": 1,
                    "toughness": 1,
                    "abilities": [{ "kind": "cantBlock" }],
                },
            })],
        ));
    }

    if let Some((cost_text, instruction)) = text.split_once(':')
        && let Some((costs, decisions)) = parse_activation_costs(cost_text)
        && costs.iter().any(|cost| cost["kind"] == "tap")
        && costs
            .iter()
            .any(|cost| cost["kind"] == "sacrificePermanent" && cost["permanent"]["kind"] == "self")
        && let Some((filter_text, tapped)) = parse_library_search_to_battlefield(instruction.trim())
    {
        let filter = if filter_text == "a basic land" {
            json!({ "kind": "typeLineContains", "value": "Basic Land" })
        } else if let Some(types) = filter_text.strip_prefix("a basic ") {
            or(types
                .split(", ")
                .flat_map(|part| part.split(", or "))
                .flat_map(|part| part.split(" or "))
                .map(|value| subtype(value.trim()))
                .collect())
        } else {
            or(filter_text
                .split(", ")
                .flat_map(|part| part.split(", or "))
                .flat_map(|part| part.split(" or "))
                .map(|value| subtype(strip_leading_article(value.trim())))
                .collect())
        };
        let declaration = (!decisions.is_empty()).then(|| {
            json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            })
        });
        return Some(activated_rule(
            costs,
            declaration,
            search_library_effects(filter, 1, "battlefield", tapped),
        ));
    }

    if let Some((cost_text, instruction)) = text.split_once(':')
        && instruction
            .trim()
            .eq_ignore_ascii_case("The next spell you cast this turn can't be countered.")
        && let Some((costs, decisions)) = parse_activation_costs(cost_text)
    {
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "effects": [{
                "kind": "installOneShotModifier",
                "capture": {
                    "player": { "kind": "abilityController" },
                },
                "expires": { "kind": "endOfCurrentTurn" },
                "match": {
                    "kind": "spellCast",
                    "player": {
                        "kind": "capturedValue",
                        "name": "player",
                    },
                },
                "apply": [{
                    "kind": "modifyStackObject",
                    "object": { "kind": "eventSpell" },
                    "modifier": { "kind": "cantBeCountered" },
                }],
                "consume": { "kind": "firstMatchingEvent" },
            }],
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Partition mana and tap costs",
                "Capture ability controller",
                "Register next matching cast event",
                "Install one-shot stack modifier",
            ],
        ));
    }

    if text
        == "{T}: Create a Treasure token. Activate only if you've cast an instant or sorcery spell this turn."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationCondition": {
                    "kind": "hasCastSpellThisTurn",
                    "player": controller(),
                    "where": or(vec![
                        card_type("Instant"),
                        card_type("Sorcery"),
                    ]),
                },
                "costs": [{
                    "kind": "tap",
                    "object": self_ref(),
                }],
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": 1,
                    "token": {
                        "kind": "namedToken",
                        "name": "Treasure",
                    },
                }],
            }),
            &[
                "Resolve instant-or-sorcery cast history",
                "Attach activation condition",
                "Resolve tap cost",
                "Resolve named Treasure token",
            ],
        ));
    }

    if text.starts_with("{5}: If this land isn't a creature, it becomes a 2/4 Wizard creature") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{
                    "kind": "payMana",
                    "manaCost": "{5}",
                }],
                "effects": [{
                    "kind": "conditional",
                    "condition": not(json!({
                        "kind": "hasCardType",
                        "object": self_ref(),
                        "value": "Creature",
                    })),
                    "then": [
                        {
                            "kind": "becomeCreature",
                            "object": self_ref(),
                            "addTypes": ["Creature"],
                            "addSubtypes": ["Wizard"],
                            "basePower": 2,
                            "baseToughness": 4,
                            "retainExistingTypes": true,
                            "duration": { "kind": "permanent" },
                        },
                        {
                            "kind": "grantAbility",
                            "object": self_ref(),
                            "duration": { "kind": "permanent" },
                            "ability": {
                                "kind": "triggeredAbility",
                                "event": {
                                    "kind": "spellCast",
                                    "player": {
                                        "kind": "controllerOf",
                                        "object": { "kind": "abilitySource" },
                                    },
                                    "where": or(vec![
                                        card_type("Instant"),
                                        card_type("Sorcery"),
                                    ]),
                                },
                                "effects": [{
                                    "kind": "modifyPowerToughness",
                                    "object": { "kind": "abilitySource" },
                                    "power": 1,
                                    "toughness": 0,
                                    "duration": {
                                        "kind": "untilEndOfCurrentTurn",
                                    },
                                }],
                            },
                        },
                    ],
                }],
            }),
            &[
                "Resolve mana activation cost",
                "Reduce noncreature condition",
                "Apply persistent land-creature characteristics",
                "Grant quoted cast trigger",
            ],
        ));
    }

    if text
        == "Sacrifice this creature: Creature tokens you control gain indestructible until end of turn."
    {
        return Some(activated_rule(
            vec![json!({
                "kind": "sacrificePermanent",
                "permanent": self_ref(),
            })],
            None,
            vec![json!({
                "kind": "grantKeyword",
                "object": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": and(vec![
                        card_type("Creature"),
                        json!({ "kind": "isToken" }),
                    ]),
                },
                "keyword": "indestructible",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }

    if let Some((cost_text, level)) = parse_class_level(text)
        && let Some((costs, decisions)) = parse_activation_costs(cost_text)
    {
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "activationCondition": {
                "kind": "classLevelIs",
                "object": self_ref(),
                "value": integer(level - 1),
            },
            "effects": [{
                "kind": "setClassLevel",
                "object": self_ref(),
                "value": integer(level),
            }],
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Recognize Class level activation",
                "Require previous level",
                "Resolve mana cost",
                "Set new class level",
            ],
        ));
    }

    if text.starts_with("{2}{W}, {T}: Whenever you attack this turn, create two 1/1 red Warrior") {
        return Some(activated_rule(
            vec![
                json!({ "kind": "payMana", "manaCost": "{2}{W}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            None,
            vec![json!({
                "kind": "installAttackTrigger",
                "player": controller(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(2),
                    "tapped": true,
                    "attacking": true,
                    "sacrificeAtNextEndStep": true,
                    "token": {
                        "types": ["Creature"],
                        "subtypes": ["Warrior"],
                        "colors": ["Red"],
                        "power": 1,
                        "toughness": 1,
                    },
                }],
            })],
        ));
    }

    if text.starts_with("Imprint â€” {1}, {T}: Exile target creature card from a graveyard.")
        || text.starts_with("Imprint — {1}, {T}: Exile target creature card from a graveyard.")
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{1}" },
                    { "kind": "tap", "object": self_ref() },
                ],
                "activationCondition": { "kind": "sorceryTiming" },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetCreatureCard",
                            json!({
                                "kind": "cards",
                                "zone": {
                                    "kind": "anyGraveyard",
                                },
                                "where": card_type("Creature"),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "exileTargetCardWithSource",
                    "card": chosen_target("targetCreatureCard"),
                    "source": self_ref(),
                }],
            }),
            &[
                "Resolve imprint activation costs",
                "Declare creature card in any graveyard",
                "Exile and associate card with source",
                "Apply sorcery timing",
            ],
        ));
    }

    if text.starts_with(
        "{6}: Create a token that's a copy of target creature card exiled with this artifact",
    ) {
        return Some(activated_rule(
            vec![json!({ "kind": "payMana", "manaCost": "{6}" })],
            None,
            vec![json!({
                "kind": "createDinoDnaToken",
                "source": self_ref(),
                "basePower": integer(6),
                "baseToughness": integer(6),
                "colors": ["Green"],
                "subtypes": ["Dinosaur"],
                "grantKeywords": ["trample"],
            })],
        ));
    }

    if text.starts_with("{2}, {T}, Sacrifice a creature: Gain control of target creature") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{2}" },
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature"),
                    },
                ],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        permanent_choice("sacrificeCreature", card_type("Creature"), false),
                        target_permanent_choice("targetCreature", card_type("Creature")),
                    ],
                },
                "effects": [{
                    "kind": "gainControlWhileSourceTapped",
                    "permanent": chosen_target("targetCreature"),
                    "source": self_ref(),
                }],
            }),
            &[
                "Resolve mana, tap, and sacrifice costs",
                "Declare creature target",
                "Gain control while source remains controlled and tapped",
            ],
        ));
    }

    if text.starts_with("{T}, Sacrifice two other creatures: Any number of target players") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature1"),
                    },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature2"),
                    },
                ],
                "distinctTargets": [["sacrificeCreature1", "sacrificeCreature2"]],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        permanent_choice("sacrificeCreature1", card_type("Creature"), true),
                        permanent_choice("sacrificeCreature2", card_type("Creature"), true),
                    ],
                },
                "effects": [{
                    "kind": "priestOfForgottenGods",
                    "controller": controller(),
                }],
            }),
            &[
                "Resolve tap and two-creature sacrifice costs",
                "Require distinct other creatures",
                "Choose any number of players",
                "Resolve loss, sacrifice, mana, and draw",
            ],
        ));
    }

    if text.starts_with("Remove four quest counters from this enchantment and sacrifice it:") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    {
                        "kind": "removeCounters",
                        "permanent": self_ref(),
                        "counter": "quest",
                        "count": integer(4),
                    },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": self_ref(),
                    },
                ],
                "effects": [{
                    "kind": "installDamageMultiplier",
                    "player": controller(),
                    "factor": integer(2),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Resolve quest-counter removal and sacrifice costs",
                "Install controller damage replacement",
            ],
        ));
    }

    if text
        == "{4}, {T}: Two target creatures you control that share a creature type can't be blocked this turn."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{4}" },
                    { "kind": "tap", "object": self_ref() },
                ],
                "distinctTargets": [["targetCreature1", "targetCreature2"]],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        permanent_choice("targetCreature1", card_type("Creature"), false),
                        permanent_choice("targetCreature2", card_type("Creature"), false),
                    ],
                },
                "activationCondition": {
                    "kind": "targetsShareCreatureType",
                    "targets": ["targetCreature1", "targetCreature2"],
                },
                "effects": [
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("targetCreature1"),
                        "keyword": "cantBeBlocked",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("targetCreature2"),
                        "keyword": "cantBeBlocked",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Resolve mana and tap costs",
                "Declare two distinct controlled creatures",
                "Require a shared creature type",
                "Prevent both from being blocked",
            ],
        ));
    }

    if text
        == "{1}{B}, {T}, Sacrifice another creature: Draw X cards, where X is that creature's power."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{1}{B}" },
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature"),
                        "bindPowerAs": "sacrificedPower",
                    },
                ],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        permanent_choice("sacrificeCreature", card_type("Creature"), true),
                    ],
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": {
                        "kind": "decisionResult",
                        "decisionId": "sacrificedPower",
                    },
                }],
            }),
            &[
                "Resolve mana, tap, and other-creature sacrifice costs",
                "Capture sacrificed creature power",
                "Draw captured number of cards",
            ],
        ));
    }

    if text == "{4}: Put this card from your hand onto the battlefield." {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "hand",
                "costs": [{
                    "kind": "payMana",
                    "manaCost": "{4}",
                }],
                "effects": [{
                    "kind": "moveAbilitySourceToBattlefield",
                    "from": "hand",
                    "tapped": false,
                }],
            }),
            &[
                "Resolve hand activation zone",
                "Resolve mana cost",
                "Move source to battlefield",
            ],
        ));
    }

    if text.starts_with(
        "{1}, {T}: Create a token that's a copy of another target creature you control",
    ) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{1}" },
                    { "kind": "tap", "object": self_ref() },
                ],
                "activationCondition": { "kind": "sorceryTiming" },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        permanent_choice("targetCreature", card_type("Creature"), true),
                    ],
                },
                "effects": [{
                    "kind": "createModifiedTokenCopy",
                    "object": chosen_target("targetCreature"),
                    "basePower": integer(1),
                    "baseToughness": integer(1),
                    "addColors": ["Red"],
                    "addTypes": ["Creature"],
                    "addSubtypes": ["Balloon"],
                    "grantKeywords": ["flying", "haste"],
                    "sacrificeAtNextEndStep": true,
                }],
            }),
            &[
                "Resolve mana and tap costs",
                "Declare another controlled creature",
                "Create modified Balloon token copy",
                "Register next-end-step sacrifice",
            ],
        ));
    }

    if text == "Sacrifice two creatures: Create a 3/1 red Beast creature token named Carnivore." {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature1"),
                    },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature2"),
                    },
                ],
                "distinctTargets": [["sacrificeCreature1", "sacrificeCreature2"]],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        permanent_choice("sacrificeCreature1", card_type("Creature"), false),
                        permanent_choice("sacrificeCreature2", card_type("Creature"), false),
                    ],
                },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": {
                        "name": "Carnivore",
                        "types": ["Creature"],
                        "subtypes": ["Beast"],
                        "power": 3,
                        "toughness": 1,
                    },
                }],
            }),
            &[
                "Resolve two-creature sacrifice cost",
                "Require distinct creatures",
                "Create named Carnivore token",
            ],
        ));
    }

    if text
        == "Vivid â€” {T}: For each color among permanents you control, add one mana of that color."
        || text
            == "Vivid — {T}: For each color among permanents you control, add one mana of that color."
        || text == "{T}: For each color among permanents you control, add one mana of that color."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{
                    "kind": "tap",
                    "object": self_ref(),
                }],
                "effects": [{
                    "kind": "addMana",
                    "player": controller(),
                    "mana": {
                        "kind": "eachColorAmongPermanents",
                        "player": controller(),
                    },
                }],
            }),
            &[
                "Resolve tap cost",
                "Determine colors among controlled permanents",
                "Add one mana of each color",
            ],
        ));
    }

    if text == "{B}{G}: Return this card from your graveyard to the battlefield tapped." {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "graveyard",
                "costs": [{
                    "kind": "payMana",
                    "manaCost": "{B}{G}",
                }],
                "effects": [{
                    "kind": "moveAbilitySourceToBattlefield",
                    "from": "graveyard",
                    "tapped": true,
                }],
            }),
            &[
                "Resolve graveyard activation zone",
                "Resolve mana cost",
                "Return source tapped",
            ],
        ));
    }

    if text
        == "{T}, Sacrifice two creatures: Return target creature card from your graveyard to the battlefield."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature1"),
                    },
                    {
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("sacrificeCreature2"),
                    },
                ],
                "distinctTargets": [["sacrificeCreature1", "sacrificeCreature2"]],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        permanent_choice("sacrificeCreature1", card_type("Creature"), false),
                        permanent_choice("sacrificeCreature2", card_type("Creature"), false),
                        target_decision(
                            "targetCreatureCard",
                            json!({
                                "kind": "cards",
                                "zone": graveyard(controller()),
                                "where": card_type("Creature"),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "moveTargetCard",
                    "card": chosen_target("targetCreatureCard"),
                    "to": "battlefield",
                    "tapped": false,
                }],
            }),
            &[
                "Resolve tap and two-creature sacrifice costs",
                "Require distinct creatures",
                "Declare graveyard creature target",
                "Return target to battlefield",
            ],
        ));
    }

    None
}
