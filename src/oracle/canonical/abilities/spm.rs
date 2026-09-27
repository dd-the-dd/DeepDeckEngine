use super::super::*;

pub(in crate::oracle::canonical) fn parse_spm_leaf_ability(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let triggered = |event: Value, declaration: Option<Value>, effects: Vec<Value>| {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": effects,
        });
        if let Some(declaration) = declaration {
            rule["declaration"] = declaration;
        }
        draft(
            rule,
            &["Recognize the SPM trigger", "Resolve its canonical effects"],
        )
    };

    if text.starts_with("{6}{B}, {T}, Exile a creature you control: Harness The Soul Stone.") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{6}{B}" },
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "exileObject",
                        "object": chosen_target("harnessCreature"),
                    },
                ],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "harnessCreature",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "setPermanentFlag",
                    "object": self_ref(),
                    "flag": "harnessed",
                    "value": true,
                }],
            }),
            &["Pay the harness costs", "Mark The Soul Stone as harnessed"],
        ));
    }

    if text.ends_with(
        "At the beginning of your upkeep, return target creature card from your graveyard to the battlefield.",
    ) && (text.starts_with('∞') || text.starts_with("âˆž"))
    {
        let mut parsed = triggered(
            json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "creatureCard",
                    json!({
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": card_type("Creature"),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("creatureCard"),
                "to": "battlefield",
                "tapped": false,
                "controller": controller(),
            })],
        );
        parsed.rule["condition"] = json!({ "kind": "sourceFlagSet", "flag": "harnessed" });
        return Some(parsed);
    }

    if text.starts_with(
        "Whenever another creature you control enters, you gain 1 life. If it's a Spider, put a +1/+1 counter on it.",
    ) {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Creature"),
                "excludeSource": true,
            }),
            None,
            vec![
                json!({
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(1),
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "objectMatchesFilter",
                        "object": { "kind": "triggeringPermanent" },
                        "where": subtype("Spider"),
                    },
                    "then": [{
                        "kind": "putCounters",
                        "permanent": { "kind": "triggeringPermanent" },
                        "counter": "+1/+1",
                        "count": integer(1),
                    }],
                    "else": [],
                }),
            ],
        ));
    }

    if text
        == "At the beginning of your end step, if two or more creatures entered the battlefield under your control this turn, you draw a card and gain 2 life."
    {
        let mut parsed = triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            None,
            vec![
                json!({ "kind": "drawCards", "player": controller(), "count": integer(1) }),
                json!({ "kind": "gainLife", "player": controller(), "amount": integer(2) }),
            ],
        );
        parsed.rule["condition"] = json!({
            "kind": "controlledPermanentsEnteredThisTurn",
            "minimum": 2,
            "where": card_type("Creature"),
        });
        return Some(parsed);
    }

    if text
        == "Target creature gets +2/+2 and gains flying until end of turn. If it's a Spider, you gain 2 life."
    {
        let target = chosen_target("targetCreature");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({ "kind": "permanents", "where": card_type("Creature") }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": target.clone(),
                        "power": integer(2),
                        "toughness": integer(2),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": target.clone(),
                        "keyword": "flying",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": {
                            "kind": "objectMatchesFilter",
                            "object": target,
                            "where": subtype("Spider"),
                        },
                        "then": [{
                            "kind": "gainLife",
                            "player": controller(),
                            "amount": integer(2),
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Target and strengthen the creature",
                "Gain life if it is a Spider",
            ],
        ));
    }

    if text
        == "Return target nonland permanent to its owner's hand. If this spell was kicked, draw a card."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
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
                },
                "effects": [
                    {
                        "kind": "returnToOwnersHand",
                        "object": chosen_target("targetPermanent"),
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": { "kind": "wasKicked", "spell": self_ref() },
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
                "Return the nonland permanent",
                "Draw a card if Whoosh was kicked",
            ],
        ));
    }

    if text
        == "When Molten Man enters, search your library for a basic Mountain card, put it onto the battlefield tapped, then shuffle."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            None,
            search_library_effects(
                and(vec![
                    json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                    subtype("Mountain"),
                ]),
                1,
                "battlefield",
                true,
            ),
        ));
    }

    if text == "When Molten Man leaves the battlefield, sacrifice a land." {
        return Some(triggered(
            json!({ "kind": "permanentLeftBattlefield", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "sacrificePermanents",
                "player": controller(),
                "where": card_type("Land"),
                "count": integer(1),
            })],
        ));
    }

    if text
        == "Whenever Spinneret and Spiderling deals 4 or more damage, exile the top card of your library. Until the end of your next turn, you may play that card."
    {
        return Some(triggered(
            json!({
                "kind": "sourceDealtDamageAtLeast",
                "object": self_ref(),
                "amount": 4,
            }),
            None,
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(controller()),
                    "count": integer(1),
                    "faceDown": false,
                    "bind": "spinneretExiledCard",
                }),
                json!({
                    "kind": "grantPermission",
                    "player": controller(),
                    "action": {
                        "kind": "play",
                        "card": { "kind": "boundObject", "binding": "spinneretExiledCard" },
                        "normalTimingApplies": true,
                        "normalCostsApply": true,
                    },
                    "duration": { "kind": "untilEndOfNextTurn", "player": controller() },
                }),
            ],
        ));
    }

    if text.starts_with("This creature has flying as long as it's modified.") {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": self_ref(),
                    "keyword": "flying",
                    "condition": { "kind": "sourceIsModified" },
                }],
            }),
            &[
                "Check whether Skyward Spider is modified",
                "Grant flying while modified",
            ],
        ));
    }

    if text
        == "At the beginning of combat on your turn, other Spiders you control gain flying, first strike, trample, lifelink, and haste until end of turn."
    {
        let spiders = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": and(vec![card_type("Creature"), subtype("Spider")]),
            "excludeSource": true,
        });
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
            None,
            ["flying", "firstStrike", "trample", "lifelink", "haste"]
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "object": spiders.clone(),
                        "keyword": keyword,
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    })
                })
                .collect(),
        ));
    }

    if text.starts_with("When J. Jonah Jameson enters, suspect up to one target creature.") {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    0,
                    1,
                )],
            })),
            vec![json!({
                "kind": "suspectPermanent",
                "object": chosen_target("targetCreature"),
            })],
        ));
    }

    if text == "Whenever a creature you control with menace attacks, create a Treasure token." {
        return Some(triggered(
            json!({
                "kind": "controlledCreatureDeclaredAttacker",
                "player": controller(),
                "where": and(vec![
                    card_type("Creature"),
                    json!({ "kind": "hasKeyword", "value": "menace" }),
                ]),
            }),
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": { "kind": "namedToken", "name": "Treasure" },
            })],
        ));
    }

    if text.starts_with(
        "Whenever a creature you control attacks alone, put a +1/+1 counter on it. Then surveil X, where X is the number of counters on it.",
    ) {
        return Some(triggered(
            json!({
                "kind": "controlledCreatureDeclaredAttacker",
                "player": controller(),
                "where": card_type("Creature"),
                "attackingAlone": true,
            }),
            None,
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": { "kind": "triggeringPermanent" },
                    "counter": "+1/+1",
                    "count": integer(1),
                }),
                json!({
                    "kind": "surveil",
                    "player": controller(),
                    "count": {
                        "kind": "countCounters",
                        "object": { "kind": "triggeringPermanent" },
                    },
                }),
            ],
        ));
    }

    if text
        == "When this artifact enters, look at the top five cards of your library. You may reveal up to two creature cards from among them and put them into your hand. Put the rest on the bottom of your library in a random order."
    {
        let looked = bound_objects("lookedCards");
        let chosen = decision_result("pictureCreatures");
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            None,
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(5),
                    "bind": "lookedCards",
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "pictureCreatures",
                    "player": controller(),
                    "from": looked.clone(),
                    "where": card_type("Creature"),
                    "minimum": integer(0),
                    "maximum": integer(2),
                }),
                json!({ "kind": "revealCards", "cards": chosen.clone() }),
                json!({ "kind": "moveCards", "cards": chosen.clone(), "to": hand(controller()) }),
                json!({
                    "kind": "moveCards",
                    "cards": { "kind": "setDifference", "left": looked, "right": chosen },
                    "to": { "kind": "library", "player": controller(), "position": "bottom" },
                    "order": { "kind": "random" },
                }),
            ],
        ));
    }

    if text == "You may cast Spider spells and noncreature spells from the top of your library." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "playCardsFromTopLibrary",
                    "player": controller(),
                    "where": or(vec![subtype("Spider"), not(card_type("Creature"))]),
                }],
            }),
            &[
                "Inspect the top library card",
                "Permit Spider and noncreature spells",
            ],
        ));
    }

    if text
        == "Top of the Food Chain â€” Kraven's power is equal to the greatest mana value among permanents you control."
        || text
            == "Top of the Food Chain — Kraven's power is equal to the greatest mana value among permanents you control."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "setPower",
                    "objects": self_ref(),
                    "power": {
                        "kind": "greatestManaValue",
                        "player": controller(),
                        "where": Value::Null,
                    },
                }],
            }),
            &[
                "Find the greatest controlled permanent mana value",
                "Set Kraven's power",
            ],
        ));
    }

    if text
        == "Whenever Rhino attacks, if you've cast a spell with mana value 4 or greater this turn, draw a card."
    {
        let mut parsed = triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        );
        parsed.rule["condition"] =
            json!({ "kind": "castSpellManaValueAtLeastThisTurn", "minimum": 4 });
        return Some(parsed);
    }

    if text.starts_with(
        "{1}{B}, Sacrifice this artifact: Mill four cards. You may put a creature card from among them into your hand.",
    ) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{1}{B}" },
                    { "kind": "sacrificePermanent", "permanent": self_ref() },
                ],
                "effects": [
                    {
                        "kind": "mill",
                        "player": controller(),
                        "count": integer(4),
                        "bind": "milledCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "milledCreature",
                        "player": controller(),
                        "from": bound_objects("milledCards"),
                        "where": card_type("Creature"),
                        "minimum": integer(0),
                        "maximum": integer(1),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("milledCreature"),
                        "to": hand(controller()),
                    },
                ],
            }),
            &["Pay and sacrifice Eerie Gravestone", "Mill four and recover a creature"],
        ));
    }

    if text == "Equipped creature gets +2/+2 and is a Spider Hero in addition to its other types." {
        let equipped = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": equipped.clone(),
                        "power": integer(2),
                        "toughness": integer(2),
                    },
                    { "kind": "addSubtype", "objects": equipped.clone(), "subtype": "Spider" },
                    { "kind": "addSubtype", "objects": equipped, "subtype": "Hero" },
                ],
            }),
            &[
                "Strengthen the equipped creature",
                "Add Spider and Hero types",
            ],
        ));
    }

    if text == "Spiders you control get +1/+1 and can't be blocked by creatures with defender." {
        let spiders = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": and(vec![card_type("Creature"), subtype("Spider")]),
        });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": spiders.clone(),
                        "power": integer(1),
                        "toughness": integer(1),
                    },
                    {
                        "kind": "blockRestriction",
                        "attackers": spiders,
                        "blockers": {
                            "kind": "permanents",
                            "where": json!({ "kind": "hasKeyword", "value": "defender" }),
                        },
                    },
                ],
            }),
            &[
                "Strengthen controlled Spiders",
                "Prevent defender creatures from blocking them",
            ],
        ));
    }

    if text.starts_with(
        "Target player draws a card, then up to one target creature you control connives.",
    ) {
        return Some(draft(
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
                        target_decision(
                            "connivingCreature",
                            json!({
                                "kind": "permanents",
                                "controller": controller(),
                                "where": card_type("Creature"),
                            }),
                            0,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "drawCards",
                        "player": chosen_target("targetPlayer"),
                        "count": integer(1),
                    },
                    {
                        "kind": "connive",
                        "permanent": chosen_target("connivingCreature"),
                        "player": controller(),
                    },
                ],
            }),
            &[
                "Target the player and optional creature",
                "Draw, then connive",
            ],
        ));
    }

    if text
        == "Enchanted creature gets +1/+1, has menace, and is a Symbiote in addition to its other types."
    {
        let enchanted = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": enchanted.clone(),
                        "power": integer(1),
                        "toughness": integer(1),
                    },
                    { "kind": "grantKeyword", "objects": enchanted.clone(), "keyword": "menace" },
                    { "kind": "addSubtype", "objects": enchanted, "subtype": "Symbiote" },
                ],
            }),
            &[
                "Strengthen the enchanted creature",
                "Grant menace and the Symbiote type",
            ],
        ));
    }

    if text
        == "Put target creature on the bottom of its owner's library. You lose 2 life unless you control a Villain."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({ "kind": "permanents", "where": card_type("Creature") }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "putPermanentOnBottomOwnersLibrary",
                        "object": chosen_target("targetCreature"),
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": {
                            "kind": "not",
                            "operand": {
                                "kind": "controlsPermanent",
                                "player": controller(),
                                "where": subtype("Villain"),
                            },
                        },
                        "then": [{
                            "kind": "loseLife",
                            "player": controller(),
                            "amount": integer(2),
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Put the target creature on the bottom",
                "Lose life unless controlling a Villain",
            ],
        ));
    }

    if text
        == "Target creature deals damage equal to its power to itself. If that creature is attacking, Wisecrack deals 2 damage to that creature's controller."
    {
        let creature = chosen_target("targetCreature");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({ "kind": "permanents", "where": card_type("Creature") }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "bind",
                        "id": "targetController",
                        "value": { "kind": "controllerOf", "object": creature.clone() },
                    },
                    {
                        "kind": "dealDamage",
                        "source": creature.clone(),
                        "recipient": creature.clone(),
                        "amount": { "kind": "powerOf", "object": creature.clone() },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": { "kind": "objectIsAttacking", "object": creature },
                        "then": [{
                            "kind": "dealDamage",
                            "source": self_ref(),
                            "recipient": { "kind": "boundValue", "id": "targetController" },
                            "amount": integer(2),
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Make the creature damage itself",
                "Deal two more if it is attacking",
            ],
        ));
    }

    if text
        == "When Scarlet Spider enters, you may discard a card. If you do, put a +1/+1 counter on him."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(1),
                },
                "onPerformed": [{
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            })],
        ));
    }

    if text
        == "At the beginning of your first main phase, you may discard a card. When you do, target creature can't block this turn."
    {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "precombatMain", "player": controller() }),
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
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(1),
                },
                "onPerformed": [{
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "cantBlock",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            })],
        ));
    }

    if text
        == "When Spiders-Man enters, if they were cast using web-slinging, you gain 3 life and create two 2/1 green Spider creature tokens with reach."
    {
        let mut parsed = triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            None,
            vec![
                json!({ "kind": "gainLife", "player": controller(), "amount": integer(3) }),
                json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(2),
                    "token": {
                        "name": "Spider Token",
                        "colors": ["green"],
                        "types": ["Creature"],
                        "subtypes": ["Spider"],
                        "power": 2,
                        "toughness": 1,
                        "keywords": [{ "kind": "reach" }],
                    },
                }),
            ],
        );
        parsed.rule["condition"] = json!({
            "kind": "or",
            "operands": [
                { "kind": "sourceFlagSet", "flag": "castWithAlternativeCost:webSlinging" },
                { "kind": "sourceFlagSet", "flag": "castWithAlternativeCost:grantedWebSlinging" },
            ],
        });
        return Some(parsed);
    }

    if text
        == "Sensational Save — If Scarlet Spider was cast using web-slinging, he enters with X +1/+1 counters on him, where X is the mana value of the returned creature."
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [{
                    "kind": "conditional",
                    "condition": {
                        "kind": "or",
                        "operands": [
                            { "kind": "wasCastWithAlternativeCost", "mode": "webSlinging" },
                            { "kind": "wasCastWithAlternativeCost", "mode": "grantedWebSlinging" },
                        ],
                    },
                    "then": [{
                        "kind": "putEnteringCounters",
                        "counter": "+1/+1",
                        "count": {
                            "kind": "decisionResult",
                            "decisionId": "alternativeReturnedManaValue",
                        },
                    }],
                    "else": [],
                }],
            }),
            &[
                "Detect web-slinging as the casting method",
                "Use the returned creature's mana value",
                "Put that many +1/+1 counters on Scarlet Spider as he enters",
            ],
        ));
    }

    if text.ends_with(
        "Put a +1/+1 counter on target creature you control. It becomes a legendary Spider Hero in addition to its other types.",
    ) && (text.starts_with("II —") || text.starts_with("II â€”"))
    {
        let target = chosen_target("targetCreature");
        return Some(triggered(
            json!({ "kind": "sagaChapterReached", "object": self_ref(), "chapters": [integer(2)] }),
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
                    "permanent": target.clone(),
                    "counter": "+1/+1",
                    "count": integer(1),
                }),
                json!({
                    "kind": "addPermanentCharacteristics",
                    "object": target,
                    "supertypes": ["Legendary"],
                    "subtypes": ["Spider", "Hero"],
                }),
            ],
        ));
    }

    if text
        == "Whenever you cast a blue spell, if Hydro-Man is a creature, he gets +1/+1 until end of turn."
    {
        let mut parsed = triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": { "kind": "colorContains", "value": "blue" },
            }),
            None,
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": self_ref(),
                "power": integer(1),
                "toughness": integer(1),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        );
        parsed.rule["condition"] = json!({ "kind": "sourceIsCreature" });
        return Some(parsed);
    }

    if text
        == "At the beginning of combat on your turn, target non-Equipment artifact you control becomes an artifact creature with base power and toughness 3/3 until end of turn. Untap it."
    {
        let artifact = chosen_target("targetArtifact");
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetArtifact",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![card_type("Artifact"), not(subtype("Equipment"))]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "becomeCreature",
                    "object": artifact.clone(),
                    "addTypes": ["Creature"],
                    "addSubtypes": [],
                    "basePower": 3,
                    "baseToughness": 3,
                    "retainExistingTypes": true,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({ "kind": "untapPermanent", "permanent": artifact }),
            ],
        ));
    }

    if text
        == "Whenever this Vehicle attacks or blocks, it gets +1/+1 until end of turn for each Spider you control."
    {
        let spider_count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": and(vec![card_type("Creature"), subtype("Spider")]),
        });
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "declaredAttacker", "object": self_ref() },
                    { "kind": "declaredBlocker", "object": self_ref() },
                ],
            }),
            None,
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": self_ref(),
                "power": spider_count.clone(),
                "toughness": spider_count,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }

    if text.starts_with(
        "Enchanted creature is a Citizen with base power and toughness 1/1. It has defender and loses all other abilities.",
    ) {
        let enchanted = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    { "kind": "loseAllAbilities", "objects": enchanted.clone() },
                    { "kind": "setSubtypes", "objects": enchanted.clone(), "subtypes": ["Citizen"] },
                    {
                        "kind": "setBasePowerToughness",
                        "objects": enchanted.clone(),
                        "power": integer(1),
                        "toughness": integer(1),
                    },
                    { "kind": "grantKeyword", "objects": enchanted, "keyword": "defender" },
                ],
            }),
            &["Replace the enchanted creature's abilities and subtype", "Set it to a 1/1 defender"],
        ));
    }

    if text
        == "Enchanted land has \"{1}, {T}: Target creature gets +1/+1 until end of turn for each creature you control. Activate only as a sorcery.\""
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantActivatedAbility",
                    "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "ability": {
                        "kind": "activatedAbility",
                        "source": { "kind": "abilitySource" },
                        "costs": [
                            { "kind": "payMana", "manaCost": "{1}" },
                            { "kind": "tap", "object": { "kind": "abilitySource" } },
                        ],
                        "activationCondition": { "kind": "sorceryTiming" },
                        "declaration": {
                            "kind": "castingDeclaration",
                            "decisions": [target_decision(
                                "targetCreature",
                                json!({ "kind": "permanents", "where": card_type("Creature") }),
                                1,
                                1,
                            )],
                        },
                        "effects": [{
                            "kind": "modifyPowerToughness",
                            "object": chosen_target("targetCreature"),
                            "power": {
                                "kind": "countPermanents",
                                "player": { "kind": "controllerOf", "object": { "kind": "abilitySource" } },
                                "where": card_type("Creature"),
                            },
                            "toughness": {
                                "kind": "countPermanents",
                                "player": { "kind": "controllerOf", "object": { "kind": "abilitySource" } },
                                "where": card_type("Creature"),
                            },
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        }],
                    },
                }],
            }),
            &[
                "Grant the enchanted land the activated ability",
                "Scale the bonus by controlled creatures",
            ],
        ));
    }

    if text
        == "Equipped creature gets +1/+1 and has reach and \"Whenever this creature attacks, tap target creature an opponent controls.\""
    {
        let equipped = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": equipped.clone(),
                        "power": integer(1),
                        "toughness": integer(1),
                    },
                    { "kind": "grantKeyword", "objects": equipped.clone(), "keyword": "reach" },
                    {
                        "kind": "grantTriggeredAbility",
                        "objects": equipped,
                        "ability": {
                            "kind": "triggeredAbility",
                            "source": { "kind": "abilitySource" },
                            "event": { "kind": "declaredAttacker", "object": { "kind": "abilitySource" } },
                            "declaration": {
                                "kind": "castingDeclaration",
                                "decisions": [target_decision(
                                    "targetCreature",
                                    json!({
                                        "kind": "permanents",
                                        "controller": { "kind": "opponentsOf", "player": { "kind": "controllerOf", "object": { "kind": "abilitySource" } } },
                                        "where": card_type("Creature"),
                                    }),
                                    1,
                                    1,
                                )],
                            },
                            "effects": [{ "kind": "tapPermanent", "permanent": chosen_target("targetCreature") }],
                        },
                    },
                ],
            }),
            &["Strengthen and grant reach", "Grant the attack tap trigger"],
        ));
    }

    if text
        == "All creatures get -2/-2 until end of turn. If this spell's mayhem cost was paid, creatures your opponents control get -2/-2 until end of turn instead."
    {
        let modifier = |object: Value| {
            json!({
                "kind": "modifyPowerToughness",
                "object": object,
                "power": integer(-2),
                "toughness": integer(-2),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })
        };
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "conditionalEffect",
                    "condition": { "kind": "wasCastWithAlternativeCost", "mode": "mayhem" },
                    "then": [modifier(json!({
                        "kind": "eachPermanent",
                        "player": { "kind": "opponentsOf", "player": controller() },
                        "where": card_type("Creature"),
                    }))],
                    "else": [modifier(json!({
                        "kind": "eachPermanent",
                        "where": card_type("Creature"),
                    }))],
                }],
            }),
            &[
                "Check whether mayhem paid for the spell",
                "Shrink opponents' or all creatures",
            ],
        ));
    }

    if text
        == "Until end of turn, target creature you control gains indestructible and \"Whenever this creature is dealt damage, put that many +1/+1 counters on it.\""
    {
        let target = chosen_target("targetCreature");
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
                            "controller": controller(),
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "grantKeyword",
                        "object": target.clone(),
                        "keyword": "indestructible",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantAbility",
                        "object": target,
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                        "ability": {
                            "kind": "triggeredAbility",
                            "source": { "kind": "abilitySource" },
                            "event": { "kind": "selfDealtDamage", "object": { "kind": "abilitySource" } },
                            "effects": [{
                                "kind": "putCounters",
                                "permanent": { "kind": "abilitySource" },
                                "counter": "+1/+1",
                                "count": { "kind": "decisionResult", "decisionId": "damageAmount" },
                            }],
                        },
                    },
                ],
            }),
            &[
                "Grant indestructible",
                "Grant the damage-to-counters trigger for the turn",
            ],
        ));
    }

    if text.starts_with(
        "Equipped creature gets +2/+2 and has \"Whenever this creature deals combat damage to a player, draw a card for each modified creature you control.\"",
    ) {
        let equipped = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": equipped.clone(),
                        "power": integer(2),
                        "toughness": integer(2),
                    },
                    {
                        "kind": "grantTriggeredAbility",
                        "objects": equipped,
                        "ability": {
                            "kind": "triggeredAbility",
                            "source": { "kind": "abilitySource" },
                            "event": { "kind": "combatDamageToPlayer", "source": { "kind": "abilitySource" } },
                            "effects": [{
                                "kind": "drawCards",
                                "player": { "kind": "controllerOf", "object": { "kind": "abilitySource" } },
                                "count": {
                                    "kind": "countPermanents",
                                    "player": { "kind": "controllerOf", "object": { "kind": "abilitySource" } },
                                    "where": and(vec![card_type("Creature"), json!({ "kind": "isModified" })]),
                                },
                            }],
                        },
                    },
                ],
            }),
            &["Strengthen the equipped creature", "Grant the modified-creature draw trigger"],
        ));
    }

    if text
        == "Dinosaur Formula â€” {1}{R}, Discard this card: Until end of turn, target creature you control gets +3/+1 and becomes a Dinosaur in addition to its other types."
        || text
            == "Dinosaur Formula — {1}{R}, Discard this card: Until end of turn, target creature you control gets +3/+1 and becomes a Dinosaur in addition to its other types."
    {
        let target = chosen_target("targetCreature");
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "hand",
                "costs": [
                    { "kind": "payMana", "manaCost": "{1}{R}" },
                    { "kind": "discardCard", "card": self_ref() },
                ],
                "declaration": {
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
                },
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": target.clone(),
                        "power": integer(3),
                        "toughness": integer(1),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "addSubtypeToPermanent",
                        "object": target,
                        "subtype": "Dinosaur",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Discard Stegron and pay mana",
                "Strengthen the target and add Dinosaur",
            ],
        ));
    }

    if text
        == "Whenever this creature deals combat damage to a player, look at that many cards from the top of your library. Put one of them into your hand and the rest into your graveyard."
    {
        let looked = bound_objects("symbioteLookedCards");
        let chosen = decision_result("symbioteHandCard");
        return Some(triggered(
            json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
            None,
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": { "kind": "decisionResult", "decisionId": "damageAmount" },
                    "bind": "symbioteLookedCards",
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "symbioteHandCard",
                    "player": controller(),
                    "from": looked.clone(),
                    "where": Value::Null,
                    "minimum": integer(1),
                    "maximum": integer(1),
                }),
                json!({ "kind": "moveCards", "cards": chosen.clone(), "to": hand(controller()) }),
                json!({
                    "kind": "moveCards",
                    "cards": { "kind": "setDifference", "left": looked, "right": chosen },
                    "to": graveyard(controller()),
                }),
            ],
        ));
    }

    if text
        == "Find New Host â€” {2}{U/B}, Exile this card from your graveyard: Put a +1/+1 counter on target creature you control. It gains this card's other abilities. Activate only as a sorcery."
        || text
            == "Find New Host — {2}{U/B}, Exile this card from your graveyard: Put a +1/+1 counter on target creature you control. It gains this card's other abilities. Activate only as a sorcery."
    {
        let target = chosen_target("targetCreature");
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "graveyard",
                "costs": [
                    { "kind": "payMana", "manaCost": "{2}{U/B}" },
                    { "kind": "exileSource", "object": self_ref(), "zone": "graveyard" },
                ],
                "activationCondition": { "kind": "sorceryTiming" },
                "declaration": {
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
                },
                "effects": [
                    {
                        "kind": "putCounters",
                        "permanent": target.clone(),
                        "counter": "+1/+1",
                        "count": integer(1),
                    },
                    { "kind": "grantSourceOtherAbilities", "object": target },
                ],
            }),
            &[
                "Exile Symbiote Spider-Man from the graveyard",
                "Empower the new host and grant its other abilities",
            ],
        ));
    }

    if text
        == "When Carnage enters, return target creature card with mana value 3 or less from your graveyard to the battlefield. It gains \"This creature attacks each combat if able\" and \"When this creature deals combat damage to a player, sacrifice it.\""
    {
        let creature = chosen_target("creatureCard");
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "creatureCard",
                    json!({
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": and(vec![
                            card_type("Creature"),
                            compare(
                                "<=",
                                json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                                integer(3),
                            ),
                        ]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "moveTargetCard",
                    "card": creature.clone(),
                    "to": "battlefield",
                    "tapped": false,
                    "controller": controller(),
                }),
                json!({
                    "kind": "grantAbility",
                    "object": creature.clone(),
                    "duration": { "kind": "permanent" },
                    "ability": {
                        "kind": "staticAbility",
                        "source": { "kind": "self" },
                        "activeWhile": active_while_battlefield(),
                        "modifiers": [{ "kind": "attackEachCombatIfAble", "object": self_ref() }],
                    },
                }),
                json!({
                    "kind": "grantAbility",
                    "object": creature,
                    "duration": { "kind": "permanent" },
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": { "kind": "self" },
                        "event": { "kind": "combatDamageToPlayer", "source": self_ref() },
                        "effects": [{ "kind": "sacrificeAbilitySource" }],
                    },
                }),
            ],
        ));
    }

    if text
        == "Whenever Green Goblin attacks, discard a card. Then draw a card for each card you've discarded this turn."
    {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![
                json!({
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(1),
                }),
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": { "kind": "cardsDiscardedThisTurn" },
                }),
            ],
        ));
    }

    if text
        == "Darkforce Inversion â€” When Mister Negative enters, you may exchange life totals with target opponent. If you lost life this way, draw that many cards."
        || text
            == "Darkforce Inversion — When Mister Negative enters, you may exchange life totals with target opponent. If you lost life this way, draw that many cards."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetOpponent",
                    json!({
                        "kind": "players",
                        "where": { "kind": "isOpponentOf", "player": controller() },
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [{
                    "kind": "exchangeLifeTotalsAndDrawLoss",
                    "otherPlayer": chosen_target("targetOpponent"),
                }],
            })],
        ));
    }

    if text
        == "At the beginning of your end step, two target players each reveal the top card of their library. They each lose life equal to the mana value of the card revealed by the other player. Then they each put the card they revealed into their hand."
    {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPlayers",
                    json!({ "kind": "players" }),
                    2,
                    2,
                )],
            })),
            vec![json!({
                "kind": "resolveParkerLuck",
                "targetsDecisionId": "targetPlayers",
            })],
        ));
    }

    if text.contains("Conceal")
        && text.contains("target creature you control becomes a Citizen")
        && text.contains("target creature you control becomes a Hero")
    {
        let conceal = selection("chosenModes", "conceal");
        let reveal = selection("chosenModes", "reveal");
        let mut conceal_target = target_decision(
            "concealTarget",
            json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
            1,
            1,
        );
        conceal_target["condition"] = conceal.clone();
        let mut reveal_target = target_decision(
            "revealTarget",
            json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
            1,
            1,
        );
        reveal_target["condition"] = reveal.clone();
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        {
                            "id": "chosenModes",
                            "kind": "chooseModes",
                            "minimum": 1,
                            "maximum": 1,
                            "options": ["conceal", "reveal"],
                            "allowRepeated": false,
                        },
                        conceal_target,
                        reveal_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": conceal,
                        "then": [
                            {
                                "kind": "becomeCreature",
                                "object": chosen_target("concealTarget"),
                                "addTypes": ["Creature"],
                                "addSubtypes": ["Citizen"],
                                "replaceSubtypes": true,
                                "basePower": 1,
                                "baseToughness": 1,
                                "retainExistingTypes": true,
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                            {
                                "kind": "grantKeyword",
                                "object": chosen_target("concealTarget"),
                                "keyword": "hexproof",
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                        ],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": reveal,
                        "then": [
                            {
                                "kind": "becomeCreature",
                                "object": chosen_target("revealTarget"),
                                "addTypes": ["Creature"],
                                "addSubtypes": ["Hero"],
                                "replaceSubtypes": true,
                                "basePower": 3,
                                "baseToughness": 4,
                                "retainExistingTypes": true,
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                            {
                                "kind": "grantKeyword",
                                "object": chosen_target("revealTarget"),
                                "keyword": "flying",
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                            {
                                "kind": "grantKeyword",
                                "object": chosen_target("revealTarget"),
                                "keyword": "vigilance",
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                        ],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose conceal or reveal",
                "Apply the selected identity until end of turn",
            ],
        ));
    }

    if text.contains("Puff Piece") && text.contains("Investigative Journalism") {
        let puff = selection("chosenModes", "puffPiece");
        let investigate = selection("chosenModes", "investigate");
        let mut puff_targets = target_decision(
            "puffTargets",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            0,
            2,
        );
        puff_targets["condition"] = puff.clone();
        let mut grave_target = target_decision(
            "graveCreature",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": and(vec![
                    card_type("Creature"),
                    compare("<=", json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }), integer(2)),
                ]),
            }),
            1,
            1,
        );
        grave_target["condition"] = investigate.clone();
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["puffPiece", "investigate"], "allowRepeated": false },
                    puff_targets,
                    grave_target,
                ],
            })),
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": puff,
                    "then": [{
                        "kind": "putCounters",
                        "permanent": { "kind": "chosenTargets", "id": "puffTargets" },
                        "counter": "+1/+1",
                        "count": integer(1),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": investigate,
                    "then": [{
                        "kind": "moveTargetCard",
                        "card": chosen_target("graveCreature"),
                        "to": "hand",
                        "tapped": false,
                        "controller": controller(),
                    }],
                    "else": [],
                }),
            ],
        ));
    }

    if text.contains("Repair")
        && text.contains("Return target card with mana value 4 or greater")
        && text.contains("Impound")
    {
        let repair = selection("chosenModes", "repair");
        let impound = selection("chosenModes", "impound");
        let mut repair_target = target_decision(
            "repairCard",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": compare(
                    ">=",
                    json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                    integer(4),
                ),
            }),
            1,
            1,
        );
        repair_target["condition"] = repair.clone();
        let mut impound_target = target_decision(
            "impoundPermanent",
            json!({
                "kind": "permanents",
                "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
            }),
            1,
            1,
        );
        impound_target["condition"] = impound.clone();
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["repair", "impound"], "allowRepeated": false },
                    repair_target,
                    impound_target,
                ],
            })),
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": repair,
                    "then": [{
                        "kind": "moveTargetCard",
                        "card": chosen_target("repairCard"),
                        "to": "hand",
                        "tapped": false,
                        "controller": controller(),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": impound,
                    "then": [{
                        "kind": "exilePermanent",
                        "permanent": chosen_target("impoundPermanent"),
                    }],
                    "else": [],
                }),
            ],
        ));
    }

    if text.contains("Look Around")
        && text.contains("Mill three cards")
        && text.contains("Bring Down")
    {
        let look = selection("chosenModes", "lookAround");
        let bring_down = selection("chosenModes", "bringDown");
        let mut flying_target = target_decision(
            "flyingCreature",
            json!({
                "kind": "permanents",
                "where": and(vec![card_type("Creature"), json!({ "kind": "hasKeyword", "value": "flying" })]),
            }),
            1,
            1,
        );
        flying_target["condition"] = bring_down.clone();
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["lookAround", "bringDown"], "allowRepeated": false },
                        flying_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": look,
                        "then": [
                            { "kind": "mill", "player": controller(), "count": integer(3), "bind": "scoutMilledCards" },
                            {
                                "kind": "chooseCards",
                                "id": "scoutPermanent",
                                "player": controller(),
                                "from": bound_objects("scoutMilledCards"),
                                "where": { "kind": "isPermanentCard" },
                                "minimum": integer(0),
                                "maximum": integer(1),
                            },
                            { "kind": "moveCards", "cards": decision_result("scoutPermanent"), "to": hand(controller()) },
                            { "kind": "gainLife", "player": controller(), "amount": integer(3) },
                        ],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": bring_down,
                        "then": [{ "kind": "destroyPermanent", "permanent": chosen_target("flyingCreature") }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose the scouting mode",
                "Resolve the mill recovery or destroy a flyer",
            ],
        ));
    }

    if text.contains("Date Night")
        && text.contains("Exile the top two cards of your library")
        && text.contains("Patrol Night")
    {
        let date = selection("chosenModes", "dateNight");
        let patrol = selection("chosenModes", "patrolNight");
        let mut patrol_targets = target_decision(
            "patrolTargets",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            2,
        );
        patrol_targets["condition"] = patrol.clone();
        let patrol_objects = json!({ "kind": "chosenTargets", "id": "patrolTargets" });
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["dateNight", "patrolNight"], "allowRepeated": false },
                        patrol_targets,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": date,
                        "then": [
                            {
                                "kind": "exileTopCards",
                                "zone": library(controller()),
                                "count": integer(2),
                                "faceDown": false,
                                "bind": "dateNightCards",
                            },
                            {
                                "kind": "chooseCards",
                                "id": "dateNightChoice",
                                "player": controller(),
                                "from": bound_objects("dateNightCards"),
                                "where": Value::Null,
                                "minimum": integer(1),
                                "maximum": integer(1),
                            },
                            {
                                "kind": "grantPermission",
                                "player": controller(),
                                "action": {
                                    "kind": "play",
                                    "card": { "kind": "decisionResult", "decisionId": "dateNightChoice" },
                                    "normalTimingApplies": true,
                                    "normalCostsApply": true,
                                },
                                "duration": { "kind": "untilEndOfNextTurn", "player": controller() },
                            },
                        ],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": patrol,
                        "then": [
                            {
                                "kind": "modifyPowerToughness",
                                "object": patrol_objects.clone(),
                                "power": integer(1),
                                "toughness": integer(0),
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                            {
                                "kind": "grantKeyword",
                                "object": patrol_objects,
                                "keyword": "firstStrike",
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                        ],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose date night or patrol night",
                "Resolve the selected Hangout mode",
            ],
        ));
    }

    if text
        == "Camouflage — {2}: Put a +1/+1 counter on Ultimate Spider-Man. He gains hexproof and becomes colorless until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payMana", "manaCost": "{2}" }],
                "effects": [
                    {
                        "kind": "putCounters",
                        "permanent": self_ref(),
                        "counter": "+1/+1",
                        "count": integer(1),
                    },
                    {
                        "kind": "grantKeyword",
                        "object": self_ref(),
                        "keyword": "hexproof",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "setColorlessUntilEndOfTurn",
                        "object": self_ref(),
                    },
                ],
            }),
            &[
                "Pay the camouflage cost",
                "Add a counter and grant temporary hexproof and colorlessness",
            ],
        ));
    }

    if text
        == "At the beginning of your end step, if you've played a land or cast a spell this turn from anywhere other than your hand, Spider-Man 2099 deals damage equal to his power to any target."
    {
        let mut parsed = triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
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
                "source": self_ref(),
                "recipient": chosen_target("damageTarget"),
                "amount": { "kind": "powerOf", "object": self_ref() },
            })],
        );
        parsed.rule["condition"] = json!({ "kind": "playedCardFromNonHandThisTurn" });
        return Some(parsed);
    }

    if text
        == "Return up to six target creature cards with different names from your graveyard to the battlefield."
    {
        let mut targets = target_decision(
            "sinisterSix",
            json!({
                "kind": "cards",
                "zone": graveyard(controller()),
                "where": card_type("Creature"),
            }),
            0,
            6,
        );
        targets["selectionConstraint"] = json!({ "kind": "distinctCardNames" });
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [targets],
                },
                "effects": [{
                    "kind": "moveCards",
                    "cards": { "kind": "chosenTargets", "id": "sinisterSix" },
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                }],
            }),
            &[
                "Choose up to six differently named creature cards",
                "Return the chosen cards to the battlefield",
            ],
        ));
    }

    if text.contains("Lizard Formula")
        && text.contains("up to one other target creature loses all abilities")
        && text.contains("green Lizard creature with base power and toughness 4/4")
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "formulaTarget",
                    json!({
                        "kind": "permanents",
                        "where": card_type("Creature"),
                        "excludeSource": true,
                    }),
                    0,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "loseAllAbilitiesPermanently",
                    "object": chosen_target("formulaTarget"),
                }),
                json!({
                    "kind": "becomeCreature",
                    "object": chosen_target("formulaTarget"),
                    "addTypes": ["Creature"],
                    "addSubtypes": ["Lizard"],
                    "addColors": ["G"],
                    "replaceSubtypes": true,
                    "basePower": 4,
                    "baseToughness": 4,
                    "retainExistingTypes": true,
                    "duration": { "kind": "permanent" },
                }),
            ],
        ));
    }

    if text
        == "At the beginning of each player's first main phase, that player may put a +1/+1 counter on this creature. If they do, they add {C} for each counter on it."
    {
        let triggering_player = json!({ "kind": "triggeringPlayer" });
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "precombatMain",
                "player": { "kind": "eachPlayer" },
            }),
            None,
            vec![json!({
                "kind": "optionalEffects",
                "player": triggering_player,
                "effects": [
                    {
                        "kind": "putCounters",
                        "permanent": self_ref(),
                        "counter": "+1/+1",
                        "count": integer(1),
                    },
                    {
                        "kind": "addMana",
                        "player": { "kind": "triggeringPlayer" },
                        "mana": {
                            "kind": "fixedMana",
                            "symbol": "C",
                            "amount": { "kind": "countCounters", "object": self_ref() },
                        },
                    },
                ],
            })],
        ));
    }

    if text
        == "{3}{G}{G}: Return this card and target land card from your graveyard to the battlefield tapped."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "graveyard",
                "costs": [{ "kind": "payMana", "manaCost": "{3}{G}{G}" }],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "landCard",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": card_type("Land"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "moveCards",
                    "cards": {
                        "kind": "union",
                        "sets": [
                            self_ref(),
                            { "kind": "chosenTargets", "id": "landCard" },
                        ],
                    },
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": true,
                    },
                }],
            }),
            &[
                "Pay the graveyard activation cost and target a land card",
                "Return Sandman and the land tapped",
            ],
        ));
    }

    if text
        == "{2}, Return a tapped creature you control to its owner's hand: Put this card from your hand onto the battlefield. Activate only as a sorcery."
    {
        let returned = chosen_target("returnedCreature");
        let filter = and(vec![card_type("Creature"), json!({ "kind": "isTapped" })]);
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "hand",
                "activationCondition": { "kind": "sorceryTiming" },
                "costs": [
                    { "kind": "payMana", "manaCost": "{2}" },
                    {
                        "kind": "returnPermanentToOwnersHand",
                        "permanent": returned,
                        "where": filter.clone(),
                    },
                ],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "returnedCreature",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": filter,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "moveCards",
                    "cards": self_ref(),
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                }],
            }),
            &[
                "Pay mana and return a tapped creature",
                "Put Urban Retreat onto the battlefield",
            ],
        ));
    }

    if text.contains("Each player may discard a card. Each player who doesn't loses 3 life")
        && (text.starts_with("II ") || text.starts_with("II "))
    {
        return Some(triggered(
            json!({
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": [integer(2)],
            }),
            None,
            vec![json!({
                "kind": "eachPlayerPaysCostOrLosesLife",
                "players": { "kind": "eachPlayer" },
                "cost": { "kind": "discardCard", "where": Value::Null },
                "amount": integer(3),
            })],
        ));
    }

    if text.contains("Exile any number of target players' graveyards") && text.starts_with("III ") {
        return Some(triggered(
            json!({
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": [integer(3)],
            }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "graveyardPlayers",
                    "kind": "chooseTargets",
                    "candidates": { "kind": "players" },
                    "minimum": integer(0),
                    "maximum": { "kind": "countPlayers" },
                }],
            })),
            vec![json!({
                "kind": "exileChosenPlayersGraveyards",
                "targetsDecisionId": "graveyardPlayers",
            })],
        ));
    }

    if text
        == "Exile X target artifacts and/or creatures. Return the exiled cards to the battlefield under their owners' control at the beginning of the next end step."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "ceilingTargets",
                        "kind": "chooseTargets",
                        "candidates": {
                            "kind": "permanents",
                            "where": or(vec![card_type("Artifact"), card_type("Creature")]),
                        },
                        "minimum": { "kind": "sourceCastXValue" },
                        "maximum": { "kind": "sourceCastXValue" },
                    }],
                },
                "effects": [{
                    "kind": "exileUntilNextEndStep",
                    "objects": { "kind": "chosenTargets", "id": "ceilingTargets" },
                    "returnUnderOwnerControl": true,
                }],
            }),
            &[
                "Target X artifacts and/or creatures",
                "Exile them until the next end step",
            ],
        ));
    }

    if text
        == "One or two target creatures you control each get +1/+0 until end of turn. They each deal damage equal to their power to target creature an opponent controls."
    {
        let allies = json!({ "kind": "chosenTargets", "id": "teamCreatures" });
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "teamCreatures",
                            json!({
                                "kind": "permanents",
                                "controller": controller(),
                                "where": card_type("Creature"),
                            }),
                            1,
                            2,
                        ),
                        target_decision(
                            "opposingCreature",
                            json!({
                                "kind": "permanents",
                                "controller": { "kind": "opponentsOf", "player": controller() },
                                "where": card_type("Creature"),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": allies.clone(),
                        "power": integer(1),
                        "toughness": integer(0),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "chosenPermanentsDealPowerDamage",
                        "sources": allies,
                        "recipient": chosen_target("opposingCreature"),
                    },
                ],
            }),
            &[
                "Target and strengthen one or two creatures",
                "Have each of them deal its power to the opposing creature",
            ],
        ));
    }

    if text == "As an additional cost to cast this spell, discard a card or pay {2}." {
        let discard = selection("bombardmentCost", "discard");
        let mana = selection("bombardmentCost", "mana");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "bombardmentCost",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 1,
                        "options": ["discard", "mana"],
                        "allowRepeated": false,
                    }],
                    "additionalCosts": [
                        {
                            "kind": "conditional",
                            "condition": discard,
                            "then": [{ "kind": "discardCard", "where": Value::Null }],
                        },
                        {
                            "kind": "conditional",
                            "condition": mana,
                            "then": [{ "kind": "payMana", "manaCost": "{2}" }],
                        },
                    ],
                },
                "effects": [],
            }),
            &[
                "Choose the additional cost",
                "Discard a card or pay two mana",
            ],
        ));
    }

    if text == "All damage that would be dealt to you is dealt to enchanted creature instead." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "redirectPlayerDamageToAttachedPermanent",
                    "player": controller(),
                    "attachment": self_ref(),
                }],
            }),
            &[
                "Recognize player damage while the Aura is attached",
                "Redirect that damage to the enchanted creature",
            ],
        ));
    }

    if text
        == "{T}: You may cast an artifact spell from your hand with mana value less than or equal to the number of ingenuity counters on Lady Octopus without paying its mana cost."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "tap", "object": self_ref() }],
                "effects": [{
                    "kind": "castAnyNumber",
                    "player": controller(),
                    "cards": {
                        "kind": "cardsInZone",
                        "zone": hand(controller()),
                        "where": and(vec![
                            card_type("Artifact"),
                            compare(
                                "<=",
                                json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                                json!({
                                    "kind": "countCounters",
                                    "object": self_ref(),
                                    "counter": "ingenuity",
                                }),
                            ),
                        ]),
                    },
                    "where": { "kind": "canBeCastAsSpell" },
                    "timing": { "kind": "duringResolution" },
                    "withoutPayingManaCost": true,
                    "alternativeCostsAllowed": false,
                    "additionalCostsApply": true,
                    "variableManaValue": integer(0),
                    "sourceZone": "hand",
                    "maximum": integer(1),
                }],
            }),
            &[
                "Tap Lady Octopus",
                "Optionally cast a qualifying artifact from hand for free",
            ],
        ));
    }

    if text
        == "When Mysterio enters, create a 3/3 blue Illusion Villain creature token for each nontoken Villain you control. Exile those tokens when Mysterio leaves the battlefield."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": {
                    "kind": "countPermanents",
                    "player": controller(),
                    "where": and(vec![
                        subtype("Villain"),
                        not(json!({ "kind": "isToken" })),
                    ]),
                },
                "token": {
                    "name": "Illusion Villain Token",
                    "colors": ["blue"],
                    "types": ["Creature"],
                    "subtypes": ["Illusion", "Villain"],
                    "power": 3,
                    "toughness": 3,
                },
                "exileWhenSourceLeaves": true,
            })],
        ));
    }

    if text
        == "Whenever a nontoken creature you control deals combat damage to a player, create a token that's a copy of it, except it isn't legendary."
    {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
                "where": not(json!({ "kind": "isToken" })),
            }),
            None,
            vec![json!({
                "kind": "createTokenCopyOfPermanent",
                "object": { "kind": "triggeringPermanent" },
                "quantity": integer(1),
                "removeLegendary": true,
                "grantKeywords": [],
                "exileAtNextEndStep": false,
            })],
        ));
    }

    if text
        == "You may have Chameleon enter as a copy of a creature you control, except his name is Chameleon, Master of Disguise."
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "chameleonCopy",
                    "kind": "chooseBattlefieldPermanent",
                    "controlledByEnteringController": true,
                    "where": card_type("Creature"),
                    "optional": true,
                }],
                "replacement": [{
                    "kind": "copyEnteringPermanent",
                    "decisionId": "chameleonCopy",
                    "tapped": false,
                    "retainName": true,
                }],
            }),
            &[
                "Optionally choose a creature you control",
                "Copy it while retaining Chameleon's name",
            ],
        ));
    }

    if text == "Whenever Spider-Slayer deals damage to a Spider, destroy that creature." {
        return Some(triggered(
            json!({
                "kind": "sourceDealtDamageToPermanent",
                "object": self_ref(),
                "where": subtype("Spider"),
            }),
            None,
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": { "kind": "triggeringPermanent" },
            })],
        ));
    }

    if text
        == "Whenever this Vehicle attacks, you may pay {U}. When you do, another target attacking creature can't be blocked this turn."
    {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "optionalPayCostCreateReflexiveTrigger",
                "player": controller(),
                "cost": { "kind": "payMana", "manaCost": "{U}" },
                "ability": {
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                    "declaration": {
                        "kind": "castingDeclaration",
                        "decisions": [target_decision(
                            "otherAttacker",
                            json!({
                                "kind": "permanents",
                                "where": and(vec![
                                    card_type("Creature"),
                                    json!({ "kind": "isAttacking" }),
                                ]),
                                "excludeSource": true,
                            }),
                            1,
                            1,
                        )],
                    },
                    "effects": [{
                        "kind": "grantKeyword",
                        "object": chosen_target("otherAttacker"),
                        "keyword": "cantBeBlocked",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    }],
                },
            })],
        ));
    }

    if text
        == "Whenever a creature an opponent controls with the greatest power among creatures that player controls dies, draw a card and put a +1/+1 counter on Kraven the Hunter."
    {
        return Some(triggered(
            json!({
                "kind": "opponentCreatureDied",
                "player": controller(),
                "greatestPowerAmongControllerCreatures": true,
            }),
            None,
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }),
                json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": integer(1),
                }),
            ],
        ));
    }

    if text
        == "Whenever Venom attacks, you may sacrifice another creature. If you do, draw X cards, then you may put a permanent card with mana value X or less from your hand onto the battlefield, where X is the sacrificed creature's mana value."
    {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![json!({ "kind": "resolveEddieBrockAttack" })],
        ));
    }

    if text
        == "I — Until your next turn, each creature attacks each combat if able and attacks a player other than you if able."
    {
        return Some(triggered(
            json!({
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": [integer(1)],
            }),
            None,
            vec![json!({ "kind": "goadAllCreaturesUntilNextTurn" })],
        ));
    }

    if text
        == "Target creature you control gets +1/+0 until end of turn. It fights target creature an opponent controls. When excess damage is dealt to the creature an opponent controls this way, destroy up to one target noncreature artifact with mana value 3 or less."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
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
                                "controller": { "kind": "opponentsOf", "player": controller() },
                                "where": card_type("Creature"),
                            }),
                            1,
                            1,
                        ),
                        target_decision(
                            "smallArtifact",
                            json!({
                                "kind": "permanents",
                                "where": and(vec![
                                    card_type("Artifact"),
                                    not(card_type("Creature")),
                                    compare(
                                        "<=",
                                        json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                                        integer(3),
                                    ),
                                ]),
                            }),
                            0,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": chosen_target("friendlyCreature"),
                        "power": integer(1),
                        "toughness": integer(0),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "fightPermanents",
                        "first": chosen_target("friendlyCreature"),
                        "second": chosen_target("opposingCreature"),
                        "bindExcessToSecondAs": "rhinoExcessDamage",
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": compare(
                            ">",
                            json!({ "kind": "boundValue", "id": "rhinoExcessDamage" }),
                            integer(0),
                        ),
                        "then": [{
                            "kind": "destroyPermanent",
                            "permanent": chosen_target("smallArtifact"),
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose the fighting creatures and optional artifact",
                "Increase the friendly creature's power and fight",
                "Destroy the artifact only if the fight dealt excess damage",
            ],
        ));
    }

    if text == "{2}, Remove two +1/+1 counters from among artifacts you control: Draw a card." {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{2}" },
                    {
                        "kind": "removeCountersFromControlledPermanents",
                        "where": card_type("Artifact"),
                        "counter": "+1/+1",
                        "count": integer(2),
                    },
                ],
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
            }),
            &[
                "Pay two generic mana",
                "Remove two +1/+1 counters distributed among controlled artifacts",
                "Draw a card",
            ],
        ));
    }

    if text
        == "{T}: Add two mana in any combination of colors. Spend this mana only to cast spells from exile."
    {
        return Some(draft(
            json!({
                "kind": "manaAbility",
                "source": self_ref(),
                "costs": [{ "kind": "tap", "object": self_ref() }],
                "effects": [{
                    "kind": "addMana",
                    "player": controller(),
                    "mana": { "kind": "chooseColors", "amount": integer(2) },
                    "spendRestriction": {
                        "kind": "castSpellFromZone",
                        "zone": "exile",
                    },
                }],
            }),
            &[
                "Tap Interdimensional Web Watch",
                "Choose two mana colors",
                "Restrict both mana to spells cast from exile",
            ],
        ));
    }

    if text
        == "You may cast this card from your graveyard by discarding a card in addition to paying its other costs."
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "graveyardCastWithDiscard" },
            }),
            &[
                "Permit casting this card from its owner's graveyard",
                "Require one card discarded in addition to its other costs",
            ],
        ));
    }

    if text
        == "Whenever Gwenom attacks, until end of turn, you may look at the top card of your library any time and you may play cards from the top of your library. If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost."
    {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "grantGwenomTopLibraryCastingUntilEndOfTurn",
                "player": controller(),
            })],
        ));
    }

    if text
        == "{2}, {T}, Remove a film counter from this artifact: Copy target activated or triggered ability you control. You may choose new targets for the copy."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{2}" },
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "removeCounters",
                        "permanent": self_ref(),
                        "counter": "film",
                        "count": integer(1),
                    },
                ],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "copiedAbility",
                        json!({
                            "kind": "stackItems",
                            "controller": controller(),
                            "where": or(vec![
                                json!({ "kind": "isActivatedAbility" }),
                                json!({ "kind": "isTriggeredAbility" }),
                            ]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "copyStackItem",
                    "object": chosen_target("copiedAbility"),
                    "controller": controller(),
                    "mayChooseNewTargets": true,
                }],
            }),
            &[
                "Pay mana, tap the Camera, and remove a film counter",
                "Copy the targeted ability and optionally retarget it",
            ],
        ));
    }

    if text == "The \"legend rule\" doesn't apply to Spiders you control." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "ignoreLegendRuleForSubtype",
                    "player": controller(),
                    "subtype": "Spider",
                }],
            }),
            &[
                "Identify legendary Spiders controlled by the source controller",
                "Exempt those permanents from the legend rule",
            ],
        ));
    }

    if text
        == "Whenever you cast a spell from anywhere other than your hand, you may copy it. If you do, you may choose new targets for the copy. If the copy is a permanent spell, it gains haste. Do this only once each turn."
    {
        let mut parsed = triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "fromZoneNot": "hand",
            }),
            None,
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [{
                    "kind": "copyStackItem",
                    "object": { "kind": "triggeringStackObject" },
                    "controller": controller(),
                    "mayChooseNewTargets": true,
                    "grantKeywords": ["haste"],
                    "grantKeywordsIfPermanentSpell": true,
                }],
            })],
        );
        parsed.rule["triggerLimit"] = json!({
            "kind": "onceEachTurn",
            "id": "spiderVerseCopy",
        });
        return Some(parsed);
    }

    if text
        == "Whenever you cast a creature spell with mana value equal to Jackal's power, copy that spell, except the copy isn't legendary. Then put a +1/+1 counter on Jackal. (The copy becomes a token.)"
    {
        let mut parsed = triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": card_type("Creature"),
            }),
            None,
            vec![
                json!({
                    "kind": "copyStackItem",
                    "object": { "kind": "triggeringStackObject" },
                    "controller": controller(),
                    "removeLegendary": true,
                }),
                json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": integer(1),
                }),
            ],
        );
        parsed.rule["condition"] = compare(
            "==",
            json!({ "kind": "triggeringSpellManaValue" }),
            json!({ "kind": "powerOf", "object": self_ref() }),
        );
        return Some(parsed);
    }

    if text
        == "Whenever you cast a spell with mana value 4 or greater, you may exile the top card of your library. If you do, you may play that card until you exile another card with this creature."
    {
        let mut parsed = triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
            }),
            None,
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [
                    {
                        "kind": "exileTopCards",
                        "zone": library(controller()),
                        "count": integer(1),
                        "faceDown": false,
                        "bind": "superiorFoesCard",
                    },
                    {
                        "kind": "grantPermission",
                        "player": controller(),
                        "action": {
                            "kind": "play",
                            "card": { "kind": "boundObject", "binding": "superiorFoesCard" },
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                        },
                        "duration": { "kind": "whileInExile" },
                        "replacePermissionsFromSource": true,
                    },
                ],
            })],
        );
        parsed.rule["condition"] = compare(
            ">=",
            json!({ "kind": "triggeringSpellManaValue" }),
            integer(4),
        );
        return Some(parsed);
    }

    if text
        == "When Black Cat enters, look at the top nine cards of target opponent's library, exile two of them face down, then put the rest on the bottom of their library in a random order. You may play the exiled cards for as long as they remain exiled. Mana of any type can be spent to cast spells this way."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "blackCatOpponent",
                    json!({
                        "kind": "players",
                        "where": { "kind": "isOpponentOf", "player": controller() },
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "resolveBlackCatEnter",
                "opponent": chosen_target("blackCatOpponent"),
            })],
        ));
    }

    if text
        == "When The Spot enters, exile up to one target nonland permanent and up to one target nonland permanent card from a graveyard."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    target_decision(
                        "spotPermanent",
                        json!({
                            "kind": "permanents",
                            "where": not(card_type("Land")),
                        }),
                        0,
                        1,
                    ),
                    target_decision(
                        "spotGraveyardCard",
                        json!({
                            "kind": "cards",
                            "zone": { "kind": "anyGraveyard" },
                            "where": and(vec![
                                json!({ "kind": "isPermanentCard" }),
                                not(card_type("Land")),
                            ]),
                        }),
                        0,
                        1,
                    ),
                ],
            })),
            vec![json!({
                "kind": "exileSpotTargets",
                "permanent": chosen_target("spotPermanent"),
                "graveyardCard": chosen_target("spotGraveyardCard"),
            })],
        ));
    }

    if text
        == "When The Spot dies, put him on the bottom of his owner's library. If you do, return the exiled cards to their owners' hands."
    {
        return Some(triggered(
            json!({ "kind": "permanentDied", "object": self_ref() }),
            None,
            vec![json!({ "kind": "resolveSpotDeath" })],
        ));
    }

    if text
        == "As Arachne enters, look at an opponent's hand, then choose a card type other than creature."
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [
                    { "id": "arachneOpponent", "kind": "chooseOpponentHand" },
                    {
                        "id": "chosenCardType",
                        "kind": "chooseCardType",
                        "options": [
                            "Artifact", "Battle", "Enchantment", "Instant", "Kindred",
                            "Land", "Planeswalker", "Sorcery"
                        ],
                    },
                ],
                "replacement": [{
                    "kind": "storeDecision",
                    "decisionId": "chosenCardType",
                }],
            }),
            &[
                "Choose and look at an opponent's hand",
                "Choose and store a noncreature card type",
            ],
        ));
    }

    if text == "Spells of the chosen type cost {1} more to cast." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "additionalCastingCost",
                    "players": { "kind": "opponentsOf", "player": controller() },
                    "where": {
                        "kind": "chosenCardType",
                        "decisionId": "chosenCardType",
                    },
                    "cost": { "kind": "payMana", "manaCost": "{1}" },
                }],
            }),
            &[
                "Match opponents' spells against Arachne's stored card type",
                "Add one generic mana to those casting costs",
            ],
        ));
    }

    if text
        == "At the beginning of your end step, you may tap two untapped creatures and/or Treasures you control. If you do, draw a card. Otherwise, sacrifice this enchantment."
    {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            None,
            vec![json!({
                "kind": "optionalPayCostPerformEffects",
                "player": controller(),
                "cost": {
                    "kind": "tapPermanents",
                    "where": or(vec![card_type("Creature"), subtype("Treasure")]),
                    "count": integer(2),
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
                "declineEffects": [{ "kind": "sacrificeAbilitySource" }],
            })],
        ));
    }

    if text
        == "At the beginning of your end step, untap Hydro-Man. Until your next turn, he becomes a land and gains \"{T}: Add {U}.\" (He's not a creature during that time.)"
    {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            None,
            vec![json!({ "kind": "resolveHydroManEndStep" })],
        ));
    }

    if text.contains("Mill five cards. When you do, this Saga deals damage equal to the greatest power among creature cards in your graveyard to target creature")
        && text.starts_with("I ")
    {
        return Some(triggered(
            json!({
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": [integer(1)],
            }),
            None,
            vec![
                json!({
                    "kind": "mill",
                    "player": controller(),
                    "count": integer(5),
                }),
                json!({
                    "kind": "createReflexiveTrigger",
                    "source": self_ref(),
                    "controller": controller(),
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": self_ref(),
                        "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                        "declaration": {
                            "kind": "castingDeclaration",
                            "decisions": [target_decision(
                                "kravenDamageTarget",
                                json!({ "kind": "permanents", "where": card_type("Creature") }),
                                1,
                                1,
                            )],
                        },
                        "effects": [{
                            "kind": "dealDamage",
                            "source": self_ref(),
                            "recipient": chosen_target("kravenDamageTarget"),
                            "amount": {
                                "kind": "greatestPowerAmongCards",
                                "zone": graveyard(controller()),
                                "where": card_type("Creature"),
                            },
                        }],
                    },
                }),
            ],
        ));
    }

    if text.contains(
        "When you next cast a creature spell this turn, copy it, except the copy isn't legendary",
    ) && text.starts_with("II ")
    {
        return Some(triggered(
            json!({
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": [integer(2)],
            }),
            None,
            vec![json!({
                "kind": "installDelayedSpellCastTrigger",
                "player": controller(),
                "where": card_type("Creature"),
                "duration": { "kind": "untilEndOfCurrentTurn" },
                "effects": [{
                    "kind": "copyStackItem",
                    "object": { "kind": "triggeringStackObject" },
                    "controller": controller(),
                    "mayChooseNewTargets": false,
                    "removeLegendary": true,
                }],
            })],
        ));
    }

    if text.contains("Choose a card name. Whenever a creature with the chosen name deals combat damage to a player this turn, draw a card")
        && text.starts_with("III ")
    {
        return Some(triggered(
            json!({
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": [integer(3)],
            }),
            None,
            vec![
                json!({
                    "kind": "chooseCardName",
                    "id": "cloneSagaName",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                json!({
                    "kind": "installControlledCombatDamageDrawUntilEndOfTurn",
                    "player": controller(),
                    "count": integer(1),
                    "nameDecisionId": "cloneSagaName",
                }),
            ],
        ));
    }

    if text
        == "When Anti-Venom enters, if he was cast, return target creature card from your graveyard to the battlefield."
    {
        let mut parsed = triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "creatureCard",
                    json!({
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": card_type("Creature"),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("creatureCard"),
                "to": "battlefield",
                "tapped": false,
                "controller": controller(),
            })],
        );
        parsed.rule["condition"] = json!({ "kind": "wasCast", "object": self_ref() });
        return Some(parsed);
    }

    if text
        == "If damage would be dealt to Anti-Venom, prevent that damage and put that many +1/+1 counters on him."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "preventDamageAndAddCounters",
                    "objects": self_ref(),
                    "counter": "+1/+1",
                }],
            }),
            &[
                "Replace damage to Anti-Venom",
                "Prevent it and add that many +1/+1 counters",
            ],
        ));
    }

    if text == "Counter target instant spell, sorcery spell, or triggered ability." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetStackObject",
                        json!({
                            "kind": "stackItems",
                            "where": or(vec![
                                json!({
                                    "kind": "and",
                                    "operands": [
                                        { "kind": "isSpell" },
                                        or(vec![card_type("Instant"), card_type("Sorcery")]),
                                    ],
                                }),
                                json!({ "kind": "isTriggeredAbility" }),
                            ]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "counterStackObject",
                    "object": chosen_target("targetStackObject"),
                }],
            }),
            &[
                "Target the qualifying spell or triggered ability",
                "Counter that stack object",
            ],
        ));
    }

    if text
        == "Target opponent loses life equal to the number of creatures they control. Then destroy all creatures."
    {
        let opponent = chosen_target("targetOpponent");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetOpponent",
                        json!({
                            "kind": "players",
                            "where": { "kind": "isOpponentOf", "player": controller() },
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "loseLife",
                        "player": opponent.clone(),
                        "amount": {
                            "kind": "countPermanents",
                            "player": opponent,
                            "where": card_type("Creature"),
                        },
                    },
                    {
                        "kind": "destroyPermanent",
                        "permanent": {
                            "kind": "eachPermanent",
                            "where": card_type("Creature"),
                        },
                    },
                ],
            }),
            &[
                "Count the opponent's creatures and make them lose that much life",
                "Destroy all creatures",
            ],
        ));
    }

    if text
        == "Whenever you attack, double the number of each kind of counter on each Spider and legendary creature you control."
    {
        return Some(triggered(
            json!({ "kind": "controlledCreaturesAttacked", "player": controller() }),
            None,
            vec![json!({
                "kind": "doubleAllCountersOnPermanents",
                "player": controller(),
                "where": or(vec![subtype("Spider"), json!({ "kind": "isLegendary" })]),
            })],
        ));
    }

    if text.starts_with("Whenever Norman Osborn deals combat damage to a player, he connives.") {
        return Some(triggered(
            json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
            None,
            vec![json!({
                "kind": "connive",
                "permanent": self_ref(),
                "player": controller(),
            })],
        ));
    }

    if text.starts_with("Whenever another Villain you control enters, Prowler connives.") {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": and(vec![card_type("Creature"), subtype("Villain")]),
                "excludeSource": true,
            }),
            None,
            vec![json!({
                "kind": "connive",
                "permanent": self_ref(),
                "player": controller(),
            })],
        ));
    }

    if text
        == "As long as there are eight or more cards in your graveyard, Doc Ock has base power and toughness 8/8."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "setBasePowerToughness",
                    "objects": self_ref(),
                    "power": integer(8),
                    "toughness": integer(8),
                    "condition": compare(
                        ">=",
                        json!({
                            "kind": "countCards",
                            "zone": graveyard(controller()),
                            "where": Value::Null,
                        }),
                        integer(8),
                    ),
                }],
            }),
            &[
                "Count cards in the controller's graveyard",
                "Set Doc Ock to 8/8",
            ],
        ));
    }

    if text.starts_with("As long as you control another Villain, Doc Ock has hexproof.") {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": self_ref(),
                    "keyword": "hexproof",
                    "condition": {
                        "kind": "controlsPermanent",
                        "player": controller(),
                        "where": and(vec![card_type("Creature"), subtype("Villain")]),
                        "excludeSource": true,
                    },
                }],
            }),
            &["Find another controlled Villain", "Grant Doc Ock hexproof"],
        ));
    }

    if text == "Molten Man gets +1/+1 for each Mountain you control." {
        let amount = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": subtype("Mountain"),
        });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": self_ref(),
                    "power": amount.clone(),
                    "toughness": amount,
                }],
            }),
            &[
                "Count controlled Mountains",
                "Apply the matching power and toughness bonus",
            ],
        ));
    }

    if text
        == "When this creature leaves the battlefield, put its +1/+1 counters on target creature you control."
    {
        return Some(triggered(
            json!({ "kind": "permanentLeftBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "counterRecipient",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Creature"),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "putCounters",
                "permanent": chosen_target("counterRecipient"),
                "counter": "+1/+1",
                "count": {
                    "kind": "countCounters",
                    "object": self_ref(),
                    "counter": "+1/+1",
                },
            })],
        ));
    }

    if text
        == "Whenever you attack with two or more Spiders, put a +1/+1 counter on Spinneret and Spiderling."
    {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "where": and(vec![card_type("Creature"), subtype("Spider")]),
                "minimum": integer(2),
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }

    if text
        == "At the beginning of combat on your turn, other Spiders you control gain flying, first strike, trample, lifelink, and haste until end of turn."
    {
        let spiders = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": and(vec![card_type("Creature"), subtype("Spider")]),
            "excludeSource": true,
        });
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
            None,
            ["flying", "firstStrike", "trample", "lifelink", "haste"]
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "object": spiders.clone(),
                        "keyword": keyword,
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    })
                })
                .collect(),
        ));
    }

    if text.starts_with(
        "Undying Vengeance — Whenever you play a land or cast a spell from anywhere other than your hand, this enchantment deals 1 damage to each opponent.",
    ) {
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "landPlayed", "player": controller(), "excludeSource": false },
                    { "kind": "spellCast", "player": controller(), "where": Value::Null, "fromZoneNot": "hand" },
                ],
            }),
            None,
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "recipient": { "kind": "opponentsOf", "player": controller() },
                "amount": integer(1),
            })],
        ));
    }

    if text
        == "Whenever you draw your first or second card each turn, put an ingenuity counter on Lady Octopus."
    {
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "cardDrawn", "player": controller(), "drawOrdinal": integer(1) },
                    { "kind": "cardDrawn", "player": controller(), "drawOrdinal": integer(2) },
                ],
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "ingenuity",
                "count": integer(1),
            })],
        ));
    }

    if text == "Whenever a creature you control with menace attacks, create a Treasure token." {
        return Some(triggered(
            json!({
                "kind": "controlledCreatureDeclaredAttacker",
                "player": controller(),
                "where": and(vec![
                    card_type("Creature"),
                    json!({ "kind": "hasKeyword", "value": "menace" }),
                ]),
            }),
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": { "kind": "namedToken", "name": "Treasure" },
            })],
        ));
    }

    if text
        == "When Gwen Stacy enters, exile the top card of your library. You may play that card for as long as you control this creature."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            None,
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(controller()),
                    "count": integer(1),
                    "faceDown": false,
                    "bind": "gwenExiledCard",
                }),
                json!({
                    "kind": "grantPermission",
                    "player": controller(),
                    "action": {
                        "kind": "play",
                        "card": { "kind": "boundObject", "binding": "gwenExiledCard" },
                        "normalTimingApplies": true,
                        "normalCostsApply": true,
                    },
                    "duration": { "kind": "whileSourceControlled" },
                }),
            ],
        ));
    }

    if text
        == "Whenever you play a land from exile or cast a spell from exile, put a +1/+1 counter on Ghost-Spider."
    {
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "landPlayedFromExile", "player": controller() },
                    { "kind": "spellCast", "player": controller(), "where": Value::Null, "fromZone": "exile" },
                ],
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }

    if text
        == "Remove two counters from Ghost-Spider: Exile the top card of your library. You may play that card this turn."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{
                    "kind": "removeCounters",
                    "permanent": self_ref(),
                    "counter": { "kind": "decisionResult", "decisionId": "removedCounterKind" },
                    "count": integer(2),
                }],
                "effects": [
                    {
                        "kind": "exileTopCards",
                        "zone": library(controller()),
                        "count": integer(1),
                        "faceDown": false,
                        "bind": "ghostSpiderExiledCard",
                    },
                    {
                        "kind": "grantPermission",
                        "player": controller(),
                        "action": {
                            "kind": "play",
                            "card": { "kind": "boundObject", "binding": "ghostSpiderExiledCard" },
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                        },
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Remove two counters of one chosen kind",
                "Exile and permit the top card this turn",
            ],
        ));
    }

    if text.starts_with(
        "Whenever a modified creature you control deals combat damage to a player, draw a card.",
    ) {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
                "where": and(vec![card_type("Creature"), json!({ "kind": "isModified" })]),
            }),
            None,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }

    if text.starts_with(
        "Whenever a modified creature you control deals combat damage to a player, exile the top card of your library. You may play that card this turn.",
    ) {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
                "where": and(vec![card_type("Creature"), json!({ "kind": "isModified" })]),
            }),
            None,
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(controller()),
                    "count": integer(1),
                    "faceDown": false,
                    "bind": "modifiedCombatExiledCard",
                }),
                json!({
                    "kind": "grantPermission",
                    "player": controller(),
                    "action": {
                        "kind": "play",
                        "card": { "kind": "boundObject", "binding": "modifiedCombatExiledCard" },
                        "normalTimingApplies": true,
                        "normalCostsApply": true,
                    },
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
        ));
    }

    if text.starts_with(
        "Whenever Silver Sable attacks, target modified creature you control gains lifelink until end of turn.",
    ) {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "modifiedCreature",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![card_type("Creature"), json!({ "kind": "isModified" })]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "grantKeyword",
                "object": chosen_target("modifiedCreature"),
                "keyword": "lifelink",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }

    if text.starts_with(
        "Whenever a modified creature you control leaves the battlefield, put a +1/+1 counter on this artifact.",
    ) {
        return Some(triggered(
            json!({
                "kind": "controlledPermanentLeftBattlefield",
                "player": controller(),
                "where": and(vec![card_type("Creature"), json!({ "kind": "isModified" })]),
                "excludeSource": false,
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }

    if text
        == "{T}: Move a +1/+1 counter from this artifact onto target creature you control. Activate only as a sorcery."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "tap", "object": self_ref() },
                    {
                        "kind": "removeCounters",
                        "permanent": self_ref(),
                        "counter": "+1/+1",
                        "count": integer(1),
                    },
                ],
                "activationCondition": { "kind": "sorceryTiming" },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "counterRecipient",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": chosen_target("counterRecipient"),
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Remove the source counter as a cost",
                "Put it on the targeted controlled creature",
            ],
        ));
    }

    if text == "When Silver Sable enters, put a +1/+1 counter on another target creature." {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
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
                "kind": "putCounters",
                "permanent": chosen_target("targetCreature"),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }

    if text
        == "At the beginning of combat on your turn, up to one target creature gains first strike and vigilance until end of turn."
    {
        let target = chosen_target("targetCreature");
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    0,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "grantKeyword",
                    "object": target.clone(),
                    "keyword": "firstStrike",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": target,
                    "keyword": "vigilance",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
        ));
    }

    if text
        == "Vibro-Shock Gauntlets — When Shocker enters, he deals 2 damage to target creature and 2 damage to that creature's controller."
    {
        let target = chosen_target("targetCreature");
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "bind",
                    "id": "damagedController",
                    "value": { "kind": "controllerOf", "object": target.clone() },
                }),
                json!({ "kind": "dealDamage", "source": self_ref(), "recipient": target, "amount": integer(2) }),
                json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "recipient": { "kind": "boundValue", "id": "damagedController" },
                    "amount": integer(2),
                }),
            ],
        ));
    }

    if text == "Whenever Vulture attacks, other Villains you control gain flying until end of turn."
    {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "grantKeyword",
                "object": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), subtype("Villain")]),
                    "excludeSource": true,
                },
                "keyword": "flying",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }

    if text
        == "When Spider-Girl leaves the battlefield, create a 1/1 green and white Human Citizen creature token."
    {
        return Some(triggered(
            json!({ "kind": "permanentLeftBattlefield", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": {
                    "name": "Human Citizen Token",
                    "colors": ["green", "white"],
                    "types": ["Creature"],
                    "subtypes": ["Human", "Citizen"],
                    "power": 1,
                    "toughness": 1,
                },
            })],
        ));
    }

    if text == "Whenever you discard a card, Hobgoblin gets +2/+0 until end of turn." {
        return Some(triggered(
            json!({ "kind": "playerDiscardedCard", "player": controller() }),
            None,
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": self_ref(),
                "power": integer(2),
                "toughness": integer(0),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }

    if text
        == "Whenever a creature you control with mana value 5 or greater enters, you may attach this Equipment to it."
    {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": and(vec![
                    card_type("Creature"),
                    compare(
                        ">=",
                        json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                        integer(5),
                    ),
                ]),
            }),
            None,
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [{
                    "kind": "attachPermanent",
                    "attachment": self_ref(),
                    "to": { "kind": "triggeringPermanent" },
                }],
            })],
        ));
    }

    if text
        == "When this Equipment enters, if it was cast from your graveyard, attach it to target creature you control."
    {
        return Some(triggered(
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
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
            vec![json!({
                "kind": "conditionalEffect",
                "condition": { "kind": "wasCastFromZone", "object": self_ref(), "zone": "graveyard" },
                "then": [{
                    "kind": "attachPermanent",
                    "attachment": self_ref(),
                    "to": chosen_target("targetCreature"),
                }],
                "else": [],
            })],
        ));
    }

    let counter_two_re = Regex::new(
        r"^When (.+?) enters, put a \+1/\+1 counter on each of up to two target creatures\.$",
    )
    .expect("SPM two-creature counter trigger regex compiles");
    if let Some(captures) = counter_two_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "counterTargets",
                        json!({ "kind": "permanents", "where": card_type("Creature") }),
                        0,
                        2,
                    )],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": { "kind": "chosenTargets", "id": "counterTargets" },
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &["Choose up to two creatures", "Put a +1/+1 counter on each"],
        ));
    }

    if text
        == "At the beginning of your end step, if you have fewer than eight cards in hand, draw cards equal to the difference."
    {
        let hand_count = json!({
            "kind": "countCards",
            "zone": { "kind": "hand", "player": controller() },
            "where": Value::Null,
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": controller() },
                "effects": [{
                    "kind": "conditionalEffect",
                    "condition": compare("<", hand_count.clone(), integer(8)),
                    "then": [{
                        "kind": "drawCards",
                        "player": controller(),
                        "count": { "kind": "subtract", "left": integer(8), "right": hand_count },
                    }],
                    "else": [],
                }],
            }),
            &[
                "Check the controller's end-step hand size",
                "Draw up to eight cards",
            ],
        ));
    }

    if text == "When this land enters from a graveyard, you lose 2 life." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                    "fromZone": "graveyard",
                },
                "effects": [{
                    "kind": "loseLife",
                    "player": controller(),
                    "amount": integer(2),
                }],
            }),
            &["Confirm the land entered from a graveyard", "Lose two life"],
        ));
    }

    None
}
