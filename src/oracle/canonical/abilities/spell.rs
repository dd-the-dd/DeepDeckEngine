use super::super::*;

pub(in crate::oracle::canonical) fn parse_own_casting_reduction(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text == "This spell costs {1} less to cast for each color among permanents you control." {
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
                        "kind": "countDistinctColors",
                        "player": controller(),
                        "where": Value::Null,
                    },
                }],
            }),
            &[
                "Count distinct colors among controlled permanents",
                "Reduce this spell's generic casting cost by that count",
            ],
        ));
    }
    if text
        == "This spell costs {3} less to cast if it targets a creature that was dealt damage this turn."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone", "object": self_ref(), "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": integer(3),
                    "targetWhere": and(vec![
                        card_type("Creature"),
                        json!({ "kind": "wasDealtDamageThisTurn" }),
                    ]),
                }],
            }),
            &[
                "Match a creature dealt damage this turn",
                "Reduce the generic cost by three",
            ],
        ));
    }
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
    if text.starts_with("Discover X, where X is the amount of mana spent to cast this spell.") {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "ashayaDiscoverSpentMana",
                }],
            }),
            &["Count mana spent to cast the spell", "Resolve Discover X"],
        ));
    }
    if let Some(parsed) = parse_own_casting_reduction(text) {
        return Some(parsed);
    }
    let council_counter_or_copy_re = Regex::new(
        r"(?i)^Will of the council (?:—|-) Choose target (.+?) spell\. Starting with you, each player votes for denial or duplication\. If denial gets more votes, counter the spell\. If duplication gets more votes or the vote is tied, copy the spell\. You may choose new targets for the copy\.$",
    )
    .expect("Will of the council counter-or-copy regex compiles");
    if let Some(captures) = council_counter_or_copy_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetSpell",
                        json!({
                            "kind": "spells",
                            "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "willOfCouncilCounterOrCopy",
                    "spell": chosen_target("targetSpell"),
                    "player": controller(),
                    "counterVote": "denial",
                    "copyVote": "duplication",
                    "tiesCopy": true,
                    "mayChooseNewTargets": true,
                }],
            }),
            &[
                "Declare the instant or sorcery spell target",
                "Collect votes in turn order starting with the controller",
                "Counter on a denial majority; otherwise copy and optionally retarget",
            ],
        ));
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

    let custom_spell = |operation: &str| {
        spell(
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": operation,
            })],
        )
    };
    match text {
        "Exile target creature or Spacecraft. Its controller may search their library for a basic land card, reveal it, put it into their hand, then shuffle." =>
        {
            return Some(custom_spell("seventyThousandLightYears"));
        }
        "Target artifact or creature you control gains hexproof and indestructible until end of turn. If it's a creature, put a +1/+1 counter on it. (It can't be the target of spells or abilities your opponents control. Damage and effects that say \"destroy\" don't destroy it.)" =>
        {
            return Some(custom_spell("shieldsUp"));
        }
        "Counter target spell with mana value 2 or less. If this spell was kicked, instead counter target spell." =>
        {
            return Some(custom_spell("highlyIllogical"));
        }
        "Target creature gets -2/-2 until end of turn. If this spell was kicked, that creature gets -6/-6 until end of turn instead." =>
        {
            return Some(custom_spell("ejectWarpCore"));
        }
        "Target creature gets +2/+0 until end of turn. When that creature dies this turn, you draw a card." =>
        {
            return Some(custom_spell("goodDayToDie"));
        }
        "Khaaaaaaaaaaaannn! deals twice X damage to target creature. If that creature would die this turn, exile it instead." =>
        {
            return Some(custom_spell("khanDoubleX"));
        }
        "Plasma Cascade deals 3 damage to any target. You may discard a card. If you do, draw a card." =>
        {
            return Some(custom_spell("plasmaCascade"));
        }
        "One or two target creatures you control each deal damage equal to their power to target creature an opponent controls." =>
        {
            return Some(custom_spell("commonGoal"));
        }
        "Mill four cards. You may put a permanent card from among them into your hand. If this spell is the first spell you've cast this game, you gain 2 life. (To mill four cards, put the top four cards of your library into your graveyard.)" =>
        {
            return Some(custom_spell("firstContact"));
        }
        "Put a +1/+1 counter on target creature you control. Untap it. Until end of turn, it gets +1/+1 and becomes a Doctor. (It loses all other creature types.)" =>
        {
            return Some(custom_spell("doctorNotA"));
        }
        "Exile all creature cards from target player's graveyard. You may cast spells from among those cards for as long as they remain exiled, and mana of any type can be spent to cast them." =>
        {
            return Some(custom_spell("shadowEnemyExile"));
        }
        "Smite the Deathless deals 3 damage to target creature. That creature loses indestructible until end of turn. If that creature would die this turn, exile it instead." =>
        {
            return Some(custom_spell("smiteDeathless"));
        }
        _ => {}
    }

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
        r"^Choose target card in a graveyard other than (?:a )?(.+?) card\. Search its owner's graveyard, hand, and library for (any number of|all) cards with the same name as that card and exile them\. Then that player shuffles\.$",
    )
    .expect("targeted same-name multi-zone search regex compiles");
    if let Some(captures) = targeted_name_search_re.captures(text) {
        let excluded = parse_permanent_criteria(&captures[1], "")?;
        let exile_all = captures[2].eq_ignore_ascii_case("all");
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
                    "all": exile_all,
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
    let named_search_re = Regex::new(
        r"^Choose (?:a|an) (.+?) card name\. Search target opponent's graveyard, hand, and library for (any number of|up to (\w+)) cards with that name and exile them\. Then that player shuffles\.$",
    )
    .expect("chosen-name multi-zone search regex compiles");
    let captures = named_search_re.captures(text)?;
    let named_where = parse_permanent_criteria(&captures[1], "")?;
    let maximum = captures
        .get(3)
        .and_then(|count| parse_number_word(count.as_str()))
        .map(integer);
    let mut exile_effect = json!({
        "kind": "exileNamedCardsFromZones",
        "player": chosen_target("targetOpponent"),
        "chooser": controller(),
        "decisionId": "chosenCardName",
        "zones": ["graveyard", "hand", "library"],
        "shuffleLibrary": true,
    });
    if let Some(maximum) = maximum {
        exile_effect["maximum"] = maximum;
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
                    "where": named_where,
                },
                exile_effect,
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
    let behold_and_exile_re = Regex::new(
        r"(?i)^As an additional cost to cast this spell, behold (?:a|an) ([A-Za-z][A-Za-z '-]+) and exile it\. \(Exile .+\)$",
    )
    .expect("behold-and-exile additional cost regex compiles");
    if let Some(captures) = behold_and_exile_re.captures(text) {
        let behold_type = captures.get(1)?.as_str();
        let selected_mode = |mode: &str| {
            json!({
                "kind": "selectionContains",
                "selection": decision_result("additionalCostMode"),
                "value": mode,
            })
        };
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        {
                            "kind": "chooseModes",
                            "id": "additionalCostMode",
                            "minimum": 1,
                            "maximum": 1,
                            "options": ["beholdBattlefield", "beholdHand"],
                        },
                        {
                            "kind": "chooseObjects",
                            "id": "beheldBattlefieldCard",
                            "condition": selected_mode("beholdBattlefield"),
                            "quantity": { "kind": "exactly", "value": 1 },
                            "candidates": {
                                "kind": "permanents",
                                "controller": controller(),
                                "where": subtype(behold_type),
                            },
                        },
                        {
                            "kind": "chooseObjects",
                            "id": "beheldHandCard",
                            "condition": selected_mode("beholdHand"),
                            "quantity": { "kind": "exactly", "value": 1 },
                            "candidates": {
                                "kind": "cards",
                                "zone": hand(controller()),
                                "where": subtype(behold_type),
                            },
                        },
                    ],
                    "additionalCosts": [
                        {
                            "kind": "conditional",
                            "condition": selected_mode("beholdBattlefield"),
                            "then": [{
                                "kind": "exileObject",
                                "object": chosen_target("beheldBattlefieldCard"),
                            }],
                            "else": [],
                        },
                        {
                            "kind": "conditional",
                            "condition": selected_mode("beholdHand"),
                            "then": [{
                                "kind": "exileObject",
                                "object": chosen_target("beheldHandCard"),
                            }],
                            "else": [],
                        },
                    ],
                },
                "effects": [],
            }),
            &[
                "Choose a controlled or hand card of the beheld type",
                "Exile the selected object as an additional casting cost",
            ],
        ));
    }
    let behold_or_pay_re = Regex::new(
        r"(?i)^As an additional cost to cast this spell, behold (?:a|an) ([A-Za-z][A-Za-z '-]+) or pay ((?:\{[^}]+\})+)\. \(To behold .+\)$",
    )
    .expect("behold-or-pay additional cost regex compiles");
    if let Some(captures) = behold_or_pay_re.captures(text) {
        let behold_type = captures.get(1)?.as_str();
        let mana_cost = captures.get(2)?.as_str();
        let selected_mode = |mode: &str| {
            json!({
                "kind": "selectionContains",
                "selection": decision_result("additionalCostMode"),
                "value": mode,
            })
        };
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        {
                            "kind": "chooseModes",
                            "id": "additionalCostMode",
                            "minimum": 1,
                            "maximum": 1,
                            "options": ["beholdBattlefield", "beholdHand", "payMana"],
                        },
                        {
                            "kind": "chooseObjects",
                            "id": "beheldBattlefieldJace",
                            "condition": selected_mode("beholdBattlefield"),
                            "quantity": { "kind": "exactly", "value": 1 },
                            "candidates": {
                                "kind": "permanents",
                                "controller": controller(),
                                "where": subtype(behold_type),
                            },
                        },
                        {
                            "kind": "chooseObjects",
                            "id": "beheldHandJace",
                            "condition": selected_mode("beholdHand"),
                            "quantity": { "kind": "exactly", "value": 1 },
                            "candidates": {
                                "kind": "cards",
                                "zone": hand(controller()),
                                "where": subtype(behold_type),
                            },
                        },
                    ],
                    "additionalCosts": [{
                        "kind": "conditional",
                        "condition": selected_mode("payMana"),
                        "then": [{ "kind": "payMana", "manaCost": mana_cost }],
                        "else": [],
                    }],
                },
                "effects": [],
            }),
            &[
                "Choose to behold a controlled or revealed Jace, or pay one mana",
                "Require the selected Jace in its appropriate zone",
                "Add mana only for the payment branch",
            ],
        ));
    }
    let graveyard_return_and_self_exile_re = Regex::new(
        r"(?i)^Return target (.+?) card from your graveyard to your hand\. Exile (.+?)\.$",
    )
    .expect("graveyard return and resolving-source exile regex compiles");
    if let Some(captures) = graveyard_return_and_self_exile_re.captures(text)
        && source_reference_matches(captures.get(2)?.as_str(), face_name)
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetGraveyardCard",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "moveTargetCard",
                    "card": chosen_target("targetGraveyardCard"),
                    "to": "hand",
                    "tapped": false,
                }],
                "exileAfterResolution": true,
            }),
            &[
                "Target a graveyard card through the shared criteria grammar",
                "Return the target to its owner's hand",
                "Replace the resolving spell's graveyard destination with exile",
            ],
        ));
    }

    let chosen_players_search_to_top_re = Regex::new(&format!(
        r"(?i)^Choose ({}) target players\. Each of them searches their library for (?:a|one) card, then shuffles(?: their library)? and puts that card on top(?: of it)?\.$",
        count_word_pattern(),
    ))
    .expect("chosen players search libraries to top regex compiles");
    if let Some(captures) = chosen_players_search_to_top_re.captures(text) {
        let count = parse_number_word(captures.get(1)?.as_str())?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPlayers",
                        json!({ "kind": "players" }),
                        count,
                        count,
                    )],
                },
                "effects": [{
                    "kind": "chosenPlayersSearchLibrariesToTop",
                    "targetsDecisionId": "targetPlayers",
                }],
            }),
            &[
                "Declare the requested number of distinct player targets",
                "Let each targeted player search their own library",
                "Shuffle each searched library and put its selected card on top",
            ],
        ));
    }

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
        == "Put one, two, or three target creature cards from graveyards onto the battlefield under your control. Each of them enters with an additional -1/-1 counter on it."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "aberrantCreatures",
                        json!({
                            "kind": "cards",
                            "zone": { "kind": "anyGraveyard" },
                            "where": card_type("Creature"),
                        }),
                        1,
                        3,
                    )],
                },
                "effects": [{
                    "kind": "moveCards",
                    "cards": { "kind": "chosenTargets", "id": "aberrantCreatures" },
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                        "enterWithCounters": [{
                            "counter": "-1/-1",
                            "count": integer(1),
                        }],
                    },
                }],
            }),
            &["Reanimate one to three targeted creatures with an additional -1/-1 counter"],
        ));
    }

    if text
        == "Remove any number of counters from among permanents on the battlefield. You draw cards and lose life equal to the number of counters removed this way."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "eventidesShadowRemoveCounters",
                }],
            }),
            &["Remove any selected battlefield counters, then draw and lose that much life"],
        ));
    }

    if text
        == "Create a number of 5/5 red and green Elemental creature tokens equal to the number of colors among permanents you control. Then you gain life equal to the number of creatures you control."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "elementalSpectacle",
                }],
            }),
            &["Create vivid Elementals, then gain life for controlled creatures"],
        ));
    }

    if text
        == "Put X -1/-1 counters on each creature. Shuffle Black Sun's Zenith into its owner's library."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "destinationAfterResolution": "library",
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "xValue",
                        "kind": "chooseNumber",
                        "minimum": 0,
                    }],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": card_type("Creature"),
                    },
                    "counter": "-1/-1",
                    "count": { "kind": "sourceCastXValue" },
                }],
            }),
            &["Put X -1/-1 counters on every creature and shuffle the spell into its library"],
        ));
    }

    if text
        == "Put a -1/-1 counter on target creature, two -1/-1 counters on another target creature, and three -1/-1 counters on a third target creature."
    {
        let candidates = json!({
            "kind": "permanents",
            "where": card_type("Creature"),
        });
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("incrementalOne", candidates.clone(), 1, 1),
                        target_decision("incrementalTwo", candidates.clone(), 1, 1),
                        target_decision("incrementalThree", candidates, 1, 1),
                    ],
                },
                "effects": [
                    {
                        "kind": "putCounters",
                        "permanent": chosen_target("incrementalOne"),
                        "counter": "-1/-1",
                        "count": integer(1),
                    },
                    {
                        "kind": "putCounters",
                        "permanent": chosen_target("incrementalTwo"),
                        "counter": "-1/-1",
                        "count": integer(2),
                    },
                    {
                        "kind": "putCounters",
                        "permanent": chosen_target("incrementalThree"),
                        "counter": "-1/-1",
                        "count": integer(3),
                    },
                ],
            }),
            &["Put one, two, and three counters on three distinct targeted creatures"],
        ));
    }

    if text
        == "As an additional cost to cast this spell, you may choose a creature type and behold two creatures of that type."
    {
        let paid = json!({
            "kind": "selectionContains",
            "selection": decision_result("additionalCostMode"),
            "value": "pay",
        });
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
                            "options": ["decline", "pay"],
                        },
                        {
                            "id": "celestialCreatureType",
                            "kind": "chooseCreatureType",
                            "condition": paid.clone(),
                        },
                    ],
                    "additionalCosts": [{
                        "kind": "conditional",
                        "condition": paid,
                        "then": [{
                            "kind": "behold",
                            "where": {
                                "kind": "chosenCreatureType",
                                "decisionId": "celestialCreatureType",
                            },
                            "count": integer(2),
                        }],
                        "else": [],
                    }],
                },
                "effects": [],
            }),
            &[
                "Offer the optional additional cost",
                "Choose its creature type",
                "Behold exactly two creatures of that type",
            ],
        ));
    }
    if text
        == "Search your library for a creature card with mana value X or less, reveal it, put it into your hand, then shuffle. If this spell's additional cost was paid and the revealed card is the chosen type, put that card onto the battlefield instead of putting it into your hand."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "searchLibrary",
                "player": controller(),
                "where": and(vec![
                    card_type("Creature"),
                    compare(
                        "<=",
                        json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                        json!({ "kind": "sourceCastXValue" }),
                    ),
                ]),
                "maximum": integer(1),
                "destination": "hand",
                "tapped": false,
                "revealSelected": true,
                "battlefieldIfAdditionalCostPaidAndChosenType": {
                    "modeDecisionId": "additionalCostMode",
                    "modeValue": "pay",
                    "creatureTypeDecisionId": "celestialCreatureType",
                },
            })],
            vec![x_value()],
        ));
    }
    if text
        == "Counter all spells your opponents control and all abilities your opponents control. Create a 1/1 blue and black Faerie creature token with flying for each spell and ability countered this way."
    {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "counterOpponentStackObjects",
                    "bindCountAs": "glenElendraCounteredCount",
                }),
                json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": { "kind": "boundValue", "id": "glenElendraCounteredCount" },
                    "token": {
                        "name": "Faerie Token",
                        "colors": ["blue", "black"],
                        "types": ["Creature"],
                        "subtypes": ["Faerie"],
                        "power": 1,
                        "toughness": 1,
                        "abilities": [{ "kind": "flying" }],
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let each_opponent_exile_to_total_re = Regex::new(
        r"(?i)^Each opponent exiles cards from the top of their library until they have exiled cards with total mana value (\d+) or greater this way\. Until end of turn, you may cast cards exiled this way without paying their mana costs\.$",
    )
    .expect("each-opponent cumulative exile regex compiles");
    if let Some(captures) = each_opponent_exile_to_total_re.captures(text) {
        let threshold = captures.get(1)?.as_str().parse::<i64>().ok()?;
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "exileFromTopUntil",
                    "zone": library(json!({
                        "kind": "opponentsOf",
                        "player": controller(),
                    })),
                    "bind": "opponentCardsExiledToManaValue",
                    "faceDown": false,
                    "stopWhen": {
                        "kind": "compare",
                        "operator": ">=",
                        "left": {
                            "kind": "sumManaValues",
                            "objects": bound_objects("opponentCardsExiledToManaValue"),
                            "variableManaSymbolsEqual": 0,
                        },
                        "right": integer(threshold),
                    },
                    "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
                }),
                json!({
                    "kind": "grantPermission",
                    "player": controller(),
                    "action": {
                        "kind": "cast",
                        "card": {
                            "kind": "boundObject",
                            "binding": "opponentCardsExiledToManaValue",
                        },
                        "normalTimingApplies": true,
                        "normalCostsApply": false,
                    },
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
            Vec::new(),
        ));
    }
    if text
        == "End-Blaze Epiphany deals X damage to target creature. When that creature dies this turn, exile a number of cards from the top of your library equal to its power, then choose a card exiled this way. Until the end of your next turn, you may play that card."
    {
        let target = chosen_target("targetCreature");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "recipient": target.clone(),
                    "amount": decision_result("xValue"),
                }),
                json!({
                    "kind": "installDelayedDeathTrigger",
                    "object": target,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                    "effects": [
                        {
                            "kind": "exileTopCards",
                            "zone": library(controller()),
                            "count": { "kind": "deadPermanentPower" },
                            "faceDown": false,
                            "bind": "endBlazeExiledCards",
                        },
                        {
                            "kind": "chooseCards",
                            "id": "endBlazeChosenCard",
                            "player": controller(),
                            "from": bound_objects("endBlazeExiledCards"),
                            "count": integer(1),
                        },
                        {
                            "kind": "grantPermission",
                            "player": controller(),
                            "action": {
                                "kind": "play",
                                "card": {
                                    "kind": "decisionResult",
                                    "decisionId": "endBlazeChosenCard",
                                },
                                "normalTimingApplies": true,
                                "normalCostsApply": true,
                            },
                            "duration": { "kind": "untilEndOfNextTurn" },
                        },
                    ],
                }),
            ],
            vec![
                x_value(),
                target_decision("targetCreature", creature_candidates(), 1, 1),
            ],
        ));
    }
    if text
        == "Target creature you control gains deathtouch and lifelink until end of turn. When that creature dies this turn, create a 2/2 black and green Elf creature token."
    {
        let target = chosen_target("targetCreature");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "grantKeyword",
                    "object": target.clone(),
                    "keyword": "deathtouch",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": target.clone(),
                    "keyword": "lifelink",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "installDelayedDeathTrigger",
                    "object": target,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                    "effects": [{
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": integer(1),
                        "token": {
                            "colors": ["black", "green"],
                            "types": ["Creature"],
                            "subtypes": ["Elf"],
                            "power": integer(2),
                            "toughness": integer(2),
                            "abilities": [],
                        },
                    }],
                }),
            ],
            vec![target_decision(
                "targetCreature",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Creature"),
                }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Create a token that's a copy of target creature you control, except it has haste and \"At the beginning of the end step, sacrifice this token.\""
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "createModifiedTokenCopy",
                "object": chosen_target("copyTarget"),
                "grantKeywords": ["haste"],
                "sacrificeAtNextEndStep": true,
            })],
            vec![target_decision(
                "copyTarget",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Creature"),
                }),
                1,
                1,
            )],
        ));
    }
    if text.starts_with("Choose two")
        && text.contains("Create a token that's a copy of target Goblin you control.")
        && text.contains(
            "Creatures target player controls get +1/+1 and gain haste until end of turn.",
        )
        && text.contains("Target player mills five cards")
    {
        let selected = |mode_name: &str| {
            json!({
                "kind": "selectionContains",
                "selection": decision_result("spellMode"),
                "value": mode_name,
            })
        };
        let conditional_target = |mut decision: Value, mode_name: &str| {
            decision["condition"] = selected(mode_name);
            decision
        };
        let boosted = json!({
            "kind": "eachPermanent",
            "player": chosen_target("boostPlayer"),
            "where": card_type("Creature"),
        });
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("copyGoblin"),
                    "then": [{
                        "kind": "createTokenCopyOfPermanent",
                        "object": chosen_target("copyGoblinTarget"),
                        "grantKeywords": [],
                        "exileAtNextEndStep": false,
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("boostCreatures"),
                    "then": [
                        {
                            "kind": "modifyPowerToughness",
                            "object": boosted.clone(),
                            "power": integer(1),
                            "toughness": integer(1),
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        },
                        {
                            "kind": "grantKeyword",
                            "object": boosted,
                            "keyword": "haste",
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        },
                    ],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("destroyPermanent"),
                    "then": [{
                        "kind": "destroyPermanent",
                        "permanent": chosen_target("destroyTarget"),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("millGoblinCards"),
                    "then": [
                        {
                            "kind": "mill",
                            "player": chosen_target("millPlayer"),
                            "count": integer(5),
                            "bind": "grubMilledCards",
                        },
                        {
                            "kind": "moveCards",
                            "cards": {
                                "kind": "filterObjects",
                                "objects": { "kind": "boundObjects", "binding": "grubMilledCards" },
                                "where": subtype("Goblin"),
                            },
                            "to": hand(chosen_target("millPlayer")),
                        },
                    ],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": integer(2),
                    "maximum": integer(2),
                    "options": ["copyGoblin", "boostCreatures", "destroyPermanent", "millGoblinCards"],
                }),
                conditional_target(
                    target_decision(
                        "copyGoblinTarget",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": subtype("Goblin"),
                        }),
                        1,
                        1,
                    ),
                    "copyGoblin",
                ),
                conditional_target(
                    target_decision("boostPlayer", json!({ "kind": "players" }), 1, 1),
                    "boostCreatures",
                ),
                conditional_target(
                    target_decision(
                        "destroyTarget",
                        json!({
                            "kind": "permanents",
                            "where": or(vec![card_type("Artifact"), card_type("Creature")]),
                        }),
                        1,
                        1,
                    ),
                    "destroyPermanent",
                ),
                conditional_target(
                    target_decision("millPlayer", json!({ "kind": "players" }), 1, 1),
                    "millGoblinCards",
                ),
            ],
        ));
    }
    if text.starts_with("Choose one or both")
        && text.contains("Target opponent exiles two cards from their hand.")
        && text.contains("Remove all counters from target creature.")
    {
        let selected = |mode_name: &str| {
            json!({
                "kind": "selectionContains",
                "selection": decision_result("spellMode"),
                "value": mode_name,
            })
        };
        let mut opponent = target_decision(
            "targetOpponent",
            json!({
                "kind": "players",
                "where": { "kind": "isOpponentOf", "player": controller() },
            }),
            1,
            1,
        );
        opponent["condition"] = selected("exileHand");
        let mut creature = target_decision(
            "targetCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        );
        creature["condition"] = selected("removeCounters");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("exileHand"),
                    "then": [{
                        "kind": "exileCardsFromHand",
                        "player": chosen_target("targetOpponent"),
                        "count": integer(2),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("removeCounters"),
                    "then": [{
                        "kind": "removeAllCounters",
                        "permanent": chosen_target("targetCreature"),
                    }],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": integer(1),
                    "maximum": integer(2),
                    "options": ["exileHand", "removeCounters"],
                }),
                opponent,
                creature,
            ],
        ));
    }
    if text.starts_with("Choose two")
        && text.contains("Create a token that's a copy of target Elf you control.")
        && text
            .contains("Return one or two target permanent cards from your graveyard to your hand.")
        && text
            .contains("Creatures target player controls get +3/+3 until end of turn. Untap them.")
    {
        let selected = |mode_name: &str| {
            json!({
                "kind": "selectionContains",
                "selection": decision_result("spellMode"),
                "value": mode_name,
            })
        };
        let conditional_target = |mut decision: Value, mode_name: &str| {
            decision["condition"] = selected(mode_name);
            decision
        };
        let boosted = json!({
            "kind": "eachPermanent",
            "player": chosen_target("boostPlayer"),
            "where": card_type("Creature"),
        });
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("copyElf"),
                    "then": [{
                        "kind": "createTokenCopyOfPermanent",
                        "object": chosen_target("copyElfTarget"),
                        "grantKeywords": [],
                        "exileAtNextEndStep": false,
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("returnCards"),
                    "then": [{
                        "kind": "moveCards",
                        "cards": { "kind": "chosenTargets", "id": "graveyardCards" },
                        "to": hand(controller()),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("destroyPermanent"),
                    "then": [{
                        "kind": "destroyPermanent",
                        "permanent": chosen_target("destroyTarget"),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("boostCreatures"),
                    "then": [
                        {
                            "kind": "modifyPowerToughness",
                            "object": boosted.clone(),
                            "power": integer(3),
                            "toughness": integer(3),
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        },
                        { "kind": "untapPermanents", "objects": boosted },
                    ],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": integer(2),
                    "maximum": integer(2),
                    "options": ["copyElf", "returnCards", "destroyPermanent", "boostCreatures"],
                }),
                conditional_target(
                    target_decision(
                        "copyElfTarget",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": subtype("Elf"),
                        }),
                        1,
                        1,
                    ),
                    "copyElf",
                ),
                conditional_target(
                    target_decision(
                        "graveyardCards",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": not(or(vec![card_type("Instant"), card_type("Sorcery")])),
                        }),
                        1,
                        2,
                    ),
                    "returnCards",
                ),
                conditional_target(
                    target_decision(
                        "destroyTarget",
                        json!({
                            "kind": "permanents",
                            "where": or(vec![card_type("Creature"), card_type("Enchantment")]),
                        }),
                        1,
                        1,
                    ),
                    "destroyPermanent",
                ),
                conditional_target(
                    target_decision("boostPlayer", json!({ "kind": "players" }), 1, 1),
                    "boostCreatures",
                ),
            ],
        ));
    }
    if text.starts_with("Choose two")
        && text.contains("Create a token that's a copy of target Elemental you control.")
        && text
            .contains("Ashling's Command deals 2 damage to each creature target player controls.")
        && text.contains("Target player creates two Treasure tokens.")
    {
        let selected = |mode_name: &str| {
            json!({
                "kind": "selectionContains",
                "selection": decision_result("spellMode"),
                "value": mode_name,
            })
        };
        let conditional_target = |mut decision: Value, mode_name: &str| {
            decision["condition"] = selected(mode_name);
            decision
        };
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("copyElemental"),
                    "then": [{
                        "kind": "createTokenCopyOfPermanent",
                        "object": chosen_target("copyElementalTarget"),
                        "grantKeywords": [],
                        "exileAtNextEndStep": false,
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("drawTwo"),
                    "then": [{
                        "kind": "drawCards",
                        "player": chosen_target("drawPlayer"),
                        "count": integer(2),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("damageCreatures"),
                    "then": [{
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "amount": integer(2),
                        "recipient": {
                            "kind": "eachPermanent",
                            "player": chosen_target("damagePlayer"),
                            "where": card_type("Creature"),
                        },
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("treasures"),
                    "then": [{
                        "kind": "createTokens",
                        "controller": chosen_target("treasurePlayer"),
                        "quantity": integer(2),
                        "token": { "kind": "namedToken", "name": "Treasure" },
                    }],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": integer(2),
                    "maximum": integer(2),
                    "options": ["copyElemental", "drawTwo", "damageCreatures", "treasures"],
                }),
                conditional_target(
                    target_decision(
                        "copyElementalTarget",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": subtype("Elemental"),
                        }),
                        1,
                        1,
                    ),
                    "copyElemental",
                ),
                conditional_target(
                    target_decision("drawPlayer", json!({ "kind": "players" }), 1, 1),
                    "drawTwo",
                ),
                conditional_target(
                    target_decision("damagePlayer", json!({ "kind": "players" }), 1, 1),
                    "damageCreatures",
                ),
                conditional_target(
                    target_decision("treasurePlayer", json!({ "kind": "players" }), 1, 1),
                    "treasures",
                ),
            ],
        ));
    }
    if text == "Each nonland permanent you control becomes a copy of target non-Aura permanent." {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "makeControlledNonlandsCopiesOfTarget",
                "player": controller(),
                "copy": chosen_target("copyTarget"),
            })],
            vec![target_decision(
                "copyTarget",
                json!({
                    "kind": "permanents",
                    "where": not(subtype("Aura")),
                }),
                1,
                1,
            )],
        ));
    }
    if text.starts_with("Choose one")
        && text.contains("Return target creature card from your graveyard to your hand.")
        && text.contains(
            "Return two target creature cards that share a creature type from your graveyard to your hand.",
        )
    {
        let selected = |mode_name: &str| json!({
            "kind": "selectionContains",
            "selection": decision_result("spellMode"),
            "value": mode_name,
        });
        let graveyard_creatures = json!({
            "kind": "cards",
            "zone": graveyard(controller()),
            "where": card_type("Creature"),
        });
        let mut one_creature = target_decision(
            "oneCreature",
            graveyard_creatures.clone(),
            1,
            1,
        );
        one_creature["condition"] = selected("oneCreature");
        let mut shared_creatures = target_decision(
            "sharedCreatures",
            graveyard_creatures,
            2,
            2,
        );
        shared_creatures["condition"] = selected("sharedCreatures");
        shared_creatures["selectionConstraint"] = json!({ "kind": "shareCardType" });
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("oneCreature"),
                    "then": [{
                        "kind": "moveCards",
                        "cards": { "kind": "chosenTargets", "id": "oneCreature" },
                        "to": hand(controller()),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("sharedCreatures"),
                    "then": [{
                        "kind": "moveCards",
                        "cards": { "kind": "chosenTargets", "id": "sharedCreatures" },
                        "to": hand(controller()),
                    }],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": integer(1),
                    "maximum": integer(1),
                    "options": ["oneCreature", "sharedCreatures"],
                }),
                one_creature,
                shared_creatures,
            ],
        ));
    }
    if text
        .starts_with("Choose one. If this spell's additional cost was paid, choose both instead.")
        && text.contains("Destroy target artifact or enchantment.")
        && text.contains("Destroy target creature with mana value 3 or greater.")
    {
        let selected = |mode_name: &str| {
            json!({
                "kind": "selectionContains",
                "selection": decision_result("spellMode"),
                "value": mode_name,
            })
        };
        let paid = json!({
            "kind": "selectionContains",
            "selection": decision_result("additionalCostMode"),
            "value": "pay",
        });
        let mut artifact_or_enchantment = target_decision(
            "artifactOrEnchantment",
            json!({
                "kind": "permanents",
                "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
            }),
            1,
            1,
        );
        artifact_or_enchantment["condition"] = selected("artifactOrEnchantment");
        let mut large_creature = target_decision(
            "largeCreature",
            json!({
                "kind": "permanents",
                "where": parse_permanent_criteria(
                    "creature with mana value 3 or greater",
                    "",
                )?,
            }),
            1,
            1,
        );
        large_creature["condition"] = selected("largeCreature");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("artifactOrEnchantment"),
                    "then": [{
                        "kind": "destroyPermanent",
                        "permanent": chosen_target("artifactOrEnchantment"),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selected("largeCreature"),
                    "then": [{
                        "kind": "destroyPermanent",
                        "permanent": chosen_target("largeCreature"),
                    }],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": integer(1),
                    "maximum": {
                        "kind": "conditionalValue",
                        "condition": paid,
                        "ifTrue": integer(2),
                        "ifFalse": integer(1),
                    },
                    "options": ["artifactOrEnchantment", "largeCreature"],
                }),
                artifact_or_enchantment,
                large_creature,
            ],
        ));
    }
    if text
        == "Choose exactly two creatures you control. You draw X cards and the chosen creatures get +X/+X and gain trample until end of turn, where X is the difference between the chosen creatures' powers."
    {
        let x = json!({
            "kind": "powerDifferenceOfChosenObjects",
            "decisionId": "chosenCreatures",
        });
        let chosen = json!({ "kind": "chosenObjects", "id": "chosenCreatures" });
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": x.clone(),
                }),
                json!({
                    "kind": "modifyPowerToughness",
                    "object": chosen.clone(),
                    "power": x.clone(),
                    "toughness": x,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen,
                    "keyword": "trample",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
            vec![json!({
                "id": "chosenCreatures",
                "kind": "chooseObjects",
                "quantity": { "kind": "exactly", "value": 2 },
                "candidates": {
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Creature"),
                },
            })],
        ));
    }
    if text == "Boulder Dash deals 2 damage to any target and 1 damage to any other target." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("primaryTarget", json!({ "kind": "anyTarget" }), 1, 1),
                        target_decision("secondaryTarget", json!({ "kind": "anyTarget" }), 1, 1),
                    ],
                },
                "effects": [
                    {
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "amount": integer(2),
                        "recipient": chosen_target("primaryTarget"),
                    },
                    {
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "amount": integer(1),
                        "recipient": chosen_target("secondaryTarget"),
                    },
                ],
            }),
            &[
                "Choose two different damage recipients",
                "Deal two damage to the first",
                "Deal one damage to the second",
            ],
        ));
    }
    let reality_fracture_spell = |operation: &str, choose_x: bool| {
        expansion_spell_rule(
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": operation,
            })],
            choose_x
                .then(|| {
                    vec![json!({
                        "id": "xValue",
                        "kind": "chooseNumber",
                        "minimum": 0,
                    })]
                })
                .unwrap_or_default(),
        )
    };
    if text == "This spell costs {2} less to cast if a creature is attacking you." {
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
                    "condition": { "kind": "controllerIsBeingAttacked" },
                }],
            }),
            &[
                "Check whether the controller is being attacked",
                "Reduce this spell's generic cost by two",
            ],
        ));
    }
    if text
        == "Cinder Strike deals 2 damage to target creature. It deals 4 damage to that creature instead if this spell's additional cost was paid."
    {
        let target = chosen_target("targetCreature");
        let damage_effect = |amount: i64| {
            json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "recipient": target.clone(),
                "amount": integer(amount),
            })
        };
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "conditionalEffect",
                "condition": {
                    "kind": "selectionContains",
                    "selection": {
                        "kind": "decisionResult",
                        "decisionId": "additionalCostMode",
                    },
                    "value": "pay",
                },
                "then": [damage_effect(4)],
                "else": [damage_effect(2)],
            })],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Exile the top two cards of your library. If this spell's additional cost was paid, exile the top three cards instead. Until the end of your next turn, you may play those cards."
    {
        let exile = |count: i64| {
            json!({
                "kind": "exileTopCards",
                "zone": library(controller()),
                "count": integer(count),
                "faceDown": false,
                "bind": "exiledTopCards",
            })
        };
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "selectionContains",
                        "selection": {
                            "kind": "decisionResult",
                            "decisionId": "additionalCostMode",
                        },
                        "value": "pay",
                    },
                    "then": [exile(3)],
                    "else": [exile(2)],
                }),
                json!({
                    "kind": "grantPermission",
                    "player": controller(),
                    "action": {
                        "kind": "play",
                        "card": {
                            "kind": "boundObject",
                            "binding": "exiledTopCards",
                        },
                        "normalTimingApplies": true,
                        "normalCostsApply": true,
                    },
                    "duration": {
                        "kind": "untilEndOfNextTurn",
                        "player": controller(),
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    if text
        == "Gain control of target creature until end of turn. Untap that creature. It gains haste until end of turn. If that creature is a Goat, it also gets +3/+0 until end of turn."
    {
        let target = chosen_target("targetCreature");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "gainControlPermanent",
                    "permanent": target.clone(),
                    "controller": controller(),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({ "kind": "untapPermanent", "permanent": target.clone() }),
                json!({
                    "kind": "grantKeyword",
                    "object": target.clone(),
                    "keyword": "haste",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "objectMatchesFilter",
                        "object": target.clone(),
                        "where": subtype("Goat"),
                    },
                    "then": [{
                        "kind": "modifyPowerToughness",
                        "object": target,
                        "power": integer(3),
                        "toughness": integer(0),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    }],
                    "else": [],
                }),
            ],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }
    if text.starts_with("Target creature gets +3/+2 until end of turn. Create a Treasure token.") {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "modifyPowerToughness",
                    "object": chosen_target("targetCreature"),
                    "power": integer(3),
                    "toughness": integer(2),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": { "kind": "namedToken", "name": "Treasure" },
                }),
            ],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }
    if text == "Target creature gets +3/-3 and loses all creature types until end of turn." {
        let target = chosen_target("targetCreature");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "modifyPowerToughness",
                    "object": target.clone(),
                    "power": integer(3),
                    "toughness": integer(-3),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "removeAllSubtypesUntilEndOfTurn",
                    "object": target,
                }),
            ],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Exile target creature. Its controller creates a 1/1 colorless Shapeshifter creature token with changeling."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "exileCreatureThenControllerCreatesChangeling",
                "creature": chosen_target("targetCreature"),
            })],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }
    if text
        == "The owner of target spell or creature puts it on their choice of the top or bottom of their library."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "ownerChoosesLibraryEndForSpellOrPermanent",
                "object": chosen_target("targetSpellOrCreature"),
            })],
            vec![target_decision(
                "targetSpellOrCreature",
                json!({
                    "kind": "union",
                    "sets": [
                        { "kind": "spells" },
                        { "kind": "permanents", "where": card_type("Creature") },
                    ],
                }),
                1,
                1,
            )],
        ));
    }
    if text
        == "The owner of target nonland permanent puts it into their library second from the top or on the bottom."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "ownerChoosesSecondFromTopOrBottom",
                "permanent": chosen_target("targetPermanent"),
            })],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": not(card_type("Land")),
                }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Return one or two target nonland permanents to their owners' hands. Then if you control a Merfolk, create a 1/1 white and blue Merfolk creature token for each permanent returned to its owner's hand this way."
    {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "returnToOwnersHand",
                    "object": { "kind": "chosenTargets", "id": "targetPermanents" },
                    "bind": "returnedPermanents",
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "controlsPermanent",
                        "player": controller(),
                        "where": subtype("Merfolk"),
                    },
                    "then": [{
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": {
                            "kind": "countBoundObjects",
                            "binding": "returnedPermanents",
                        },
                        "token": {
                            "colors": ["white", "blue"],
                            "types": ["Creature"],
                            "subtypes": ["Merfolk"],
                            "power": 1,
                            "toughness": 1,
                            "abilities": [],
                        },
                    }],
                    "else": [],
                }),
            ],
            vec![target_decision(
                "targetPermanents",
                json!({
                    "kind": "permanents",
                    "where": not(card_type("Land")),
                }),
                1,
                2,
            )],
        ));
    }
    if text
        == "Choose two target creatures controlled by different players. Return those creatures to their owners' hands."
    {
        let mut decision = target_decision(
            "targetCreatures",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            2,
            2,
        );
        decision["selectionConstraint"] = json!({ "kind": "distinctPermanentControllers" });
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": { "kind": "chosenTargets", "id": "targetCreatures" },
            })],
            vec![decision],
        ));
    }
    if text == "Draw three cards. Then discard two cards unless you discard a creature card." {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(3),
                }),
                json!({
                    "kind": "discardTwoUnlessDiscardCreature",
                    "player": controller(),
                }),
            ],
            Vec::new(),
        ));
    }
    if text.starts_with(
        "Exile all creatures. Incubate X, where X is the number of creatures exiled this way.",
    ) {
        return Some(reality_fracture_spell("sunfall", false));
    }
    if text.starts_with("You gain X life. Create X 1/1 colorless Phyrexian Mite artifact creature tokens with toxic 1") {
        return Some(reality_fracture_spell("whiteSunsTwilight", true));
    }
    if text
        == "Draw X cards, then discard X cards. Create a 1/1 white Spirit creature token with flying for each card type among cards discarded this way."
    {
        return Some(reality_fracture_spell("occultEpiphany", true));
    }
    if text
        == "Reveal the top five cards of your library. An opponent separates those cards into two piles. Put one pile into your hand and the other into your graveyard."
    {
        return Some(reality_fracture_spell("factOrFiction", false));
    }
    if text.starts_with("Exile all creatures you control, then reveal cards from the top of your library until you reveal that many creature cards.") {
        return Some(reality_fracture_spell("massPolymorph", false));
    }
    if text.starts_with("Exile all creatures you control. At the beginning of the next end step, reveal cards from the top of your library until you reveal that many creature cards,") {
        return Some(reality_fracture_spell("syntheticDestiny", false));
    }
    if text.starts_with("Choose target opponent. Until that player's next turn, they gain protection from everything and their life total can't change.") {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "teferisReproach",
            })],
            vec![target_decision(
                "targetOpponent",
                json!({
                    "kind": "players",
                    "where": { "kind": "isOpponentOf", "player": controller() },
                }),
                1,
                1,
            )],
        ));
    }
    if text == "Exile two target artifacts." {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "exilePermanent",
                "permanent": { "kind": "chosenTargets", "id": "targetArtifacts" },
            })],
            vec![target_decision(
                "targetArtifacts",
                json!({ "kind": "permanents", "where": card_type("Artifact") }),
                2,
                2,
            )],
        ));
    }
    if text
        == "The owner of target permanent shuffles it into their library, then reveals the top card of their library. If it's a permanent card, they put it onto the battlefield."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "chaosWarp",
            })],
            vec![target_decision(
                "targetPermanent",
                json!({ "kind": "permanents", "where": Value::Null }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Until end of turn, you may activate loyalty abilities of Jace planeswalkers you control on any player's turn any time you could cast an instant."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "allowJaceLoyaltyAtInstantSpeed",
                "player": controller(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    if text
        == "Choose target nonland permanent. Its owner may put it on top of their library. If they do, Clash of Elements deals 2 damage to them. If they didn't put the card on top of their library, they put it on the bottom."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "ownerChoosesLibraryEndOrTakesDamage",
                "permanent": chosen_target("targetPermanent"),
                "damage": integer(2),
            })],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": not(card_type("Land")),
                }),
                1,
                1,
            )],
        ));
    }
    if text
        == "For each creature target player controls, create a token that's a copy of that creature, except it has haste and \"At the beginning of the end step, if you don't control a planeswalker, sacrifice this creature.\""
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "copyTargetPlayersCreaturesWithConditionalSacrifice",
                "player": chosen_target("targetPlayer"),
                "controller": controller(),
            })],
            vec![target_decision(
                "targetPlayer",
                json!({ "kind": "players" }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Draw two cards. Then you may exile this spell and four cards named Sphinx's Approach from your graveyard. If you do, search your library for a Sphinx creature card, put it onto the battlefield, then shuffle."
    {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(2),
                }),
                json!({
                    "kind": "resolveSphinxApproach",
                    "player": controller(),
                }),
            ],
            Vec::new(),
        ));
    }
    if text
        == "You may reveal exactly two cards you own with different names from outside the game. An opponent chooses one of them. You put that card into your hand."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "resolveExtrapolateImpossible",
                "player": controller(),
            })],
            Vec::new(),
        ));
    }
    if text.starts_with("Choose one")
        && text.contains("Draw a card. Empower Jace 2.")
        && text.contains("Return target spell or creature to its owner's hand.")
        && text.contains("Creatures you control get +1/+2 until end of turn.")
    {
        let mode = |name: &str| selection("spellMode", name);
        let mut target = target_decision(
            "targetSpellOrCreature",
            json!({
                "kind": "union",
                "sets": [
                    { "kind": "spells", "where": Value::Null },
                    { "kind": "permanents", "where": card_type("Creature") },
                ],
            }),
            1,
            1,
        );
        target["condition"] = mode("return");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": mode("draw"),
                    "then": [
                        { "kind": "drawCards", "player": controller(), "count": integer(1) },
                        { "kind": "empowerJace", "player": controller(), "count": integer(2) },
                    ],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": mode("return"),
                    "then": [{
                        "kind": "returnToOwnersHand",
                        "object": chosen_target("targetSpellOrCreature"),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": mode("boost"),
                    "then": [{
                        "kind": "modifyPowerToughness",
                        "object": {
                            "kind": "eachPermanent",
                            "player": controller(),
                            "where": card_type("Creature"),
                        },
                        "power": integer(1),
                        "toughness": integer(2),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    }],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "spellMode",
                    "kind": "chooseModes",
                    "minimum": 1,
                    "maximum": 1,
                    "options": ["draw", "return", "boost"],
                }),
                target,
            ],
        ));
    }
    if text
        == "Create two 2/2 colorless Wizard Soldier creature tokens named Cadet. If this spell was cast from a graveyard, put a +1/+1 counter on each of them for every three cards in your graveyard."
    {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(2),
                    "bind": "cadets",
                    "token": {
                        "name": "Cadet",
                        "colors": [],
                        "types": ["Creature"],
                        "subtypes": ["Wizard", "Soldier"],
                        "power": 2,
                        "toughness": 2,
                    },
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "wasCastFromZone",
                        "object": self_ref(),
                        "zone": "graveyard",
                    },
                    "then": [{
                        "kind": "putCounters",
                        "permanent": bound_objects("cadets"),
                        "counter": "+1/+1",
                        "count": {
                            "kind": "divide",
                            "left": {
                                "kind": "countCards",
                                "zone": graveyard(controller()),
                                "where": Value::Null,
                            },
                            "right": integer(3),
                            "round": "down",
                        },
                    }],
                    "else": [],
                }),
            ],
            Vec::new(),
        ));
    }
    if text
        == "Target creature or planeswalker an opponent controls that's red or white loses all abilities until end of turn. Target creature you control deals damage equal to its power to that permanent."
    {
        let opposing = chosen_target("targetOpposingPermanent");
        let creature = chosen_target("targetControlledCreature");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "installLoseAllAbilities",
                    "object": opposing.clone(),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "dealDamage",
                    "source": creature.clone(),
                    "recipient": opposing,
                    "amount": { "kind": "powerOf", "object": creature },
                }),
            ],
            vec![
                target_decision(
                    "targetOpposingPermanent",
                    json!({
                        "kind": "permanents",
                        "controller": { "kind": "opponentsOf", "player": controller() },
                        "where": and(vec![
                            or(vec![card_type("Creature"), card_type("Planeswalker")]),
                            or(vec![color_filter("red")?, color_filter("white")?]),
                        ]),
                    }),
                    1,
                    1,
                ),
                target_decision(
                    "targetControlledCreature",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Creature"),
                    }),
                    1,
                    1,
                ),
            ],
        ));
    }
    if text.starts_with(
        "Exile all creatures. Empower Jace X, where X is the number of creatures exiled this way.",
    ) {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "exilePermanent",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": card_type("Creature"),
                    },
                    "bindCountAs": "exiledCreatureCount",
                }),
                json!({
                    "kind": "empowerJace",
                    "player": controller(),
                    "count": { "kind": "boundValue", "id": "exiledCreatureCount" },
                }),
            ],
            Vec::new(),
        ));
    }
    if text
        == "Exile target creature or planeswalker. If that permanent's mana value was 3 or less, return it to the battlefield tapped under your control. Exile it at the beginning of the next end step."
    {
        let target = chosen_target("targetPermanent");
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "bind",
                    "id": "targetManaValue",
                    "value": { "kind": "manaValueOf", "object": target.clone() },
                }),
                json!({ "kind": "exilePermanent", "permanent": target.clone() }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": compare(
                        "<=",
                        json!({ "kind": "boundValue", "id": "targetManaValue" }),
                        integer(3),
                    ),
                    "then": [{
                        "kind": "moveTargetCard",
                        "card": target,
                        "to": "battlefield",
                        "controller": controller(),
                        "tapped": true,
                        "exileAtNextEndStep": true,
                    }],
                    "else": [],
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Essence Burn deals 5 damage to target black or green creature or planeswalker. If that permanent would die this turn, exile it instead."
    {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "recipient": chosen_target("targetPermanent"),
                    "amount": integer(5),
                }),
                json!({
                    "kind": "installDeathExileReplacement",
                    "object": chosen_target("targetPermanent"),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": and(vec![
                        or(vec![color_filter("black")?, color_filter("green")?]),
                        or(vec![card_type("Creature"), card_type("Planeswalker")]),
                    ]),
                }),
                1,
                1,
            )],
        ));
    }
    if text.starts_with(
        "Violent Echoes deals 6 damage to target creature or planeswalker. If excess damage was dealt to that permanent this way, empower Jace X",
    ) {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "recipient": chosen_target("targetPermanent"),
                    "amount": integer(6),
                    "bindExcessAs": "excessDamage",
                }),
                json!({
                    "kind": "empowerJace",
                    "player": controller(),
                    "count": { "kind": "boundValue", "id": "excessDamage" },
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                }),
                1,
                1,
            )],
        ));
    }
    if text == "Destroy target creature. If it wasn't attacking, its controller draws a card." {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "destroyCreatureThenDrawIfNotAttacking",
            })],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }
    if text
        == "Draw X cards, where X is the number of cards that were put into target player's graveyard from their library this turn."
    {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": {
                    "kind": "countEventsThisTurn",
                    "event": "cardMilled",
                    "player": chosen_target("targetPlayer"),
                },
            })],
            vec![target_decision(
                "targetPlayer",
                json!({ "kind": "players" }),
                1,
                1,
            )],
        ));
    }
    if text == "Draw a card. If this spell wasn't cast from your hand, draw two cards instead." {
        return Some(expansion_spell_rule(
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": {
                    "kind": "conditionalValue",
                    "condition": {
                        "kind": "not",
                        "operand": { "kind": "wasCastFromHand", "object": self_ref() },
                    },
                    "ifTrue": integer(2),
                    "ifFalse": integer(1),
                },
            })],
            Vec::new(),
        ));
    }
    if text.starts_with("Choose one")
        && text.contains("Draw cards equal to the greatest power among creatures you control.")
        && text.contains("All creatures get -3/-3 until end of turn.")
    {
        return Some(expansion_spell_rule(
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": selection("spellMode", "draw"),
                    "then": [
                        {
                            "kind": "drawCards",
                            "player": controller(),
                            "count": {
                                "kind": "greatestPower",
                                "player": controller(),
                                "where": card_type("Creature"),
                            },
                            "bind": "drawnCards",
                        },
                        {
                            "kind": "loseLife",
                            "player": controller(),
                            "amount": {
                                "kind": "countObjects",
                                "objects": bound_objects("drawnCards"),
                            },
                        },
                    ],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": selection("spellMode", "weaken"),
                    "then": [{
                        "kind": "modifyPowerToughness",
                        "object": {
                            "kind": "eachPermanent",
                            "where": card_type("Creature"),
                        },
                        "power": integer(-3),
                        "toughness": integer(-3),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    }],
                    "else": [],
                }),
            ],
            vec![json!({
                "id": "spellMode",
                "kind": "chooseModes",
                "minimum": 1,
                "maximum": 1,
                "options": ["draw", "weaken"],
            })],
        ));
    }
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
