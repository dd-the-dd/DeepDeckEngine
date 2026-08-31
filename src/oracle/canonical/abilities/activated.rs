use super::super::*;

pub(in crate::oracle::canonical) fn parse_hand_put_haste_delayed_sacrifice(
    instruction: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let pattern = Regex::new(
        r"(?i)^You may put (?:an? )?(.+?) card from your hand onto the battlefield\. That (?:creature|permanent) gains haste\. Sacrifice (?:the creature|that creature|it) at the beginning of the next end step\.$",
    )
    .expect("hand put, haste, and delayed sacrifice regex compiles");
    let captures = pattern.captures(instruction)?;
    let decisions = vec![target_decision(
        "handCard",
        json!({
            "kind": "cards",
            "zone": hand(controller()),
            "where": parse_permanent_criteria(&captures[1], "")?,
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

    if let Some((cost_text, raw_instruction)) = normalized.split_once(':') {
        let instruction = raw_instruction
            .trim()
            .split_once(" (")
            .map(|(instruction, _)| instruction)
            .unwrap_or(raw_instruction.trim());
        let search_then_optional_behold_untap_re = Regex::new(
            r"(?i)^Search your library for a basic land card, put it onto the battlefield tapped, then shuffle\. You may behold (?:a|an) (.+?)\. If you do, untap that land\.$",
        )
        .expect("basic land search then optional behold regex compiles");
        if let Some(captures) = search_then_optional_behold_untap_re.captures(instruction) {
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
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
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
    let activation_instruction = raw_instruction.trim().to_string();
    let raw_instruction = activation_instruction
        .split_once(" Activate only ")
        .map(|(instruction, _)| instruction.trim())
        .unwrap_or(activation_instruction.as_str());
    let fixed_activation_reduction_re =
        Regex::new(r"(?i)^(.*?) This ability costs \{(\d+)\} less to activate if (.+?)\.$")
            .expect("conditional fixed activation cost reduction regex compiles");
    let activation_reduction_re = Regex::new(
        r"(?i)^(.*?) This ability costs \{1\} less to activate for each (.+?) you control\.$",
    )
    .expect("activation cost reduction regex compiles");
    let (raw_instruction, mana_cost_reduction) =
        if let Some(captures) = fixed_activation_reduction_re.captures(raw_instruction.trim()) {
            (
                captures.get(1)?.as_str().to_string(),
                Some(json!({
                    "kind": "conditionalValue",
                    "condition": parse_condition_text(captures.get(3)?.as_str()).or_else(|| {
                        parse_controlled_permanent_condition(captures.get(3)?.as_str(), "")
                    })?,
                    "ifTrue": integer(captures[2].parse::<i64>().ok()?),
                    "ifFalse": integer(0),
                })),
            )
        } else if let Some(captures) = activation_reduction_re.captures(raw_instruction.trim()) {
            (
                captures.get(1)?.as_str().to_string(),
                Some(json!({
                    "kind": "countPermanents",
                    "player": controller(),
                    "where": parse_permanent_criteria(&captures[2], "")?,
                })),
            )
        } else {
            (raw_instruction.to_string(), None)
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

    let opponent_choose_types_sacrifice_rest_re = Regex::new(
        r"(?i)^Each opponent chooses (.+?) from among the nonland permanents they control, then sacrifices the rest\.$",
    )
    .expect("opponent choose permanent types then sacrifice rest regex compiles");
    if let Some(captures) = opponent_choose_types_sacrifice_rest_re.captures(&instruction) {
        let normalized_choices = captures[1].replace(", and ", ", ").replace(" and ", ", ");
        let choices = normalized_choices
            .split(", ")
            .map(|choice| {
                let criteria = choice
                    .trim()
                    .strip_prefix("an ")
                    .or_else(|| choice.trim().strip_prefix("a "))
                    .unwrap_or(choice.trim());
                parse_permanent_criteria(criteria, "")
            })
            .collect::<Option<Vec<_>>>()?;
        if choices.is_empty() {
            return None;
        }
        effects.push(json!({
            "kind": "eachOpponentChoosesPermanentsByCriteriaThenSacrificesRest",
            "player": controller(),
            "choices": choices,
            "among": not(card_type("Land")),
        }));
    }

    let token_reflexive_count_damage_re = Regex::new(
        r"(?i)^(Create .+? tokens?\.) When you do, if you control (?:a|an) (.+?) permanent other than (.+?), (?:it|he|she|they) deals damage equal to the number of (.+?) you control to any target\.$",
    )
    .expect("token creation with conditional reflexive damage regex compiles");
    if effects.is_empty()
        && let Some(captures) = token_reflexive_count_damage_re.captures(&instruction)
    {
        effects.extend([
            create_token_effect(captures.get(1)?.as_str())?,
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
                        "where": parse_permanent_criteria(captures.get(2)?.as_str(), "")?,
                        "excludeName": captures.get(3)?.as_str(),
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
                            "where": parse_permanent_criteria(captures.get(4)?.as_str(), "")?,
                        },
                        "recipient": chosen_target("targetDamageable"),
                    }],
                },
            }),
        ]);
    }

    let protected_attack_penalty_re = Regex::new(
        r"(?i)^Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets ([+-]\d+)/([+-]\d+) until end of turn\.$",
    )
    .expect("protected player or planeswalker attack trigger regex compiles");
    if effects.is_empty()
        && let Some(captures) = protected_attack_penalty_re.captures(&instruction)
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
                "power": integer(captures[1].parse::<i64>().ok()?),
                "toughness": integer(captures[2].parse::<i64>().ok()?),
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
        let sacrificed_power_draw_re = Regex::new(
            r"(?i)^Draw cards equal to the sacrificed (.+?)'s power, then discard a card\.$",
        )
        .expect("sacrificed-power draw then discard regex compiles");
        if let Some(captures) = sacrificed_power_draw_re.captures(&instruction) {
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
            if sacrificed_where != parse_permanent_criteria(captures.get(1)?.as_str(), "")? {
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
            let mill_sacrificed_power_re = Regex::new(
                r"(?i)^Target player mills cards equal to the sacrificed (.+?)'s power\.$",
            )
            .expect("sacrificed-power mill activation regex compiles");
            if mill_sacrificed_power_re.is_match(&instruction) {
                let sacrifice = costs
                    .iter_mut()
                    .find(|cost| cost["kind"].as_str() == Some("sacrificePermanent"))?;
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
                let single_graveyard_cast_re = Regex::new(
                    r"(?i)^Choose target (.+) card in your graveyard\. If you haven't cast a spell this turn, you may cast that card\. If you do, you can't cast additional spells this turn\.$",
                )
                .expect("single graveyard cast permission regex compiles");
                if let Some(captures) = single_graveyard_cast_re.captures(&instruction) {
                    let criteria = captures.get(1)?.as_str();
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
                } else if let Some(captures) = Regex::new(
        r"(?i)^Choose target (.+?) a player controls and target (.+?) card in that player's graveyard\. If both targets are still legal as this ability resolves, that player simultaneously sacrifices the (.+?) and returns the (.+?) card to the battlefield\.$",
    )
    .expect("linked permanent and graveyard-card exchange regex compiles")
    .captures(&instruction)
    {
        let battlefield_criteria = parse_permanent_criteria(&captures[1], "")?;
        let graveyard_criteria = parse_permanent_criteria(&captures[2], "")?;
        if battlefield_criteria != parse_permanent_criteria(&captures[3], "")?
            || graveyard_criteria != parse_permanent_criteria(&captures[4], "")?
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
    } else if let Some(captures) = Regex::new(&format!(
        r"^Draw ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("activated draw count regex compiles")
    .captures(&instruction)
    {
        effects.push(json!({
            "kind": "drawCards",
            "player": controller(),
            "count": integer(parse_number_word(&captures[1])?),
        }));
    } else if instruction.starts_with("Return ")
        && instruction.ends_with("to its owner's hand.")
        && !instruction.to_ascii_lowercase().contains("target")
    {
        effects.push(json!({
            "kind": "returnToOwnersHand",
            "object": self_ref(),
        }));
    } else if let Some(captures) = Regex::new(&format!(
        r"^Put ({}) ([^ ]+) counter on this (?:artifact|creature|permanent)\.$",
        count_word_pattern(),
    ))
    .expect("activated self counter regex compiles")
    .captures(&instruction)
    {
        effects.push(json!({
            "kind": "putCounters",
            "permanent": self_ref(),
            "counter": &captures[2],
            "count": integer(parse_number_word(&captures[1])?),
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
    } else if let Some(captures) = Regex::new(r"^You gain (\d+) life\.$")
        .expect("activated life gain regex compiles")
        .captures(&instruction)
    {
        effects.push(json!({
            "kind": "gainLife",
            "player": controller(),
            "amount": integer(captures[1].parse::<i64>().ok()?),
        }));
    } else if let Some(effect) = create_token_effect(&instruction) {
        effects.push(effect);
    } else if let Some(captures) = Regex::new(r"^Remove an? ([^ ]+) counter from .+\.$")
        .expect("remove counter regex compiles")
        .captures(&instruction)
    {
        effects.push(json!({
            "kind": "removeCounters",
            "permanent": self_ref(),
            "counter": &captures[1],
            "count": integer(1),
        }));
    } else if let Some(captures) = Regex::new(
        r"^(?:This creature|It) deals (\d+) damage to each opponent(?:\. Create (.+))?\.$",
    )
    .expect("activated opponent damage regex compiles")
    .captures(&instruction)
    {
        effects.push(json!({
            "kind": "dealDamageToEachOpponent",
            "amount": integer(captures[1].parse::<i64>().ok()?),
        }));
        if let Some(token_text) = captures.get(2) {
            effects.push(create_token_effect(&format!("Create {}.", token_text.as_str()))?);
        }
    } else if let Some(captures) = Regex::new(r"^It deals (\d+) damage to any target\.$")
        .expect("activated any-target damage regex compiles")
        .captures(&instruction)
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
            "amount": integer(captures[1].parse::<i64>().ok()?),
            "recipient": chosen_target("damageTarget"),
        }));
    } else if let Some(captures) = Regex::new(&format!(
        r"^Put ({}) ([A-Za-z0-9+/ -]+) counter on [^.]+\.$",
        count_word_pattern(),
    ))
    .expect("named-source counter activation regex compiles")
    .captures(&instruction)
    {
        effects.push(json!({
            "kind": "putCounters",
            "permanent": self_ref(),
            "counter": captures[2].trim(),
            "count": integer(parse_number_word(&captures[1])?),
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
    } else if let Some(captures) = Regex::new(r"(?i)^Target (.+?) can't be blocked this turn\.$")
        .expect("temporary unblockable target regex compiles")
        .captures(&instruction)
    {
        decisions.push(target_decision(
            "targetCreature",
            permanent_target_candidates(captures.get(1)?.as_str(), "")?,
            1,
            1,
        ));
        effects.push(json!({
            "kind": "grantKeyword",
            "object": chosen_target("targetCreature"),
            "keyword": "cantBeBlocked",
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
    } else if let Some(captures) = Regex::new(
        r"^Put an? ([A-Za-z0-9+/ -]+) counter on each (.+) you control\.$",
    )
    .expect("generic each-permanent counter activation regex compiles")
    .captures(&instruction)
    {
        effects.push(json!({
            "kind": "putCounters",
            "permanent": {
                "kind": "eachPermanent",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[2], "")?,
            },
            "counter": captures[1].trim(),
            "count": integer(1),
        }));
    } else if let Some(captures) = Regex::new(r"^Destroy target (.+)\.$")
        .expect("generic destroy target activation regex compiles")
        .captures(&instruction)
    {
        decisions.push(target_decision(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "where": parse_permanent_criteria(&captures[1], "")?,
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
        rule["activationCondition"] = json!({ "kind": "sorceryTiming" });
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

    let copy_permanent_retaining_ability_re = Regex::new(
        r"(?i)^This (?:permanent|artifact|creature|enchantment|land) becomes a copy of target (.+?), except it has this ability\.$",
    )
    .expect("copy permanent while retaining activation regex compiles");
    if let Some(captures) = copy_permanent_retaining_ability_re.captures(instruction) {
        decisions.push(target_decision(
            "copyTarget",
            permanent_target_candidates(&captures[1], "this permanent")?,
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
    let basic_land_search_re = Regex::new(&format!(
        r"^Search your library for ((?:a|an|one)|up to ({})) (basic land|basic [A-Za-z, ]+) cards?, (?:reveal (?:it|them), )?put (?:it|them) (onto the battlefield tapped|into your hand), then shuffle\.?$",
        count_word_pattern(),
    ))
    .expect("activated basic-land search regex compiles");
    if let Some(captures) = basic_land_search_re.captures(instruction) {
        let maximum = captures
            .get(2)
            .and_then(|count| parse_number_word(count.as_str()))
            .unwrap_or(1);
        let description = &captures[3];
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
        let destination = if &captures[4] == "into your hand" {
            "hand"
        } else {
            "battlefield"
        };
        let mut result = activated(
            costs,
            decisions,
            search_library_effects(filter, maximum, destination, destination == "battlefield"),
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
    let damage_flyer_re = Regex::new(r"^It deals (\d+) damage to target creature with flying\.$")
        .expect("activated flyer damage regex compiles");
    if let Some(captures) = damage_flyer_re.captures(instruction) {
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
                "amount": integer(captures[1].parse::<i64>().ok()?),
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
    let station_threshold_re =
        Regex::new(r"^(\d+)\+ \| (.+)$").expect("station threshold activation regex compiles");
    if let Some(captures) = station_threshold_re.captures(text) {
        let mut parsed = parse_simple_activated_ability(captures.get(2)?.as_str())
            .or_else(|| parse_common_activated_ability(captures.get(2)?.as_str()))?;
        let station_condition = compare(
            ">=",
            json!({
                "kind": "countCounters",
                "object": self_ref(),
                "counter": "charge",
            }),
            integer(captures[1].parse::<i64>().ok()?),
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

    let paid_sacrifice_re = Regex::new(
        r"^((?:\{[^}]+\})+), \{T\}, Sacrifice (this (?:land|artifact)|a creature): (.+)$",
    )
    .expect("paid sacrifice activation regex compiles");
    if let Some(captures) = paid_sacrifice_re.captures(text) {
        let sacrifice_self = captures[2].starts_with("this ");
        let mut costs = vec![
            json!({ "kind": "payMana", "manaCost": &captures[1] }),
            json!({ "kind": "tap", "object": self_ref() }),
        ];
        let declaration = if sacrifice_self {
            costs.push(json!({
                "kind": "sacrificePermanent",
                "permanent": self_ref(),
            }));
            None
        } else {
            costs.push(json!({
                "kind": "sacrificePermanent",
                "permanent": chosen_target("sacrificeCreature"),
            }));
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [
                    permanent_choice("sacrificeCreature", card_type("Creature"), false),
                ],
            }))
        };
        let effects = match &captures[3] {
            "Draw a card." => Some(vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })]),
            "You gain 3 life." => Some(vec![json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": integer(3),
            })]),
            "Return a creature card at random from your graveyard to your hand." => {
                Some(vec![json!({
                    "kind": "moveRandomCard",
                    "from": graveyard(controller()),
                    "where": card_type("Creature"),
                    "to": { "kind": "hand", "player": controller() },
                })])
            }
            _ => None,
        };
        if let Some(effects) = effects {
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

    let fetch_land_re = Regex::new(
        r"^\{T\}, (?:(Pay 1 life), )?Sacrifice this land: Search your library for (.+) card, put it onto the battlefield( tapped)?, then shuffle\.$",
    )
    .expect("fetch land regex compiles");
    if let Some(captures) = fetch_land_re.captures(text) {
        let filter_text = &captures[2];
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
        let mut costs = vec![
            json!({ "kind": "tap", "object": self_ref() }),
            json!({ "kind": "sacrificePermanent", "permanent": self_ref() }),
        ];
        if captures.get(1).is_some() {
            costs.insert(
                1,
                json!({
                    "kind": "payLife",
                    "player": controller(),
                    "amount": integer(1),
                }),
            );
        }
        return Some(activated_rule(
            costs,
            None,
            search_library_effects(filter, 1, "battlefield", captures.get(3).is_some()),
        ));
    }

    let protected_spell_re = Regex::new(
        r"^((?:\{[^}]+\})+), \{T\}: The next spell you cast this turn can't be countered\.$",
    )
    .expect("next-spell modifier regex compiles");
    if let Some(captures) = protected_spell_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                    {
                        "kind": "tap",
                        "object": self_ref(),
                    },
                ],
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
            }),
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

    let class_level_re =
        Regex::new(r"^((?:\{[^}]+\})+): Level ([23])$").expect("class level regex compiles");
    if let Some(captures) = class_level_re.captures(text) {
        let level = captures[2].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{
                    "kind": "payMana",
                    "manaCost": &captures[1],
                }],
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
            }),
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
