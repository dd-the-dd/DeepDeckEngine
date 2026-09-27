use super::super::*;

fn self_enters() -> Value {
    json!({ "kind": "enterBattlefield", "object": self_ref() })
}

pub(in crate::oracle::canonical) fn parse_tla_leaf_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text.starts_with(
        "Exhaust — Waterbend {3}: This Vehicle becomes an artifact creature. Put three +1/+1 counters on it.",
    ) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payWaterbend", "amount": integer(3) }],
                "activationLimit": { "kind": "oncePerGameObject", "id": "exhaust" },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaInvasionSubmersible",
                }],
            }),
            &["Pay the waterbend exhaust cost", "Make the Vehicle a creature", "Put three +1/+1 counters on it"],
        ));
    }

    if text
        == "Whenever another creature you control dies, if it had counters on it, put its counters on this creature."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                    "hadAnyCounters": true,
                },
                "effects": [{
                    "kind": "putSameCountersAs",
                    "permanent": self_ref(),
                    "source": { "kind": "triggeringPermanent" },
                }],
            }),
            &[
                "Watch another controlled creature with counters die",
                "Move each of its counters to this creature",
            ],
        ));
    }

    if text.starts_with(
        "Whenever a nonland creature you control dies, earthbend X, where X is that creature's power.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), not(card_type("Land"))]),
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "earthbendLand",
                        json!({ "kind": "permanents", "controller": controller(), "where": card_type("Land") }),
                        1,
                        1,
                    )],
                },
                "effects": [earthbend_effect(
                    "earthbendLand",
                    json!({ "kind": "deadPermanentPower" }),
                )],
            }),
            &["Watch a controlled nonland creature die", "Remember its power", "Earthbend that much"],
        ));
    }

    if text.starts_with(
        "When Kyoshi Island Plaza enters, search your library for up to X basic land cards, where X is the number of Shrines you control.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "searchLibrary",
                    "player": controller(),
                    "where": and(vec![
                        json!({ "kind": "typeLineContains", "value": "Basic" }),
                        card_type("Land"),
                    ]),
                    "maximum": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": subtype("Shrine"),
                    },
                    "destination": "battlefield",
                    "tapped": true,
                }],
            }),
            &["Count controlled Shrines", "Find up to that many basic lands", "Put them onto the battlefield tapped and shuffle"],
        ));
    }

    if text.starts_with("When this creature enters, earthbend 1, then earthbend 1.") {
        let lands = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": card_type("Land"),
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("firstEarthbendLand", lands.clone(), 1, 1),
                        target_decision("secondEarthbendLand", lands, 1, 1),
                    ],
                },
                "effects": [
                    earthbend_effect("firstEarthbendLand", integer(1)),
                    earthbend_effect("secondEarthbendLand", integer(1)),
                ],
            }),
            &[
                "Watch the creature enter",
                "Choose a land and earthbend one",
                "Choose a land and earthbend one again",
            ],
        ));
    }

    if text
        == "Whenever this creature attacks, each opponent loses X life and you gain X life, where X is the number of creatures you control with +1/+1 counters on them."
    {
        let amount = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": and(vec![
                card_type("Creature"),
                json!({ "kind": "hasCounter", "counter": "+1/+1" }),
            ]),
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "effects": [
                    { "kind": "loseLifeEachOpponent", "amount": amount.clone() },
                    { "kind": "gainLife", "player": controller(), "amount": amount },
                ],
            }),
            &[
                "Count controlled creatures with +1/+1 counters",
                "Drain each opponent for that amount",
                "Gain that amount of life",
            ],
        ));
    }

    if text.starts_with(
        "The owner of target creature or enchantment puts it into their library second from the top or on the bottom.",
    ) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "lostDaysPermanent",
                        json!({
                            "kind": "permanents",
                            "where": or(vec![card_type("Creature"), card_type("Enchantment")]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "ownerChoosesSecondFromTopOrBottom",
                        "permanent": chosen_target("lostDaysPermanent"),
                    },
                    {
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": integer(1),
                        "token": { "kind": "namedToken", "name": "Clue" },
                    },
                ],
            }),
            &["Target a creature or enchantment", "Let its owner choose its library position", "Create a Clue"],
        ));
    }

    if text.starts_with("Affinity for Allies ") {
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
                        "player": controller(),
                        "where": subtype("Ally"),
                    },
                }],
            }),
            &[
                "Count controlled Allies",
                "Reduce this spell's generic cost by that amount",
            ],
        ));
    }

    if text
        == "Up to two target creatures you control each deal damage equal to their power to target creature an opponent controls."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "alliesAtLastSources",
                            json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
                            0,
                            2,
                        ),
                        target_decision(
                            "alliesAtLastDefender",
                            json!({ "kind": "permanents", "controller": { "kind": "opponentsOf", "player": controller() }, "where": card_type("Creature") }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaAlliesAtLastDamage",
                }],
            }),
            &[
                "Choose up to two controlled creatures",
                "Target an opposing creature",
                "Have the chosen creatures deal their power",
            ],
        ));
    }

    if text.starts_with(
        "Whenever one or more creatures you control with flying attack, draw a card, then discard a card.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "minimum": integer(1),
                    "where": json!({ "kind": "hasKeyword", "value": "flying" }),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaTeoLoot",
                }],
            }),
            &["Watch one or more flying creatures attack", "Draw then discard", "Counter a controlled creature if the discard was nonland"],
        ));
    }

    if text.starts_with(
        "Whenever you scry or surveil, look at the top card of your library. You may cast that card without paying its mana cost.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "scried", "player": controller() },
                        { "kind": "surveilled", "player": controller() },
                    ],
                },
                "triggerLimit": { "kind": "onceEachTurn", "id": "tlaPlanetarium" },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaPlanetariumCast",
                }],
            }),
            &["Watch the controller scry or surveil once each turn", "Look at the top card", "Optionally cast it for free"],
        ));
    }

    if text
        == "{T}: Add X mana of any one color, where X is the greatest number of creatures you control that have a creature type in common."
    {
        return Some(draft(
            json!({
                "kind": "manaAbility",
                "source": self_ref(),
                "costs": [{ "kind": "tap", "object": self_ref() }],
                "effects": [{
                    "kind": "addMana",
                    "player": controller(),
                    "mana": {
                        "kind": "chooseColor",
                        "amount": {
                            "kind": "greatestSharedCreatureTypeCount",
                            "player": controller(),
                        },
                    },
                }],
            }),
            &[
                "Tap the Tile",
                "Find the most common controlled creature type",
                "Add that much mana of a chosen color",
            ],
        ));
    }

    if text.starts_with(
        "Waterbend {5}, {T}: You may cast a noncreature spell from your hand without paying its mana cost.",
    ) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payWaterbend", "amount": integer(5) },
                    { "kind": "tap", "object": self_ref() },
                ],
                "effects": [
                    {
                        "kind": "chooseCards",
                        "id": "yueSpell",
                        "player": controller(),
                        "minimum": integer(0),
                        "maximum": integer(1),
                        "candidates": {
                            "kind": "cards",
                            "zone": hand(controller()),
                            "where": not(card_type("Creature")),
                        },
                    },
                    {
                        "kind": "castAnyNumber",
                        "player": controller(),
                        "cards": decision_result("yueSpell"),
                        "where": { "kind": "canBeCastAsSpell" },
                        "timing": { "kind": "duringResolution" },
                        "withoutPayingManaCost": true,
                        "alternativeCostsAllowed": false,
                        "additionalCostsApply": true,
                        "variableManaValue": integer(0),
                    },
                ],
            }),
            &["Pay waterbend five and tap Yue", "Choose an optional noncreature spell from hand", "Cast it without paying its mana cost"],
        ));
    }

    if text
        == "Exile X target creature cards from graveyards. For each creature card exiled this way, create a token that's a copy of it. At the beginning of your next end step, sacrifice those tokens."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "foggyCreatures",
                        "kind": "chooseTargets",
                        "minimum": { "kind": "sourceCastXValue" },
                        "maximum": { "kind": "sourceCastXValue" },
                        "candidates": {
                            "kind": "cards",
                            "zone": { "kind": "anyGraveyard" },
                            "where": card_type("Creature"),
                        },
                    }],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaFoggySwampVisions",
                }],
            }),
            &[
                "Target X creature cards in graveyards",
                "Exile them and create token copies",
                "Sacrifice the copies at the next end step",
            ],
        ));
    }

    if text
        == "Whenever Combustion Man attacks, destroy target permanent unless its controller has Combustion Man deal damage to them equal to his power."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "combustionPermanent",
                        json!({ "kind": "permanents", "where": Value::Null }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaCombustionManAttack",
                }],
            }),
            &[
                "Watch Combustion Man attack",
                "Target a permanent",
                "Let its controller take damage or have it destroyed",
            ],
        ));
    }

    if text
        == "Target creature can't block this turn. If this spell was kicked, gain control of that creature until end of turn, untap it, and it gains haste until end of turn."
    {
        let target = chosen_target("brainwashedCreature");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "brainwashedCreature",
                        json!({ "kind": "permanents", "where": card_type("Creature") }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "grantKeyword",
                        "object": target.clone(),
                        "keyword": "cantBlock",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": { "kind": "wasKicked", "spell": self_ref() },
                        "then": [
                            {
                                "kind": "gainControlPermanent",
                                "permanent": target.clone(),
                                "controller": controller(),
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                            { "kind": "untapPermanent", "permanent": target.clone() },
                            {
                                "kind": "grantKeyword",
                                "object": target,
                                "keyword": "haste",
                                "duration": { "kind": "untilEndOfCurrentTurn" },
                            },
                        ],
                        "else": [],
                    },
                ],
            }),
            &[
                "Prevent the target from blocking",
                "Check whether the spell was kicked",
                "Temporarily steal, untap, and haste it",
            ],
        ));
    }

    if text
        == "When Lo and Li enter, search your library for a Lesson or Noble card, reveal it, put it into your hand, then shuffle."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "searchLibrary",
                    "player": controller(),
                    "where": or(vec![subtype("Lesson"), subtype("Noble")]),
                    "maximum": integer(1),
                    "destination": "hand",
                    "tapped": false,
                }],
            }),
            &[
                "Watch Lo and Li enter",
                "Find a Lesson or Noble",
                "Reveal it, put it into hand, and shuffle",
            ],
        ));
    }

    if text == "Noble creatures you control and Lesson spells you control have lifelink." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "grantKeyword",
                        "objects": {
                            "kind": "permanents",
                            "controller": controller(),
                            "where": and(vec![card_type("Creature"), subtype("Noble")]),
                        },
                        "keyword": "lifelink",
                    },
                    {
                        "kind": "grantKeyword",
                        "objects": {
                            "kind": "spells",
                            "controller": controller(),
                            "where": subtype("Lesson"),
                        },
                        "keyword": "lifelink",
                    },
                ],
            }),
            &[
                "Grant lifelink to controlled Noble creatures",
                "Grant lifelink to controlled Lesson spells",
            ],
        ));
    }

    if text.starts_with(
        "Whenever this creature attacks, choose target creature an opponent controls. Tap it, then you may sacrifice an artifact or creature.",
    ) {
        let target = chosen_target("vengefulCreature");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "vengefulCreature",
                        json!({
                            "kind": "permanents",
                            "controller": { "kind": "opponentsOf", "player": controller() },
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    { "kind": "tapPermanent", "permanent": target.clone() },
                    {
                        "kind": "optionalAction",
                        "player": controller(),
                        "action": {
                            "kind": "sacrificePermanents",
                            "player": controller(),
                            "where": or(vec![card_type("Artifact"), card_type("Creature")]),
                            "count": integer(1),
                            "excludeSource": false,
                        },
                        "onPerformed": [{
                            "kind": "putCounters",
                            "permanent": target,
                            "counter": "stun",
                            "count": integer(1),
                        }],
                    },
                ],
            }),
            &["Tap a targeted opposing creature", "Optionally sacrifice an artifact or creature", "Put a stun counter on the target if paid"],
        ));
    }

    if text.starts_with("Choose one")
        && text.contains("copy of target creature you control")
        && text.contains("4/4 Hero in addition to its other types")
        && text.contains("copy of target creature an opponent controls")
        && text.contains("2/2 Coward in addition to its other types")
    {
        let hero_mode = selection("chosenModes", "hero");
        let coward_mode = selection("chosenModes", "coward");
        let mut hero_target = target_decision(
            "heroCreature",
            json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
            1,
            1,
        );
        hero_target["condition"] = hero_mode.clone();
        let mut coward_target = target_decision(
            "cowardCreature",
            json!({
                "kind": "permanents",
                "controller": { "kind": "opponentsOf", "player": controller() },
                "where": card_type("Creature"),
            }),
            1,
            1,
        );
        coward_target["condition"] = coward_mode.clone();
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["hero", "coward"] },
                        hero_target,
                        coward_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": hero_mode,
                        "then": [{
                            "kind": "createModifiedTokenCopy",
                            "object": chosen_target("heroCreature"),
                            "controller": controller(),
                            "removeLegendary": true,
                            "basePower": integer(4),
                            "baseToughness": integer(4),
                            "addSubtypes": ["Hero"],
                            "grantKeywords": [],
                        }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": coward_mode,
                        "then": [{
                            "kind": "createModifiedTokenCopy",
                            "object": chosen_target("cowardCreature"),
                            "controller": controller(),
                            "removeLegendary": true,
                            "basePower": integer(2),
                            "baseToughness": integer(2),
                            "addSubtypes": ["Coward"],
                            "grantKeywords": [],
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose Hero or Coward",
                "Target a creature controlled by the proper player",
                "Create the modified nonlegendary copy",
            ],
        ));
    }

    if text.starts_with(
        "Enchanted creature loses all abilities and is a Citizen with base power and toughness 1/1",
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
                    { "kind": "setName", "objects": enchanted.clone(), "name": "Humble Merchant" },
                    { "kind": "setBasePowerToughness", "objects": enchanted.clone(), "power": integer(1), "toughness": integer(1) },
                    { "kind": "grantManaAbility", "objects": enchanted, "mana": "{C}" },
                ],
            }),
            &[
                "Remove the enchanted creature's abilities",
                "Rename it and make it a 1/1 Citizen",
                "Grant its colorless mana ability",
            ],
        ));
    }

    if text.starts_with(
        "Tap up to X target creatures, then distribute three stun counters among any number of tapped creatures your opponents control.",
    ) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "crashingWaveCreatures",
                        "kind": "chooseTargets",
                        "minimum": integer(0),
                        "maximum": { "kind": "sourceCastXValue" },
                        "candidates": { "kind": "permanents", "where": card_type("Creature") },
                    }],
                },
                "effects": [
                    {
                        "kind": "tapPermanent",
                        "permanent": { "kind": "chosenTargets", "id": "crashingWaveCreatures" },
                    },
                    {
                        "kind": "resolveTriggeredInstruction",
                        "operation": "tlaCrashingWaveStun",
                    },
                ],
            }),
            &["Target up to X creatures", "Tap them", "Distribute three stun counters among tapped opposing creatures"],
        ));
    }

    if text.starts_with(
        "Whenever you sacrifice a Food, you may search your library for a basic land card and put it onto the battlefield tapped.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentSacrificed",
                    "player": controller(),
                    "where": subtype("Food"),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaUnluckyCabbageMerchant",
                }],
            }),
            &["Watch a Food be sacrificed", "Optionally find a basic land tapped", "If found, shuffle the Merchant into its owner's library"],
        ));
    }

    if text
        == "Each player chooses any number of creatures they control with total power 4 or less, then sacrifices all other creatures they control."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaDestinedConfrontation",
                }],
            }),
            &[
                "Let each player choose creatures totaling at most four power",
                "Remember every chosen creature",
                "Sacrifice all other creatures",
            ],
        ));
    }

    if text
        == "Each non-Human creature you control gets +1/+1 for each of its creature types, to a maximum of 10."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughnessByCreatureTypeCount",
                    "objects": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![
                            card_type("Creature"),
                            not(subtype("Human")),
                        ]),
                    },
                    "maximum": integer(10),
                }],
            }),
            &[
                "Select controlled non-Human creatures",
                "Count each creature's creature types",
                "Give +1/+1 per type, capped at ten",
            ],
        ));
    }

    if text.starts_with(
        "When Bumi enters, choose up to X, where X is the number of Lesson cards in your graveyard",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaBumiThreeTrials",
                }],
            }),
            &[
                "Count Lesson cards in the graveyard",
                "Choose that many distinct modes, up to three",
                "Resolve counters, scry, or earthbend",
            ],
        ));
    }

    if text.starts_with(
        "At the beginning of combat on your turn, you may have target opponent gain control of target permanent you control.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "player": controller(), "step": "beginCombat" },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "irohOpponent",
                            json!({ "kind": "players" }),
                            1,
                            1,
                        ),
                        target_decision(
                            "irohPermanent",
                            json!({ "kind": "permanents", "controller": controller(), "where": Value::Null }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaIrohTeaMaster",
                }],
            }),
            &["Target an opponent and a controlled permanent", "Optionally transfer the permanent", "Create and grow an Ally token"],
        ));
    }

    if text
        == "Equipped creature has \"{1}, {T}: Tap target creature. Return Trusty Boomerang to its owner's hand.\""
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
                        "source": self_ref(),
                        "costs": [
                            { "kind": "payMana", "manaCost": "{1}" },
                            { "kind": "tap", "object": self_ref() },
                        ],
                        "declaration": {
                            "kind": "castingDeclaration",
                            "decisions": [target_decision(
                                "boomerangCreature",
                                json!({ "kind": "permanents", "where": card_type("Creature") }),
                                1,
                                1,
                            )],
                        },
                        "effects": [{
                            "kind": "resolveTriggeredInstruction",
                            "operation": "tlaTrustyBoomerang",
                        }],
                    },
                }],
            }),
            &[
                "Grant the activated ability to the equipped creature",
                "Tap the equipped creature and a target creature",
                "Return Trusty Boomerang to its owner's hand",
            ],
        ));
    }

    if text
        == "All creatures get -2/-2 until end of turn. If this spell's additional cost was paid, whenever a creature dies this turn, you gain 1 life."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": {
                            "kind": "eachPermanent",
                            "where": card_type("Creature"),
                        },
                        "power": integer(-2),
                        "toughness": integer(-2),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": selection("additionalCostMode", "pay"),
                        "then": [{
                            "kind": "installCreatureDeathLifeGainUntilEndOfTurn",
                            "player": controller(),
                            "amount": integer(1),
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Give all creatures -2/-2 this turn",
                "Check whether the optional additional cost was paid",
                "Gain life for each creature that dies this turn",
            ],
        ));
    }

    if text
        == "When enchanted creature dies, mill cards equal to its power. Return this card to its owner's hand and up to one creature card milled this way to the battlefield under your control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "attachedPermanentDied", "attachment": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaAvatarDestiny",
                }],
            }),
            &[
                "Remember the enchanted creature's power",
                "Mill that many and return Avatar Destiny",
                "Optionally return a creature milled this way",
            ],
        ));
    }

    if text.starts_with(
        "At the beginning of your first main phase, choose one that hasn't been chosen and you lose 2 life",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "player": controller(),
                    "step": "precombatMain",
                    "firstOnly": true,
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaZukoConflicted",
                }],
            }),
            &["Trigger at the first main phase", "Choose an unused mode and lose two life", "Persist the chosen mode on Zuko"],
        ));
    }

    if text.starts_with("Draw three cards. Then discard a card unless you waterbend {2}.") {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    { "kind": "drawCards", "player": controller(), "count": integer(3) },
                    {
                        "kind": "optionalPayCostPerformEffects",
                        "player": controller(),
                        "cost": { "kind": "payWaterbend", "amount": integer(2) },
                        "effects": [],
                        "declineEffects": [{
                            "kind": "discardCards",
                            "player": controller(),
                            "count": integer(1),
                        }],
                    },
                ],
            }),
            &[
                "Draw three cards",
                "Offer waterbend two",
                "Discard a card if the waterbend cost is declined",
            ],
        ));
    }

    if text.starts_with(
        "During your turn, each non-Lesson instant and sorcery card in your graveyard has flashback.",
    ) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantFlashbackInGraveyard",
                    "player": controller(),
                    "where": and(vec![
                        or(vec![card_type("Instant"), card_type("Sorcery")]),
                        not(subtype("Lesson")),
                    ]),
                    "cost": { "kind": "manaCostOfCard" },
                    "duringControllerTurnOnly": true,
                }],
            }),
            &["Select non-Lesson instants and sorceries in the controller's graveyard", "Grant flashback equal to each card's mana cost during the controller's turn"],
        ));
    }

    if text == "During your turn, each Lesson card in your graveyard has flashback {1}." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantFlashbackInGraveyard",
                    "player": controller(),
                    "where": subtype("Lesson"),
                    "cost": { "kind": "payMana", "manaCost": "{1}" },
                    "duringControllerTurnOnly": true,
                }],
            }),
            &[
                "Select Lesson cards in the controller's graveyard",
                "Grant flashback one during the controller's turn",
            ],
        ));
    }

    if text.starts_with("When Hama enters, target opponent mills three cards.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(), "event": self_enters(),
                "declaration": { "kind": "castingDeclaration", "decisions": [target_decision("hamaOpponent", json!({ "kind": "players" }), 1, 1)] },
                "effects": [{ "kind": "resolveTriggeredInstruction", "operation": "tlaHamaEnter" }],
            }),
            &[
                "Mill the targeted opponent",
                "Optionally exile a noncreature nonland card",
                "Grant its linked waterbend casting permission while Hama is controlled",
            ],
        ));
    }

    if text == "When Koh enters, exile up to one other target creature." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(), "event": self_enters(),
                "declaration": { "kind": "castingDeclaration", "decisions": [target_decision("kohCreature", json!({ "kind": "permanents", "where": card_type("Creature"), "excludeSource": true }), 0, 1)] },
                "effects": [{ "kind": "resolveTriggeredInstruction", "operation": "tlaKohExileTarget" }],
            }),
            &[
                "Watch Koh enter",
                "Exile up to one other creature and link it to Koh",
            ],
        ));
    }

    if text == "Whenever another nontoken creature dies, you may exile it." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(),
                "event": { "kind": "permanentDied", "where": card_type("Creature"), "excludeSource": true, "nontoken": true },
                "effects": [{ "kind": "resolveTriggeredInstruction", "operation": "tlaKohExileDead" }],
            }),
            &[
                "Watch another nontoken creature die",
                "Optionally exile and link it to Koh",
            ],
        ));
    }

    if text == "Pay 1 life: Choose a creature card exiled with Koh." {
        return Some(draft(
            json!({
                "kind": "activatedAbility", "source": self_ref(), "costs": [{ "kind": "payLife", "amount": integer(1) }],
                "effects": [{ "kind": "resolveTriggeredInstruction", "operation": "tlaKohChooseFace" }],
            }),
            &[
                "Pay one life",
                "Choose a creature exiled with Koh",
                "Install its activated and triggered abilities",
            ],
        ));
    }

    if text == "Koh has all activated and triggered abilities of the last chosen card." {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Abilities are installed dynamically by Koh's linked choice ability"],
        ));
    }

    if text == "You may cast Ally spells from the top of your library." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "playCardsFromTopLibrary",
                    "player": controller(),
                    "where": subtype("Ally"),
                }],
            }),
            &["Permit Ally spells from the top of the controller's library"],
        ));
    }

    if text.starts_with("Choose one")
        && text.contains("Target opponent reveals their hand")
        && text.contains("You choose a nonland permanent card from it")
        && text.contains("Earthbend 2")
    {
        return Some(draft(
            json!({
                "kind": "spellAbility", "source": self_ref(),
                "effects": [{ "kind": "resolveTriggeredInstruction", "operation": "tlaDaiLiIndoctrination" }],
            }),
            &[
                "Choose hand disruption or earthbend",
                "Resolve the selected Dai Li Indoctrination mode",
            ],
        ));
    }

    if text
        == "The first non-Lemur creature spell with flying you cast during each of your turns costs {1} less to cast."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "reduceCastingCost",
                    "player": controller(),
                    "where": and(vec![
                        card_type("Creature"),
                        json!({ "kind": "hasKeyword", "value": "flying" }),
                        not(subtype("Lemur")),
                    ]),
                    "amount": integer(1),
                    "firstEachTurn": true,
                    "duringControllerTurnOnly": true,
                }],
            }),
            &[
                "Filter non-Lemur flying creature spells",
                "Reduce the first qualifying spell by one",
            ],
        ));
    }

    if text
        == "Whenever another creature you control with flying enters, Momo gets +1/+1 until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": and(vec![
                        card_type("Creature"),
                        json!({ "kind": "hasKeyword", "value": "flying" }),
                    ]),
                    "excludeSource": true,
                },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": self_ref(),
                    "power": integer(1),
                    "toughness": integer(1),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Watch another controlled flying creature enter",
                "Give Momo +1/+1 this turn",
            ],
        ));
    }

    if text == "At the beginning of your upkeep, tap this creature." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "player": controller(), "step": "upkeep" },
                "effects": [{ "kind": "tapPermanent", "permanent": self_ref() }],
            }),
            &["Trigger on the controller's upkeep", "Tap the source"],
        ));
    }

    if text == "Whenever a player casts a noncreature spell, they lose 2 life." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "anyPlayer": true,
                    "where": not(card_type("Creature")),
                },
                "effects": [{
                    "kind": "loseLife",
                    "player": { "kind": "triggeringPlayer" },
                    "amount": integer(2),
                }],
            }),
            &[
                "Watch every noncreature spell",
                "Make its caster lose two life",
            ],
        ));
    }

    if text
        == "Whenever another Ally you control enters, put a +1/+1 counter on that creature and it gains haste until end of turn."
    {
        let entering = json!({ "kind": "triggeringPermanent" });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": subtype("Ally"),
                    "excludeSource": true,
                },
                "effects": [
                    {
                        "kind": "putCounters",
                        "permanent": entering.clone(),
                        "counter": "+1/+1",
                        "count": integer(1),
                    },
                    {
                        "kind": "grantKeyword",
                        "object": entering,
                        "keyword": "haste",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Watch another controlled Ally enter",
                "Add a counter and temporary haste",
            ],
        ));
    }

    if text
        == "{T}: Until end of turn, target artifact token you control becomes a 3/1 Construct artifact creature with flying."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "tap", "object": self_ref() }],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "artifactToken",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": and(vec![card_type("Artifact"), json!({ "kind": "isToken" })]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "becomeCreature",
                        "object": chosen_target("artifactToken"),
                        "addTypes": ["Creature"],
                        "addSubtypes": ["Construct"],
                        "basePower": 3,
                        "baseToughness": 1,
                        "retainExistingTypes": true,
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("artifactToken"),
                        "keyword": "flying",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Pay the tap cost",
                "Target a controlled artifact token",
                "Animate it with flying",
            ],
        ));
    }

    if text == "Tap an untapped Ally you control: Exile target card from a graveyard." {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{
                    "kind": "tap",
                    "where": subtype("Ally"),
                    "excludeSource": false,
                }],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "graveyardCard",
                        json!({ "kind": "cards", "zone": { "kind": "anyGraveyard" }, "where": Value::Null }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "exileTargetCards",
                    "cards": { "kind": "chosenTargets", "id": "graveyardCard" },
                }],
            }),
            &[
                "Tap an untapped controlled Ally",
                "Exile the targeted graveyard card",
            ],
        ));
    }

    if matches!(
        text,
        "When this creature enters, discard up to two cards, then draw that many cards."
            | "When Sokka enters, discard up to two cards, then draw that many cards."
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaSokkaDiscardDraw",
                }],
            }),
            &[
                "Choose up to two cards at resolution",
                "Discard and draw the same number",
            ],
        ));
    }

    if text.starts_with("When The Fire Nation Drill enters, you may tap it. When you do, destroy target creature with power 4 or less.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tlaFireNationDrill",
                }],
            }),
            &["Optionally tap the Drill", "Destroy a creature with power four or less"],
        ));
    }

    if text
        == "Whenever you cast a spell from exile and whenever a permanent you control enters from exile, put a +1/+1 counter on each creature you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        {
                            "kind": "spellCast",
                            "player": controller(),
                            "where": Value::Null,
                            "fromZone": "exile",
                        },
                        {
                            "kind": "permanentEntered",
                            "player": controller(),
                            "where": Value::Null,
                            "fromZone": "exile",
                        },
                    ],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Watch controlled spells and permanents arrive from exile",
                "Counter every controlled creature",
            ],
        ));
    }

    let lesson_threshold = || {
        compare(
            ">=",
            json!({
                "kind": "countCards",
                "zone": graveyard(controller()),
                "where": subtype("Lesson"),
            }),
            integer(1),
        )
    };

    if text == "This creature gets +1/+1 as long as there's a Lesson card in your graveyard." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": self_ref(),
                    "power": integer(1),
                    "toughness": integer(1),
                    "condition": lesson_threshold(),
                }],
            }),
            &[
                "Count Lesson cards in the controller's graveyard",
                "Apply the conditional bonus",
            ],
        ));
    }

    if text
        == "Noncreature spells you cast cost {1} less to cast as long as there are three or more Lesson cards in your graveyard."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "reduceCastingCost",
                    "player": controller(),
                    "where": not(card_type("Creature")),
                    "amount": integer(1),
                    "condition": compare(
                        ">=",
                        json!({
                            "kind": "countCards",
                            "zone": graveyard(controller()),
                            "where": subtype("Lesson"),
                        }),
                        integer(3),
                    ),
                }],
            }),
            &[
                "Count Lesson cards in the controller's graveyard",
                "Reduce noncreature spell costs at three Lessons",
            ],
        ));
    }

    if text.starts_with(
        "Waterbend {3}: This creature has base power and toughness 5/2 until end of turn.",
    ) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payWaterbend", "amount": integer(3) }],
                "effects": [{
                    "kind": "setBasePowerToughness",
                    "object": self_ref(),
                    "power": integer(5),
                    "toughness": integer(2),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Pay waterbend three",
                "Set the source's base power and toughness for the turn",
            ],
        ));
    }

    if text.starts_with("Waterbend {3}: This creature can't be blocked this turn.") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payWaterbend", "amount": integer(3) }],
                "effects": [{
                    "kind": "grantKeyword",
                    "object": self_ref(),
                    "keyword": "cantBeBlocked",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Pay waterbend three",
                "Prevent the source from being blocked this turn",
            ],
        ));
    }

    if text == "{3}: Until end of turn, this creature has base power 4 and gains trample." {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payMana", "manaCost": "{3}" }],
                "effects": [
                    {
                        "kind": "setPower",
                        "object": self_ref(),
                        "power": integer(4),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": self_ref(),
                        "keyword": "trample",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Pay three mana",
                "Set base power four and grant trample for the turn",
            ],
        ));
    }

    if text.starts_with(
        "When this creature enters, tap target creature an opponent controls and put a stun counter on it.",
    ) {
        let target = chosen_target("snowballTarget");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "snowballTarget",
                        json!({
                            "kind": "permanents",
                            "controller": { "kind": "opponentsOf", "player": controller() },
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    { "kind": "tapPermanent", "permanent": target.clone() },
                    { "kind": "putCounters", "permanent": target, "counter": "stun", "count": integer(1) },
                ],
            }),
            &[
                "Target an opposing creature",
                "Tap it and put a stun counter on it",
            ],
        ));
    }

    if text == "When The Spirit Oasis enters, draw a card for each Shrine you control." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": subtype("Shrine"),
                    },
                }],
            }),
            &["Count controlled Shrines", "Draw that many cards"],
        ));
    }

    if text == "Creatures you control with flying get +1/+1." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![
                            card_type("Creature"),
                            json!({ "kind": "hasKeyword", "value": "flying" }),
                        ]),
                    },
                    "power": integer(1),
                    "toughness": integer(1),
                }],
            }),
            &["Select controlled flying creatures", "Give them +1/+1"],
        ));
    }

    if matches!(
        text,
        "When this creature enters, put a +1/+1 counter on each of up to two target creatures you control."
            | "II — Put a +1/+1 counter on each of up to two target creatures you control."
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": if text.starts_with("II") {
                    json!({ "kind": "sagaChapterReached", "object": self_ref(), "chapters": [integer(2)] })
                } else {
                    self_enters()
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "tlaCounterTargets",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                        }),
                        0,
                        2,
                    )],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": { "kind": "chosenTargets", "id": "tlaCounterTargets" },
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Choose up to two controlled creatures",
                "Put a +1/+1 counter on each",
            ],
        ));
    }

    if matches!(
        text,
        "When this creature dies, if there's a Lesson card in your graveyard, you gain 2 life."
            | "When this creature dies, if there's a Lesson card in your graveyard, draw a card."
    ) {
        let reward = if text.ends_with("draw a card.") {
            json!({ "kind": "drawCards", "player": controller(), "count": integer(1) })
        } else {
            json!({ "kind": "gainLife", "player": controller(), "amount": integer(2) })
        };
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "effects": [{
                    "kind": "conditionalEffect",
                    "condition": lesson_threshold(),
                    "then": [reward],
                    "else": [],
                }],
            }),
            &[
                "Check for a Lesson card in the controller's graveyard",
                "Apply the death reward",
            ],
        ));
    }

    if text == "III — Draw a card if there's a creature or Lesson card in your graveyard." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "sagaChapterReached", "object": self_ref(), "chapters": [integer(3)] },
                "effects": [{
                    "kind": "conditionalEffect",
                    "condition": compare(
                        ">=",
                        json!({
                            "kind": "countCards",
                            "zone": graveyard(controller()),
                            "where": or(vec![card_type("Creature"), subtype("Lesson")]),
                        }),
                        integer(1),
                    ),
                    "then": [{ "kind": "drawCards", "player": controller(), "count": integer(1) }],
                    "else": [],
                }],
            }),
            &[
                "Check for a creature or Lesson card in the graveyard",
                "Draw a card when found",
            ],
        ));
    }

    if text
        == "This creature's power is equal to the number of noncreature, nonland cards in your graveyard."
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
                        "kind": "countCards",
                        "zone": graveyard(controller()),
                        "where": and(vec![not(card_type("Creature")), not(card_type("Land"))]),
                    },
                }],
            }),
            &[
                "Count noncreature nonland graveyard cards",
                "Set the source's power",
            ],
        ));
    }

    if text
        == "As long as this Vehicle has three or more fire counters on it, it's an artifact creature."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "addCardType",
                    "objects": self_ref(),
                    "cardType": "Creature",
                    "condition": compare(
                        ">=",
                        json!({ "kind": "countCounters", "object": self_ref(), "counter": "fire" }),
                        integer(3),
                    ),
                }],
            }),
            &[
                "Count fire counters on the source",
                "Make it a creature at three counters",
            ],
        ));
    }

    if text
        == "Whenever you draw your second card each turn, this creature gets +1/+2 until end of turn and can't be blocked this turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "cardDrawn", "player": controller(), "drawOrdinal": integer(2) },
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": self_ref(),
                        "power": integer(1),
                        "toughness": integer(2),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": self_ref(),
                        "keyword": "cantBeBlocked",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Watch the controller's second card draw",
                "Pump the source and make it unblockable",
            ],
        ));
    }

    if text
        == "This spell costs {1} less to cast for each noncreature, nonland card in your graveyard."
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
                    "amount": {
                        "kind": "countCards",
                        "zone": graveyard(controller()),
                        "where": and(vec![not(card_type("Creature")), not(card_type("Land"))]),
                    },
                }],
            }),
            &[
                "Count noncreature nonland graveyard cards",
                "Reduce the source spell's generic cost",
            ],
        ));
    }

    if text
        == "If there are three or more Lesson cards in your graveyard, you may cast this spell as though it had flash."
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "flashIfGraveyardThreshold",
                    "where": subtype("Lesson"),
                    "minimum": integer(3),
                },
            }),
            &[
                "Count Lesson cards in the controller's graveyard",
                "Permit flash timing at three Lessons",
            ],
        ));
    }

    if text
        == "This creature can't attack or block unless you control another creature with power 4 or greater."
    {
        let qualifying_creature = and(vec![
            card_type("Creature"),
            compare(
                ">=",
                json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                integer(4),
            ),
        ]);
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "cantAttackUnlessControls",
                        "object": self_ref(),
                        "player": controller(),
                        "where": qualifying_creature.clone(),
                        "minimum": integer(1),
                        "excludeSource": true,
                    },
                    {
                        "kind": "grantKeyword",
                        "objects": self_ref(),
                        "keyword": "cantBlock",
                        "condition": not(json!({
                            "kind": "controlsPermanent",
                            "where": qualifying_creature,
                            "excludeSource": true,
                        })),
                    },
                ],
            }),
            &[
                "Check for another controlled creature of power four",
                "Restrict attacking and blocking until one exists",
            ],
        ));
    }

    if text
        == "The Lion-Turtle can't attack or block unless there are three or more Lesson cards in your graveyard."
    {
        let restricted = compare(
            "<",
            json!({
                "kind": "countCards",
                "zone": graveyard(controller()),
                "where": subtype("Lesson"),
            }),
            integer(3),
        );
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    { "kind": "grantKeyword", "objects": self_ref(), "keyword": "cantAttack", "condition": restricted.clone() },
                    { "kind": "grantKeyword", "objects": self_ref(), "keyword": "cantBlock", "condition": restricted },
                ],
            }),
            &[
                "Count Lesson cards in the graveyard",
                "Restrict combat below three Lessons",
            ],
        ));
    }

    if text
        == "As long as there is a Lesson card in your graveyard, this creature can attack as though it didn't have defender."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "allowAttackWithDefender",
                    "objects": self_ref(),
                    "condition": lesson_threshold(),
                }],
            }),
            &[
                "Check for a Lesson card in the graveyard",
                "Allow the source to attack with defender",
            ],
        ));
    }

    if text
        == "Each creature you control with power 4 or greater can't be blocked by more than one creature."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![
                            card_type("Creature"),
                            compare(
                                ">=",
                                json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                                integer(4),
                            ),
                        ]),
                    },
                    "keyword": "maxOneBlocker",
                }],
            }),
            &[
                "Select controlled creatures of power four or greater",
                "Limit each to one blocker",
            ],
        ));
    }

    if text
        == "This creature gets +1/+0 and has trample as long as you control a land creature or a land entered the battlefield under your control this turn."
    {
        let condition = or(vec![
            json!({
                "kind": "controlsPermanent",
                "where": and(vec![card_type("Land"), card_type("Creature")]),
            }),
            compare(
                ">=",
                json!({
                    "kind": "countEventsThisTurn",
                    "event": "permanentEnteredBattlefield",
                    "player": controller(),
                    "where": card_type("Land"),
                }),
                integer(1),
            ),
        ]);
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": self_ref(),
                        "power": integer(1),
                        "toughness": integer(0),
                        "condition": condition.clone(),
                    },
                    {
                        "kind": "grantKeyword",
                        "objects": self_ref(),
                        "keyword": "trample",
                        "condition": condition,
                    },
                ],
            }),
            &[
                "Check for a land creature or a land entry this turn",
                "Grant +1/+0 and trample",
            ],
        ));
    }

    let attacked_this_turn = || {
        compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "declaredAttacker",
                "player": controller(),
            }),
            integer(1),
        )
    };

    if text.starts_with(
        "Raid — When this creature enters, if you attacked this turn, create a Clue token.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "condition": attacked_this_turn(),
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": { "kind": "namedToken", "name": "Clue" },
                }],
            }),
            &[
                "Check that the controller attacked this turn",
                "Create a Clue token",
            ],
        ));
    }

    if text
        == "Raid — At the beginning of your end step, if you attacked this turn, put a +1/+1 counter on another target creature or Vehicle you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": controller() },
                "condition": attacked_this_turn(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "raidCounterTarget",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": or(vec![card_type("Creature"), subtype("Vehicle")]),
                            "excludeSource": true,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": chosen_target("raidCounterTarget"),
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Check raid at the controller's end step",
                "Counter another controlled creature or Vehicle",
            ],
        ));
    }

    if text.starts_with("When this Vehicle dies, create a Clue token.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": { "kind": "namedToken", "name": "Clue" },
                }],
            }),
            &["Watch the source Vehicle die", "Create a Clue token"],
        ));
    }

    if text
        == "Enchanted creature gets +1/+1 for each creature card in your graveyard and is an Avatar in addition to its other types."
    {
        let attached = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        let amount = json!({
            "kind": "countCards",
            "zone": graveyard(controller()),
            "where": card_type("Creature"),
        });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": attached.clone(),
                        "power": amount.clone(),
                        "toughness": amount,
                    },
                    { "kind": "addSubtype", "objects": attached, "subtype": "Avatar" },
                ],
            }),
            &[
                "Count creature cards in the graveyard",
                "Pump the enchanted creature and add Avatar",
            ],
        ));
    }

    if text == "This creature gets +1/+0 for each color among Allies you control." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": self_ref(),
                    "power": {
                        "kind": "eachColorAmongPermanents",
                        "player": controller(),
                        "where": subtype("Ally"),
                    },
                    "toughness": integer(0),
                }],
            }),
            &[
                "Count colors among controlled Allies",
                "Give the source +1/+0 for each color",
            ],
        ));
    }

    if text == "When Jet dies, put a +1/+1 counter on each of up to two target creatures." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "jetCounterTargets",
                        json!({ "kind": "permanents", "where": card_type("Creature") }),
                        0,
                        2,
                    )],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": { "kind": "chosenTargets", "id": "jetCounterTargets" },
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Choose up to two creatures when Jet dies",
                "Put a +1/+1 counter on each",
            ],
        ));
    }

    if text
        == "Whenever you cast a Lesson, Saga, or Shrine spell, put a +1/+1 counter on another target creature you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": or(vec![subtype("Lesson"), subtype("Saga"), subtype("Shrine")]),
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "guruCounterTarget",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                            "excludeSource": true,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": chosen_target("guruCounterTarget"),
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Watch Lesson, Saga, and Shrine spells",
                "Counter another controlled creature",
            ],
        ));
    }

    if text.starts_with(
        "As long as Zhao has a conqueror counter on him, nonbasic lands are Mountains.",
    ) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "nonbasicLandsBecomeMountains",
                    "condition": compare(
                        ">=",
                        json!({ "kind": "countCounters", "object": self_ref(), "counter": "conqueror" }),
                        integer(1),
                    ),
                }],
            }),
            &[
                "Count conqueror counters on Zhao",
                "Turn nonbasic lands into Mountains",
            ],
        ));
    }

    if text == "Toph's power is equal to the number of +1/+1 counters on lands you control." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "setPower",
                    "objects": self_ref(),
                    "power": {
                        "kind": "countCountersOnPermanents",
                        "player": controller(),
                        "where": card_type("Land"),
                        "counter": "+1/+1",
                    },
                }],
            }),
            &[
                "Count +1/+1 counters on controlled lands",
                "Set Toph's power to that total",
            ],
        ));
    }

    if text.starts_with("{2}{W}{U}{B}{R}{G}: Earthbend 5.") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payMana", "manaCost": "{2}{W}{U}{B}{R}{G}" }],
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "earthbendLand",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Land"),
                        }),
                        1,
                        1,
                    )],
                },
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [earthbend_effect("earthbendLand", integer(5))],
            }),
            &[
                "Pay the five-color activation cost",
                "Target a controlled land",
                "Earthbend five",
            ],
        ));
    }

    if text
        == "When Jet enters, he deals damage equal to the number of creatures you control to target creature an opponent controls."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "jetDamageTarget",
                        json!({
                            "kind": "permanents",
                            "controller": { "kind": "opponentsOf", "player": controller() },
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "dealDamage",
                    "recipient": chosen_target("jetDamageTarget"),
                    "amount": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                    "source": self_ref(),
                }],
            }),
            &[
                "Count controlled creatures",
                "Deal that much damage to the opposing creature",
            ],
        ));
    }

    if text == "When this creature enters, target opponent discards a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "discardOpponent",
                        json!({
                            "kind": "players",
                            "where": { "kind": "isOpponentOf", "player": controller() },
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "discardCards",
                    "player": chosen_target("discardOpponent"),
                    "count": integer(1),
                }],
            }),
            &["Target an opponent", "Make that player discard a card"],
        ));
    }

    if text == "June can't be blocked as long as you've drawn two or more cards this turn." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "cantBeBlocked",
                    "object": self_ref(),
                    "condition": compare(
                        ">=",
                        json!({
                            "kind": "countEventsThisTurn",
                            "event": "cardDrawn",
                            "player": controller(),
                        }),
                        integer(2),
                    ),
                }],
            }),
            &[
                "Count cards drawn by the controller this turn",
                "Make June unblockable after two",
            ],
        ));
    }

    if text.starts_with(
        "At the beginning of combat on your turn, target creature you control with a +1/+1 counter on it gains menace until end of turn.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "beginCombat", "player": controller() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "menaceTarget",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
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
                    "object": chosen_target("menaceTarget"),
                    "keyword": "menace",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &["Target a controlled creature with a +1/+1 counter", "Grant menace for the turn"],
        ));
    }

    if text.starts_with("III — Earthbend 3.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "sagaChapterReached", "object": self_ref(), "chapters": [integer(3)] },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "earthbendLand",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Land"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [earthbend_effect("earthbendLand", integer(3))],
            }),
            &[
                "Resolve Saga chapter three",
                "Target a controlled land",
                "Earthbend three",
            ],
        ));
    }

    if text.starts_with("When this creature dies, earthbend 2.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "earthbendLand",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Land"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [earthbend_effect("earthbendLand", integer(2))],
            }),
            &[
                "Watch the source die",
                "Target a controlled land",
                "Earthbend two",
            ],
        ));
    }

    if text.starts_with(
        "This creature has firebending 2 as long as there's a Lesson card in your graveyard.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "condition": lesson_threshold(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "resolveFirebending",
                }],
                "firebendingQuantity": integer(2),
            }),
            &[
                "Check for a Lesson card when the source attacks",
                "Resolve firebending two",
            ],
        ));
    }

    if text == "Whenever Sokka and at least one other creature attack, draw a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "minimum": integer(2),
                    "sourceMustAttack": true,
                },
                "effects": [{ "kind": "drawCards", "player": controller(), "count": integer(1) }],
            }),
            &[
                "Require Sokka and another controlled attacker",
                "Draw a card",
            ],
        ));
    }

    if text
        == "Whenever you sacrifice a permanent during your turn, create a 1/1 white Ally creature token. This ability triggers only once each turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentSacrificed", "player": controller() },
                "condition": { "kind": "duringControllerTurn", "player": controller() },
                "triggerLimit": { "kind": "onceEachTurn", "id": "tlaTollsOfWar" },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": {
                        "name": "Ally Token",
                        "types": ["Creature"],
                        "subtypes": ["Ally"],
                        "colors": ["White"],
                        "power": integer(1),
                        "toughness": integer(1),
                    },
                }],
            }),
            &[
                "Watch a permanent sacrificed during the controller's turn",
                "Trigger once per turn",
                "Create a white Ally",
            ],
        ));
    }

    if text == "{3}: Exile the top card of your library. You may play that card this turn." {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payMana", "manaCost": "{3}" }],
                "effects": [
                    {
                        "kind": "exileTopCards",
                        "zone": library(controller()),
                        "count": integer(1),
                        "faceDown": false,
                        "bind": "zukoExiled",
                    },
                    {
                        "kind": "grantPermission",
                        "player": controller(),
                        "action": {
                            "kind": "play",
                            "card": { "kind": "boundObject", "binding": "zukoExiled" },
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                        },
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Pay three mana",
                "Exile the top card",
                "Permit playing it this turn",
            ],
        ));
    }

    if text.starts_with(
        "When Crescent Island Temple enters, for each Shrine you control, create a 1/1 red Monk creature token with prowess.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": subtype("Shrine"),
                    },
                    "token": {
                        "name": "Monk Token",
                        "types": ["Creature"],
                        "subtypes": ["Monk"],
                        "colors": ["Red"],
                        "power": integer(1),
                        "toughness": integer(1),
                        "abilities": [{
                            "kind": "keywordAbility",
                            "source": self_ref(),
                            "ability": { "kind": "prowess" },
                        }],
                    },
                }],
            }),
            &["Count controlled Shrines", "Create that many red Monk tokens with prowess"],
        ));
    }

    if text == "Ozai's Cruelty deals 2 damage to target player. That player discards two cards." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPlayer",
                        json!({ "kind": "players" }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "dealDamage",
                        "recipient": chosen_target("targetPlayer"),
                        "amount": integer(2),
                        "source": self_ref(),
                    },
                    {
                        "kind": "discardCards",
                        "player": chosen_target("targetPlayer"),
                        "count": integer(2),
                    },
                ],
            }),
            &[
                "Target a player",
                "Deal two damage",
                "Make that player discard two cards",
            ],
        ));
    }

    if text
        .starts_with("Create a 1/1 white Ally creature token for each Plains you control. Scry 2.")
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": {
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": subtype("Plains"),
                        },
                        "token": {
                            "name": "Ally Token",
                            "types": ["Creature"],
                            "subtypes": ["Ally"],
                            "colors": ["White"],
                            "power": integer(1),
                            "toughness": integer(1),
                        },
                    },
                    { "kind": "scry", "player": controller(), "count": integer(2) },
                ],
            }),
            &[
                "Count controlled Plains",
                "Create that many white Allies",
                "Scry two",
            ],
        ));
    }

    if text
        == "When Southern Air Temple enters, put X +1/+1 counters on each creature you control, where X is the number of Shrines you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [{
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                    "counter": "+1/+1",
                    "count": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": subtype("Shrine"),
                    },
                }],
            }),
            &[
                "Count controlled Shrines",
                "Put that many counters on each controlled creature",
            ],
        ));
    }

    if text
        == "Whenever another permanent you control leaves the battlefield during your turn, create a 1/1 white Ally creature token. This ability triggers only once each turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentLeftBattlefield",
                    "player": controller(),
                    "excludeSource": true,
                },
                "condition": { "kind": "duringControllerTurn", "player": controller() },
                "triggerLimit": { "kind": "onceEachTurn", "id": "tlaSukiLeaves" },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": {
                        "name": "Ally Token",
                        "types": ["Creature"],
                        "subtypes": ["Ally"],
                        "colors": ["White"],
                        "power": integer(1),
                        "toughness": integer(1),
                    },
                }],
            }),
            &[
                "Watch another controlled permanent leave during the controller's turn",
                "Create one Ally once per turn",
            ],
        ));
    }

    if text
        == "Whenever a creature you control attacks alone, it gets +X/+X until end of turn, where X is the number of creatures you control."
    {
        let amount = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": card_type("Creature"),
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "minimum": integer(1),
                    "maximum": integer(1),
                },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": { "kind": "triggeringPermanent" },
                    "power": amount.clone(),
                    "toughness": amount,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Watch a controlled creature attack alone",
                "Pump it by the controlled creature count",
            ],
        ));
    }

    if text.starts_with(
        "Whenever Master Pakku becomes tapped, target player mills X cards, where X is the number of Lesson cards in your graveyard.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentTapped", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision("pakkuPlayer", json!({ "kind": "players" }), 1, 1)],
                },
                "effects": [{
                    "kind": "mill",
                    "player": chosen_target("pakkuPlayer"),
                    "count": {
                        "kind": "countCards",
                        "zone": graveyard(controller()),
                        "where": subtype("Lesson"),
                    },
                }],
            }),
            &["Watch Pakku become tapped", "Target a player", "Mill for the Lesson count"],
        ));
    }

    if text.starts_with(
        "Raid — This creature enters with a +1/+1 counter on it if you attacked this turn.",
    ) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "condition": attacked_this_turn(),
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &["Check raid before entry", "Enter with a +1/+1 counter"],
        ));
    }

    if text
        == "Whenever another creature you control or a land you control is put into a graveyard from the battlefield, put a +1/+1 counter on target creature you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentLeftBattlefield",
                    "player": controller(),
                    "where": or(vec![card_type("Creature"), card_type("Land")]),
                    "destination": "graveyard",
                    "excludeSource": true,
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "longFengTarget",
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
                    "permanent": chosen_target("longFengTarget"),
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Watch another controlled creature or land go to the graveyard",
                "Counter a controlled creature",
            ],
        ));
    }

    if text.starts_with(
        "When Ran and Shaw enter, if you cast them and there are three or more Dragon and/or Lesson cards in your graveyard, create a token that's a copy of Ran and Shaw, except it's not legendary.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "condition": and(vec![
                    json!({ "kind": "wasCast", "object": self_ref() }),
                    compare(
                        ">=",
                        json!({
                            "kind": "countCards",
                            "zone": graveyard(controller()),
                            "where": or(vec![subtype("Dragon"), subtype("Lesson")]),
                        }),
                        integer(3),
                    ),
                ]),
                "effects": [{
                    "kind": "createTokenCopyOfPermanent",
                    "object": self_ref(),
                    "removeLegendary": true,
                    "grantKeywords": [],
                    "exileAtNextEndStep": false,
                }],
            }),
            &["Require Ran and Shaw to have been cast", "Count Dragon and Lesson graveyard cards", "Create a nonlegendary copy"],
        ));
    }

    if text
        == "At the beginning of your end step, if you sacrificed a permanent this turn, create a token that's a copy of this Vehicle."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": controller() },
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countEventsThisTurn",
                        "event": "permanentSacrificed",
                        "player": controller(),
                    }),
                    integer(1),
                ),
                "effects": [{
                    "kind": "createTokenCopyOfPermanent",
                    "object": self_ref(),
                    "grantKeywords": [],
                    "exileAtNextEndStep": false,
                }],
            }),
            &[
                "Check for a sacrificed permanent this turn",
                "Create a token copy of the source Vehicle",
            ],
        ));
    }

    if text
        == "As long as you control eight or more permanents named Phoenix Fleet Airship, this Vehicle is an artifact creature."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "addCardType",
                    "objects": self_ref(),
                    "cardType": "Creature",
                    "condition": compare(
                        ">=",
                        json!({
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": { "kind": "nameEquals", "value": "Phoenix Fleet Airship" },
                        }),
                        integer(8),
                    ),
                }],
            }),
            &[
                "Count controlled Phoenix Fleet Airships",
                "Make the source a creature at eight",
            ],
        ));
    }

    if text.starts_with(
        "When this enchantment enters and at the beginning of your upkeep, you lose 1 life and create a Clue token.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        self_enters(),
                        { "kind": "stepBegan", "step": "upkeep", "player": controller() },
                    ],
                },
                "effects": [
                    { "kind": "loseLife", "player": controller(), "amount": integer(1) },
                    {
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": integer(1),
                        "token": { "kind": "namedToken", "name": "Clue" },
                    },
                ],
            }),
            &["Trigger on entry and each upkeep", "Lose one life and create a Clue"],
        ));
    }

    if text
        == "Whenever you attack, put X +1/+1 counters on target attacking creature, where X is the number of permanents you've sacrificed this turn. If X is three or more, that creature gains lifelink until end of turn."
    {
        let amount = json!({
            "kind": "countEventsThisTurn",
            "event": "permanentSacrificed",
            "player": controller(),
        });
        let target = chosen_target("pursuitAttacker");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "controlledCreaturesAttacked", "player": controller(), "minimum": integer(1) },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "pursuitAttacker",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": and(vec![card_type("Creature"), json!({ "kind": "isAttacking" })]),
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
                        "count": amount.clone(),
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": compare(">=", amount, integer(3)),
                        "then": [{
                            "kind": "grantKeyword",
                            "object": target,
                            "keyword": "lifelink",
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Count permanents sacrificed this turn",
                "Counter a target attacker",
                "Grant lifelink at three or more",
            ],
        ));
    }

    if text.starts_with(
        "Whenever this creature enters or attacks, exile up to one target card from a graveyard. If a creature card is exiled this way, create a Clue token.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        self_enters(),
                        { "kind": "declaredAttacker", "object": self_ref() },
                    ],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "ravenCard",
                        json!({
                            "kind": "cards",
                            "zone": { "kind": "anyGraveyard" },
                            "where": Value::Null,
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "exileTargetCards",
                        "cards": { "kind": "chosenTargets", "id": "ravenCard" },
                    },
                    {
                        "kind": "resolveTriggeredInstruction",
                        "operation": "tlaRavenEagleClue",
                    },
                ],
            }),
            &["Watch the source enter or attack", "Exile up to one graveyard card", "Create a Clue if it was a creature"],
        ));
    }

    if text
        == "When Hei Bai leaves the battlefield, put its counters on target creature you control."
    {
        let candidates = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": card_type("Creature"),
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentLeftBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision("heiBaiRecipient", candidates.clone(), 1, 1)],
                },
                "effects": [{
                    "kind": "putSameCountersAs",
                    "permanent": chosen_target("heiBaiRecipient"),
                    "source": self_ref(),
                    "candidates": candidates,
                }],
            }),
            &[
                "Watch Hei Bai leave",
                "Target a controlled creature",
                "Put Hei Bai's counters on it",
            ],
        ));
    }

    if text
        == "Target creature you control gets +2/+2 until end of turn. If that creature is an Ally, it also gains flying until end of turn."
    {
        let target = chosen_target("yipYipCreature");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "yipYipCreature",
                        json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
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
                        "kind": "conditionalEffect",
                        "condition": { "kind": "objectMatchesFilter", "object": target.clone(), "where": subtype("Ally") },
                        "then": [{
                            "kind": "grantKeyword",
                            "object": target,
                            "keyword": "flying",
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Target a controlled creature",
                "Give it +2/+2",
                "Grant flying if it is an Ally",
            ],
        ));
    }

    if text
        == "Razor Rings deals 4 damage to target attacking or blocking creature. You gain life equal to the excess damage dealt this way."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "combatCreature",
                        json!({
                            "kind": "permanents",
                            "where": and(vec![
                                card_type("Creature"),
                                or(vec![json!({ "kind": "isAttacking" }), json!({ "kind": "isBlocking" })]),
                            ]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "dealDamage",
                        "source": self_ref(),
                        "recipient": chosen_target("combatCreature"),
                        "amount": integer(4),
                        "bindExcessAs": "excessDamage",
                    },
                    {
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": { "kind": "boundValue", "id": "excessDamage" },
                    },
                ],
            }),
            &[
                "Target an attacking or blocking creature",
                "Deal four damage",
                "Gain life equal to excess damage",
            ],
        ));
    }

    if text
        == "Whenever you attack, create a 2/1 colorless Construct artifact creature token with flying named Ballistic Boulder that's tapped and attacking. Sacrifice that token at the beginning of the next end step."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "controlledCreaturesAttacked", "player": controller(), "minimum": integer(1) },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "tapped": true,
                    "attacking": true,
                    "sacrificeAtNextEndStep": true,
                    "token": {
                        "name": "Ballistic Boulder",
                        "types": ["Artifact", "Creature"],
                        "subtypes": ["Construct"],
                        "colors": [],
                        "power": integer(2),
                        "toughness": integer(1),
                        "abilities": [{
                            "kind": "keywordAbility",
                            "source": self_ref(),
                            "ability": { "kind": "flying" },
                        }],
                    },
                }],
            }),
            &[
                "Watch the controller attack",
                "Create Ballistic Boulder tapped and attacking",
                "Sacrifice it at the next end step",
            ],
        ));
    }

    if text.starts_with(
        "Waterbend {5}: Look at the top four cards of your library. You may reveal a creature card with power 3 or less from among them and put it into your hand.",
    ) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payWaterbend", "amount": integer(5) }],
                "effects": [
                    {
                        "kind": "lookAtTopCards",
                        "zone": library(controller()),
                        "count": integer(4),
                        "bind": "rallierCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "rallierCreature",
                        "player": controller(),
                        "from": bound_objects("rallierCards"),
                        "where": and(vec![
                            card_type("Creature"),
                            compare(
                                "<=",
                                json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                                integer(3),
                            ),
                        ]),
                        "minimum": 0,
                        "maximum": 1,
                    },
                    { "kind": "revealCards", "cards": decision_result("rallierCreature") },
                    { "kind": "moveCards", "cards": decision_result("rallierCreature"), "to": hand(controller()) },
                    {
                        "kind": "moveCards",
                        "cards": {
                            "kind": "setDifference",
                            "left": bound_objects("rallierCards"),
                            "right": decision_result("rallierCreature"),
                        },
                        "to": { "kind": "library", "player": controller(), "position": "bottom" },
                        "order": { "kind": "random" },
                    },
                ],
            }),
            &["Pay waterbend five", "Look at four cards", "Take an optional small creature", "Randomize the rest on the bottom"],
        ));
    }

    if text
        == "Look at the top three cards of your library. Put one of those cards into your hand and the rest on the bottom of your library in any order. Put each of those cards into your hand instead if there are three or more Lesson cards in your graveyard."
    {
        let all_to_hand = compare(
            ">=",
            json!({
                "kind": "countCards",
                "zone": graveyard(controller()),
                "where": subtype("Lesson"),
            }),
            integer(3),
        );
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "lookAtTopCards",
                        "zone": library(controller()),
                        "count": integer(3),
                        "bind": "wisdomCards",
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": all_to_hand,
                        "then": [{
                            "kind": "moveCards",
                            "cards": bound_objects("wisdomCards"),
                            "to": hand(controller()),
                        }],
                        "else": [
                            {
                                "kind": "chooseCards",
                                "id": "wisdomCard",
                                "player": controller(),
                                "from": bound_objects("wisdomCards"),
                                "where": Value::Null,
                                "minimum": 1,
                                "maximum": 1,
                            },
                            {
                                "kind": "moveCards",
                                "cards": decision_result("wisdomCard"),
                                "to": hand(controller()),
                            },
                            {
                                "kind": "chooseOrder",
                                "id": "wisdomBottomOrder",
                                "player": controller(),
                                "objects": {
                                    "kind": "setDifference",
                                    "left": bound_objects("wisdomCards"),
                                    "right": decision_result("wisdomCard"),
                                },
                            },
                            {
                                "kind": "moveCards",
                                "cards": decision_result("wisdomBottomOrder"),
                                "to": { "kind": "library", "player": controller(), "position": "bottom" },
                                "order": { "kind": "decisionOrder", "decisionId": "wisdomBottomOrder" },
                            },
                        ],
                    },
                ],
            }),
            &[
                "Look at three cards",
                "Take all at three Lessons",
                "Otherwise take one and bottom the rest",
            ],
        ));
    }

    if text.starts_with("Choose one")
        && text.contains("Destroy target creature with power 4 or greater.")
        && text.contains("Earthbend 3.")
    {
        let destroy_mode = selection("chosenModes", "destroyCreature");
        let earthbend_mode = selection("chosenModes", "earthbend");
        let mut destroy_target = target_decision(
            "largeCreature",
            json!({
                "kind": "permanents",
                "where": and(vec![
                    card_type("Creature"),
                    compare(">=", json!({ "kind": "powerOf", "object": { "kind": "candidate" } }), integer(4)),
                ]),
            }),
            1,
            1,
        );
        destroy_target["condition"] = destroy_mode.clone();
        let mut land_target = target_decision(
            "earthbendLand",
            json!({ "kind": "permanents", "controller": controller(), "where": card_type("Land") }),
            1,
            1,
        );
        land_target["condition"] = earthbend_mode.clone();
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["destroyCreature", "earthbend"] },
                        destroy_target,
                        land_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": destroy_mode,
                        "then": [{ "kind": "destroyPermanent", "permanent": chosen_target("largeCreature") }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": earthbend_mode,
                        "then": [earthbend_effect("earthbendLand", integer(3))],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose destruction or earthbend",
                "Declare only the selected mode's target",
                "Resolve the selected mode",
            ],
        ));
    }

    if text.starts_with("Choose one")
        && text.contains(
            "Bumi Bash deals damage equal to the number of lands you control to target creature.",
        )
        && text.contains("Destroy target land creature or nonbasic land.")
    {
        let damage_mode = selection("chosenModes", "damageCreature");
        let destroy_mode = selection("chosenModes", "destroyLand");
        let mut creature_target = target_decision(
            "bumiCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        );
        creature_target["condition"] = damage_mode.clone();
        let mut land_target = target_decision(
            "bumiLand",
            json!({
                "kind": "permanents",
                "where": or(vec![
                    and(vec![card_type("Land"), card_type("Creature")]),
                    and(vec![card_type("Land"), not(json!({ "kind": "isBasic" }))]),
                ]),
            }),
            1,
            1,
        );
        land_target["condition"] = destroy_mode.clone();
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["damageCreature", "destroyLand"] },
                        creature_target,
                        land_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": damage_mode,
                        "then": [{
                            "kind": "dealDamage",
                            "source": self_ref(),
                            "recipient": chosen_target("bumiCreature"),
                            "amount": { "kind": "countPermanents", "player": controller(), "where": card_type("Land") },
                        }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": destroy_mode,
                        "then": [{ "kind": "destroyPermanent", "permanent": chosen_target("bumiLand") }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose the damage or land-destruction mode",
                "Declare its target",
                "Resolve the selected mode",
            ],
        ));
    }

    if text.starts_with("When Momo leaves the battlefield, choose one")
        && text.contains("Create a Food token.")
        && text.contains("Put a +1/+1 counter on target creature you control.")
        && text.contains("Scry 2.")
    {
        let food_mode = selection("chosenModes", "food");
        let counter_mode = selection("chosenModes", "counter");
        let scry_mode = selection("chosenModes", "scry");
        let mut counter_target = target_decision(
            "momoCounterTarget",
            json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
            1,
            1,
        );
        counter_target["condition"] = counter_mode.clone();
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentLeftBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["food", "counter", "scry"] },
                        counter_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": food_mode,
                        "then": [{
                            "kind": "createTokens",
                            "controller": controller(),
                            "quantity": integer(1),
                            "token": { "kind": "namedToken", "name": "Food" },
                        }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": counter_mode,
                        "then": [{
                            "kind": "putCounters",
                            "permanent": chosen_target("momoCounterTarget"),
                            "counter": "+1/+1",
                            "count": integer(1),
                        }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": scry_mode,
                        "then": [{ "kind": "scry", "player": controller(), "count": integer(2) }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose Momo's leave mode",
                "Declare the counter target only for that mode",
                "Resolve Food, counter, or scry",
            ],
        ));
    }

    if text.starts_with("Whenever Appa enters or attacks, choose one")
        && text.contains("Target creature you control gains flying until end of turn.")
        && text.contains("Airbend another target nonland permanent you control.")
    {
        let flying_mode = selection("chosenModes", "flying");
        let airbend_mode = selection("chosenModes", "airbend");
        let mut flying_target = target_decision(
            "appaFlyingTarget",
            json!({ "kind": "permanents", "controller": controller(), "where": card_type("Creature") }),
            1,
            1,
        );
        flying_target["condition"] = flying_mode.clone();
        let airbend_candidates = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": not(card_type("Land")),
            "excludeSource": true,
        });
        let mut airbend_target =
            target_decision("appaAirbendTarget", airbend_candidates.clone(), 1, 1);
        airbend_target["condition"] = airbend_mode.clone();
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [self_enters(), { "kind": "declaredAttacker", "object": self_ref() }],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        { "id": "chosenModes", "kind": "chooseModes", "minimum": 1, "maximum": 1, "options": ["flying", "airbend"] },
                        flying_target,
                        airbend_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": flying_mode,
                        "then": [{
                            "kind": "grantKeyword",
                            "object": chosen_target("appaFlyingTarget"),
                            "keyword": "flying",
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": airbend_mode,
                        "then": [{
                            "kind": "airbend",
                            "object": chosen_target("appaAirbendTarget"),
                            "candidates": airbend_candidates,
                            "alternativeManaCost": "{2}",
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Watch Appa enter or attack",
                "Choose flying or airbend",
                "Declare and resolve only the selected mode",
            ],
        ));
    }

    if text.starts_with(
        "When this creature enters, mill three cards. You may put a land card from among them into your hand. If you don't, put a +1/+1 counter on this creature.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": self_enters(),
                "effects": [
                    { "kind": "mill", "player": controller(), "count": integer(3), "bind": "ostrichMilled" },
                    {
                        "kind": "chooseCards",
                        "id": "ostrichLand",
                        "player": controller(),
                        "from": bound_objects("ostrichMilled"),
                        "where": card_type("Land"),
                        "minimum": 0,
                        "maximum": 1,
                    },
                    { "kind": "moveCards", "cards": decision_result("ostrichLand"), "to": hand(controller()) },
                    {
                        "kind": "conditionalEffect",
                        "condition": not(json!({
                            "kind": "selectionNotEmpty",
                            "selection": decision_result("ostrichLand"),
                        })),
                        "then": [{
                            "kind": "putCounters",
                            "permanent": self_ref(),
                            "counter": "+1/+1",
                            "count": integer(1),
                        }],
                        "else": [],
                    },
                ],
            }),
            &["Mill three cards", "Optionally take a milled land", "Counter the source if no land was taken"],
        ));
    }

    if text.starts_with(
        "{T}: Mill a card. You may put a land card milled this way into your hand. You gain 2 life if a Lesson card is milled this way.",
    ) {
        let milled_lessons = json!({
            "kind": "filterObjects",
            "objects": bound_objects("dummyMilled"),
            "where": subtype("Lesson"),
        });
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "tap", "object": self_ref() }],
                "effects": [
                    { "kind": "mill", "player": controller(), "count": integer(1), "bind": "dummyMilled" },
                    {
                        "kind": "chooseCards",
                        "id": "dummyLand",
                        "player": controller(),
                        "from": bound_objects("dummyMilled"),
                        "where": card_type("Land"),
                        "minimum": 0,
                        "maximum": 1,
                    },
                    { "kind": "moveCards", "cards": decision_result("dummyLand"), "to": hand(controller()) },
                    {
                        "kind": "conditionalEffect",
                        "condition": compare(
                            ">=",
                            json!({ "kind": "countObjects", "objects": milled_lessons }),
                            integer(1),
                        ),
                        "then": [{ "kind": "gainLife", "player": controller(), "amount": integer(2) }],
                        "else": [],
                    },
                ],
            }),
            &["Tap the source and mill one", "Optionally take it if it is a land", "Gain two life if it is a Lesson"],
        ));
    }

    None
}
