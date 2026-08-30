use super::super::*;

pub(in crate::oracle::canonical) fn count_bound_objects(binding: &str) -> Value {
    json!({
        "kind": "countObjects",
        "objects": bound_objects(binding),
    })
}

pub(in crate::oracle::canonical) fn minimum(operands: Vec<Value>) -> Value {
    json!({ "kind": "minimum", "operands": operands })
}

pub(in crate::oracle::canonical) fn hand(player: Value) -> Value {
    json!({ "kind": "hand", "player": player })
}

pub(in crate::oracle::canonical) fn parse_library_spell(text: &str) -> Option<CanonicalRuleDraft> {
    if let Some((effects, decisions)) = parse_top_card_partition_sequence(text, "") {
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
                "Bind the inspected top-card set",
                "Select cards using reusable criteria and cardinality",
                "Move the chosen cards and set difference to their destinations",
            ],
        ));
    }
    let opponent_partitioned_search_re = Regex::new(&format!(
        r"(?i)^Search your library for ({}) cards? and reveal them\. Target opponent chooses one\. Put that card into your hand and the rest into your graveyard\. Then shuffle\.$",
        count_word_pattern(),
    ))
    .expect("opponent-partitioned library search regex compiles");
    if let Some(captures) = opponent_partitioned_search_re.captures(text) {
        let searched_count = parse_number_word(&captures[1])?;
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
                        "kind": "chooseCards",
                        "id": "searchedCards",
                        "player": controller(),
                        "minimum": integer(0),
                        "maximum": integer(searched_count),
                        "candidates": {
                            "kind": "cards",
                            "zone": library(controller()),
                            "where": Value::Null,
                        },
                    },
                    { "kind": "revealCards", "cards": decision_result("searchedCards") },
                    {
                        "kind": "chooseCards",
                        "id": "opponentChoice",
                        "player": chosen_target("targetOpponent"),
                        "from": decision_result("searchedCards"),
                        "minimum": integer(1),
                        "maximum": integer(1),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("opponentChoice"),
                        "to": hand(controller()),
                    },
                    {
                        "kind": "moveCards",
                        "cards": {
                            "kind": "setDifference",
                            "left": decision_result("searchedCards"),
                            "right": decision_result("opponentChoice"),
                        },
                        "to": graveyard(controller()),
                    },
                    { "kind": "shuffleZone", "zone": library(controller()) },
                ],
            }),
            &[
                "Search and reveal the requested cards",
                "Let the targeted opponent choose one",
                "Partition the chosen card and remainder into their destinations",
                "Shuffle the searched library",
            ],
        ));
    }
    let target_player_inspect_re = Regex::new(&format!(
        r"(?i)^Look at the top ({}) cards? of target player's library, then put them back in any order\. You may have that player shuffle\.$",
        count_word_pattern(),
    ))
    .expect("target-player look-reorder-shuffle regex compiles");
    if let Some(captures) = target_player_inspect_re.captures(text) {
        let target_player = chosen_target("targetPlayer");
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
                        "kind": "lookAtTopCards",
                        "zone": library(target_player.clone()),
                        "count": integer(parse_number_word(&captures[1])?),
                        "bind": "lookedCards",
                    },
                    {
                        "kind": "chooseOrder",
                        "id": "topOrder",
                        "player": controller(),
                        "objects": bound_objects("lookedCards"),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("topOrder"),
                        "to": {
                            "kind": "library",
                            "player": target_player.clone(),
                            "position": "top",
                        },
                        "order": { "kind": "decisionOrder", "decisionId": "topOrder" },
                    },
                    {
                        "kind": "optionalEffects",
                        "player": controller(),
                        "effects": [{
                            "kind": "shuffleZone",
                            "zone": library(target_player),
                        }],
                    },
                ],
            }),
            &[
                "Target a player",
                "Inspect and reorder that player's top cards",
                "Offer the controller the optional shuffle",
            ],
        ));
    }
    let inspect_re = Regex::new(&format!(
        r"(?i)^Look at the top ({}) cards? of your library, then put (?:it|them) back in any order\. You may shuffle\.$",
        count_word_pattern(),
    ))
    .expect("look, reorder, and optional shuffle regex compiles");
    if let Some(captures) = inspect_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "lookAtTopCards",
                        "zone": library(controller()),
                        "count": integer(parse_number_word(&captures[1])?),
                        "bind": "lookedCards",
                    },
                    {
                        "kind": "chooseOrder",
                        "id": "topOrder",
                        "player": controller(),
                        "objects": bound_objects("lookedCards"),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("topOrder"),
                        "to": {
                            "kind": "library",
                            "player": controller(),
                            "position": "top",
                        },
                        "order": {
                            "kind": "decisionOrder",
                            "decisionId": "topOrder",
                        },
                    },
                    {
                        "kind": "optionalEffects",
                        "player": controller(),
                        "effects": [{
                            "kind": "shuffleZone",
                            "zone": library(controller()),
                        }],
                    },
                ],
            }),
            &[
                "Bind the inspected top cards",
                "Let their controller choose their order",
                "Offer the optional library shuffle",
            ],
        ));
    }
    if text
        == "Look at the top two cards of your library. Put one of them into your hand and the other on the bottom of your library."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "lookAtTopCards",
                        "zone": library(controller()),
                        "count": integer(2),
                        "bind": "lookedCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "cardForHand",
                        "player": controller(),
                        "from": bound_objects("lookedCards"),
                        "count": minimum(vec![
                            integer(1),
                            count_bound_objects("lookedCards"),
                        ]),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("cardForHand"),
                        "to": hand(controller()),
                    },
                    {
                        "kind": "moveCards",
                        "cards": {
                            "kind": "setDifference",
                            "left": bound_objects("lookedCards"),
                            "right": decision_result("cardForHand"),
                        },
                        "to": {
                            "kind": "library",
                            "player": controller(),
                            "position": "bottom",
                        },
                        "order": { "kind": "random" },
                    },
                ],
            }),
            &[
                "Bind the top two cards",
                "Choose one for hand",
                "Move the remainder to library bottom",
            ],
        ));
    }
    if text.starts_with("Search your library for an instant or sorcery card, reveal it") {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "chooseCards",
                        "id": "foundCard",
                        "player": controller(),
                        "minimum": 0,
                        "maximum": 1,
                        "search": true,
                        "candidates": {
                            "kind": "cards",
                            "zone": library(controller()),
                            "where": or(vec![
                                card_type("Instant"),
                                card_type("Sorcery"),
                            ]),
                        },
                    },
                    {
                        "kind": "revealCards",
                        "cards": decision_result("foundCard"),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("foundCard"),
                        "to": hand(controller()),
                    },
                    {
                        "kind": "shuffleZone",
                        "zone": library(controller()),
                    },
                ],
            }),
            &[
                "Resolve hidden-zone search candidates",
                "Create optional search choice",
                "Bind found-card decision",
                "Order reveal, move, and shuffle effects",
            ],
        ));
    }

    if text.starts_with(
        "Look at the top X cards of your library, where X is the number of lands you control.",
    ) {
        let available = count_bound_objects("lookedCards");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "bind",
                        "id": "landCount",
                        "value": {
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": card_type("Land"),
                        },
                    },
                    {
                        "kind": "lookAtTopCards",
                        "player": controller(),
                        "zone": library(controller()),
                        "count": {
                            "kind": "boundValue",
                            "id": "landCount",
                        },
                        "bind": "lookedCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "cardsForHand",
                        "player": controller(),
                        "from": bound_objects("lookedCards"),
                        "count": {
                            "kind": "conditionalValue",
                            "condition": {
                                "kind": "wasKicked",
                                "spell": self_ref(),
                            },
                            "ifTrue": minimum(vec![
                                integer(2),
                                available.clone(),
                            ]),
                            "ifFalse": minimum(vec![
                                integer(1),
                                available,
                            ]),
                        },
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("cardsForHand"),
                        "to": hand(controller()),
                    },
                    {
                        "kind": "moveCards",
                        "cards": {
                            "kind": "setDifference",
                            "left": bound_objects("lookedCards"),
                            "right": decision_result("cardsForHand"),
                        },
                        "to": {
                            "kind": "library",
                            "player": controller(),
                            "position": "bottom",
                        },
                        "order": { "kind": "random" },
                    },
                ],
            }),
            &[
                "Read and bind controlled-land count",
                "Bind looked-at card set",
                "Branch hand count on kicked state",
                "Cap choice by available cards",
                "Move remainder in random bottom order",
            ],
        ));
    }

    if text.starts_with("Exile cards from the top of your library until you exile cards with total mana value 4 or greater.")
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "exileFromTopUntil",
                        "zone": library(controller()),
                        "bind": "exiledCards",
                        "faceDown": false,
                        "stopWhen": compare(
                            ">=",
                            json!({
                                "kind": "sumManaValues",
                                "objects": bound_objects("exiledCards"),
                                "variableManaSymbolsEqual": 0,
                            }),
                            integer(4),
                        ),
                        "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
                    },
                    {
                        "kind": "castAnyNumber",
                        "player": controller(),
                        "cards": bound_objects("exiledCards"),
                        "where": { "kind": "canBeCastAsSpell" },
                        "timing": { "kind": "duringResolution" },
                        "withoutPayingManaCost": true,
                        "alternativeCostsAllowed": false,
                        "additionalCostsApply": true,
                        "variableManaValue": 0,
                    },
                ],
            }),
            &[
                "Bind incrementally exiled top cards",
                "Accumulate mana values",
                "Create threshold-or-empty stop condition",
                "Resolve any-number casting permission",
                "Apply without-paying-cost rules",
            ],
        ));
    }

    if text.starts_with("Target opponent exiles the top X cards of their library face down.") {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        {
                            "id": "xValue",
                            "kind": "chooseNumber",
                            "minimum": 0,
                        },
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
                        "kind": "exileTopCards",
                        "zone": library(chosen_target("targetOpponent")),
                        "count": decision_result("xValue"),
                        "faceDown": true,
                        "bind": "exiledCards",
                    },
                    {
                        "kind": "grantCardPermission",
                        "player": controller(),
                        "cards": bound_objects("exiledCards"),
                        "permissions": ["lookAt", "play"],
                        "play": {
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                            "castingModifier": {
                                "kind": "spendManaAsAnyType",
                            },
                        },
                        "duration": {
                            "kind": "whileObjectsRemainInZone",
                            "zone": { "kind": "exile" },
                        },
                    },
                ],
            }),
            &[
                "Extract X declaration",
                "Extract required opponent target",
                "Bind face-down exiled top cards",
                "Install object-tracking look and play permission",
                "Attach mana-type spending modifier",
            ],
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_avatar_spell_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let without_reminder = text
        .split_once(". (")
        .map(|(instruction, _)| format!("{instruction}."))
        .unwrap_or_else(|| text.to_string());
    let earthbend_re =
        Regex::new(r"(?i)^Earthbend ([^.]+)\.?$").expect("spell earthbend regex compiles");
    if let Some(captures) = earthbend_re.captures(&without_reminder) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
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
                "effects": [earthbend_effect(
                    "earthbendLand",
                    avatar_quantity(&captures[1])?,
                )],
            }),
            &[
                "Declare controlled land target",
                "Resolve earthbend quantity and effect",
            ],
        ));
    }
    if without_reminder
        .to_ascii_lowercase()
        .starts_with("airbend ")
    {
        let decision = airbend_target_decision(&without_reminder)?;
        let candidates = decision["candidates"].clone();
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [decision],
                },
                "effects": [{
                    "kind": "airbend",
                    "object": chosen_target("airbendTarget"),
                    "candidates": candidates,
                    "alternativeManaCost": "{2}",
                }],
            }),
            &[
                "Declare airbend target",
                "Exile target and grant alternative casting cost",
            ],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_simple_spell_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let text = strip_short_oracle_label(text);
    let alternative_additional_cost_re = Regex::new(
        r"(?i)^As an additional cost to cast this spell, (sacrifice .+?) or pay ((?:\{[^}]+\})+)\.$",
    )
    .expect("sacrifice-or-mana additional cost regex compiles");
    let reversed_alternative_additional_cost_re = Regex::new(
        r"(?i)^As an additional cost to cast this spell, pay ((?:\{[^}]+\})+) or (sacrifice .+?)\.$",
    )
    .expect("mana-or-sacrifice additional cost regex compiles");
    let parsed_alternatives = alternative_additional_cost_re
        .captures(text)
        .map(|captures| {
            (
                captures.get(1).map(|value| value.as_str().to_string()),
                captures.get(2).map(|value| value.as_str().to_string()),
                false,
            )
        })
        .or_else(|| {
            reversed_alternative_additional_cost_re
                .captures(text)
                .map(|captures| {
                    (
                        captures.get(2).map(|value| value.as_str().to_string()),
                        captures.get(1).map(|value| value.as_str().to_string()),
                        true,
                    )
                })
        });
    if let Some((Some(sacrifice_text), Some(mana_text), mana_first)) = parsed_alternatives {
        let (sacrifice_costs, mut decisions) = parse_activation_costs(&sacrifice_text)?;
        let (mana_costs, mana_decisions) = parse_activation_costs(&mana_text)?;
        if sacrifice_costs.len() != 1
            || sacrifice_costs[0]["kind"].as_str() != Some("sacrificePermanent")
            || mana_costs.len() != 1
            || mana_costs[0]["kind"].as_str() != Some("payMana")
            || !mana_decisions.is_empty()
        {
            return None;
        }
        let condition = |mode: &str| selection("additionalCostMode", mode);
        for decision in &mut decisions {
            decision["condition"] = condition("sacrifice");
        }
        let options = if mana_first {
            vec!["payMana", "sacrifice"]
        } else {
            vec!["sacrifice", "payMana"]
        };
        decisions.insert(
            0,
            json!({
                "id": "additionalCostMode",
                "kind": "chooseModes",
                "minimum": 1,
                "maximum": 1,
                "options": options,
            }),
        );
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                    "additionalCosts": [
                        {
                            "kind": "conditional",
                            "condition": condition("sacrifice"),
                            "then": sacrifice_costs,
                            "else": [],
                        },
                        {
                            "kind": "conditional",
                            "condition": condition("payMana"),
                            "then": mana_costs,
                            "else": [],
                        },
                    ],
                },
                "effects": [],
            }),
            &[
                "Split the alternative additional-cost choices",
                "Parse sacrifice criteria and mana through the shared cost grammar",
                "Condition declarations and payments on exactly one chosen mode",
            ],
        ));
    }
    let optional_additional_cost_re =
        Regex::new(r"(?i)^As an additional cost to cast this spell, you may (.+)\.$")
            .expect("optional additional casting-cost regex compiles");
    let optional_additional_cost_text;
    let optional_additional_cost_input = if let Some((instruction, reminder)) =
        text.rsplit_once(" (")
        && reminder.ends_with(')')
    {
        optional_additional_cost_text = instruction.to_string();
        optional_additional_cost_text.as_str()
    } else {
        text
    };
    if let Some(captures) = optional_additional_cost_re.captures(optional_additional_cost_input)
        && let Some((costs, mut cost_decisions)) = parse_activation_costs(&captures[1])
    {
        let paid = selection("additionalCostMode", "pay");
        for decision in &mut cost_decisions {
            decision["condition"] = paid.clone();
        }
        let mut decisions = vec![json!({
            "id": "additionalCostMode",
            "kind": "chooseModes",
            "minimum": 1,
            "maximum": 1,
            "options": ["decline", "pay"],
        })];
        decisions.extend(cost_decisions);
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                    "additionalCosts": [{
                        "kind": "conditional",
                        "condition": paid,
                        "then": costs,
                        "else": [],
                    }],
                },
                "effects": [],
            }),
            &[
                "Offer the optional additional cost during casting",
                "Parse its payment through the shared cost grammar",
                "Declare and pay cost objects only when selected",
            ],
        ));
    }
    let additional_cost_re = Regex::new(r"^As an additional cost to cast this spell, (.+)\.$")
        .expect("generic additional casting-cost regex compiles");
    if let Some(captures) = additional_cost_re.captures(text)
        && let Some((costs, decisions)) = parse_activation_costs(&captures[1])
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                    "additionalCosts": costs,
                },
                "effects": [],
            }),
            &[
                "Parse the additional cost using shared cost grammar",
                "Declare any required payment objects",
                "Pay the cost before the spell is cast",
            ],
        ));
    }
    let kicked_target_group_return_re = Regex::new(
        r"(?i)^Choose target (.+?) you own\. If this spell was kicked, instead choose any number of target (.+?) you own\. Return each chosen (.+?) to your hand\. At the beginning of the next upkeep, create (.+?) for each (.+?) returned to your hand this way\.$",
    )
    .expect("kicked target-group return and delayed token regex compiles");
    if let Some(captures) = kicked_target_group_return_re.captures(text) {
        let base_criteria = parse_permanent_criteria(&captures[1], "")?;
        if base_criteria != parse_permanent_criteria(&captures[2], "")?
            || base_criteria != parse_permanent_criteria(&captures[3], "")?
            || base_criteria != parse_permanent_criteria(&captures[5], "")?
        {
            return None;
        }
        let mut candidates =
            permanent_target_candidates(&format!("{} you own", captures[1].trim()), "")?;
        candidates["where"] = base_criteria.clone();
        let kicked = json!({ "kind": "wasKicked", "spell": self_ref() });
        let conditional = |if_kicked: Value, otherwise: Value| {
            json!({
                "kind": "conditionalValue",
                "condition": kicked.clone(),
                "ifTrue": if_kicked,
                "ifFalse": otherwise,
            })
        };
        let mut create = create_token_effect(&format!("Create {}.", captures[4].trim()))?;
        create["quantity"] = count_bound_objects("returnedPermanents");
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "kind": "chooseTargets",
                        "id": "targetPermanents",
                        "minimum": conditional(integer(0), integer(1)),
                        "maximum": conditional(
                            json!({
                                "kind": "countPermanents",
                                "player": controller(),
                                "allPlayers": true,
                                "ownership": "owned",
                                "where": base_criteria,
                            }),
                            integer(1),
                        ),
                        "candidates": candidates,
                    }],
                },
                "effects": [
                    {
                        "kind": "returnToOwnersHand",
                        "object": { "kind": "chosenTargets", "id": "targetPermanents" },
                        "bind": "returnedPermanents",
                    },
                    {
                        "kind": "installDelayedStepTrigger",
                        "step": "upkeep",
                        "controller": controller(),
                        "effects": [create],
                    },
                ],
            }),
            &[
                "Choose one owned permanent or any number when kicked",
                "Return each legal target and bind the moved objects",
                "Create one token per returned object at the next upkeep",
            ],
        ));
    }
    if let Some(mana) = fixed_mana_sequence(text) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "addMana",
                    "player": controller(),
                    "mana": mana,
                }],
            }),
            &[
                "Recognize a fixed mana-symbol sequence",
                "Add every recognized mana symbol to the controller's pool",
            ],
        ));
    }
    if let Some((effects, decisions)) = parse_general_effect_instruction(text, "") {
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
        if text.contains("(Then exile this card. You may cast the ") {
            rule["exileAfterResolution"] = Value::Bool(true);
            rule["adventureAfterResolution"] = Value::Bool(true);
        }
        return Some(draft(
            rule,
            &[
                "Parse reusable effect vocabulary",
                "Declare generic targets",
                "Resolve effects in Oracle order",
            ],
        ));
    }
    if let Some((effects, decisions)) = parse_general_effect_sequence(text, "") {
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
                "Split independent Oracle sentences",
                "Parse each sentence through reusable effect vocabulary",
                "Resolve effects in Oracle order",
            ],
        ));
    }
    let targeted_zone_change_re =
        Regex::new(r"^(Destroy|Exile) target creature(?: with mana value (\d+) or greater)?\.$")
            .expect("targeted creature zone-change regex compiles");
    if let Some(captures) = targeted_zone_change_re.captures(text) {
        let mut filter = card_type("Creature");
        if let Some(minimum) = captures.get(2) {
            filter = and(vec![
                filter,
                compare(
                    ">=",
                    json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                    integer(minimum.as_str().parse::<i64>().ok()?),
                ),
            ]);
        }
        let effect_kind = if &captures[1] == "Destroy" {
            "destroyPermanent"
        } else {
            "exilePermanent"
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
                    "kind": effect_kind,
                    "permanent": chosen_target("targetCreature"),
                }],
            }),
            &[
                "Recognize a targeted creature zone change",
                "Resolve optional mana-value qualification",
                "Move the legal target to the requested zone",
            ],
        ));
    }
    if text == "Exile target artifact, creature, or enchantment." {
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
                            "where": or(vec![
                                card_type("Artifact"),
                                card_type("Creature"),
                                card_type("Enchantment"),
                            ]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "exilePermanent",
                    "permanent": chosen_target("targetPermanent"),
                }],
            }),
            &[
                "Resolve permanent type alternatives",
                "Exile the legal target",
            ],
        ));
    }
    if text == "Target attacking creature gets +3/+3 and gains trample until end of turn." {
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
                            "where": and(vec![
                                card_type("Creature"),
                                json!({ "kind": "isAttacking" }),
                            ]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": chosen_target("targetCreature"),
                        "power": integer(3),
                        "toughness": integer(3),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("targetCreature"),
                        "keyword": "trample",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Require an attacking creature",
                "Apply combat bonus and trample",
            ],
        ));
    }
    if text.starts_with("Target creature gains menace until end of turn.") {
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
                "effects": [{
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "menace",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &["Declare a creature target", "Grant menace for the turn"],
        ));
    }
    if text == "Creatures target player controls get +2/+0 until end of turn." {
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
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": {
                        "kind": "eachPermanent",
                        "player": chosen_target("targetPlayer"),
                        "where": card_type("Creature"),
                    },
                    "power": integer(2),
                    "toughness": integer(0),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Declare a player target",
                "Boost that player's creatures for the turn",
            ],
        ));
    }
    if text == "You may cast spells this turn as though they had flash." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "grantFlashUntilEndOfTurn",
                }],
            }),
            &["Install flash timing for the controller this turn"],
        ));
    }
    let mass_firebending_re =
        Regex::new(r"^Each creature you control gains firebending (\d+) until end of turn\.")
            .expect("mass firebending spell regex compiles");
    if let Some(captures) = mass_firebending_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "grantFirebending",
                    "object": {
                        "kind": "eachPermanent",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                    "quantity": integer(captures[1].parse::<i64>().ok()?),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Select controlled creatures",
                "Grant fixed firebending for the turn",
            ],
        ));
    }
    if text == "Until end of turn, you don't lose unspent red mana as steps and phases end." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "retainUnspentMana",
                    "player": controller(),
                    "symbols": ["R"],
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &["Retain red mana through steps and phases this turn"],
        ));
    }
    if text
        == "As an additional cost to cast this spell, pay {4} or sacrifice an artifact or creature."
    {
        let condition = |mode: &str| selection("additionalCostMode", mode);
        let mut sacrifice_target = target_decision(
            "additionalCostPermanent",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": or(vec![card_type("Artifact"), card_type("Creature")]),
            }),
            1,
            1,
        );
        sacrifice_target["condition"] = condition("sacrifice");
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
                            "options": ["payMana", "sacrifice"],
                        },
                        sacrifice_target,
                    ],
                    "additionalCosts": [
                        {
                            "kind": "conditional",
                            "condition": condition("payMana"),
                            "then": [{ "kind": "payMana", "manaCost": "{4}" }],
                        },
                        {
                            "kind": "conditional",
                            "condition": condition("sacrifice"),
                            "then": [{
                                "kind": "sacrificePermanent",
                                "permanent": chosen_target("additionalCostPermanent"),
                            }],
                        },
                    ],
                },
                "effects": [],
            }),
            &[
                "Choose mana or permanent sacrifice additional cost",
                "Pay exactly the selected additional cost",
            ],
        ));
    }
    if text
        == "(You may cast a legendary sorcery only if you control a legendary creature or planeswalker.)"
    {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "legendarySorceryRestriction": true,
            }),
            &["Require a controlled legendary creature or planeswalker"],
        ));
    }
    if text
        == "Destroy target artifact or enchantment. (Then exile this card. You may cast the creature later from exile.)"
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
                            "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetPermanent"),
                }],
                "exileAfterResolution": true,
                "adventureAfterResolution": true,
            }),
            &[
                "Declare the artifact or enchantment target",
                "Destroy the target",
                "Exile the Adventure and grant its later creature cast",
            ],
        ));
    }
    if text
        == "Surveil X, where X is the number of artifacts you control. Then draw three cards. (To surveil X, look at the top X cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)"
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "surveil",
                        "player": controller(),
                        "count": {
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": card_type("Artifact"),
                        },
                    },
                    {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(3),
                    },
                ],
            }),
            &[
                "Count controlled artifacts",
                "Surveil that many cards",
                "Draw three cards",
            ],
        ));
    }
    if text == "Destroy all enchantments." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": card_type("Enchantment"),
                    },
                }],
            }),
            &["Select all enchantments", "Destroy them simultaneously"],
        ));
    }
    if text
        == "Tap all creatures your opponents control. Creatures you control gain lifelink until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "tapPermanents",
                        "where": card_type("Creature"),
                        "excludedController": controller(),
                    },
                    {
                        "kind": "grantKeyword",
                        "object": {
                            "kind": "eachPermanent",
                            "player": controller(),
                            "where": card_type("Creature"),
                        },
                        "keyword": "lifelink",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Tap opposing creatures",
                "Grant lifelink to controlled creatures for the turn",
            ],
        ));
    }
    if text == "Creatures you control gain indestructible until end of turn." {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "grantKeyword",
                    "object": {
                        "kind": "eachPermanent",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                    "keyword": "indestructible",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Select controlled creatures",
                "Grant indestructible for the turn",
            ],
        ));
    }
    if text.contains("If you cast this spell during your main phase, put a +1/+1 counter on each of those creatures and they gain vigilance until end of turn.")
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "conditional",
                    "condition": {
                        "kind": "duringControllerMainPhase",
                        "player": controller(),
                    },
                    "then": [
                        {
                            "kind": "putCounters",
                            "permanent": {
                                "kind": "eachPermanent",
                                "player": controller(),
                                "where": card_type("Creature"),
                            },
                            "counter": "+1/+1",
                            "count": integer(1),
                        },
                        {
                            "kind": "grantKeyword",
                            "object": {
                                "kind": "eachPermanent",
                                "player": controller(),
                                "where": card_type("Creature"),
                            },
                            "keyword": "vigilance",
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        },
                    ],
                }],
            }),
            &[
                "Detect a cast during the controller's main phase",
                "Put counters on the affected creatures",
                "Grant vigilance for the turn",
            ],
        ));
    }
    if text
        == "Attach target Equipment to target creature. (Control of the Equipment doesn't change.)"
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetEquipment",
                            json!({
                                "kind": "permanents",
                                "where": subtype("Equipment"),
                            }),
                            1,
                            1,
                        ),
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
                    "kind": "attachPermanent",
                    "attachment": chosen_target("targetEquipment"),
                    "to": chosen_target("targetCreature"),
                }],
            }),
            &[
                "Declare the Equipment target",
                "Declare the creature target",
                "Attach without changing control",
            ],
        ));
    }
    if text == "Double target creature's power until end of turn." {
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
                    "kind": "modifyPowerToughness",
                    "object": chosen_target("targetCreature"),
                    "power": {
                        "kind": "powerOf",
                        "object": chosen_target("targetCreature"),
                    },
                    "toughness": integer(0),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Declare the creature target",
                "Read its power during resolution",
                "Apply an equal power bonus for the turn",
            ],
        ));
    }
    if text == "Target creature gets +2/+2 and gains reach until end of turn. Untap it." {
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
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": chosen_target("targetCreature"),
                        "power": integer(2),
                        "toughness": integer(2),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("targetCreature"),
                        "keyword": "reach",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "untapPermanent",
                        "permanent": chosen_target("targetCreature"),
                    },
                ],
            }),
            &[
                "Declare the creature target",
                "Apply +2/+2 and reach for the turn",
                "Untap the target",
            ],
        ));
    }
    if text
        == "Target creature you control gets +X/+0 and gains first strike and trample until end of turn."
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
                                "controller": controller(),
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
                        "object": chosen_target("targetCreature"),
                        "power": decision_result("xValue"),
                        "toughness": integer(0),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("targetCreature"),
                        "keyword": "firstStrike",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": chosen_target("targetCreature"),
                        "keyword": "trample",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Declare controlled creature target",
                "Reuse the cast X decision as power bonus",
                "Grant first strike and trample for the turn",
            ],
        ));
    }
    let self_exile_re = Regex::new(r"^Exile [^.]+\.$").expect("self-exile spell regex compiles");
    if self_exile_re.is_match(text) && !text.to_ascii_lowercase().contains("target") {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "exileAfterResolution": true,
            }),
            &[
                "Recognize spell self-exile instruction",
                "Exile after successful resolution",
            ],
        ));
    }
    let draw_re = Regex::new(&format!(r"^Draw ({}) cards?\.$", count_word_pattern()))
        .expect("simple draw spell regex compiles");
    if let Some(captures) = draw_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(parse_number_word(&captures[1])?),
                }],
            }),
            &["Resolve draw quantity", "Draw cards for spell controller"],
        ));
    }
    if let Some(effect) = create_token_effect(text) {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [effect],
            }),
            &[
                "Parse token specification",
                "Create tokens for spell controller",
            ],
        ));
    }
    None
}
