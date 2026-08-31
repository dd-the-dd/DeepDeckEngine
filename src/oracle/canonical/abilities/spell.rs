use super::super::*;

pub(in crate::oracle::canonical) fn parse_own_casting_reduction(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text == "This spell costs {1} less to cast for each creature on the battlefield." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone",
                    "object": self_ref(),
                    "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": {
                        "kind": "countPermanents",
                        "allPlayers": true,
                        "where": card_type("Creature"),
                    },
                }],
            }),
            &[
                "Count creatures across the battlefield",
                "Reduce the spell's generic casting cost",
            ],
        ));
    }
    let targeted_casting_reduction_re =
        Regex::new(r"(?i)^This spell costs \{(\d+)\} less to cast if it targets (?:a|an) (.+?)\.$")
            .expect("target-qualified casting reduction regex compiles");
    if let Some(captures) = targeted_casting_reduction_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone",
                    "object": self_ref(),
                    "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": integer(captures[1].parse::<i64>().ok()?),
                    "targetWhere": parse_permanent_criteria(&captures[2], "")?,
                }],
            }),
            &[
                "Match a declared permanent target against shared criteria",
                "Reduce only the spell's generic casting cost",
            ],
        ));
    }
    let conditional_casting_reduction_re =
        Regex::new(r"(?i)^This spell costs \{(\d+)\} less to cast if (.+)\.$")
            .expect("condition-qualified casting reduction regex compiles");
    if let Some(captures) = conditional_casting_reduction_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone",
                    "object": self_ref(),
                    "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": integer(captures[1].parse::<i64>().ok()?),
                    "condition": parse_condition_text(captures.get(2)?.as_str())?,
                }],
            }),
            &[
                "Parse the casting-cost condition",
                "Reduce only the spell's generic casting cost",
            ],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_common_spell_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if let Some(parsed) = parse_own_casting_reduction(text) {
        return Some(parsed);
    }
    let spell = |declaration: Option<Value>, effects: Vec<Value>| {
        let mut rule = json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": effects,
        });
        if let Some(declaration) = declaration {
            rule["declaration"] = declaration;
        }
        draft(
            rule,
            &[
                "Compose common spell primitives",
                "Declare required values and targets",
                "Resolve effects in Oracle order",
            ],
        )
    };

    let variable_creature_search_re = Regex::new(
        r"(?i)^Search your library for (?:a|one) (white|blue|black|red|green) creature card with mana value X or less, put it onto the battlefield, then shuffle\. Shuffle .+? into its owner's library\.$",
    )
    .expect("variable bounded creature-search regex compiles");
    if let Some(captures) = variable_creature_search_re.captures(text) {
        let mut parsed = spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [x_value()],
            })),
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "searchedCards",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "cards",
                        "zone": library(controller()),
                        "where": and(vec![
                            card_type("Creature"),
                            color_filter(captures.get(1)?.as_str())?,
                            compare(
                                "<=",
                                json!({
                                    "kind": "manaValueOf",
                                    "object": { "kind": "candidate" },
                                }),
                                json!({ "kind": "sourceCastXValue" }),
                            ),
                        ]),
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("searchedCards"),
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                }),
                json!({ "kind": "shuffleZone", "zone": library(controller()) }),
            ],
        );
        parsed.rule["destinationAfterResolution"] = Value::String("library".to_string());
        return Some(parsed);
    }

    let targeted_hand_disruption_re = Regex::new(
        r"(?i)^Target (player|opponent) reveals their hand\. You choose (?:a|an) (.+?) card from it\. That player discards that card\.(?: You lose (\d+) life\.)?$",
    )
    .expect("targeted hand disruption regex compiles");
    if let Some(captures) = targeted_hand_disruption_re.captures(text) {
        let target_player = chosen_target("targetPlayer");
        let mut effects = vec![
            json!({
                "kind": "revealHand",
                "player": target_player.clone(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            }),
            json!({
                "kind": "chooseCards",
                "id": "discardedCard",
                "player": controller(),
                "minimum": integer(1),
                "maximum": integer(1),
                "candidates": {
                    "kind": "cards",
                    "zone": hand(target_player.clone()),
                    "where": parse_permanent_criteria(&captures[2], "")?,
                },
            }),
            json!({
                "kind": "discardCards",
                "player": target_player.clone(),
                "cards": decision_result("discardedCard"),
            }),
        ];
        if let Some(life) = captures.get(3) {
            effects.push(json!({
                "kind": "loseLife",
                "player": controller(),
                "amount": integer(life.as_str().parse::<i64>().ok()?),
            }));
        }
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPlayer",
                    if captures[1].eq_ignore_ascii_case("opponent") {
                        json!({
                            "kind": "players",
                            "where": { "kind": "isOpponentOf", "player": controller() },
                        })
                    } else {
                        json!({ "kind": "players" })
                    },
                    1,
                    1,
                )],
            })),
            effects,
        ));
    }

    let named_hand_disruption_re = Regex::new(
        r"(?i)^Choose a (.+?) card name\. Target player reveals their hand and discards all cards with that name\.$",
    )
    .expect("named hand disruption regex compiles");
    if let Some(captures) = named_hand_disruption_re.captures(text) {
        let target_player = chosen_target("targetPlayer");
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPlayer",
                    json!({ "kind": "players" }),
                    1,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "chooseCardName",
                    "id": "chosenCardName",
                    "player": controller(),
                    "where": parse_permanent_criteria(&captures[1], "")?,
                }),
                json!({
                    "kind": "revealHand",
                    "player": target_player.clone(),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "discardNamedCardsFromHand",
                    "player": target_player,
                    "decisionId": "chosenCardName",
                }),
            ],
        ));
    }

    if text.starts_with("Return target nonland permanent to its owner's hand. If it was attacking,")
    {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPermanent",
                    json!({
                        "kind": "permanents",
                        "where": not(card_type("Land")),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "desculptingBlast",
            })],
        ));
    }

    if text
        == "Destroy target artifact or enchantment. If that permanent was blue or black, draw a card."
    {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPermanent",
                    json!({
                        "kind": "permanents",
                        "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "filigreeFracture",
            })],
        ));
    }

    if text.starts_with("Choose one or both ")
        && text.contains("Return target creature card from your graveyard to your hand.")
        && text.contains("Return target enchantment card from your graveyard to your hand.")
    {
        let mut creature_target = target_decision(
            "targetCreatureCard",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": card_type("Creature"),
            }),
            1,
            1,
        );
        creature_target["condition"] = selection("chosenModes", "creature");
        let mut enchantment_target = target_decision(
            "targetEnchantmentCard",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": card_type("Enchantment"),
            }),
            1,
            1,
        );
        enchantment_target["condition"] = selection("chosenModes", "enchantment");
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    {
                        "id": "chosenModes",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 2,
                        "options": ["creature", "enchantment"],
                    },
                    creature_target,
                    enchantment_target,
                ],
            })),
            vec![
                json!({
                    "kind": "conditional",
                    "condition": selection("chosenModes", "creature"),
                    "then": [{
                        "kind": "moveTargetCard",
                        "card": chosen_target("targetCreatureCard"),
                        "to": "hand",
                        "tapped": false,
                    }],
                }),
                json!({
                    "kind": "conditional",
                    "condition": selection("chosenModes", "enchantment"),
                    "then": [{
                        "kind": "moveTargetCard",
                        "card": chosen_target("targetEnchantmentCard"),
                        "to": "hand",
                        "tapped": false,
                    }],
                }),
            ],
        ));
    }

    if text.starts_with("Choose one or both ")
        && text.contains("Search your library for an artifact card")
        && text.contains("Return target artifact card from your graveyard to your hand.")
    {
        let mut graveyard_target = target_decision(
            "targetArtifactCard",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": card_type("Artifact"),
            }),
            1,
            1,
        );
        graveyard_target["condition"] = selection("chosenModes", "graveyard");
        let mut search_effects = search_library_effects(card_type("Artifact"), 1, "hand", false);
        search_effects.insert(
            1,
            json!({ "kind": "revealCards", "cards": decision_result("searchedCards") }),
        );
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    {
                        "id": "chosenModes",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 2,
                        "options": ["library", "graveyard"],
                    },
                    graveyard_target,
                ],
            })),
            vec![
                json!({
                    "kind": "conditional",
                    "condition": selection("chosenModes", "library"),
                    "then": search_effects,
                }),
                json!({
                    "kind": "conditional",
                    "condition": selection("chosenModes", "graveyard"),
                    "then": [{
                        "kind": "moveTargetCard",
                        "card": chosen_target("targetArtifactCard"),
                        "to": "hand",
                        "tapped": false,
                    }],
                }),
            ],
        ));
    }

    if text
        == "Create a token that's a copy of target non-Aura permanent you control, except it's a 0/0 Fractal creature in addition to its other types. Put six +1/+1 counters on it."
    {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "copyTarget",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": not(subtype("Aura")),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "createModifiedTokenCopy",
                "object": chosen_target("copyTarget"),
                "basePower": integer(0),
                "baseToughness": integer(0),
                "addTypes": ["Creature"],
                "addSubtypes": ["Fractal"],
                "counters": [{
                    "counter": "+1/+1",
                    "count": integer(6),
                }],
            })],
        ));
    }
    let text_without_reminder = text
        .split_once(" (Damage and effects")
        .map(|(instruction, _)| instruction)
        .unwrap_or(text);
    if text_without_reminder
        == "Put a +1/+1 counter on target creature you control. It gains reach, trample, and indestructible until end of turn. Untap it."
    {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
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
                    "kind": "putCounters",
                    "permanent": chosen_target("targetCreature"),
                    "counter": "+1/+1",
                    "count": integer(1),
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "reach",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "trample",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "indestructible",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "untapPermanent",
                    "permanent": chosen_target("targetCreature"),
                }),
            ],
        ));
    }
    if text
        == "Target creature you control gets +X/+X and gains hexproof and indestructible until end of turn. (It can't be the target of spells or abilities your opponents control. Damage and effects that say \"destroy\" don't destroy it.)"
    {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    {
                        "id": "xValue",
                        "kind": "chooseNumber",
                        "minimum": integer(0),
                    },
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
                ],
            })),
            vec![
                json!({
                    "kind": "modifyPowerToughness",
                    "object": chosen_target("targetCreature"),
                    "power": decision_result("xValue"),
                    "toughness": decision_result("xValue"),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "hexproof",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "indestructible",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
        ));
    }
    if text == "Target creature you control fights up to one target creature you don't control." {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    target_decision(
                        "friendlyCreature",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    ),
                    target_decision(
                        "opposingCreature",
                        json!({
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": controller(),
                            },
                            "where": card_type("Creature"),
                        }),
                        0,
                        1,
                    ),
                ],
            })),
            vec![json!({
                "kind": "fightPermanents",
                "first": chosen_target("friendlyCreature"),
                "second": chosen_target("opposingCreature"),
            })],
        ));
    }

    if text == "Draw two cards. Exile Inspiring Refrain with three time counters on it." {
        let mut result = spell(
            None,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(2),
            })],
        );
        result.rule["suspendAfterResolution"] = integer(3);
        return Some(result);
    }
    if text == "Each player chooses a creature they control. Destroy the rest." {
        return Some(spell(
            None,
            vec![json!({ "kind": "keepOneCreatureDestroyRest" })],
        ));
    }
    if text.starts_with("Exile target creature and put two time counters on it.") {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "suspendPermanent",
                "permanent": chosen_target("targetCreature"),
                "timeCounters": integer(2),
            })],
        ));
    }
    if text.starts_with(
        "Create X Map tokens, where X is one plus the number of opponents who control an artifact.",
    ) {
        return Some(spell(
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": {
                    "kind": "add",
                    "left": integer(1),
                    "right": {
                        "kind": "countOpponentsWithPermanent",
                        "player": controller(),
                        "where": card_type("Artifact"),
                    },
                },
                "token": { "kind": "namedToken", "name": "Map" },
            })],
        ));
    }
    if text
        == "Until your next turn, creatures can't attack you. Exile Chronomantic Escape with three time counters on it."
    {
        let mut result = spell(
            None,
            vec![json!({
                "kind": "preventAttacksUntilNextTurn",
                "player": controller(),
            })],
        );
        result.rule["suspendAfterResolution"] = integer(3);
        return Some(result);
    }
    if text
        == "Exile cards from the top of your library until you exile a land card. Put that card onto the battlefield and the rest on the bottom of your library in a random order. Exile Venture Forth with three time counters on it."
    {
        let mut result = spell(None, vec![json!({ "kind": "ventureForth" })]);
        result.rule["suspendAfterResolution"] = integer(3);
        return Some(result);
    }

    if matches!(
        text,
        "Create a token that's a copy of target creature you control, except it isn't legendary."
            | "Create a token that's a copy of target artifact or creature."
    ) {
        let controlled_only = text.contains("creature you control");
        let mut candidates = json!({
            "kind": "permanents",
            "where": if controlled_only {
                card_type("Creature")
            } else {
                or(vec![card_type("Artifact"), card_type("Creature")])
            },
        });
        if controlled_only {
            candidates["controller"] = controller();
        }
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision("copyTarget", candidates, 1, 1)],
            })),
            vec![json!({
                "kind": "createModifiedTokenCopy",
                "object": chosen_target("copyTarget"),
                "removeLegendary": controlled_only,
            })],
        ));
    }

    if text
        == "Exile any number of target creatures and/or planeswalkers you control. At the beginning of the next end step, return each of them to the battlefield under its owner's control. Each of them enters with an additional +1/+1 counter on it if it's a creature and an additional loyalty counter on it if it's a planeswalker."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "blinkTargets",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                        }),
                        0,
                        64,
                    )],
                },
                "effects": [{
                    "kind": "exileUntilNextEndStep",
                    "objects": {
                        "kind": "chosenTargets",
                        "id": "blinkTargets",
                    },
                    "returnUnderOwnerControl": true,
                    "creatureCounter": "+1/+1",
                    "planeswalkerCounter": "loyalty",
                }],
            }),
            &[
                "Declare any number of controlled creature or planeswalker targets",
                "Exile the legal targets together",
                "Register their next-end-step owner returns",
                "Apply the appropriate additional entering counter",
            ],
        ));
    }

    if matches!(
        text,
        "Search your library for a Forest card, put it onto the battlefield, then shuffle."
            | "Search your library for a Forest card, put that card onto the battlefield, then shuffle."
    ) {
        return Some(spell(
            None,
            search_library_effects(subtype("Forest"), 1, "battlefield", false),
        ));
    }
    if text
        == "Search your library for a creature card, reveal that card, put it into your hand, then shuffle."
    {
        return Some(spell(
            None,
            search_library_effects(card_type("Creature"), 1, "hand", false),
        ));
    }
    if text
        == "Search your library for up to ten land cards, put them onto the battlefield tapped, then shuffle."
    {
        return Some(spell(
            None,
            search_library_effects(card_type("Land"), 10, "battlefield", true),
        ));
    }
    if text
        == "Sacrifice a land. Search your library for up to two basic land cards, put them onto the battlefield tapped, then shuffle."
    {
        let mut effects = vec![json!({
            "kind": "sacrificePermanents",
            "player": controller(),
            "where": card_type("Land"),
            "count": integer(1),
        })];
        effects.extend(search_library_effects(
            json!({ "kind": "typeLineContains", "value": "Basic Land" }),
            2,
            "battlefield",
            true,
        ));
        return Some(spell(None, effects));
    }
    let simple_damage_re = Regex::new(r"^[A-Za-z' ]+ deals (\d+) damage to any target\.$")
        .expect("common any-target damage regex compiles");
    if let Some(captures) = simple_damage_re.captures(text) {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "damageTarget",
                    json!({ "kind": "anyTarget" }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "dealDamage",
                "recipient": chosen_target("damageTarget"),
                "amount": integer(captures[1].parse::<i64>().ok()?),
            })],
        ));
    }
    if text
        == "Destroy target creature. Search your library for a basic land card, put it onto the battlefield tapped, then shuffle."
    {
        let mut effects = vec![json!({
            "kind": "destroyPermanent",
            "permanent": chosen_target("targetCreature"),
        })];
        effects.extend(search_library_effects(
            json!({ "kind": "typeLineContains", "value": "Basic Land" }),
            1,
            "battlefield",
            true,
        ));
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            })),
            effects,
        ));
    }
    if text == "You gain X life and draw X cards." {
        return Some(spell(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "xValue",
                    "kind": "chooseNumber",
                    "minimum": integer(1),
                }],
            })),
            vec![
                json!({
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": decision_result("xValue"),
                }),
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": decision_result("xValue"),
                }),
            ],
        ));
    }
    if text
        == "Chain Reaction deals X damage to each creature, where X is the number of creatures on the battlefield."
    {
        let amount = json!({
            "kind": "countPermanents",
            "allPlayers": true,
            "where": card_type("Creature"),
        });
        return Some(spell(
            None,
            vec![json!({
                "kind": "dealDamage",
                "recipient": {
                    "kind": "eachPermanent",
                    "where": card_type("Creature"),
                },
                "amount": amount,
            })],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_composed_spell(text: &str) -> Option<CanonicalRuleDraft> {
    let inevitable_re = Regex::new(
        r"^Exile target nonland permanent\. Its controller loses (\d+) life and you gain (\d+) life\.$",
    )
    .expect("linked exile life regex compiles");
    if let Some(captures) = inevitable_re.captures(text) {
        let loss = captures[1].parse::<i64>().ok()?;
        let gain = captures[2].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetPermanent",
                            json!({
                                "kind": "permanents",
                                "where": not(card_type("Land")),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "bind",
                        "id": "targetController",
                        "value": {
                            "kind": "controllerOf",
                            "object": chosen_target("targetPermanent"),
                        },
                    },
                    {
                        "kind": "exilePermanent",
                        "permanent": chosen_target("targetPermanent"),
                    },
                    {
                        "kind": "loseLife",
                        "player": {
                            "kind": "boundValue",
                            "id": "targetController",
                        },
                        "amount": integer(loss),
                    },
                    {
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": integer(gain),
                    },
                ],
            }),
            &[
                "Extract required nonland-permanent target",
                "Bind target controller before zone change",
                "Resolve exile vocabulary",
                "Apply linked life loss and gain",
            ],
        ));
    }

    if text.starts_with(
        "Target instant or sorcery card in your graveyard gains flashback until end of turn.",
    ) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetCard",
                            json!({
                                "kind": "cards",
                                "zone": graveyard(controller()),
                                "where": or(vec![
                                    card_type("Instant"),
                                    card_type("Sorcery"),
                                ]),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "grantAbility",
                    "object": chosen_target("targetCard"),
                    "ability": {
                        "kind": "flashback",
                        "cost": {
                            "kind": "manaCostOf",
                            "card": { "kind": "abilitySource" },
                        },
                    },
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Extract graveyard instant-or-sorcery target",
                "Recognize granted flashback vocabulary",
                "Bind flashback cost to granted ability source",
                "Attach end-of-turn duration",
            ],
        ));
    }

    if text.starts_with("Return target spell or permanent to its owner's hand.") {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetToReturn",
                            json!({
                                "kind": "union",
                                "sets": [
                                    { "kind": "spells" },
                                    { "kind": "permanents" },
                                ],
                            }),
                            1,
                            1,
                        ),
                        target_decision(
                            "targetForDamage",
                            json!({ "kind": "anyTarget" }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "returnToOwnersHand",
                        "object": chosen_target("targetToReturn"),
                    },
                    {
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "amount": integer(4),
                        "recipient": chosen_target("targetForDamage"),
                    },
                    {
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": 2,
                        "token": {
                            "colors": ["White"],
                            "types": ["Creature"],
                            "subtypes": ["Monk"],
                            "power": 1,
                            "toughness": 1,
                            "abilities": [{ "kind": "prowess" }],
                        },
                    },
                    {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": 2,
                    },
                    {
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": integer(4),
                    },
                ],
            }),
            &[
                "Extract independent return and damage targets",
                "Resolve return-to-owner vocabulary",
                "Resolve damage vocabulary",
                "Resolve Monk token specification",
                "Order draw and life-gain effects",
            ],
        ));
    }

    if text == "Target opponent exiles a creature they control and their graveyard." {
        let opponent = chosen_target("targetOpponent");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetOpponent",
                            json!({
                                "kind": "players",
                                "where": {
                                    "kind": "isOpponentOf",
                                    "player": controller(),
                                },
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "choosePermanents",
                        "id": "creatureToExile",
                        "player": opponent.clone(),
                        "minimum": minimum(vec![
                            integer(1),
                            json!({
                                "kind": "countPermanents",
                                "player": opponent.clone(),
                                "where": card_type("Creature"),
                            }),
                        ]),
                        "maximum": 1,
                        "candidates": {
                            "kind": "permanents",
                            "controller": opponent.clone(),
                            "where": card_type("Creature"),
                        },
                    },
                    {
                        "kind": "exileTogether",
                        "objects": {
                            "kind": "union",
                            "sets": [
                                decision_result("creatureToExile"),
                                {
                                    "kind": "cardsInZone",
                                    "zone": graveyard(opponent),
                                },
                            ],
                        },
                    },
                ],
            }),
            &[
                "Extract required opponent target",
                "Assign resolution choice to targeted opponent",
                "Cap creature choice by available permanents",
                "Union chosen creature with graveyard",
                "Resolve simultaneous exile instruction",
            ],
        ));
    }

    if text.starts_with("Draw two cards. Then you may discard two cards. When you do,") {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": 2,
                    },
                    {
                        "kind": "optionalAction",
                        "player": controller(),
                        "action": {
                            "kind": "discardCards",
                            "player": controller(),
                            "count": 2,
                            "bind": "discardedCards",
                        },
                        "onPerformed": [{
                            "kind": "createReflexiveTrigger",
                            "source": self_ref(),
                            "controller": controller(),
                            "effects": [{
                                "kind": "dealDamage",
                                "source": self_ref(),
                                "amount": {
                                    "kind": "maximumManaValue",
                                    "collection": bound_objects("discardedCards"),
                                    "variableManaSymbolsEqual": 0,
                                },
                                "recipient": {
                                    "kind": "eachPermanent",
                                    "where": card_type("Creature"),
                                },
                            }],
                        }],
                    },
                ],
            }),
            &[
                "Resolve initial draw vocabulary",
                "Create optional exact-two discard action",
                "Bind discarded-card event objects",
                "Create reflexive trigger on performed action",
                "Reduce greatest-mana-value damage expression",
            ],
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_ancient_vendetta(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let targeted_name_search_re = Regex::new(
        r"^Choose target card in a graveyard other than (?:a )?(.+?) card\. Search its owner's graveyard, hand, and library for any number of cards with the same name as that card and exile them\. Then that player shuffles\.$",
    )
    .expect("targeted same-name multi-zone search regex compiles");
    if let Some(captures) = targeted_name_search_re.captures(text) {
        let excluded = parse_permanent_criteria(&captures[1], "")?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetNamedCard",
                        json!({
                            "kind": "cards",
                            "zone": { "kind": "anyGraveyard" },
                            "where": not(excluded),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "exileNamedCardsFromZones",
                    "player": chosen_target("targetNamedCard"),
                    "chooser": controller(),
                    "nameFromCard": chosen_target("targetNamedCard"),
                    "zones": ["graveyard", "hand", "library"],
                    "shuffleLibrary": true,
                }],
            }),
            &[
                "Target a card in any graveyard",
                "Exclude cards matching the stated criterion",
                "Bind the target card's owner and name",
                "Search and exile every same-name card across the stated zones",
                "Shuffle the searched library",
            ],
        ));
    }
    if text
        != "Choose a card name. Search target opponent's graveyard, hand, and library for up to four cards with that name and exile them. Then that player shuffles."
    {
        return None;
    }
    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [
                    target_decision(
                        "targetOpponent",
                        json!({
                            "kind": "players",
                            "where": {
                                "kind": "isOpponentOf",
                                "player": controller(),
                            },
                        }),
                        1,
                        1,
                    ),
                ],
            },
            "effects": [
                {
                    "kind": "chooseCardName",
                    "id": "chosenCardName",
                    "player": controller(),
                    "where": Value::Null,
                },
                {
                    "kind": "exileNamedCardsFromZones",
                    "player": chosen_target("targetOpponent"),
                    "chooser": controller(),
                    "decisionId": "chosenCardName",
                    "maximum": integer(4),
                    "zones": ["graveyard", "hand", "library"],
                    "shuffleLibrary": true,
                },
            ],
        }),
        &[
            "Declare target opponent",
            "Choose a card name",
            "Search named cards across specified zones",
            "Exile up to four matches",
            "Shuffle searched library",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_common_zone_and_value_spell(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    if let Some((effects, decisions)) = parse_conditional_effect_amendment(text, face_name) {
        let mut rule = json!({
            "kind": "spellAbility",
            "source": self_ref(),
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
                "Strip the reusable ability-word label",
                "Parse the added condition through the condition grammar",
                "Compose the added effect with its linked object reference",
            ],
        ));
    }
    if text
        == "Shuffle your library. Then exile the top card of your library. Until end of turn, you may play that card without paying its mana cost."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "mindDesireExileTopFree",
                }],
            }),
            &[
                "Shuffle the controller's library",
                "Exile the top card",
                "Grant a free play permission this turn",
            ],
        ));
    }
    if text.ends_with(
        "Draw a card for each spell you've cast this turn from anywhere other than your hand.",
    ) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "drawForSpellsCastOutsideHandThisTurn",
                }],
            }),
            &[
                "Count this turn's spells cast outside hand",
                "Draw that many cards",
            ],
        ));
    }
    if text
        == "Search your library for a basic land card, put it onto the battlefield, then shuffle."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": search_library_effects(
                    json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                    1,
                    "battlefield",
                    false,
                ),
            }),
            &[
                "Choose a basic land from the library",
                "Move it to the battlefield untapped",
                "Shuffle the searched library",
            ],
        ));
    }
    let search_filter = match text {
        "Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle."
        | "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle." => {
            Some(json!({ "kind": "typeLineContains", "value": "Basic Land" }))
        }
        "Search your library for a Plains, Island, Swamp, or Mountain card, put it onto the battlefield tapped, then shuffle." => {
            Some(or(vec![
                subtype("Plains"),
                subtype("Island"),
                subtype("Swamp"),
                subtype("Mountain"),
            ]))
        }
        _ => None,
    };
    if let Some(filter) = search_filter {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": search_library_effects(filter, 1, "battlefield", true),
            }),
            &[
                "Choose a matching library card",
                "Move it to the battlefield tapped",
                "Shuffle the searched library",
            ],
        ));
    }

    let mandatory_sacrifice_filter = match text {
        "As an additional cost to cast this spell, sacrifice a creature." => {
            Some(card_type("Creature"))
        }
        "As an additional cost to cast this spell, sacrifice an artifact or creature." => {
            Some(or(vec![card_type("Artifact"), card_type("Creature")]))
        }
        "As an additional cost to cast this spell, sacrifice a nonland permanent." => {
            Some(not(card_type("Land")))
        }
        _ => None,
    };
    if let Some(filter) = mandatory_sacrifice_filter {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "additionalCostPermanent",
                            json!({
                                "kind": "permanents",
                                "controller": controller(),
                                "where": filter,
                            }),
                            1,
                            1,
                        ),
                    ],
                    "additionalCosts": [{
                        "kind": "sacrificePermanent",
                        "permanent": chosen_target("additionalCostPermanent"),
                    }],
                },
                "effects": [],
            }),
            &[
                "Recognize mandatory additional cost",
                "Declare controlled permanent choice",
                "Attach sacrifice payment",
            ],
        ));
    }

    if text == "As an additional cost to cast this spell, sacrifice a creature or pay {3}{B}." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        {
                            "id": "additionalCostMode",
                            "kind": "chooseModes",
                            "minimum": 1,
                            "maximum": 1,
                            "options": ["sacrifice", "payMana"],
                        },
                        {
                            "id": "additionalCostPermanent",
                            "kind": "chooseTargets",
                            "condition": {
                                "kind": "selectionContains",
                                "selection": decision_result("additionalCostMode"),
                                "value": "sacrifice",
                            },
                            "minimum": 1,
                            "maximum": 1,
                            "candidates": {
                                "kind": "permanents",
                                "controller": controller(),
                                "where": card_type("Creature"),
                            },
                        },
                    ],
                    "additionalCosts": [
                        {
                            "kind": "conditional",
                            "condition": {
                                "kind": "selectionContains",
                                "selection": decision_result("additionalCostMode"),
                                "value": "sacrifice",
                            },
                            "then": [{
                                "kind": "sacrificePermanent",
                                "permanent": chosen_target("additionalCostPermanent"),
                            }],
                        },
                        {
                            "kind": "conditional",
                            "condition": {
                                "kind": "selectionContains",
                                "selection": decision_result("additionalCostMode"),
                                "value": "payMana",
                            },
                            "then": [{
                                "kind": "payMana",
                                "manaCost": "{3}{B}",
                            }],
                        },
                    ],
                },
                "effects": [],
            }),
            &[
                "Declare alternative additional cost",
                "Condition sacrifice choice",
                "Attach sacrifice or mana payment",
            ],
        ));
    }

    let untargeted_effects = match text {
        "Draw two cards." => Some(vec![json!({
            "kind": "drawCards",
            "player": controller(),
            "count": integer(2),
        })]),
        "You draw a card and lose 1 life." => Some(vec![
            json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            }),
            json!({
                "kind": "loseLife",
                "player": controller(),
                "amount": integer(1),
            }),
        ]),
        "Draw two cards and create a Map token. (It's an artifact with \"{1}, {T}, Sacrifice this token: Target creature you control explores. Activate only as a sorcery.\")" => {
            Some(vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(2),
                }),
                json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": {
                        "kind": "namedToken",
                        "name": "Map",
                    },
                }),
            ])
        }
        "All creatures get -2/-2 until end of turn." => Some(vec![json!({
            "kind": "modifyPowerToughness",
            "object": {
                "kind": "eachPermanent",
                "where": card_type("Creature"),
            },
            "power": integer(-2),
            "toughness": integer(-2),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        })]),
        _ => None,
    };
    if let Some(effects) = untargeted_effects {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": effects,
            }),
            &[
                "Recognize ordered common effects",
                "Resolve controller references",
            ],
        ));
    }

    let target_player_life =
        Regex::new(r"^Target player gains (\d+) life\.$").expect("target life regex compiles");
    if let Some(captures) = target_player_life.captures(text) {
        let amount = captures[1].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("targetPlayer", json!({ "kind": "players" }), 1, 1),
                    ],
                },
                "effects": [{
                    "kind": "gainLife",
                    "player": chosen_target("targetPlayer"),
                    "amount": integer(amount),
                }],
            }),
            &["Declare player target", "Apply life gain"],
        ));
    }

    let (filter, trailing_life) = match text {
        "Destroy target artifact." => (Some(card_type("Artifact")), 0),
        "Destroy target nonland permanent." => (Some(not(card_type("Land"))), 0),
        "Destroy target enchantment." => (Some(card_type("Enchantment")), 0),
        "Destroy target artifact or enchantment." => (
            Some(or(vec![card_type("Artifact"), card_type("Enchantment")])),
            0,
        ),
        "Destroy target artifact or creature. You gain 1 life." => (
            Some(or(vec![card_type("Artifact"), card_type("Creature")])),
            1,
        ),
        _ => (None, 0),
    };
    if let Some(filter) = filter {
        let mut effects = vec![json!({
            "kind": "destroyPermanent",
            "permanent": chosen_target("targetPermanent"),
        })];
        if trailing_life > 0 {
            effects.push(json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": integer(trailing_life),
            }));
        }
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetPermanent",
                            json!({ "kind": "permanents", "where": filter }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": effects,
            }),
            &[
                "Declare filtered permanent target",
                "Destroy target",
                "Apply trailing effects",
            ],
        ));
    }

    if let Some((effects, decisions)) = parse_mana_value_guarded_destroy_instruction(text, "") {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                },
                "effects": effects,
            }),
            &[
                "Declare criteria permanent target",
                "Check mana value",
                "Destroy target",
            ],
        ));
    }

    let destroy_target_re =
        Regex::new(r"(?i)^Destroy target (.+?)(\. It can't be regenerated)?\.$")
            .expect("generic destroy target spell regex compiles");
    if let Some(captures) = destroy_target_re.captures(text) {
        let cannot_regenerate = captures.get(2).is_some();
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetPermanent",
                            json!({
                                "kind": "permanents",
                                "where": parse_permanent_criteria(&captures[1], "")?,
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetPermanent"),
                    "cannotRegenerate": cannot_regenerate,
                }],
            }),
            &["Declare criteria permanent target", "Destroy target"],
        ));
    }

    let exile_target_re = Regex::new(r"(?i)^Exile target (.+)\.$")
        .expect("generic exile target spell regex compiles");
    if let Some(captures) = exile_target_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetPermanent",
                            json!({
                                "kind": "permanents",
                                "where": parse_permanent_criteria(&captures[1], "")?,
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "exilePermanent",
                    "permanent": chosen_target("targetPermanent"),
                }],
            }),
            &["Declare criteria permanent target", "Exile target"],
        ));
    }

    if text == "Target player draws two cards and loses 2 life." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("targetPlayer", json!({ "kind": "players" }), 1, 1),
                    ],
                },
                "effects": [
                    {
                        "kind": "drawCards",
                        "player": chosen_target("targetPlayer"),
                        "count": integer(2),
                    },
                    {
                        "kind": "loseLife",
                        "player": chosen_target("targetPlayer"),
                        "amount": integer(2),
                    },
                ],
            }),
            &["Declare player target", "Draw cards", "Lose life"],
        ));
    }

    let graveyard_move = match text {
        "Return target creature card from your graveyard to your hand." => Some(("hand", false)),
        "Return target creature card from your graveyard to the battlefield." => {
            Some(("battlefield", false))
        }
        "Return target creature card from your graveyard to the battlefield tapped." => {
            Some(("battlefield", true))
        }
        _ => None,
    };
    if let Some((destination, tapped)) = graveyard_move {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
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
                    "to": destination,
                    "tapped": tapped,
                }],
            }),
            &["Declare graveyard card target", "Move target card"],
        ));
    }

    if text
        == "Target creature gets -2/-2 until end of turn. If this spell was kicked, that creature gets -5/-5 until end of turn instead."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetCreature",
                            json!({
                                "kind": "permanents",
                                "where": card_type("Creature"),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": chosen_target("targetCreature"),
                    "power": {
                        "kind": "conditionalValue",
                        "condition": {
                            "kind": "wasKicked",
                            "spell": self_ref(),
                        },
                        "ifTrue": integer(-5),
                        "ifFalse": integer(-2),
                    },
                    "toughness": {
                        "kind": "conditionalValue",
                        "condition": {
                            "kind": "wasKicked",
                            "spell": self_ref(),
                        },
                        "ifTrue": integer(-5),
                        "ifFalse": integer(-2),
                    },
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Declare creature target",
                "Branch modifier on kicked state",
                "Apply temporary modifier",
            ],
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_avatar_casting_instruction(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text
        == "This spell costs {2} less to cast if a creature left the battlefield under your control this turn."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone",
                    "object": self_ref(),
                    "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": integer(2),
                    "condition": {
                        "kind": "controlledPermanentLeftThisTurn",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                }],
            }),
            &[
                "Check whether a controlled creature left this turn",
                "Reduce generic cost by two",
            ],
        ));
    }
    if text == "If you control a commander, you may cast this spell without paying its mana cost." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "castingInstruction": {
                    "kind": "freeIfControlCommander",
                },
                "effects": [],
            }),
            &[
                "Check for a commander controlled by the caster",
                "Offer the spell without paying its mana cost",
            ],
        ));
    }
    if text
        != "As an additional cost to cast this spell, sacrifice half the lands you control, rounded up."
    {
        return None;
    }
    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "castingInstruction": {
                "kind": "sacrificeHalfControlledLands",
            },
            "effects": [],
        }),
        &[
            "Count lands controlled as the spell is cast",
            "Round half the count up",
            "Sacrifice exactly that many lands as an additional cost",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_avatar_deck_spell(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let spell = |operation: &str, declaration: Option<Value>| {
        let mut rule = json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [{
                "kind": "resolveSpellInstruction",
                "operation": operation,
            }],
        });
        if let Some(declaration) = declaration {
            rule["declaration"] = declaration;
        }
        draft(
            rule,
            &[
                "Normalize an Avatar deck spell",
                "Declare variable values, modes, and targets",
                "Resolve the complete ordered instruction",
            ],
        )
    };
    let declaration = |decisions: Vec<Value>| {
        json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        })
    };
    let x_value = || {
        json!({
            "id": "xValue",
            "kind": "chooseNumber",
            "minimum": 0,
        })
    };
    let target_player = |id: &str, opponent_only: bool| {
        let mut candidates = json!({ "kind": "players" });
        if opponent_only {
            candidates["where"] = json!({
                "kind": "isOpponentOf",
                "player": controller(),
            });
        }
        target_decision(id, candidates, 1, 1)
    };
    let controlled_creature = |id: &str, minimum: i64| {
        target_decision(
            id,
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            minimum,
            1,
        )
    };
    let controlled_land = |id: &str| {
        target_decision(
            id,
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Land"),
            }),
            1,
            1,
        )
    };

    let opponent_color_alternative_cost_re = Regex::new(
        r"^If an opponent cast a (white|blue|black|red|green) spell this turn, you may pay ((?:\{[^}]+\})+) rather than pay this spell's mana cost",
    )
    .expect("opponent color alternative cost regex compiles");
    if let Some(captures) = opponent_color_alternative_cost_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "conditionalAlternativeCost": {
                    "opponentCastColor": captures[1].to_ascii_lowercase(),
                    "manaCost": captures[2].to_string(),
                },
            }),
            &[
                "Inspect opponent spells cast this turn",
                "Offer the color-conditional alternative cost",
            ],
        ));
    }

    match text {
        value if value.starts_with("Gain control of target spell that targets only a single permanent or player") => {
            return Some(spell("chefsKiss", None));
        }
        value if value.starts_with("Goad all creatures you don't control") => {
            return Some(spell("disruptDecorum", None));
        }
        value if value.starts_with("Search your library for an artifact card, reveal it, put it into your hand, shuffle, then discard a card at random") => {
            return Some(spell("recklessHandling", None));
        }
        "Change the target of target spell with a single target." => {
            return Some(spell("ricochetTrapChangeTarget", None));
        }
        value if value.starts_with("Target creature you control deals damage equal to its power to each other creature") => {
            return Some(spell("waltzOfRage", Some(declaration(vec![controlled_creature("targetCreature", 1)]))));
        }
        value if value.starts_with("You may choose new targets for target instant or sorcery spell. Then copy that spell") => {
            return Some(spell("wildRicochet", None));
        }
        value if value.starts_with("Choose target spell or ability with one or more targets. Roll a d20") => {
            return Some(spell("wyllsReversalRoll", None));
        }
        value if value.contains("1â€”14") || value.contains("1—14") => {
            return Some(spell("wyllsReversalRetarget", None));
        }
        value if value.contains("15+") && value.contains("Then copy it") => {
            return Some(spell("wyllsReversalCopy", None));
        }
        "Until your next turn, your life total can't change and you gain protection from everything. All permanents you control phase out. (While they're phased out, they're treated as though they don't exist. They phase in before you untap during your untap step.)" => {
            return Some(spell("aangsShelter", None));
        }
        value if value.contains("Destroy target attacking creature") && value.contains("Airbend target creature you control") => {
            return Some(spell(
                "airbendersReversal",
                Some(declaration(vec![
                    json!({
                        "id": "spellMode",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 1,
                        "options": ["destroyAttacker", "airbendCreature"],
                    }),
                ])),
            ));
        }
        "Choose up to one target creature, then airbend all other creatures. (Exile them. While each one is exiled, its owner may cast it for {2} rather than its mana cost.)" => {
            return Some(spell("avatarsWrathAirbend", None));
        }
        "Until your next turn, your opponents can't cast spells from anywhere other than their hands." => {
            return Some(spell("avatarsWrathCastLock", None));
        }
        "Choose up to two target permanent cards in your graveyard that were put there from the battlefield this turn. Return them to the battlefield tapped." => {
            return Some(spell("broughtBack", None));
        }
        "Target opponent skips all combat phases of their next turn." => {
            return Some(spell("emptyCityRuse", Some(declaration(vec![target_player("targetOpponent", true)]))));
        }
        "Lands you control gain all basic land types until end of turn." => {
            return Some(spell("energybending", None));
        }
        "Search your library for an artifact or enchantment card, reveal it, then shuffle and put that card on top." => {
            return Some(spell("enlightenedTutor", None));
        }
        "Until end of turn, target creature you control becomes an Avatar in addition to its other types and gains flying, first strike, lifelink, and hexproof. (A creature with hexproof can't be the target of spells or abilities your opponents control.)" => {
            return Some(spell("enterAvatarState", Some(declaration(vec![controlled_creature("targetCreature", 1)]))));
        }
        "Reckless Blaze deals 5 damage to each creature. Whenever a creature you control dealt damage this way dies this turn, add {R}." => {
            return Some(spell("recklessBlaze", None));
        }
        value if value.starts_with("Buyback {4}") => {
            return Some(draft(
                json!({
                    "kind": "spellAbility",
                    "source": self_ref(),
                    "declaration": {
                        "kind": "castingDeclaration",
                        "decisions": [{
                            "id": "buybackMode",
                            "kind": "chooseModes",
                            "minimum": 1,
                            "maximum": 1,
                            "options": ["decline", "buyback"],
                        }],
                        "additionalCosts": [{
                            "kind": "conditional",
                            "condition": selection("buybackMode", "buyback"),
                            "then": [{ "kind": "payMana", "manaCost": "{4}" }],
                        }],
                    },
                    "effects": [{
                        "kind": "resolveSpellInstruction",
                        "operation": "searingTouchBuyback",
                    }],
                }),
                &["Offer the buyback choice", "Pay four additional mana", "Return the resolving spell to hand"],
            ));
        }
        value if value.starts_with("Cascade (") => {
            return Some(spell("volcanicTorrentCascade", None));
        }
        "Volcanic Torrent deals X damage to each creature and planeswalker your opponents control, where X is the number of spells you've cast this turn." => {
            return Some(spell("volcanicTorrentDamage", None));
        }
        _ => {}
    }

    if text.contains("greatest power among non-Human creatures you control")
        && text.contains("Non-Human creatures you control get +3/+3")
    {
        return Some(spell("earthRumbleTriumph", None));
    }
    if text.contains("two or more instant and/or sorcery cards in your graveyard")
        && text.contains("untap those lands")
    {
        return Some(spell("animistsAwakeningSpellMastery", None));
    }
    if text.contains("Destroy target artifact or enchantment")
        && text.contains("It gains indestructible until end of turn")
    {
        return Some(spell("originOfMetalbending", None));
    }
    if text.contains("Search your library for a creature or land card")
        && text.contains("Exile target artifact or enchantment")
    {
        return Some(spell("archdruidsCharm", None));
    }
    if text.contains("Target player draws X cards")
        && text.contains("Target player mills twice X cards")
    {
        return Some(spell("drownInDreams", Some(declaration(vec![x_value()]))));
    }
    if text.contains("Invoke the Firemind deals X damage to any target") {
        return Some(spell(
            "invokeTheFiremind",
            Some(declaration(vec![x_value()])),
        ));
    }
    if text.contains("Breathe Flame") && text.contains("Smash Relics") {
        return Some(spell("klauthsWill", Some(declaration(vec![x_value()]))));
    }
    if text.contains("Counter target spell")
        && text.contains("Tap all creatures your opponents control")
    {
        return Some(spell("toTheCrystalTower", None));
    }
    if text.starts_with("Search your library for up to four land cards with different names") {
        return Some(spell("elementalTeachings", None));
    }
    if text == "For each of X target permanents, create X tokens that are copies of that permanent."
    {
        return Some(spell("doppelgang", Some(declaration(vec![x_value()]))));
    }
    if text
        == "Copy target instant or sorcery spell with mana value 4 or less. You may choose new targets for the copy."
    {
        return Some(spell(
            "expansion",
            Some(declaration(vec![target_decision(
                "targetSpell",
                json!({
                    "kind": "spells",
                    "where": {
                        "kind": "compare",
                        "operator": "<=",
                        "left": {
                            "kind": "manaValueOf",
                            "object": { "kind": "candidate" },
                        },
                        "right": integer(4),
                    },
                }),
                1,
                1,
            )])),
        ));
    }
    if text == "Explosion deals X damage to any target. Target player draws X cards." {
        return Some(spell(
            "explosion",
            Some(declaration(vec![
                x_value(),
                target_decision("damageTarget", json!({ "kind": "anyTarget" }), 1, 1),
                target_player("drawTarget", false),
            ])),
        ));
    }
    if text
        == "Prevent all damage that would be dealt this turn by creatures your opponents control."
    {
        return Some(spell("obscuringHaze", None));
    }
    if text
        == "Return target nonland permanent to its owner's hand. If you controlled that permanent, draw a card."
    {
        return Some(spell(
            "boomerangBasics",
            Some(declaration(vec![target_decision(
                "targetPermanent",
                json!({ "kind": "permanents", "where": not(card_type("Land")) }),
                1,
                1,
            )])),
        ));
    }
    if text == "Untap one or two target creatures. They each get +2/+2 until end of turn." {
        return Some(spell(
            "fancyFootwork",
            Some(declaration(vec![target_decision(
                "targetCreatures",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                2,
            )])),
        ));
    }
    if text.starts_with("Choose a creature type. Look at the top six cards of your library.") {
        return Some(spell("forTheAncestors", None));
    }
    if text.starts_with("The next time a source of your choice would deal damage to you this turn")
    {
        return Some(spell("interventionPact", None));
    }
    if text == "Counter up to four target spells and/or abilities." {
        return Some(spell(
            "katarasReversalCounter",
            Some(declaration(vec![target_decision(
                "stackTargets",
                json!({ "kind": "stackItems", "where": Value::Null }),
                0,
                4,
            )])),
        ));
    }
    if text == "Untap up to four target artifacts and/or creatures." {
        return Some(spell(
            "katarasReversalUntap",
            Some(declaration(vec![target_decision(
                "permanentTargets",
                json!({
                    "kind": "permanents",
                    "where": or(vec![card_type("Artifact"), card_type("Creature")]),
                }),
                0,
                4,
            )])),
        ));
    }
    if text.starts_with("Choose a creature type. Reveal cards from the top of your library until you reveal X creature cards")
    {
        return Some(spell("kindredSummons", None));
    }
    if text
        == "Create a 1/1 white Ally creature token. Put a +1/+1 counter on it for each creature your opponents control."
    {
        return Some(spell("matchTheOdds", None));
    }
    if text
        == "Choose a creature type. Return all creatures that aren't of the chosen type to their owners' hands."
    {
        return Some(spell("raiseThePalisade", None));
    }
    if text.starts_with("You control target opponent during their next combat phase.") {
        return Some(spell(
            "secretOfBloodbending",
            Some(declaration(vec![target_player("targetOpponent", true)])),
        ));
    }
    if text
        == "Draw two cards. If this spell's additional cost was paid, instead shuffle your graveyard into your library, draw seven cards, and you have no maximum hand size for the rest of the game."
    {
        return Some(spell("spiritWaterRevival", None));
    }
    if text.starts_with("Search your library and/or graveyard for an Ally creature card") {
        return Some(spell("taleOfMomoSearch", None));
    }
    if text
        == "Create X 1/1 white Ally creature tokens, then put a +1/+1 counter on each creature you control."
    {
        return Some(spell("unitedFront", Some(declaration(vec![x_value()]))));
    }
    if text.starts_with("Exile X target creatures you control. Return those cards to the battlefield under their owner's control at the beginning of the next end step.")
    {
        return Some(spell(
            "waterbendersRestoration",
            Some(declaration(vec![x_value()])),
        ));
    }

    match text {
        "Earthbend 3, then earthbend 3. You gain 3 life. (To earthbend 3, target land you control becomes a 0/0 creature with haste that's still a land. Put three +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)" => {
            Some(spell(
                "crackedEarthTechnique",
                Some(declaration(vec![
                    controlled_land("firstEarthbendLand"),
                    controlled_land("secondEarthbendLand"),
                ])),
            ))
        }
        "Earthbend 2. When you do, up to one target creature you control fights target creature an opponent controls. (To earthbend 2, target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped. Creatures that fight each deal damage equal to their power to the other.)" => {
            Some(spell(
                "earthRumble",
                Some(declaration(vec![
                    controlled_land("earthbendLand"),
                    controlled_creature("friendlyFighter", 0),
                    target_decision(
                        "opposingFighter",
                        json!({
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": controller(),
                            },
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    ),
                ])),
            ))
        }
        "Choose one â€”\nâ€¢ Draw cards equal to the greatest power among non-Human creatures you control.\nâ€¢ Non-Human creatures you control get +3/+3 until end of turn." => {
            Some(spell("earthRumbleTriumph", None))
        }
        "Earthbend 3. Then each creature you control with power less than or equal to that land's power gains hexproof and indestructible until end of turn. You gain hexproof until end of turn." => {
            Some(spell(
                "earthshape",
                Some(declaration(vec![controlled_land("earthbendLand")])),
            ))
        }
        "Search your library for a card, put that card into your hand, discard a card at random, then shuffle." => {
            Some(spell("gamble", None))
        }
        "Draw a card for each creature you control with a +1/+1 counter on it. Those creatures gain indestructible until end of turn. (Damage and effects that say \"destroy\" don't destroy them.)" => {
            Some(spell("inspiringCall", None))
        }
        "Untap all creatures and gain control of them until end of turn. They gain haste until end of turn." => {
            Some(spell("insurrection", None))
        }
        "Choose one â€”\nâ€¢ Destroy target artifact or enchantment.\nâ€¢ Put a +1/+1 counter on target creature you control. It gains indestructible until end of turn. (Damage and effects that say \"destroy\" don't destroy it.)" => {
            Some(spell("originOfMetalbending", None))
        }
        "Target creature you control deals damage equal to its power to target creature an opponent controls." => {
            Some(spell(
                "rockyRebuke",
                Some(declaration(vec![
                    controlled_creature("sourceCreature", 1),
                    target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": controller(),
                            },
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    ),
                ])),
            ))
        }
        "Reveal the top X cards of your library. Put all land cards from among them onto the battlefield tapped and the rest on the bottom of your library in a random order." => {
            Some(spell(
                "animistsAwakening",
                Some(declaration(vec![x_value()])),
            ))
        }
        "Spell mastery â€” If there are two or more instant and/or sorcery cards in your graveyard, untap those lands." => {
            Some(spell("animistsAwakeningSpellMastery", None))
        }
        "Choose one â€”\nâ€¢ Search your library for a creature or land card and reveal it. Put it onto the battlefield tapped if it's a land card. Otherwise, put it into your hand. Then shuffle.\nâ€¢ Put a +1/+1 counter on target creature you control. It deals damage equal to its power to target creature you don't control.\nâ€¢ Exile target artifact or enchantment." => {
            Some(spell("archdruidsCharm", None))
        }
        "Target player draws X cards. Shuffle Blue Sun's Zenith into its owner's library." => {
            let mut parsed = spell(
                "blueSunsZenith",
                Some(declaration(vec![
                    x_value(),
                    target_player("targetPlayer", false),
                ])),
            );
            parsed.rule["destinationAfterResolution"] = Value::String("library".to_string());
            Some(parsed)
        }
        "Search your library for up to X basic land cards, where X is the number of lands you control, put them onto the battlefield tapped, then shuffle." => {
            Some(spell("boundlessRealms", None))
        }
        "Each opponent loses two times X life. You gain life equal to the life lost this way." => {
            Some(spell(
                "debtToTheDeathless",
                Some(declaration(vec![x_value()])),
            ))
        }
        "Choose one. If you control a commander as you cast this spell, you may choose both instead.\nâ€¢ Target player draws X cards.\nâ€¢ Target player mills twice X cards." => {
            Some(spell("drownInDreams", Some(declaration(vec![x_value()]))))
        }
        "Choose up to one creature. Destroy the rest." => Some(spell(
            "duneblast",
            Some(declaration(vec![controlled_creature("savedCreature", 0)])),
        )),
        "Sacrifice a land. Search your library for up to two basic land cards, put them onto the battlefield tapped, then shuffle. If you control a creature with power 4 or greater, instead search your library for up to three basic land cards, put them onto the battlefield tapped, then shuffle." => {
            Some(spell("entishRestoration", None))
        }
        "Choose one â€”\nâ€¢ Draw X cards.\nâ€¢ Invoke the Firemind deals X damage to any target." => {
            Some(spell(
                "invokeTheFiremind",
                Some(declaration(vec![x_value()])),
            ))
        }
        "Destroy each creature that isn't all colors." => Some(spell("iridianMaelstrom", None)),
        "Choose one. If you control a commander as you cast this spell, you may choose both instead.\nâ€¢ Breathe Flame â€” Klauth's Will deals X damage to each creature without flying.\nâ€¢ Smash Relics â€” Destroy up to X target artifacts and/or enchantments." => {
            Some(spell("klauthsWill", Some(declaration(vec![x_value()]))))
        }
        "Lavalanche deals X damage to target player or planeswalker and each creature that player or that planeswalker's controller controls." => {
            Some(spell(
                "lavalanche",
                Some(declaration(vec![
                    x_value(),
                    target_decision(
                        "damageTarget",
                        json!({
                            "kind": "union",
                            "sets": [
                                { "kind": "players" },
                                { "kind": "permanents", "where": card_type("Planeswalker") },
                            ],
                        }),
                        1,
                        1,
                    ),
                ])),
            ))
        }
        "Each opponent reveals cards from the top of their library until they reveal X land cards, then puts all cards revealed this way into their graveyard. X can't be 0." =>
        {
            let mut x = x_value();
            x["minimum"] = integer(1);
            Some(spell("mindGrind", Some(declaration(vec![x]))))
        }
        "Return a creature you control to its owner's hand, then destroy all creatures." => {
            Some(spell(
                "timeWipe",
                Some(declaration(vec![controlled_creature("savedCreature", 1)])),
            ))
        }
        "Choose two â€”\nâ€¢ Counter target spell.\nâ€¢ Return target permanent to its owner's hand.\nâ€¢ Tap all creatures your opponents control.\nâ€¢ Draw a card." => {
            Some(spell("toTheCrystalTower", None))
        }
        "Target opponent exiles the top X cards of their library. You may cast any number of spells with mana value X or less from among them without paying their mana costs." => {
            Some(spell(
                "villainousWealth",
                Some(declaration(vec![
                    x_value(),
                    target_player("targetOpponent", true),
                ])),
            ))
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn parse_azula_spell_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let spell = |operation: &str, decisions: Vec<Value>| {
        let mut rule = json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [{
                "kind": "resolveSpellInstruction",
                "operation": operation,
            }],
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
                "Recognize the reusable Azula spell structure",
                "Declare all values, modes, and targets",
                "Resolve the ordered canonical operation",
            ],
        )
    };
    let target_creature = |id: &str, controller_filter: Option<Value>| {
        let mut candidates = json!({
            "kind": "permanents",
            "where": card_type("Creature"),
        });
        if let Some(controller_filter) = controller_filter {
            candidates["controller"] = controller_filter;
        }
        target_decision(id, candidates, 1, 1)
    };
    let target_any = |id: &str| target_decision(id, json!({ "kind": "anyTarget" }), 1, 1);
    let x_value = || json!({ "id": "xValue", "kind": "chooseNumber", "minimum": 0 });

    if text
        == "You may cast this spell as though it had flash if you control an attacking legendary creature."
    {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "flashWithAttackingLegendary": true,
            }),
            &[
                "Detect an attacking legendary creature",
                "Grant flash timing to this spell",
            ],
        ));
    }

    if text.starts_with("Choose one or both")
        && text.contains("Target creature gets -1/-1 until end of turn.")
        && text.contains("Put a +1/+1 counter on target creature.")
    {
        let mut weaken_target = target_creature("weakenTarget", None);
        weaken_target["condition"] = selection("spellModes", "weaken");
        let mut counter_target = target_creature("counterTarget", None);
        counter_target["condition"] = selection("spellModes", "counter");
        return Some(spell(
            "azulaAlwaysLies",
            vec![
                json!({
                    "id": "spellModes",
                    "kind": "chooseModes",
                    "minimum": 1,
                    "maximum": 2,
                    "options": ["weaken", "counter"],
                }),
                weaken_target,
                counter_target,
            ],
        ));
    }
    if text
        == "Each creature with mana value X or less loses all abilities until end of turn. Destroy those creatures."
    {
        return Some(spell("dayOfBlackSun", vec![x_value()]));
    }
    if text.starts_with("Choose three. You may choose the same mode more than once.")
        && text.contains("Fiery Confluence deals 1 damage to each creature.")
    {
        return Some(spell("fieryConfluence", Vec::new()));
    }
    if text
        == "Copy target instant or sorcery spell, then return it to its owner's hand. You may choose new targets for the copy."
    {
        return Some(spell(
            "narsetsReversal",
            vec![target_decision(
                "targetSpell",
                json!({
                    "kind": "spells",
                    "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                }),
                1,
                1,
            )],
        ));
    }
    if text.starts_with("Overwhelming Victory deals 5 damage to target creature.") {
        return Some(spell(
            "overwhelmingVictory",
            vec![target_creature("targetCreature", None)],
        ));
    }
    if text.starts_with(
        "Exile target creature. If it was dealt damage this turn, create a Clue token.",
    ) {
        return Some(spell(
            "soldOut",
            vec![target_creature("targetCreature", None)],
        ));
    }
    if text.starts_with("Exile cards from the top of your library until you exile a nonland card.")
    {
        return Some(spell("solsticeRevelations", Vec::new()));
    }
    if text.starts_with("Target creature you control fights target creature an opponent controls.")
        && text.contains("excess damage")
        && text.contains("add that much {R}")
    {
        return Some(spell(
            "theLastAgniKai",
            vec![
                target_creature("friendlyCreature", Some(controller())),
                target_creature(
                    "opposingCreature",
                    Some(json!({ "kind": "opponentsOf", "player": controller() })),
                ),
            ],
        ));
    }
    if text.starts_with(
        "Exile target artifact, creature, or enchantment. Its controller creates a Clue token.",
    ) {
        return Some(spell(
            "zukosExile",
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": or(vec![
                        card_type("Artifact"),
                        card_type("Creature"),
                        card_type("Enchantment"),
                    ]),
                }),
                1,
                1,
            )],
        ));
    }
    if text.starts_with("Combustion Technique deals damage equal to 2 plus the number of Lesson cards in your graveyard") {
        return Some(spell(
            "combustionTechnique",
            vec![target_creature("targetCreature", None)],
        ));
    }
    if text
        .starts_with("Choose target creature. When that creature dies this turn, you earthbend 4.")
    {
        return Some(spell(
            "fatalFissure",
            vec![target_creature("targetCreature", None)],
        ));
    }
    if text.starts_with("Draw a card. Until end of turn, target creature gains trample and gets +1/+0 for each card you've drawn this turn.") {
        return Some(spell(
            "fistsOfFlame",
            vec![target_creature("targetCreature", None)],
        ));
    }
    if text.starts_with("Choose one")
        && text.contains("Destroy target creature with no counters on it.")
        && text.contains("Remove up to three counters from target creature.")
    {
        return Some(spell(
            "heartlessAct",
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": 1,
                    "maximum": 1,
                    "options": ["destroy", "removeCounters"],
                }),
                target_creature("targetCreature", None),
            ],
        ));
    }
    if text.starts_with("Roku's Mastery deals X damage to target creature.") {
        return Some(spell(
            "rokusMastery",
            vec![x_value(), target_creature("targetCreature", None)],
        ));
    }
    if text.starts_with(
        "Searing Blood deals 2 damage to target creature. When that creature dies this turn",
    ) {
        return Some(spell(
            "searingBlood",
            vec![target_creature("targetCreature", None)],
        ));
    }
    if text.starts_with("Each creature you control gains trample and gets +X/+0 until end of turn")
        && text.contains("excess damage dealt this way")
    {
        return Some(spell(
            "overwhelmingVictory",
            vec![target_creature("targetCreature", None)],
        ));
    }
    if text.starts_with("Target creature gains menace until end of turn.") {
        return None;
    }
    if text.starts_with("It deals X damage to target player") {
        return Some(spell(
            "electroReflexiveDamage",
            vec![x_value(), target_any("damageTarget")],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_remaining_deck_spell(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let spell = |operation: &str, declaration: Option<Value>| {
        let mut rule = json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [{
                "kind": "resolveSpellInstruction",
                "operation": operation,
            }],
        });
        if let Some(declaration) = declaration {
            rule["declaration"] = declaration;
        }
        draft(
            rule,
            &[
                "Normalize the complete spell instruction",
                "Declare modes, values, and targets",
                "Apply ordered spell effects",
            ],
        )
    };
    let declaration = |decisions: Vec<Value>| {
        json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        })
    };
    let mode_decision = |options: Vec<&str>| {
        json!({
            "id": "spellMode",
            "kind": "chooseModes",
            "minimum": 1,
            "maximum": 1,
            "options": options,
        })
    };
    let target_permanent = |id: &str, filter: Value| {
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

    match text {
        "Join forces — Starting with you, each player may pay any amount of mana. Each player creates X 1/1 white Soldier creature tokens, where X is the total amount of mana paid this way." => {
            Some(spell("joinForcesSoldiers", None))
        }
        "Choose one —\n• Boros Charm deals 4 damage to target player or planeswalker.\n• Permanents you control gain indestructible until end of turn.\n• Target creature gains double strike until end of turn." =>
        {
            let mut damage_target = target_decision(
                "damageTarget",
                json!({
                    "kind": "union",
                    "sets": [
                        { "kind": "players" },
                        {
                            "kind": "permanents",
                            "where": card_type("Planeswalker"),
                        },
                    ],
                }),
                1,
                1,
            );
            damage_target["condition"] = selection("spellMode", "damage");
            let mut creature_target = target_permanent("creatureTarget", card_type("Creature"));
            creature_target["condition"] = selection("spellMode", "doubleStrike");
            Some(spell(
                "borosCharm",
                Some(declaration(vec![
                    mode_decision(vec!["damage", "indestructible", "doubleStrike"]),
                    damage_target,
                    creature_target,
                ])),
            ))
        }
        "As an additional cost to cast this spell, you may sacrifice one or more creatures. When you do, copy this spell for each creature sacrificed this way." => {
            Some(draft(
                json!({
                    "kind": "spellAbility",
                    "source": self_ref(),
                    "castingInstruction": {
                        "kind": "optionalSacrificeAnyCreaturesAndCopy",
                    },
                    "effects": [],
                }),
                &[
                    "Recognize optional any-number creature sacrifice",
                    "Apply sacrifices as an additional casting cost",
                    "Create one spell copy per sacrificed creature",
                ],
            ))
        }
        "As an additional cost to cast this spell, pay 5 life or pay {2}." => Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "additionalCostMode",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 1,
                        "options": ["payLife", "payMana"],
                    }],
                    "additionalCosts": [
                        {
                            "kind": "conditional",
                            "condition": selection("additionalCostMode", "payLife"),
                            "then": [{
                                "kind": "payLife",
                                "amount": integer(5),
                            }],
                        },
                        {
                            "kind": "conditional",
                            "condition": selection("additionalCostMode", "payMana"),
                            "then": [{
                                "kind": "payMana",
                                "manaCost": "{2}",
                            }],
                        },
                    ],
                },
                "effects": [],
            }),
            &[
                "Declare one alternative additional-cost mode",
                "Attach five-life or two-mana payment",
                "Apply the selected cost while casting",
            ],
        )),
        "Change the target of target spell or ability with a single target." => Some(spell(
            "changeSingleTarget",
            Some(declaration(vec![target_decision(
                "targetStackObject",
                json!({
                    "kind": "spells",
                    "singleTarget": true,
                    "includeAbilities": true,
                }),
                1,
                1,
            )])),
        )),
        "Target opponent sacrifices a creature of their choice for each creature put into your graveyard from the battlefield this turn." => {
            Some(spell(
                "urborgJustice",
                Some(declaration(vec![target_decision(
                    "targetOpponent",
                    json!({
                        "kind": "players",
                        "where": {
                            "kind": "isOpponentOf",
                            "player": controller(),
                        },
                    }),
                    1,
                    1,
                )])),
            ))
        }
        "Destroy target artifact. If you controlled that artifact, create three 1/1 red Phyrexian Goblin creature tokens." => {
            Some(spell(
                "gleefulDemolition",
                Some(declaration(vec![target_permanent(
                    "targetArtifact",
                    card_type("Artifact"),
                )])),
            ))
        }
        "Copy target instant or sorcery spell you control. If this spell was cast from a graveyard, copy that spell twice instead. You may choose new targets for the copies." => {
            Some(spell(
                "increasingVengeance",
                Some(declaration(vec![target_decision(
                    "targetSpell",
                    json!({
                        "kind": "spells",
                        "controller": controller(),
                        "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    }),
                    1,
                    1,
                )])),
            ))
        }
        "Undergrowth — Target creature gets -X/-X until end of turn, where X is the number of creature cards in your graveyard. If that creature would die this turn, exile it instead." => {
            Some(spell(
                "necroticWound",
                Some(declaration(vec![target_permanent(
                    "targetCreature",
                    card_type("Creature"),
                )])),
            ))
        }
        "Choose one —\n• Add two mana of any one color and two mana of any other color. Spend this mana only to cast creature or enchantment spells.\n• Creatures you control get +1/+0 until end of turn." => {
            Some(spell(
                "openOmenpaths",
                Some(declaration(vec![mode_decision(vec!["mana", "creatures"])])),
            ))
        }
        "Choose one —\n• Each opponent sacrifices an artifact of their choice.\n• Each opponent sacrifices an enchantment of their choice.\n• Each opponent sacrifices a creature with flying of their choice." => {
            Some(spell(
                "pickYourPoison",
                Some(declaration(vec![mode_decision(vec![
                    "artifact",
                    "enchantment",
                    "flyingCreature",
                ])])),
            ))
        }
        "Until end of turn, any number of target creatures you control each get +1/+0 and gain \"When this creature dies, draw a card.\"" => {
            Some(spell(
                "rabidAttack",
                Some(declaration(vec![target_decision(
                    "targetCreatures",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Creature"),
                    }),
                    0,
                    64,
                )])),
            ))
        }
        "Choose target creature. When that creature dies this turn, return a creature card from its owner's graveyard to the battlefield under the control of that creature's owner." => {
            Some(spell(
                "reincarnation",
                Some(declaration(vec![target_permanent(
                    "targetCreature",
                    card_type("Creature"),
                )])),
            ))
        }
        "Create a tapped 1/1 black Rat creature token for each creature card in your graveyard." => {
            Some(spell("revengeOfTheRats", None))
        }
        "Gain control of target creature until end of turn. Untap that creature. Until end of turn, it gains haste and \"Whenever this creature deals damage, destroy target Equipment attached to it.\"" => {
            Some(spell(
                "shacklesOfTreachery",
                Some(declaration(vec![target_permanent(
                    "targetCreature",
                    card_type("Creature"),
                )])),
            ))
        }
        "Discard all the cards in your hand, then draw that many cards." => {
            Some(spell("shatteredPerception", None))
        }
        "Look at twice X cards from the top of your library. Put X cards from among them into your hand and the rest into your graveyard. You lose X life." => {
            Some(spell(
                "stargaze",
                Some(declaration(vec![json!({
                    "id": "xValue",
                    "kind": "chooseNumber",
                    "minimum": 0,
                })])),
            ))
        }
        "Choose a color. Sudden Demise deals X damage to each creature of the chosen color." => {
            Some(spell(
                "suddenDemise",
                Some(declaration(vec![
                    json!({
                        "id": "xValue",
                        "kind": "chooseNumber",
                        "minimum": 0,
                    }),
                    json!({
                        "id": "chosenColor",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 1,
                        "options": ["White", "Blue", "Black", "Red", "Green"],
                    }),
                ])),
            ))
        }
        "Choose an opponent. You and that player each create an X/X green Treefolk creature token." => {
            Some(spell(
                "sylvanOfferingTreefolk",
                Some(declaration(vec![
                    json!({
                        "id": "xValue",
                        "kind": "chooseNumber",
                        "minimum": 0,
                    }),
                    target_decision(
                        "chosenOpponent",
                        json!({
                            "kind": "players",
                            "where": {
                                "kind": "isOpponentOf",
                                "player": controller(),
                            },
                        }),
                        1,
                        1,
                    ),
                ])),
            ))
        }
        "Choose an opponent. You and that player each create X 1/1 green Elf Warrior creature tokens." => {
            Some(spell("sylvanOfferingElves", None))
        }
        "Tap target untapped creature. It deals damage equal to its power to its controller." => {
            Some(spell(
                "traitorsRoar",
                Some(declaration(vec![target_permanent(
                    "targetCreature",
                    and(vec![
                        card_type("Creature"),
                        not(json!({ "kind": "isTapped" })),
                    ]),
                )])),
            ))
        }
        "Choose one —\n• You may sacrifice a permanent. If you do, draw two cards.\n• You gain 5 life.\n• Destroy target nonland permanent with mana value 2 or less." =>
        {
            let mut target = target_permanent(
                "targetPermanent",
                and(vec![
                    not(card_type("Land")),
                    compare(
                        "<=",
                        json!({
                            "kind": "manaValueOf",
                            "object": { "kind": "candidate" },
                        }),
                        integer(2),
                    ),
                ]),
            );
            target["condition"] = selection("spellMode", "destroy");
            Some(spell(
                "witherbloomCharm",
                Some(declaration(vec![
                    mode_decision(vec!["sacrifice", "gainLife", "destroy"]),
                    target,
                ])),
            ))
        }
        "Infusion — If you gained life this turn, destroy all creatures instead." => {
            Some(spell("witheringCurseInfusion", None))
        }
        "Return target creature card from your graveyard to your hand. If this spell was kicked, instead put that card onto the battlefield tapped." => {
            Some(spell(
                "zukosConviction",
                Some(declaration(vec![target_decision(
                    "targetCreatureCard",
                    json!({
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": card_type("Creature"),
                    }),
                    1,
                    1,
                )])),
            ))
        }
        "Each opponent sacrifices a creature of their choice with flying." => {
            Some(spell("clipWings", None))
        }
        "Delirium — This spell costs {2} less to cast as long as there are four or more card types among cards in your graveyard." => {
            Some(draft(
                json!({
                    "kind": "staticAbility",
                    "source": self_ref(),
                    "activeWhile": {
                        "kind": "inZone",
                        "object": self_ref(),
                        "zone": { "kind": "stackOrCast" },
                    },
                    "modifiers": [{
                        "kind": "reduceOwnGenericCastingCost",
                        "amount": integer(2),
                        "condition": {
                            "kind": "delirium",
                            "player": controller(),
                        },
                    }],
                }),
                &[
                    "Count card types in the controller's graveyard",
                    "Require four or more distinct card types",
                    "Reduce the spell's generic casting cost by two",
                ],
            ))
        }
        "Until end of turn, target creature gets +2/+0 and gains \"When this creature dies, return it to the battlefield tapped under its owner's control and you create a Treasure token.\" (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")" => {
            Some(spell(
                "fakeYourOwnDeath",
                Some(declaration(vec![target_permanent(
                    "targetCreature",
                    card_type("Creature"),
                )])),
            ))
        }
        "Prevent all combat damage that would be dealt this turn." => Some(spell("fog", None)),
        "Choose one —\n• Destroy target artifact.\n• Glorious Decay deals 4 damage to target creature with flying.\n• Exile target card from a graveyard. Draw a card." =>
        {
            let mut artifact = target_permanent("targetArtifact", card_type("Artifact"));
            artifact["condition"] = selection("spellMode", "artifact");
            let mut flyer = target_permanent(
                "targetFlyer",
                and(vec![
                    card_type("Creature"),
                    json!({ "kind": "hasKeyword", "value": "flying" }),
                ]),
            );
            flyer["condition"] = selection("spellMode", "flyer");
            let mut grave_card = target_decision(
                "targetGraveCard",
                json!({
                    "kind": "cards",
                    "zone": { "kind": "anyGraveyard" },
                    "where": Value::Null,
                }),
                1,
                1,
            );
            grave_card["condition"] = selection("spellMode", "graveyard");
            Some(spell(
                "gloriousDecay",
                Some(declaration(vec![
                    mode_decision(vec!["artifact", "flyer", "graveyard"]),
                    artifact,
                    flyer,
                    grave_card,
                ])),
            ))
        }
        "Choose one —\n• All creatures get -1/-1 until end of turn.\n• Destroy target enchantment.\n• Regenerate each creature you control." =>
        {
            let mut enchantment = target_permanent("targetEnchantment", card_type("Enchantment"));
            enchantment["condition"] = selection("spellMode", "destroy");
            Some(spell(
                "golgariCharm",
                Some(declaration(vec![
                    mode_decision(vec!["weaken", "destroy", "regenerate"]),
                    enchantment,
                ])),
            ))
        }
        "Put three -1/-1 counters on target creature with flying." => Some(spell(
            "stingingShot",
            Some(declaration(vec![target_permanent(
                "targetCreature",
                and(vec![
                    card_type("Creature"),
                    json!({ "kind": "hasKeyword", "value": "flying" }),
                ]),
            )])),
        )),
        "Tweeze deals 3 damage to any target. You may discard a card. If you do, draw a card." => {
            Some(spell(
                "tweeze",
                Some(declaration(vec![target_decision(
                    "damageTarget",
                    json!({ "kind": "anyTarget" }),
                    1,
                    1,
                )])),
            ))
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn expansion_spell_rule(
    effects: Vec<Value>,
    decisions: Vec<Value>,
) -> CanonicalRuleDraft {
    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
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
            "Parse reusable Oracle primitives",
            "Resolve the declared effects in order",
        ],
    )
}

pub(in crate::oracle::canonical) fn parse_expansion_spell(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text
        == "The next creature spell you cast this turn can be cast as though it had flash. That spell can't be countered. That creature enters with an additional +1/+1 counter on it."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "installNextCreatureSpellModifier",
                "player": controller(),
                "expiresAfterTurn": { "kind": "currentTurn" },
                "grantFlash": true,
                "cantBeCountered": true,
                "enteringCounter": "+1/+1",
                "enteringCounterCount": integer(1),
            })],
            Vec::new(),
        ));
    }
    if text
        == "Search your library for a creature card with mana value equal to 1 plus the sacrificed creature's mana value, put that card onto the battlefield with an additional +1/+1 counter on it, then shuffle."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "searchLibraryRelativeToSacrificedPermanent",
                "player": controller(),
                "where": card_type("Creature"),
                "manaValueOffset": integer(1),
                "destination": "battlefield",
                "tapped": false,
                "counter": "+1/+1",
                "counterCount": integer(1),
            })],
            Vec::new(),
        ));
    }
    if text
        == "Sacrifice any number of lands. Search your library for up to that many land cards, put them onto the battlefield tapped, then shuffle."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "sacrificeThenSearchLibrary",
                "player": controller(),
                "sacrificeWhere": card_type("Land"),
                "searchWhere": card_type("Land"),
                "destination": "battlefield",
                "tapped": true,
            })],
            Vec::new(),
        ));
    }
    let search = |filter: Value, maximum: i64, destination: &str, tapped: bool| {
        expansion_spell_rule(
            search_library_effects(filter, maximum, destination, tapped),
            Vec::new(),
        )
    };
    let simple_land_search_re = Regex::new(&format!(
        r"(?i)^Search your library for (?:(?:a|one)|up to ({})) (basic land|land|Forest) cards?, (?:reveal (?:that card|those cards|it|them), )?put (?:that card|those cards|it|them) onto the battlefield( tapped)?, then shuffle\.$",
        count_word_pattern(),
    ))
    .expect("generic land search spell regex compiles");
    if let Some(captures) = simple_land_search_re.captures(text) {
        let maximum = captures
            .get(1)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        let filter = entry_search_filter(&format!("a {} card", &captures[2]))?;
        return Some(search(
            filter,
            maximum,
            "battlefield",
            captures.get(3).is_some(),
        ));
    }

    if let Some(effects) = split_library_search_between_battlefield_and_hand_effects(text, "") {
        return Some(expansion_spell_rule(effects, Vec::new()));
    }

    if text
        == "Search your library for up to X basic land cards, where X is the greatest power among creatures you control. Put those cards onto the battlefield tapped, then shuffle."
    {
        let mut effects = search_library_effects(
            and(vec![
                json!({ "kind": "typeLineContains", "value": "Basic" }),
                card_type("Land"),
            ]),
            0,
            "battlefield",
            true,
        );
        effects[0]["maximum"] = json!({
            "kind": "greatestPower",
            "player": controller(),
            "where": card_type("Creature"),
        });
        return Some(expansion_spell_rule(effects, Vec::new()));
    }

    if text == "Return all land cards from your graveyard to the battlefield tapped." {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "moveCards",
                "cards": {
                    "kind": "cardsInZone",
                    "zone": graveyard(controller()),
                    "where": card_type("Land"),
                },
                "to": { "kind": "battlefield", "player": controller(), "tapped": true },
            })],
            Vec::new(),
        ));
    }

    if text
        == "Counter target enchantment, instant, or sorcery spell. Its controller creates a 2/2 blue Bird creature token with flying."
    {
        let spell = chosen_target("targetSpell");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "bind",
                    "id": "counteredSpellController",
                    "value": { "kind": "controllerOf", "object": spell.clone() },
                }),
                json!({ "kind": "counterSpell", "spell": spell }),
                json!({
                    "kind": "createTokens",
                    "controller": { "kind": "boundValue", "id": "counteredSpellController" },
                    "quantity": integer(1),
                    "token": {
                        "name": "Bird Token",
                        "colors": ["blue"],
                        "types": ["Creature"],
                        "subtypes": ["Bird"],
                        "power": 2,
                        "toughness": 2,
                        "abilities": [{ "kind": "flying" }],
                    },
                }),
            ],
            vec![target_decision(
                "targetSpell",
                json!({
                    "kind": "stackObjects",
                    "where": or(vec![
                        card_type("Enchantment"),
                        card_type("Instant"),
                        card_type("Sorcery"),
                    ]),
                }),
                1,
                1,
            )],
        ));
    }

    if text
        == "Exile target creature. Its controller may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "exileTargetThenControllerMaySearchBasicLand",
            })],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }
    if text == "Counter target spell that targets you or a permanent you control." {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "counterSpell",
                "spell": chosen_target("targetSpell"),
            })],
            vec![target_decision(
                "targetSpell",
                json!({
                    "kind": "spells",
                    "targetingControllerOrPermanent": true,
                }),
                1,
                1,
            )],
        ));
    }
    if text.starts_with("Choose one")
        && text.contains("Creatures you control gain lifelink until end of turn.")
        && text.contains("Draw a card.")
        && text.contains("Put target attacking or blocking creature on top of its owner's library.")
    {
        let mut creature_target = target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "where": {
                    "kind": "or",
                    "operands": [{ "kind": "isAttacking" }, { "kind": "isBlocking" }],
                },
            }),
            1,
            1,
        );
        creature_target["condition"] = selection("spellMode", "library");
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "azoriusCharm",
            })],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": 1,
                    "maximum": 1,
                    "options": ["lifelink", "draw", "library"],
                }),
                creature_target,
            ],
        ));
    }

    match text {
        "Search your library for a basic land card, reveal it, put it into your hand, then shuffle." =>
        {
            return Some(search(
                json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                1,
                "hand",
                false,
            ));
        }
        "Search your library for an artifact card, reveal it, put it into your hand, then shuffle." =>
        {
            return Some(search(card_type("Artifact"), 1, "hand", false));
        }
        "Search your library for up to three basic land cards, reveal them, put them into your hand, then shuffle." =>
        {
            return Some(search(
                json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                3,
                "hand",
                false,
            ));
        }
        "Search your library for up to two Forest cards, put them onto the battlefield tapped, then shuffle." =>
        {
            return Some(search(
                json!({ "kind": "typeLineContains", "value": "Forest" }),
                2,
                "battlefield",
                true,
            ));
        }
        "Put all creatures on the bottom of their owners' libraries." => {
            return Some(expansion_spell_rule(
                vec![json!({
                    "kind": "movePermanentsToOwnersLibraries",
                    "where": card_type("Creature"),
                    "position": "bottom",
                })],
                Vec::new(),
            ));
        }
        "Destroy all nonland creatures." => {
            return Some(expansion_spell_rule(
                vec![json!({
                    "kind": "destroyPermanent",
                    "permanent": { "kind": "eachPermanent", "where": and(vec![card_type("Creature"), not(card_type("Land"))]) },
                })],
                Vec::new(),
            ));
        }
        "Return all attacking creatures to their owner's hand." => {
            return Some(expansion_spell_rule(
                vec![json!({
                    "kind": "returnAttackingCreaturesToOwnersHands",
                })],
                Vec::new(),
            ));
        }
        "Put three +1/+1 counters on target creature." => {
            return Some(expansion_spell_rule(
                vec![
                    json!({ "kind": "putCounters", "permanent": chosen_target("targetCreature"), "counter": "+1/+1", "count": integer(3) }),
                ],
                vec![target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            ));
        }
        "Put a +1/+1 counter on target creature you control, then double the number of +1/+1 counters on that creature." =>
        {
            let target = chosen_target("targetCreature");
            return Some(expansion_spell_rule(
                vec![
                    json!({ "kind": "putCounters", "permanent": target.clone(), "counter": "+1/+1", "count": integer(1) }),
                    json!({ "kind": "doubleCounters", "permanent": target, "counter": "+1/+1" }),
                ],
                vec![target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
                    1,
                    1,
                )],
            ));
        }
        "Put five +1/+1 counters on target creature. If this spell was cast from a graveyard, put ten +1/+1 counters on that creature instead." =>
        {
            return Some(expansion_spell_rule(
                vec![
                    json!({ "kind": "resolveSpellInstruction", "operation": "increasingSavagery" }),
                ],
                vec![target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            ));
        }
        "Remove all +1/+1 counters from target creature you control. Draw that many cards." => {
            return Some(expansion_spell_rule(
                vec![json!({
                    "kind": "resolveSpellInstruction",
                    "operation": "takeCountersDraw",
                    "targetId": "targetCreature",
                })],
                vec![target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
                    1,
                    1,
                )],
            ));
        }
        "Target creature gets +2/+2 until end of turn. You may put a land card from your hand onto the battlefield." =>
        {
            return Some(expansion_spell_rule(
                vec![
                    json!({ "kind": "modifyPowerToughness", "object": chosen_target("targetCreature"), "power": integer(2), "toughness": integer(2), "duration": { "kind": "untilEndOfCurrentTurn" } }),
                    json!({ "kind": "resolveSpellInstruction", "operation": "mayPutLandFromHand" }),
                ],
                vec![target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            ));
        }
        "Draw three cards. You may play an additional land this turn." => {
            return Some(expansion_spell_rule(
                vec![
                    json!({ "kind": "drawCards", "player": controller(), "count": integer(3) }),
                    json!({ "kind": "resolveSpellInstruction", "operation": "urbanEvolutionLand" }),
                ],
                Vec::new(),
            ));
        }
        "Put target land card from a graveyard onto the battlefield under your control." => {
            return Some(expansion_spell_rule(
                vec![
                    json!({ "kind": "moveTargetCard", "card": chosen_target("targetLand"), "to": "battlefield", "tapped": false, "controller": controller() }),
                ],
                vec![target_decision(
                    "targetLand",
                    json!({ "kind": "cards", "zone": { "kind": "anyGraveyard" }, "where": card_type("Land") }),
                    1,
                    1,
                )],
            ));
        }
        "Counter target artifact or enchantment spell." => {
            return Some(expansion_spell_rule(
                vec![json!({ "kind": "counterSpell", "spell": chosen_target("targetSpell") })],
                vec![target_decision(
                    "targetSpell",
                    json!({ "kind": "stackObjects", "where": or(vec![card_type("Artifact"), card_type("Enchantment")]) }),
                    1,
                    1,
                )],
            ));
        }
        "Counter target noncreature spell unless its controller pays {2}." => {
            return Some(expansion_spell_rule(
                vec![
                    json!({ "kind": "counterStackObjectUnlessPays", "spell": chosen_target("targetSpell"), "manaCost": "{2}" }),
                ],
                vec![target_decision(
                    "targetSpell",
                    json!({ "kind": "stackObjects", "where": not(card_type("Creature")) }),
                    1,
                    1,
                )],
            ));
        }
        "Proliferate. (Choose any number of permanents and/or players, then give each another counter of each kind already there.)" =>
        {
            return Some(expansion_spell_rule(
                vec![json!({ "kind": "resolveSpellInstruction", "operation": "proliferateOnce" })],
                Vec::new(),
            ));
        }
        _ => {}
    }
    None
}
