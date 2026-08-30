use super::super::*;

pub(in crate::oracle::canonical) fn parse_top_card_partition_sequence(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let any_matching_to_battlefield_re = Regex::new(&format!(
        r"(?i)^Look at the top ({}) cards? of your library, put any number of (.+?) cards? from among them onto the battlefield( tapped)?, then shuffle\.(?: (.+))?$",
        count_word_pattern(),
    ))
    .expect("top cards any matching to battlefield regex compiles");
    if let Some(captures) = any_matching_to_battlefield_re.captures(instruction) {
        let mut effects = vec![
            json!({
                "kind": "lookAtTopCards",
                "zone": library(controller()),
                "count": integer(parse_number_word(captures.get(1)?.as_str())?),
                "bind": "lookedCards",
            }),
            json!({
                "kind": "chooseCards",
                "id": "cardsForBattlefield",
                "player": controller(),
                "from": bound_objects("lookedCards"),
                "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
                "minimum": integer(0),
                "maximum": count_bound_objects("lookedCards"),
            }),
            json!({
                "kind": "moveCards",
                "cards": decision_result("cardsForBattlefield"),
                "to": {
                    "kind": "battlefield",
                    "player": controller(),
                    "tapped": captures.get(3).is_some(),
                },
            }),
            json!({ "kind": "shuffleZone", "zone": library(controller()) }),
        ];
        let mut decisions = Vec::new();
        if let Some(trailing) = captures.get(4) {
            let (trailing_effects, trailing_decisions) =
                parse_general_effect_instruction(trailing.as_str(), face_name)?;
            effects.extend(trailing_effects);
            decisions.extend(trailing_decisions);
        }
        return Some((effects, decisions));
    }
    let optional_selection_re = Regex::new(&format!(
        r"(?i)^(Look at|Reveal) the top ({}) cards? of your library\. You may (?:(reveal) (?:a|an) (.+?) card from among them and put (?:it|that card)|put (?:a|an) (.+?) card from among them) into your hand\. Put the rest (on the bottom of your library in (?:a random order|any order)|into your graveyard)\.(?: (.+))?$",
        count_word_pattern(),
    ))
    .expect("optional top-card partition sequence regex compiles");
    if let Some(captures) = optional_selection_re.captures(instruction) {
        let criteria = captures.get(4).or_else(|| captures.get(5))?.as_str();
        let mut effects = vec![json!({
            "kind": "lookAtTopCards",
            "zone": library(controller()),
            "count": integer(parse_number_word(&captures[2])?),
            "bind": "lookedCards",
        })];
        if captures[1].eq_ignore_ascii_case("reveal") {
            effects.push(json!({
                "kind": "revealCards",
                "cards": bound_objects("lookedCards"),
            }));
        }
        effects.push(json!({
            "kind": "chooseCards",
            "id": "cardForHand",
            "player": controller(),
            "from": bound_objects("lookedCards"),
            "where": parse_permanent_criteria(criteria, face_name)?,
            "minimum": integer(0),
            "maximum": integer(1),
        }));
        if captures.get(3).is_some() {
            effects.push(json!({
                "kind": "revealCards",
                "cards": decision_result("cardForHand"),
            }));
        }
        effects.push(json!({
            "kind": "moveCards",
            "cards": decision_result("cardForHand"),
            "to": hand(controller()),
        }));
        let remainder = json!({
            "kind": "setDifference",
            "left": bound_objects("lookedCards"),
            "right": decision_result("cardForHand"),
        });
        if captures[6]
            .to_ascii_lowercase()
            .contains("bottom of your library")
        {
            if captures[6].to_ascii_lowercase().contains("any order") {
                effects.push(json!({
                    "kind": "chooseOrder",
                    "id": "remainderOrder",
                    "player": controller(),
                    "objects": remainder,
                }));
                effects.push(json!({
                    "kind": "moveCards",
                    "cards": decision_result("remainderOrder"),
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "bottom",
                    },
                    "order": { "kind": "decisionOrder", "decisionId": "remainderOrder" },
                }));
            } else {
                effects.push(json!({
                    "kind": "moveCards",
                    "cards": remainder,
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "bottom",
                    },
                    "order": { "kind": "random" },
                }));
            }
        } else {
            effects.push(json!({
                "kind": "moveCards",
                "cards": remainder,
                "to": graveyard(controller()),
            }));
        }
        let mut decisions = Vec::new();
        if let Some(trailing) = captures.get(7) {
            let (trailing_effects, trailing_decisions) =
                parse_general_effect_instruction(trailing.as_str(), face_name)?;
            effects.extend(trailing_effects);
            decisions.extend(trailing_decisions);
        }
        return Some((effects, decisions));
    }

    let all_matching_re = Regex::new(&format!(
        r"(?i)^Reveal the top ({}) cards? of your library\. Put all (.+?) cards? revealed this way into your hand and the rest into your graveyard\.$",
        count_word_pattern(),
    ))
    .expect("all-matching revealed-card partition sequence regex compiles");
    if let Some(captures) = all_matching_re.captures(instruction) {
        let selected = json!({
            "kind": "filterObjects",
            "objects": bound_objects("revealedCards"),
            "where": parse_permanent_criteria(&captures[2], face_name)?,
        });
        return Some((
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(parse_number_word(&captures[1])?),
                    "bind": "revealedCards",
                }),
                json!({
                    "kind": "revealCards",
                    "cards": bound_objects("revealedCards"),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": selected,
                    "to": hand(controller()),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "setDifference",
                        "left": bound_objects("revealedCards"),
                        "right": selected,
                    },
                    "to": graveyard(controller()),
                }),
            ],
            Vec::new(),
        ));
    }

    let fixed_count_re = Regex::new(&format!(
        r"(?i)^Look at the top ({}) cards? of your library\. Put ({}) of them into your hand and the rest on the bottom of your library in (a random order|any order)\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("fixed-count top-card partition sequence regex compiles");
    if let Some(captures) = fixed_count_re.captures(instruction) {
        let remainder = json!({
            "kind": "setDifference",
            "left": bound_objects("lookedCards"),
            "right": decision_result("cardsForHand"),
        });
        let mut effects = vec![
            json!({
                "kind": "lookAtTopCards",
                "zone": library(controller()),
                "count": integer(parse_number_word(&captures[1])?),
                "bind": "lookedCards",
            }),
            json!({
                "kind": "chooseCards",
                "id": "cardsForHand",
                "player": controller(),
                "from": bound_objects("lookedCards"),
                "count": minimum(vec![
                    integer(parse_number_word(&captures[2])?),
                    count_bound_objects("lookedCards"),
                ]),
            }),
            json!({
                "kind": "moveCards",
                "cards": decision_result("cardsForHand"),
                "to": hand(controller()),
            }),
        ];
        if captures[3].eq_ignore_ascii_case("any order") {
            effects.push(json!({
                "kind": "chooseOrder",
                "id": "remainderOrder",
                "player": controller(),
                "objects": remainder,
            }));
            effects.push(json!({
                "kind": "moveCards",
                "cards": decision_result("remainderOrder"),
                "to": {
                    "kind": "library",
                    "player": controller(),
                    "position": "bottom",
                },
                "order": { "kind": "decisionOrder", "decisionId": "remainderOrder" },
            }));
        } else {
            effects.push(json!({
                "kind": "moveCards",
                "cards": remainder,
                "to": {
                    "kind": "library",
                    "player": controller(),
                    "position": "bottom",
                },
                "order": { "kind": "random" },
            }));
        }
        return Some((effects, Vec::new()));
    }

    let conditional_count_re = Regex::new(&format!(
        r"(?i)^Look at the top ({}) cards? of your library\. Put one of them into your hand and the rest on the bottom of your library in any order\. If (.+?), instead put two of them into your hand and the rest on the bottom of your library in any order\.$",
        count_word_pattern(),
    ))
    .expect("conditional top-card partition sequence regex compiles");
    if let Some(captures) = conditional_count_re.captures(instruction) {
        let selection_count = json!({
            "kind": "conditionalValue",
            "condition": parse_condition_text(&captures[2])?,
            "ifTrue": minimum(vec![integer(2), count_bound_objects("lookedCards")]),
            "ifFalse": minimum(vec![integer(1), count_bound_objects("lookedCards")]),
        });
        return Some((
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(parse_number_word(&captures[1])?),
                    "bind": "lookedCards",
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "cardsForHand",
                    "player": controller(),
                    "from": bound_objects("lookedCards"),
                    "count": selection_count,
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("cardsForHand"),
                    "to": hand(controller()),
                }),
                json!({
                    "kind": "chooseOrder",
                    "id": "remainderOrder",
                    "player": controller(),
                    "objects": {
                        "kind": "setDifference",
                        "left": bound_objects("lookedCards"),
                        "right": decision_result("cardsForHand"),
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("remainderOrder"),
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "bottom",
                    },
                    "order": { "kind": "decisionOrder", "decisionId": "remainderOrder" },
                }),
            ],
            Vec::new(),
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_delayed_blink_sequence(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let delayed_blink_re = Regex::new(
        r"(?i)^Exile (up to one )?(?:(another|other) )?target (.+?)\. (?:If you do, )?Return (?:that card|it) to the battlefield under its owner's control at the beginning of the next end step\.$",
    )
    .expect("delayed blink sequence regex compiles");
    let captures = delayed_blink_re.captures(instruction)?;
    let mut candidates = permanent_target_candidates(&captures[3], face_name)?;
    if captures.get(2).is_some() {
        candidates["excludeSource"] = Value::Bool(true);
    }
    Some((
        vec![json!({
            "kind": "exileUntilNextEndStep",
            "objects": { "kind": "chosenTargets", "id": "delayedBlinkTargets" },
            "returnUnderOwnerControl": true,
            "creatureCounter": "",
            "planeswalkerCounter": "",
        })],
        vec![target_decision(
            "delayedBlinkTargets",
            candidates,
            if captures.get(1).is_some() { 0 } else { 1 },
            1,
        )],
    ))
}

pub(in crate::oracle::canonical) fn parse_general_effect_sequence(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let discard_then_draw_re = Regex::new(&format!(
        r"(?i)^Discard ({0}) cards?\. If you do, draw ({0}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("conditional discard-then-draw regex compiles");
    if let Some(captures) = discard_then_draw_re.captures(instruction) {
        let discard_count = parse_number_word(captures.get(1)?.as_str())?;
        let draw_count = parse_number_word(captures.get(2)?.as_str())?;
        if discard_count <= 0 || draw_count <= 0 {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "discardThenDraw",
                "player": controller(),
                "discardCount": integer(discard_count),
                "drawCount": integer(draw_count),
            })],
            Vec::new(),
        ));
    }
    let linked_exiled_card_copy_re = Regex::new(
        r"(?i)^(You may )?copy the exiled card\. (?:If you do, )?you may cast the copy without paying its mana cost\.$",
    )
    .expect("linked exiled-card copy sequence regex compiles");
    if let Some(captures) = linked_exiled_card_copy_re.captures(instruction) {
        let copy_effects = vec![
            json!({
                "kind": "createCardCopy",
                "card": { "kind": "cardExiledWithSource" },
                "fromZone": "exile",
                "player": controller(),
                "to": "exile",
                "bind": "copiedLinkedExiledCard",
            }),
            json!({
                "kind": "castAnyNumber",
                "player": controller(),
                "cards": bound_objects("copiedLinkedExiledCard"),
                "where": { "kind": "canBeCastAsSpell" },
                "timing": { "kind": "duringResolution" },
                "withoutPayingManaCost": true,
                "alternativeCostsAllowed": false,
                "additionalCostsApply": true,
                "variableManaValue": 0,
                "maximum": integer(1),
            }),
            json!({
                "kind": "ceaseToExist",
                "objects": bound_objects("copiedLinkedExiledCard"),
                "fromZone": "exile",
            }),
        ];
        return Some((
            if captures.get(1).is_some() {
                vec![json!({
                    "kind": "optionalEffects",
                    "player": controller(),
                    "effects": copy_effects,
                })]
            } else {
                copy_effects
            },
            Vec::new(),
        ));
    }
    let linked_target_untap_keywords_re =
        Regex::new(r"(?i)^(.+?\.) Untap it\. It gains (.+?) until end of turn\.$")
            .expect("linked target untap and keyword sequence regex compiles");
    if let Some(captures) = linked_target_untap_keywords_re.captures(instruction) {
        let (mut effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        if effects.len() != 1
            || effects[0]["kind"] != "gainControlPermanent"
            || decisions.len() != 1
            || decisions[0]["id"] != "targetPermanent"
        {
            return None;
        }
        let target = chosen_target("targetPermanent");
        effects.push(json!({ "kind": "untapPermanent", "permanent": target.clone() }));
        for keyword in oracle_keyword_list(captures.get(2)?.as_str())? {
            effects.push(json!({
                "kind": "grantKeyword",
                "object": target.clone(),
                "keyword": keyword,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            }));
        }
        return Some((effects, decisions));
    }
    let triggering_player_exile_until_cast_re = Regex::new(
        r"(?i)^That player exiles cards from the top of their library until they exile (?:a|an) (.+?) card\. You may cast that card without paying its mana cost\. Then that player puts the exiled cards that weren't cast this way on the bottom of their library in a random order\.$",
    )
    .expect("triggering-player exile-until-cast sequence regex compiles");
    if let Some(captures) = triggering_player_exile_until_cast_re.captures(instruction) {
        let where_filter = card_qualifier_list_filter(captures.get(1)?.as_str(), face_name)
            .or_else(|| parse_permanent_criteria(captures.get(1)?.as_str(), face_name))?;
        return Some((
            vec![
                json!({
                    "kind": "exileFromTopUntil",
                    "zone": library(json!({ "kind": "triggeringPlayer" })),
                    "bind": "exiledUntilCards",
                    "stopCardBind": "exiledUntilMatchingCard",
                    "faceDown": false,
                    "stopWhere": where_filter,
                    "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
                }),
                json!({
                    "kind": "castAnyNumber",
                    "player": controller(),
                    "cards": bound_objects("exiledUntilMatchingCard"),
                    "where": { "kind": "canBeCastAsSpell" },
                    "timing": { "kind": "duringResolution" },
                    "withoutPayingManaCost": true,
                    "alternativeCostsAllowed": false,
                    "additionalCostsApply": true,
                    "variableManaValue": 0,
                    "maximum": integer(1),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": bound_objects("exiledUntilCards"),
                    "to": {
                        "kind": "library",
                        "player": { "kind": "triggeringPlayer" },
                        "position": "bottom",
                    },
                    "order": { "kind": "random" },
                }),
            ],
            Vec::new(),
        ));
    }
    let opponent_mill_reflexive_copy_re = Regex::new(&format!(
        r"(?i)^Each opponent mills ({}) cards?\. When one or more cards are milled this way, exile target (.+?) card with equal or lesser mana value than that spell from an opponent's graveyard\. Copy the exiled card\. You may cast the copy without paying its mana cost\.$",
        count_word_pattern(),
    ))
    .expect("opponent mill with reflexive graveyard-copy regex compiles");
    if let Some(captures) = opponent_mill_reflexive_copy_re.captures(instruction) {
        let first_instruction = format!("Each opponent mills {} cards.", captures.get(1)?.as_str());
        let (mut mill_effects, decisions) =
            parse_general_effect_instruction(&first_instruction, face_name)?;
        if mill_effects.len() != 1 || mill_effects[0]["kind"] != "millEachPlayer" {
            return None;
        }
        mill_effects[0]["bind"] = Value::String("milledOpponentCards".to_string());
        let card_where = card_qualifier_list_filter(captures.get(2)?.as_str(), face_name)
            .or_else(|| parse_permanent_criteria(captures.get(2)?.as_str(), face_name))?;
        mill_effects.push(json!({
            "kind": "conditionalEffect",
            "condition": {
                "kind": "bindingNotEmpty",
                "binding": "milledOpponentCards",
            },
            "then": [{
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
                            "graveyardCardToCopy",
                            json!({
                                "kind": "cards",
                                "zone": { "kind": "anyGraveyard" },
                                "owner": {
                                    "kind": "opponentsOf",
                                    "player": controller(),
                                },
                                "where": and(vec![
                                    card_where,
                                    compare(
                                        "<=",
                                        json!({
                                            "kind": "manaValueOf",
                                            "object": { "kind": "candidate" },
                                        }),
                                        json!({ "kind": "triggeringSpellManaValue" }),
                                    ),
                                ]),
                            }),
                            1,
                            1,
                        )],
                    },
                    "effects": [
                        {
                            "kind": "moveTargetCard",
                            "card": chosen_target("graveyardCardToCopy"),
                            "to": "exile",
                            "tapped": false,
                        },
                        {
                            "kind": "createCardCopy",
                            "card": chosen_target("graveyardCardToCopy"),
                            "fromZone": "exile",
                            "player": controller(),
                            "to": "exile",
                            "bind": "copiedExiledCard",
                        },
                        {
                            "kind": "castAnyNumber",
                            "player": controller(),
                            "cards": bound_objects("copiedExiledCard"),
                            "where": { "kind": "canBeCastAsSpell" },
                            "timing": { "kind": "duringResolution" },
                            "withoutPayingManaCost": true,
                            "alternativeCostsAllowed": false,
                            "additionalCostsApply": true,
                            "variableManaValue": 0,
                            "maximum": integer(1),
                        },
                        {
                            "kind": "ceaseToExist",
                            "objects": bound_objects("copiedExiledCard"),
                            "fromZone": "exile",
                        },
                    ],
                },
            }],
            "else": [],
        }));
        return Some((mill_effects, decisions));
    }
    let counter_linked_characteristics_re = Regex::new(
        r"(?is)^(.+?\.) For as long as that (?:creature|permanent) has (?:a|an) ([^ ]+) counter on it, (?:it's|it is) (?:a|an) ([A-Za-z][A-Za-z '-]+) in addition to its other types\.(?: \(.+\))?$",
    )
    .expect("counter-linked characteristics sequence regex compiles");
    if let Some(captures) = counter_linked_characteristics_re.captures(instruction) {
        let (mut effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let target_id = sole_target_decision_id(&decisions)?.to_string();
        let counter = captures.get(2)?.as_str().to_ascii_lowercase();
        if !effects.iter().any(|effect| {
            effect["kind"].as_str() == Some("putCounters")
                && effect["counter"]
                    .as_str()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&counter))
                && value_references_chosen_target(effect, &target_id)
        }) {
            return None;
        }
        let subtype = singular_card_term(captures.get(3)?.as_str());
        effects.push(json!({
            "kind": "installCounterLinkedCharacteristics",
            "object": chosen_target(&target_id),
            "counter": counter,
            "addSubtypes": [subtype],
            "keywords": ["shadow"],
        }));
        return Some((effects, decisions));
    }
    let remove_counter_then_zero_check_re = Regex::new(&format!(
        r"(?i)^Remove ({}) ([^ ]+) counters? from (.+?)\. If you do, (.+?)\. Then if (.+?) has no ([^ ]+) counters? on (?:it|him|her|them), (.+)$",
        count_word_pattern(),
    ))
    .expect("remove source counter then zero-counter check regex compiles");
    if let Some(captures) = remove_counter_then_zero_check_re.captures(instruction) {
        let source = captures.get(3)?.as_str();
        let checked_source = captures.get(5)?.as_str();
        if !source_reference_matches(source, face_name)
            || !source_reference_matches(checked_source, face_name)
            || !captures[2].eq_ignore_ascii_case(&captures[6])
        {
            return None;
        }
        let followup = format!("{}.", captures.get(4)?.as_str().trim_end_matches('.'));
        let final_effect = format!("{}.", captures.get(7)?.as_str().trim_end_matches('.'));
        let (followup_effects, mut decisions) =
            parse_general_effect_instruction(&followup, face_name)?;
        let (final_effects, final_decisions) =
            parse_general_effect_instruction(&final_effect, face_name)?;
        decisions.extend(final_decisions);
        let counter = captures.get(2)?.as_str().to_ascii_lowercase();
        return Some((
            vec![
                json!({
                    "kind": "removeCounters",
                    "permanent": self_ref(),
                    "counter": counter.clone(),
                    "count": integer(parse_number_word(captures.get(1)?.as_str())?),
                    "bind": "removedSourceCounters",
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "bindingNotEmpty",
                        "binding": "removedSourceCounters",
                    },
                    "then": followup_effects,
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": compare(
                        "==",
                        json!({
                            "kind": "countCounters",
                            "object": self_ref(),
                            "counter": counter,
                        }),
                        integer(0),
                    ),
                    "then": final_effects,
                    "else": [],
                }),
            ],
            decisions,
        ));
    }
    let linked_target_and_source_counters_re =
        Regex::new(r"(?i)^(.+?\.) Put (.+?) on that (?:creature|permanent) and (.+?) on (.+?)\.$")
            .expect("linked target and source counter distribution regex compiles");
    if let Some(captures) = linked_target_and_source_counters_re.captures(instruction) {
        let source_reference = captures.get(4)?.as_str().trim();
        let explicit_source = matches!(
            source_reference.to_ascii_lowercase().as_str(),
            "it" | "this artifact" | "this creature" | "this enchantment" | "this permanent"
        ) || source_reference_matches(source_reference, face_name);
        let named_source = source_reference
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
            && !source_reference
                .to_ascii_lowercase()
                .contains(" you control")
            && !source_reference.to_ascii_lowercase().starts_with("target ");
        if !explicit_source && !named_source {
            return None;
        }
        let parse_counter_list = |text: &str| {
            let counter_re = Regex::new(r"(?i)^(?:a|an|one) (.+?) counter$")
                .expect("counter-list item regex compiles");
            text.split(" and ")
                .map(str::trim)
                .map(|item| {
                    counter_re
                        .captures(item)?
                        .get(1)
                        .map(|counter| counter.as_str().to_ascii_lowercase())
                })
                .collect::<Option<Vec<_>>>()
        };
        let target_counters = parse_counter_list(captures.get(2)?.as_str())?;
        let source_counters = parse_counter_list(captures.get(3)?.as_str())?;
        let (mut effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        if !decisions
            .iter()
            .any(|decision| decision["id"] == "targetPermanent")
        {
            return None;
        }
        effects.extend(target_counters.into_iter().map(|counter| {
            json!({
                "kind": "putCounters",
                "permanent": chosen_target("targetPermanent"),
                "counter": counter,
                "count": integer(1),
            })
        }));
        effects.extend(source_counters.into_iter().map(|counter| {
            json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": counter,
                "count": integer(1),
            })
        }));
        return Some((effects, decisions));
    }
    let destroy_then_gain_per_destroyed_re = Regex::new(&format!(
        r"(?i)^(.+?)\. You gain ({}) life for each permanent destroyed this way\.$",
        count_word_pattern(),
    ))
    .expect("destruction linked life-gain sequence regex compiles");
    if let Some(captures) = destroy_then_gain_per_destroyed_re.captures(instruction) {
        let first_instruction = format!("{}.", captures.get(1)?.as_str());
        let (mut effects, decisions) =
            parse_general_effect_instruction(&first_instruction, face_name)?;
        if effects.len() != 1
            || effects[0]["kind"] != "destroyPermanent"
            || effects[0]["permanent"]["kind"] != "eachPermanent"
        {
            return None;
        }
        effects[0]["bind"] = Value::String("destroyedPermanents".to_string());
        let life_permanent = parse_number_word(captures.get(2)?.as_str())?;
        let destroyed_count = count_bound_objects("destroyedPermanents");
        effects.push(json!({
            "kind": "gainLife",
            "player": controller(),
            "amount": if life_permanent == 1 {
                destroyed_count
            } else {
                json!({
                    "kind": "multiply",
                    "left": destroyed_count,
                    "right": integer(life_permanent),
                })
            },
        }));
        return Some((effects, decisions));
    }
    let effect_then_conditional_source_sacrifice_re =
        Regex::new(r"(?i)^(.+?)\. Then if (.+?), sacrifice (.+?)\. If you do, (.+)\.$")
            .expect("effect then conditional source sacrifice regex compiles");
    if let Some(captures) = effect_then_conditional_source_sacrifice_re.captures(instruction) {
        if !source_reference_matches(captures.get(3)?.as_str(), face_name) {
            return None;
        }
        let (mut effects, mut decisions) = parse_general_effect_instruction(
            &format!("{}.", captures.get(1)?.as_str()),
            face_name,
        )?;
        let (on_sacrifice, on_sacrifice_decisions) = parse_general_effect_instruction(
            &format!("{}.", captures.get(4)?.as_str()),
            face_name,
        )?;
        let mut decision_ids = decisions
            .iter()
            .filter_map(|decision| decision["id"].as_str().map(ToOwned::to_owned))
            .collect::<BTreeSet<_>>();
        if !on_sacrifice_decisions.iter().all(|decision| {
            decision["id"]
                .as_str()
                .is_some_and(|id| decision_ids.insert(id.to_string()))
        }) {
            return None;
        }
        decisions.extend(on_sacrifice_decisions);
        effects.push(json!({
            "kind": "conditionalEffect",
            "condition": parse_condition_text(captures.get(2)?.as_str())?,
            "then": [
                {
                    "kind": "sacrificePermanent",
                    "permanent": self_ref(),
                    "bind": "sacrificedSource",
                },
                {
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "bindingNotEmpty",
                        "binding": "sacrificedSource",
                    },
                    "then": on_sacrifice,
                    "else": [],
                },
            ],
            "else": [],
        }));
        return Some((effects, decisions));
    }
    let exile_opponent_top_then_play_for_life_re = Regex::new(&format!(
        r"(?i)^Exile the top (?:(card)|(X|{}) cards) of target opponent's library\. You may play (it|that card|them|those cards) this turn\. If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost\.$",
        count_word_pattern(),
    ))
    .expect("opponent top exile with life alternative cost regex compiles");
    if let Some(captures) = exile_opponent_top_then_play_for_life_re.captures(instruction) {
        let singular = captures.get(1).is_some();
        let permission_pronoun_is_singular = matches!(
            captures.get(3)?.as_str().to_ascii_lowercase().as_str(),
            "it" | "that card"
        );
        if singular != permission_pronoun_is_singular {
            return None;
        }
        let (count, decisions) = match captures.get(2).map(|value| value.as_str()) {
            Some(value) if value.eq_ignore_ascii_case("X") => {
                (decision_result("xValue"), vec![x_value()])
            }
            Some(value) => (integer(parse_number_word(value)?), Vec::new()),
            None => (integer(1), Vec::new()),
        };
        return Some((
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(chosen_target("targetOpponent")),
                    "count": count,
                    "faceDown": false,
                    "bind": "exiledTopCards",
                }),
                json!({
                    "kind": "grantCardPermission",
                    "player": controller(),
                    "cards": bound_objects("exiledTopCards"),
                    "permissions": ["lookAt", "play"],
                    "play": {
                        "normalTimingApplies": true,
                        "normalCostsApply": false,
                        "castingModifier": { "kind": "none" },
                        "alternativeCost": {
                            "kind": "payLife",
                            "player": controller(),
                            "amount": {
                                "kind": "manaValueOf",
                                "object": { "kind": "grantedCard" },
                            },
                        },
                    },
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
            {
                let mut declarations = decisions;
                declarations.push(target_decision(
                    "targetOpponent",
                    json!({
                        "kind": "players",
                        "where": { "kind": "isOpponentOf", "player": controller() },
                    }),
                    1,
                    1,
                ));
                declarations
            },
        ));
    }
    let look_exile_conditional_play_re = Regex::new(&format!(
        r"(?i)^Look at the top (?:(card)|({}) cards) of your library and exile (it|them) face down\. For as long as (it|they) remains? exiled, you may play (it|them) if (.+)\.$",
        count_word_pattern(),
    ))
    .expect("look, face-down exile, and conditional play regex compiles");
    if let Some(captures) = look_exile_conditional_play_re.captures(instruction) {
        let count = captures
            .get(2)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        let singular = captures.get(1).is_some();
        let exile_pronoun_is_singular = captures[3].eq_ignore_ascii_case("it");
        let remain_pronoun_is_singular = captures[4].eq_ignore_ascii_case("it");
        let play_pronoun_is_singular = captures[5].eq_ignore_ascii_case("it");
        if singular != exile_pronoun_is_singular
            || singular != remain_pronoun_is_singular
            || singular != play_pronoun_is_singular
        {
            return None;
        }
        return Some((
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(controller()),
                    "count": integer(count),
                    "faceDown": true,
                    "bind": "exiledTopCards",
                }),
                json!({
                    "kind": "grantCardPermission",
                    "player": controller(),
                    "cards": bound_objects("exiledTopCards"),
                    "permissions": ["lookAt", "play"],
                    "play": {
                        "normalTimingApplies": true,
                        "normalCostsApply": true,
                        "castingModifier": { "kind": "none" },
                    },
                    "condition": parse_condition_text(captures.get(6)?.as_str())?,
                    "duration": {
                        "kind": "whileObjectsRemainInZone",
                        "zone": { "kind": "exile" },
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let attach_group_then_reflexive_power_damage_re = Regex::new(
        r"(?i)^Attach any number of target (.+?) you control to target (.+?) you control\. When one or more (.+?) become attached to that (.+?) this way, that (.+?) deals damage equal to its power to (up to one )?target (.+?)\.$",
    )
    .expect("group attachment and reflexive power-damage regex compiles");
    if let Some(captures) = attach_group_then_reflexive_power_damage_re.captures(instruction) {
        let attachment_filter = parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?;
        let recipient_filter = parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?;
        if attachment_filter != parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?
            || recipient_filter != parse_permanent_criteria(captures.get(4)?.as_str(), face_name)?
            || recipient_filter != parse_permanent_criteria(captures.get(5)?.as_str(), face_name)?
        {
            return None;
        }
        return Some((
            vec![
                json!({
                    "kind": "bind",
                    "id": "attachmentRecipient",
                    "value": chosen_target("attachmentRecipient"),
                }),
                json!({
                    "kind": "attachPermanent",
                    "attachment": { "kind": "chosenTargets", "id": "attachmentTargets" },
                    "to": chosen_target("attachmentRecipient"),
                    "bind": "attachedPermanents",
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "bindingNotEmpty",
                        "binding": "attachedPermanents",
                    },
                    "then": [{
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
                                    "damageTarget",
                                    permanent_target_candidates(
                                        captures.get(7)?.as_str(),
                                        face_name,
                                    )?,
                                    if captures.get(6).is_some() { 0 } else { 1 },
                                    1,
                                )],
                            },
                            "effects": [{
                                "kind": "dealDamage",
                                "source": { "kind": "boundValue", "id": "attachmentRecipient" },
                                "amount": {
                                    "kind": "powerOf",
                                    "object": { "kind": "boundValue", "id": "attachmentRecipient" },
                                },
                                "recipient": chosen_target("damageTarget"),
                            }],
                        },
                    }],
                    "else": [],
                }),
            ],
            vec![
                json!({
                    "id": "attachmentTargets",
                    "kind": "chooseTargets",
                    "minimum": integer(0),
                    "maximum": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": attachment_filter.clone(),
                    },
                    "candidates": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": attachment_filter,
                    },
                }),
                target_decision(
                    "attachmentRecipient",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": recipient_filter,
                    }),
                    1,
                    1,
                ),
            ],
        ));
    }
    if instruction.eq_ignore_ascii_case(
        "Untap all attacking creatures. After this phase, there is an additional combat phase.",
    ) {
        return Some((
            vec![
                json!({ "kind": "untapAttackingPermanents" }),
                json!({ "kind": "addCombatPhase" }),
            ],
            Vec::new(),
        ));
    }
    let source_counter_by_target_opponent_then_attach_re = Regex::new(&format!(
        r"(?i)^Put (?:a|an) ([A-Za-z0-9+/' -]+) counter on (.+?) for each (.+?) target opponent controls\. Attach (.+?) to (up to one )?target (.+?) you control\.$",
    ))
    .expect("source counters by target opponent then attach regex compiles");
    if let Some(captures) = source_counter_by_target_opponent_then_attach_re.captures(instruction)
        && source_reference_matches(captures.get(2)?.as_str(), face_name)
        && source_reference_matches(captures.get(4)?.as_str(), face_name)
    {
        return Some((
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": captures.get(1)?.as_str(),
                    "count": {
                        "kind": "countPermanents",
                        "player": chosen_target("targetOpponent"),
                        "where": parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?,
                    },
                }),
                json!({
                    "kind": "attachPermanent",
                    "attachment": self_ref(),
                    "to": chosen_target("targetAttachmentRecipient"),
                }),
            ],
            vec![
                target_decision(
                    "targetOpponent",
                    json!({
                        "kind": "players",
                        "where": { "kind": "isOpponentOf", "player": controller() },
                    }),
                    1,
                    1,
                ),
                target_decision(
                    "targetAttachmentRecipient",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": parse_permanent_criteria(captures.get(6)?.as_str(), face_name)?,
                    }),
                    if captures.get(5).is_some() { 0 } else { 1 },
                    1,
                ),
            ],
        ));
    }
    let normalized_instruction;
    let instruction = if let Some((main, reminder)) = instruction.rsplit_once(" (")
        && reminder.ends_with(')')
    {
        normalized_instruction = main.to_string();
        normalized_instruction.as_str()
    } else {
        instruction
    };
    let loot_land_to_battlefield_re = Regex::new(
        r"(?i)^Draw (?:a|one) card, then discard (?:a|one) card\. If you discard (?:a|an) (.+?) card this way, put it from your graveyard onto the battlefield tapped\.?$",
    )
    .expect("loot then move matching discarded card regex compiles");
    if let Some(captures) = loot_land_to_battlefield_re.captures(instruction) {
        let discarded_filter = card_qualifier_list_filter(captures.get(1)?.as_str(), face_name)
            .or_else(|| parse_permanent_criteria(captures.get(1)?.as_str(), face_name))?;
        return Some((
            vec![
                json!({ "kind": "drawCards", "player": controller(), "count": integer(1) }),
                json!({
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(1),
                    "bind": "discardedCards",
                }),
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "filterObjects",
                        "objects": bound_objects("discardedCards"),
                        "where": discarded_filter,
                    },
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": true,
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let target_then_pronoun_keyword_re =
        Regex::new(r"(?i)^(.+?\.) (?:It|He|She|They) gains (.+?) until end of turn\.?$")
            .expect("targeted effect followed by pronoun keyword regex compiles");
    if let Some(captures) = target_then_pronoun_keyword_re.captures(instruction) {
        let (mut effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let target_ids = decisions
            .iter()
            .filter(|decision| decision["kind"].as_str() == Some("chooseTargets"))
            .filter_map(|decision| decision["id"].as_str())
            .collect::<Vec<_>>();
        if target_ids.len() != 1 {
            return None;
        }
        for keyword in oracle_keyword_list(captures.get(2)?.as_str())? {
            effects.push(json!({
                "kind": "grantKeyword",
                "object": chosen_target(target_ids[0]),
                "keyword": keyword,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            }));
        }
        return Some((effects, decisions));
    }
    let damage_then_destroy_matching_recipient_re = Regex::new(&format!(
        r"(?i)^(.+?) deals ({}) damage to any target\. If (?:a|an) (.+?) is dealt damage this way, destroy it\.?$",
        count_word_pattern(),
    ))
    .expect("damage then destroy matching recipient regex compiles");
    if let Some(captures) = damage_then_destroy_matching_recipient_re.captures(instruction) {
        if !source_reference_matches(captures.get(1)?.as_str(), face_name) {
            return None;
        }
        return Some((
            vec![
                json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(parse_number_word(captures.get(2)?.as_str())?),
                    "recipient": chosen_target("damageTarget"),
                    "bindRecipientAs": "damagedObjects",
                }),
                json!({
                    "kind": "destroyPermanent",
                    "permanent": {
                        "kind": "filterObjects",
                        "objects": bound_objects("damagedObjects"),
                        "where": parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?,
                    },
                }),
            ],
            vec![target_decision(
                "damageTarget",
                json!({ "kind": "anyTarget" }),
                1,
                1,
            )],
        ));
    }
    if let Some(parsed) = parse_destroy_then_controller_amass_sequence(instruction, face_name) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_delayed_blink_sequence(instruction, face_name) {
        return Some(parsed);
    }
    let amass_then_attach_re = Regex::new(
        r"(?i)^Amass ([A-Za-z][A-Za-z '-]+) (\d+), then attach this (?:Aura|Equipment|permanent) to the amassed Army\.?$",
    )
    .expect("amass then attach source regex compiles");
    if let Some(captures) = amass_then_attach_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "amass",
                    "player": controller(),
                    "armySubtype": singular_card_term(captures.get(1)?.as_str()),
                    "count": integer(captures[2].parse::<i64>().ok()?),
                    "bind": "amassedArmy",
                }),
                json!({
                    "kind": "attachPermanent",
                    "attachment": self_ref(),
                    "to": bound_objects("amassedArmy"),
                }),
            ],
            Vec::new(),
        ));
    }
    let optional_sacrifice_reflexive_excess_amass_re = Regex::new(
        r"(?i)^You may sacrifice (another )?(.+?)\. When you do, (.+?) deals damage equal to that (.+?)'s power to (another )?target (.+?)\. If excess damage was dealt this way, amass ([A-Za-z][A-Za-z '-]+) X, where X is that excess damage\.$",
    )
    .expect("optional sacrifice reflexive excess-damage amass regex compiles");
    if let Some(captures) = optional_sacrifice_reflexive_excess_amass_re.captures(instruction) {
        if !source_reference_matches(captures.get(3)?.as_str(), face_name) {
            return None;
        }
        let sacrifice_where = parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?;
        if sacrifice_where != parse_permanent_criteria(captures.get(4)?.as_str(), face_name)? {
            return None;
        }
        let mut candidates = permanent_target_candidates(captures.get(6)?.as_str(), face_name)?;
        if captures.get(5).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "sacrificePermanents",
                    "player": controller(),
                    "where": sacrifice_where,
                    "count": integer(1),
                    "excludeSource": captures.get(1).is_some(),
                    "bindPowerAs": "sacrificedPower",
                },
                "onPerformed": [{
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
                                "damageTarget",
                                candidates,
                                1,
                                1,
                            )],
                        },
                        "effects": [
                            {
                                "kind": "dealDamage",
                                "source": self_ref(),
                                "amount": { "kind": "boundValue", "id": "sacrificedPower" },
                                "recipient": chosen_target("damageTarget"),
                                "bindExcessAs": "excessDamage",
                            },
                            {
                                "kind": "conditionalEffect",
                                "condition": compare(
                                    ">",
                                    json!({ "kind": "boundValue", "id": "excessDamage" }),
                                    integer(0),
                                ),
                                "then": [{
                                    "kind": "amass",
                                    "player": controller(),
                                    "armySubtype": singular_card_term(captures.get(7)?.as_str()),
                                    "count": { "kind": "boundValue", "id": "excessDamage" },
                                }],
                                "else": [],
                            },
                        ],
                    },
                }],
            })],
            Vec::new(),
        ));
    }
    let linked_target_subtype_threshold_re = Regex::new(
        r"(?is)^(.+?\.) It becomes (?:a|an) ([A-Za-z][A-Za-z '-]+) in addition to its other types\. Then if (.+?), (.+)$",
    )
    .expect("linked target subtype and threshold regex compiles");
    if let Some(captures) = linked_target_subtype_threshold_re.captures(instruction) {
        let (mut effects, mut decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let target_id = sole_target_decision_id(&decisions)?.to_string();
        let (trailing_effects, trailing_decisions) =
            parse_general_effect_instruction(captures.get(4)?.as_str(), face_name)?;
        effects.push(json!({
            "kind": "addSubtypeToPermanent",
            "object": chosen_target(&target_id),
            "subtype": singular_card_term(captures.get(2)?.as_str()),
            "duration": { "kind": "permanent" },
        }));
        effects.push(json!({
            "kind": "conditionalEffect",
            "condition": parse_condition_text(captures.get(3)?.as_str())?,
            "then": trailing_effects,
            "else": [],
        }));
        decisions.extend(trailing_decisions);
        return Some((effects, decisions));
    }
    let choose_target_copy_source_stats_re = Regex::new(
        r"(?i)^Choose (up to one )?(other )?target (.+?)\. Its base power and toughness become equal to (.+?)(?:'s|\x{2019}s) power and toughness until end of turn\.$",
    )
    .expect("chosen target copies source base stats regex compiles");
    if let Some(captures) = choose_target_copy_source_stats_re.captures(instruction)
        && source_reference_matches(captures.get(4)?.as_str(), face_name)
    {
        let mut candidates = permanent_target_candidates(captures.get(3)?.as_str(), face_name)?;
        if captures.get(2).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": "setBasePowerToughness",
                "object": chosen_target("targetPermanent"),
                "power": { "kind": "powerOf", "object": self_ref() },
                "toughness": { "kind": "toughnessOf", "object": self_ref() },
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            vec![target_decision(
                "targetPermanent",
                candidates,
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }
    let linked_target_filter_counter_re = Regex::new(&format!(
        r"(?is)^(.+?\.) If that (?:creature|permanent) is (?:a|an) (.+?), put ({}) ([A-Za-z0-9+/ -]+) counters? on it\.$",
        count_word_pattern(),
    ))
    .expect("linked target filter and counter regex compiles");
    if let Some(captures) = linked_target_filter_counter_re.captures(instruction) {
        let (mut effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let target_id = sole_target_decision_id(&decisions)?;
        effects.push(json!({
            "kind": "conditionalEffect",
            "condition": {
                "kind": "objectMatchesFilter",
                "object": chosen_target(target_id),
                "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
            },
            "then": [{
                "kind": "putCounters",
                "permanent": chosen_target(target_id),
                "counter": captures.get(4)?.as_str(),
                "count": integer(parse_number_word(captures.get(3)?.as_str())?),
            }],
            "else": [],
        }));
        return Some((effects, decisions));
    }
    let linked_target_fight_re = Regex::new(r"(?is)^(.+?\.) Then it fights target (.+?)\.$")
        .expect("linked target fight regex compiles");
    if let Some(captures) = linked_target_fight_re.captures(instruction) {
        let (mut effects, mut decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let first_target_id = sole_target_decision_id(&decisions)?.to_string();
        effects.push(json!({
            "kind": "fightPermanents",
            "first": chosen_target(&first_target_id),
            "second": chosen_target("fightTarget"),
        }));
        decisions.push(target_decision(
            "fightTarget",
            permanent_target_candidates(captures.get(2)?.as_str(), face_name)?,
            1,
            1,
        ));
        return Some((effects, decisions));
    }
    let shared_until_end_of_turn_re =
        Regex::new(r"(?i)^Until end of turn, (.+? gets [+-]\d+/[+-]\d+) and (.+? gains? .+)\.$")
            .expect("shared until-end-of-turn effects regex compiles");
    if let Some(captures) = shared_until_end_of_turn_re.captures(instruction) {
        let first = format!("{} until end of turn.", captures.get(1)?.as_str());
        let second = format!("{} until end of turn.", captures.get(2)?.as_str());
        let (mut effects, mut decisions) = parse_general_effect_instruction(&first, face_name)?;
        let (second_effects, second_decisions) =
            parse_general_effect_instruction(&second, face_name)?;
        effects.extend(second_effects);
        decisions.extend(second_decisions);
        return Some((effects, decisions));
    }
    let optional_sacrifice_power_counters_re = Regex::new(
        r"(?i)^You may sacrifice (another )?(.+?)\. If you do, put a number of \+1/\+1 counters on (.+?) equal to the sacrificed (.+?)'s power\.$",
    )
    .expect("optional sacrifice followed by sacrificed-power counters regex compiles");
    if let Some(captures) = optional_sacrifice_power_counters_re.captures(instruction) {
        let effect_subject = captures.get(3)?.as_str();
        if !matches!(
            effect_subject.to_ascii_lowercase().as_str(),
            "this creature" | "this permanent"
        ) && !source_reference_matches(effect_subject, face_name)
        {
            return None;
        }
        let sacrificed_criteria = parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?;
        let linked_criteria = parse_permanent_criteria(captures.get(4)?.as_str(), face_name)?;
        if sacrificed_criteria != linked_criteria {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "sacrificePermanents",
                    "player": controller(),
                    "where": sacrificed_criteria,
                    "count": integer(1),
                    "excludeSource": captures.get(1).is_some(),
                    "bindPowerAs": "sacrificedPermanentPower",
                },
                "onPerformed": [{
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": { "kind": "boundValue", "id": "sacrificedPermanentPower" },
                }],
            })],
            Vec::new(),
        ));
    }
    let optional_discard_hand_variable_re = Regex::new(
        r"(?i)^You may discard your hand\. Draw X cards, where X is the number of cards discarded this way\. If (.+?), (.+?) deals X damage to each opponent\.$",
    )
    .expect("optional discard-hand linked quantity regex compiles");
    if let Some(captures) = optional_discard_hand_variable_re.captures(instruction) {
        if !source_reference_matches(captures.get(2)?.as_str(), face_name) {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "discardHand",
                    "player": controller(),
                    "bindCountAs": "discardedHandCount",
                },
                "onPerformed": [
                    {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": { "kind": "boundValue", "id": "discardedHandCount" },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": parse_condition_text(captures.get(1)?.as_str())?,
                        "then": [{
                            "kind": "dealDamageToEachOpponent",
                            "amount": { "kind": "boundValue", "id": "discardedHandCount" },
                        }],
                        "else": [],
                    },
                ],
            })],
            Vec::new(),
        ));
    }
    let optional_discard_cards_followup_re = Regex::new(&format!(
        r"(?is)^You may discard ({}) cards?\. If you do, (.+)$",
        count_word_pattern(),
    ))
    .expect("optional discard-cards followed by effect regex compiles");
    if let Some(captures) = optional_discard_cards_followup_re.captures(instruction) {
        let trailing_instruction = captures.get(2)?.as_str().trim();
        let (trailing_effects, trailing_decisions) =
            parse_general_effect_sequence(trailing_instruction, face_name)
                .or_else(|| parse_general_effect_instruction(trailing_instruction, face_name))?;
        if !trailing_decisions.is_empty() {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(parse_number_word(captures.get(1)?.as_str())?),
                },
                "onPerformed": trailing_effects,
            })],
            Vec::new(),
        ));
    }
    let optional_sacrifice_followup_re =
        Regex::new(r"(?is)^You may sacrifice (another )?(.+?)\. If you do, (.+)$")
            .expect("optional sacrifice followed by effects regex compiles");
    if let Some(captures) = optional_sacrifice_followup_re.captures(instruction) {
        let trailing_instruction = captures.get(3)?.as_str().trim();
        let (trailing_effects, trailing_decisions) =
            parse_general_effect_sequence(trailing_instruction, face_name)
                .or_else(|| parse_general_effect_instruction(trailing_instruction, face_name))?;
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "sacrificePermanents",
                    "player": controller(),
                    "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
                    "count": integer(1),
                    "excludeSource": captures.get(1).is_some(),
                },
                "onPerformed": trailing_effects,
            })],
            trailing_decisions,
        ));
    }
    let optional_target_followup_re = Regex::new(r"(?is)^(.+?)\. If you do, (.+)$")
        .expect("optional target followed by conditional effect regex compiles");
    if let Some(captures) = optional_target_followup_re.captures(instruction) {
        let first_instruction = format!("{}.", captures.get(1)?.as_str().trim());
        let trailing_instruction = captures.get(2)?.as_str().trim();
        if let Some((mut effects, mut decisions)) =
            parse_general_effect_instruction(&first_instruction, face_name)
            && decisions.len() == 1
            && decisions[0]["kind"].as_str() == Some("chooseTargets")
            && decisions[0]["minimum"].as_i64() == Some(0)
            && decisions[0]["maximum"].as_i64() == Some(1)
        {
            let target_id = decisions[0]["id"].as_str()?.to_string();
            let (trailing_effects, trailing_decisions) =
                parse_general_effect_sequence(trailing_instruction, face_name).or_else(|| {
                    parse_general_effect_instruction(trailing_instruction, face_name)
                })?;
            effects.push(json!({
                "kind": "ifTargetWasChosen",
                "target": chosen_target(&target_id),
                "then": trailing_effects,
            }));
            decisions.extend(trailing_decisions);
            return Some((effects, decisions));
        }
    }
    let targeted_effect_death_exile_re = Regex::new(
        r"(?is)^(.+?\.) If that (?:creature|permanent) would die this turn, exile it instead\.$",
    )
    .expect("targeted effect then death-exile replacement regex compiles");
    if let Some(captures) = targeted_effect_death_exile_re.captures(instruction) {
        let base_instruction = captures.get(1)?.as_str();
        let (mut effects, decisions) =
            parse_general_effect_instruction(base_instruction, face_name)?;
        if decisions.len() != 1 || decisions[0]["kind"].as_str() != Some("chooseTargets") {
            return None;
        }
        let target_id = decisions[0]["id"].as_str()?;
        if !effects
            .iter()
            .any(|effect| value_references_chosen_target(effect, target_id))
        {
            return None;
        }
        effects.push(json!({
            "kind": "installDeathExileReplacement",
            "object": chosen_target(target_id),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
        return Some((effects, decisions));
    }
    if let Some(parsed) = parse_top_card_partition_sequence(instruction, face_name) {
        return Some(parsed);
    }
    let optional_variable_draw_then_discard_re = Regex::new(&format!(
        r"(?i)^You may draw X cards, where X is ({})\. If you do, discard ({}) cards?\.$",
        variable_clause_pattern(),
        count_word_pattern(),
    ))
    .expect("optional variable draw then discard regex compiles");
    if let Some(captures) = optional_variable_draw_then_discard_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "drawCards",
                    "player": controller(),
                    "count": x_variable_expression(captures.get(1)?.as_str())?,
                },
                "onPerformed": [{
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(parse_number_word(captures.get(2)?.as_str())?),
                }],
            })],
            Vec::new(),
        ));
    }
    let counter_exile_and_cast_re = Regex::new(
        r"(?is)^Counter target spell\. If (?:a|an) (.+?) spell is countered this way, exile it instead of putting it into its owner's graveyard\. You may cast that card without paying its mana cost for as long as it remains exiled\.$",
    )
    .expect("counter filtered spell to exile with linked free-cast permission regex compiles");
    if let Some(captures) = counter_exile_and_cast_re.captures(instruction) {
        let exile_filter = if captures[1].eq_ignore_ascii_case("permanent") {
            json!({ "kind": "isPermanentCard" })
        } else {
            parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?
        };
        return Some((
            vec![
                json!({
                    "kind": "counterSpell",
                    "spell": chosen_target("targetSpell"),
                    "exileInsteadWhere": exile_filter,
                    "bindExiledAs": "counteredExiledSpell",
                }),
                json!({
                    "kind": "grantCardPermission",
                    "player": controller(),
                    "cards": bound_objects("counteredExiledSpell"),
                    "permissions": ["lookAt", "play"],
                    "play": {
                        "normalTimingApplies": true,
                        "normalCostsApply": false,
                        "castingModifier": { "kind": "none" },
                    },
                    "duration": {
                        "kind": "whileObjectsRemainInZone",
                        "zone": { "kind": "exile" },
                    },
                }),
            ],
            vec![target_decision(
                "targetSpell",
                json!({ "kind": "spells" }),
                1,
                1,
            )],
        ));
    }
    let counter_then_mana_value_effect_re = Regex::new(&format!(
        r"(?is)^Counter target spell\. If that spell's mana value was ({}) or (less|greater), (.+)$",
        count_word_pattern(),
    ))
    .expect("counter then mana-value conditional effect regex compiles");
    if let Some(captures) = counter_then_mana_value_effect_re.captures(instruction) {
        let follow_up = format!("{}", captures.get(3)?.as_str().trim());
        let (follow_up_effects, follow_up_decisions) =
            parse_general_effect_instruction(&follow_up, face_name)?;
        if !follow_up_decisions.is_empty() {
            return None;
        }
        let target_spell = chosen_target("targetSpell");
        return Some((
            vec![
                json!({
                    "kind": "bind",
                    "id": "counteredSpellManaValue",
                    "value": { "kind": "manaValueOf", "object": target_spell.clone() },
                }),
                json!({ "kind": "counterSpell", "spell": target_spell }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": compare(
                        if captures[2].eq_ignore_ascii_case("less") { "<=" } else { ">=" },
                        json!({ "kind": "boundValue", "id": "counteredSpellManaValue" }),
                        integer(parse_number_word(captures.get(1)?.as_str())?),
                    ),
                    "then": follow_up_effects,
                    "else": [],
                }),
            ],
            vec![target_decision(
                "targetSpell",
                json!({ "kind": "spells" }),
                1,
                1,
            )],
        ));
    }
    let conditional_instead_re =
        Regex::new(r"(?is)^(.+?\.) If (.+?), (?:(instead) (.+?)|(.+?) instead)\.$")
            .expect("conditional alternate effect sequence regex compiles");
    if let Some(captures) = conditional_instead_re.captures(instruction) {
        let (base_effects, base_decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let alternate_text = captures.get(4).or_else(|| captures.get(5))?.as_str();
        let alternate_instruction = format!("{}.", alternate_text.trim_end_matches('.'));
        let token_pronoun_re = Regex::new(&format!(
            r"(?i)^create ({}) of those tokens that are (tapped)(?: and (attacking))?\.$",
            count_word_pattern(),
        ))
        .expect("conditional token-pronoun alternate regex compiles");
        let (alternate_effects, alternate_decisions) = if let Some(alternate) =
            token_pronoun_re.captures(&alternate_instruction)
        {
            if base_effects.len() != 1 || base_effects[0]["kind"] != "createTokens" {
                return None;
            }
            let mut alternate_effect = base_effects[0].clone();
            alternate_effect["quantity"] = integer(parse_number_word(alternate.get(1)?.as_str())?);
            alternate_effect["tapped"] = Value::Bool(alternate.get(2).is_some());
            alternate_effect["attacking"] = Value::Bool(alternate.get(3).is_some());
            (vec![alternate_effect], Vec::new())
        } else {
            parse_general_effect_instruction(&alternate_instruction, face_name)?
        };
        if !base_decisions.is_empty() || !alternate_decisions.is_empty() {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": parse_condition_text(captures.get(2)?.as_str())?,
                "then": alternate_effects,
                "else": base_effects,
            })],
            Vec::new(),
        ));
    }
    let linked_target_player_search_re = Regex::new(
        r"(?i)^(.+\.) That player may search their library for that many (.+?) cards, put (?:those cards|them) onto the battlefield( tapped)?, then shuffle\.$",
    )
    .expect("linked target-player variable library-search sequence regex compiles");
    if let Some(captures) = linked_target_player_search_re.captures(instruction) {
        let (mut effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let has_target_player = decisions
            .iter()
            .any(|decision| decision["id"].as_str() == Some("targetPlayer"));
        let has_exiled_binding = effects
            .iter()
            .any(|effect| effect["bind"].as_str() == Some("exiledPermanents"));
        if !has_target_player || !has_exiled_binding {
            return None;
        }
        effects.push(json!({
            "kind": "searchLibrary",
            "player": chosen_target("targetPlayer"),
            "where": entry_search_filter(&format!("a {} card", &captures[2]))?,
            "maximum": count_bound_objects("exiledPermanents"),
            "destination": "battlefield",
            "tapped": captures.get(3).is_some(),
        }));
        return Some((effects, decisions));
    }
    let linked_target_bonus_attachment_re = Regex::new(
        r"(?i)^((?:Tap|Untap) target .+?\.) It gets ([+-]\d+)/([+-]\d+) until end of turn\. If it's (?:a|an) (.+?), you may attach (?:a|an) (.+?) you control to it\.$",
    )
    .expect("linked target bonus and optional attachment sequence regex compiles");
    if let Some(captures) = linked_target_bonus_attachment_re.captures(instruction) {
        let (mut effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        if decisions.len() != 1 || decisions[0]["id"].as_str() != Some("targetPermanent") {
            return None;
        }
        let target = chosen_target("targetPermanent");
        effects.push(json!({
            "kind": "modifyPowerToughness",
            "object": target.clone(),
            "power": integer(captures[2].parse::<i64>().ok()?),
            "toughness": integer(captures[3].parse::<i64>().ok()?),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
        effects.push(json!({
            "kind": "conditionalEffect",
            "condition": {
                "kind": "objectMatchesFilter",
                "object": target.clone(),
                "where": parse_permanent_criteria(captures.get(4)?.as_str(), face_name)?,
            },
            "then": [
                {
                    "kind": "choosePermanents",
                    "id": "attachmentPermanent",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": parse_permanent_criteria(captures.get(5)?.as_str(), face_name)?,
                    },
                },
                {
                    "kind": "attachPermanent",
                    "attachment": decision_result("attachmentPermanent"),
                    "to": target,
                },
            ],
            "else": [],
        }));
        return Some((effects, decisions));
    }
    let temporary_death_return_re = Regex::new(&format!(
        r#"(?i)^Choose target (.+?)\. You lose ({}) life\. Until end of turn, that (?:creature|permanent) gains \"When this (?:creature|permanent) dies, return it to the battlefield( tapped)? under its owner's control\.\"$"#,
        count_word_pattern(),
    ))
    .expect("temporary death-return ability regex compiles");
    if let Some(captures) = temporary_death_return_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "loseLife",
                    "player": controller(),
                    "amount": integer(parse_number_word(&captures[2])?),
                }),
                json!({
                    "kind": "grantAbility",
                    "object": chosen_target("targetPermanent"),
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": self_ref(),
                        "event": { "kind": "permanentDied", "object": self_ref() },
                        "effects": [{
                            "kind": "returnAbilitySourceFromGraveyard",
                            "tapped": captures.get(3).is_some(),
                            "grantKeywords": [],
                            "exileAtNextEndStep": false,
                            "exileIfLeavesBattlefield": false,
                        }],
                    },
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(captures.get(1)?.as_str(), face_name)?,
                1,
                1,
            )],
        ));
    }
    let chosen_type_graveyard_return_re = Regex::new(
        r"(?i)^Choose a creature type\. Return all creature cards of the chosen type from your graveyard to the battlefield\.$",
    )
    .expect("chosen creature-type graveyard return regex compiles");
    if chosen_type_graveyard_return_re.is_match(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "chooseCreatureType",
                    "id": "chosenCreatureType",
                    "player": controller(),
                }),
                json!({
                    "kind": "returnCardsFromGraveyard",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), chosen_creature_type()]),
                    "controller": controller(),
                }),
            ],
            Vec::new(),
        ));
    }
    let chosen_type_draw_re = Regex::new(
        r"(?i)^Choose a creature type\. Draw a card for each permanent you control of that type\.$",
    )
    .expect("chosen creature-type permanent draw regex compiles");
    if chosen_type_draw_re.is_match(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "chooseCreatureType",
                    "id": "chosenCreatureType",
                    "player": controller(),
                }),
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": chosen_creature_type(),
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let chosen_type_token_per_controlled_re = Regex::new(
        r"(?i)^Choose a creature type\. Create (?:a|one) (.+?) token for each creature you control of that type\.$",
    )
    .expect("chosen creature-type token count regex compiles");
    if let Some(captures) = chosen_type_token_per_controlled_re.captures(instruction) {
        let mut create =
            create_token_effect(&format!("Create a {} token.", captures.get(1)?.as_str()))?;
        create["quantity"] = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": and(vec![card_type("Creature"), chosen_creature_type()]),
        });
        return Some((
            vec![
                json!({
                    "kind": "chooseCreatureType",
                    "id": "chosenCreatureType",
                    "player": controller(),
                }),
                create,
            ],
            Vec::new(),
        ));
    }
    let converge_draw_life_re = Regex::new(
        r"(?i)^You draw X cards and lose X life, where X is the number of colors of mana spent to cast this spell\.$",
    )
    .expect("converge draw and life-loss regex compiles");
    if converge_draw_life_re.is_match(instruction) {
        let amount = json!({ "kind": "colorsOfManaSpentToCastSource" });
        return Some((
            vec![
                json!({ "kind": "drawCards", "player": controller(), "count": amount.clone() }),
                json!({ "kind": "loseLife", "player": controller(), "amount": amount }),
            ],
            Vec::new(),
        ));
    }
    let hand_bottom_then_draw_re = Regex::new(
        r"(?i)^Put any number of cards from your hand on the bottom of your library, then draw that many cards plus one\.$",
    )
    .expect("hand bottom then replacement draw regex compiles");
    if hand_bottom_then_draw_re.is_match(instruction) {
        let selected = decision_result("cardsToBottom");
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "cardsToBottom",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": {
                        "kind": "countCards",
                        "zone": hand(controller()),
                        "where": Value::Null,
                    },
                    "candidates": {
                        "kind": "cards",
                        "zone": hand(controller()),
                        "where": Value::Null,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": selected.clone(),
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "bottom",
                    },
                }),
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": {
                        "kind": "add",
                        "left": { "kind": "countObjects", "objects": selected },
                        "right": integer(1),
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let targeted_immediate_blink_re = Regex::new(&format!(
        r"(?i)^Exile ({}) target (.+?) you control, then return them to the battlefield under their owner's control\.$",
        count_word_pattern(),
    ))
    .expect("targeted immediate multi-blink regex compiles");
    if let Some(captures) = targeted_immediate_blink_re.captures(instruction) {
        let count = parse_number_word(&captures[1])?;
        if count < 1 {
            return None;
        }
        let candidates = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": parse_permanent_criteria(&captures[2], face_name)?,
        });
        return Some((
            vec![json!({
                "kind": "blinkPermanents",
                "objects": { "kind": "chosenTargets", "id": "blinkPermanents" },
                "repeat": integer(1),
            })],
            vec![target_decision("blinkPermanents", candidates, count, count)],
        ));
    }
    let repeated_mass_blink_re = Regex::new(
        r"(?i)^Exile any number of (.+?) you control, then return them to the battlefield under their owner's control\. Then repeat this process X more times\.$",
    )
    .expect("repeated mass-blink regex compiles");
    if let Some(captures) = repeated_mass_blink_re.captures(instruction) {
        let where_filter = parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?;
        return Some((
            vec![
                json!({
                    "kind": "choosePermanents",
                    "id": "blinkPermanents",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": where_filter.clone(),
                    },
                    "candidates": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": where_filter,
                    },
                }),
                json!({
                    "kind": "blinkPermanents",
                    "objects": decision_result("blinkPermanents"),
                    "repeat": {
                        "kind": "add",
                        "left": decision_result("xValue"),
                        "right": integer(1),
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let graveyard_return_color_mana_re = Regex::new(
        r"(?i)^Return target (.+?) card from your graveyard to your hand\. If it's a (white|blue|black|red|green) card, add one mana of any color\.$",
    )
    .expect("graveyard return with color-conditioned mana regex compiles");
    if let Some(captures) = graveyard_return_color_mana_re.captures(instruction) {
        let target = chosen_target("targetGraveyardCard");
        return Some((
            vec![
                json!({
                    "kind": "moveTargetCard",
                    "card": target.clone(),
                    "to": "hand",
                    "tapped": false,
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "objectMatchesFilter",
                        "object": target,
                        "where": color_filter(captures.get(2)?.as_str())?,
                    },
                    "then": [{
                        "kind": "addMana",
                        "player": controller(),
                        "mana": { "kind": "chooseColor", "amount": integer(1) },
                    }],
                    "else": [],
                }),
            ],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                }),
                1,
                1,
            )],
        ));
    }
    let independent_comma_then_re =
        Regex::new(r"(?i)^(.+?), then (.+)\.$").expect("comma-then effect sequence regex compiles");
    if let Some(captures) = independent_comma_then_re.captures(instruction) {
        let first_clause = captures
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let second_clause = captures
            .get(2)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let second_lower = second_clause.to_ascii_lowercase();
        if ![
            "if ",
            "otherwise",
            "that ",
            "those ",
            "it ",
            "its ",
            "they ",
            "them ",
            "he ",
            "she ",
        ]
        .iter()
        .any(|prefix| second_lower.starts_with(prefix))
        {
            let first = format!("{first_clause}.");
            let second = format!("{second_clause}.");
            if let (Some((mut effects, mut decisions)), Some((second_effects, second_decisions))) = (
                parse_general_effect_instruction(&first, face_name),
                parse_general_effect_instruction(&second, face_name),
            ) {
                let mut decision_ids = decisions
                    .iter()
                    .filter_map(|decision| decision["id"].as_str().map(ToOwned::to_owned))
                    .collect::<BTreeSet<_>>();
                if second_decisions.iter().all(|decision| {
                    decision["id"]
                        .as_str()
                        .is_some_and(|id| decision_ids.insert(id.to_string()))
                }) {
                    effects.extend(second_effects);
                    decisions.extend(second_decisions);
                    return Some((effects, decisions));
                }
            }
        }
    }
    let variable_counter_sweep_re = Regex::new(&format!(
        r"(?i)^You get (X|{}) \{{E\}}(?: \((?:an? )?energy counters?\))?, then you may pay any amount of \{{E\}}\. Destroy each (.+?) with mana value less than or equal to the amount of \{{E\}} paid this way\.$",
        count_word_pattern(),
    ))
    .expect("variable player-counter sweep regex compiles");
    if let Some(captures) = variable_counter_sweep_re.captures(instruction) {
        let gained = if captures[1].eq_ignore_ascii_case("X") {
            decision_result("xValue")
        } else {
            integer(parse_number_word(&captures[1])?)
        };
        let paid = decision_result("paidPlayerCounters");
        return Some((
            vec![
                json!({
                    "kind": "addPlayerCounters",
                    "player": controller(),
                    "counter": "energy",
                    "count": gained,
                }),
                json!({
                    "kind": "chooseNumber",
                    "id": "paidPlayerCounters",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": {
                        "kind": "countPlayerCounters",
                        "player": controller(),
                        "counter": "energy",
                    },
                }),
                json!({
                    "kind": "removePlayerCounters",
                    "player": controller(),
                    "counter": "energy",
                    "count": paid.clone(),
                }),
                json!({
                    "kind": "destroyPermanentsByManaValue",
                    "where": parse_permanent_criteria(&captures[2], face_name)?,
                    "maximum": paid,
                }),
            ],
            Vec::new(),
        ));
    }
    let reveal_put_countered_permanent_re = Regex::new(&format!(
        r"(?i)^Reveal the top ({}) cards? of your library\. Put (?:a|an) (.+?) card from among them onto the battlefield with ({}) ([^ ]+) counters? on it\. It gains (.+?) until your next turn\. Then shuffle\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("reveal and put a countered permanent regex compiles");
    if let Some(captures) = reveal_put_countered_permanent_re.captures(instruction) {
        let selected = decision_result("revealedPermanent");
        return Some((
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(parse_number_word(&captures[1])?),
                    "bind": "revealedTopCards",
                }),
                json!({
                    "kind": "revealCards",
                    "cards": bound_objects("revealedTopCards"),
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "revealedPermanent",
                    "player": controller(),
                    "from": bound_objects("revealedTopCards"),
                    "where": parse_permanent_criteria(&captures[2], face_name)?,
                    "minimum": integer(0),
                    "maximum": integer(1),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": selected.clone(),
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                }),
                json!({
                    "kind": "putCounters",
                    "permanent": selected.clone(),
                    "counter": captures.get(4)?.as_str(),
                    "count": integer(parse_number_word(&captures[3])?),
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": selected,
                    "keyword": oracle_keyword_kind(&captures[5])?,
                    "duration": { "kind": "untilNextTurn", "player": controller() },
                }),
                json!({
                    "kind": "shuffleZone",
                    "zone": library(controller()),
                }),
            ],
            Vec::new(),
        ));
    }
    let exile_top_then_play_re = Regex::new(&format!(
        r"(?i)^Exile the top ({}) cards? of your library\. You may play them\.$",
        count_word_pattern(),
    ))
    .expect("exile top cards then play permission regex compiles");
    if let Some(captures) = exile_top_then_play_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(controller()),
                    "count": integer(parse_number_word(&captures[1])?),
                    "faceDown": false,
                    "bind": "exiledTopCards",
                }),
                json!({
                    "kind": "grantCardPermission",
                    "player": controller(),
                    "cards": bound_objects("exiledTopCards"),
                    "permissions": ["lookAt", "play"],
                    "play": {
                        "normalTimingApplies": true,
                        "normalCostsApply": true,
                        "castingModifier": { "kind": "none" },
                    },
                    "duration": {
                        "kind": "whileObjectsRemainInZone",
                        "zone": { "kind": "exile" },
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let draw_reveal_cast_one_re = Regex::new(&format!(
        r"(?i)^Draw ({}) cards? and reveal them\. You may cast one of them without paying its mana cost\.$",
        count_word_pattern(),
    ))
    .expect("draw reveal and cast one free regex compiles");
    if let Some(captures) = draw_reveal_cast_one_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(parse_number_word(&captures[1])?),
                    "bind": "drawnCards",
                }),
                json!({
                    "kind": "revealCards",
                    "cards": bound_objects("drawnCards"),
                }),
                json!({
                    "kind": "castAnyNumber",
                    "player": controller(),
                    "cards": bound_objects("drawnCards"),
                    "where": { "kind": "canBeCastAsSpell" },
                    "timing": { "kind": "duringResolution" },
                    "withoutPayingManaCost": true,
                    "alternativeCostsAllowed": false,
                    "additionalCostsApply": true,
                    "variableManaValue": integer(0),
                    "sourceZone": "hand",
                    "maximum": integer(1),
                }),
            ],
            Vec::new(),
        ));
    }
    let targeted_discard_then_copy_re = Regex::new(&format!(
        r"(?i)^Target (player|opponent) discards ({}) cards?\. That player may copy this spell and may choose (?:a new target|new targets) for (?:that copy|the copy)\.$",
        count_word_pattern(),
    ))
    .expect("targeted discard followed by optional resolving-spell copy regex compiles");
    if let Some(captures) = targeted_discard_then_copy_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        return Some((
            vec![
                json!({
                    "kind": "discardCards",
                    "player": target_player.clone(),
                    "count": integer(parse_number_word(&captures[2])?),
                }),
                json!({
                    "kind": "optionalEffects",
                    "player": target_player.clone(),
                    "effects": [{
                        "kind": "copyResolvingStackObject",
                        "controller": target_player,
                        "mayChooseNewTargets": true,
                    }],
                }),
            ],
            vec![target_decision(
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
        ));
    }
    let destroy_then_controller_search_re = Regex::new(
        r"(?i)^Destroy target (.+?)\. That player may search their library for (?:a|an) (.+?), put it onto the battlefield, then shuffle\.$",
    )
    .expect("destroy then affected-controller library search regex compiles");
    if let Some(captures) = destroy_then_controller_search_re.captures(instruction) {
        let affected_player = json!({ "kind": "boundValue", "id": "affectedPlayer" });
        let mut effects = vec![
            json!({
                "kind": "bind",
                "id": "affectedPlayer",
                "value": {
                    "kind": "controllerOf",
                    "object": chosen_target("targetPermanent"),
                },
            }),
            json!({
                "kind": "destroyPermanent",
                "permanent": chosen_target("targetPermanent"),
            }),
        ];
        effects.extend(search_library_effects_for(
            affected_player,
            card_qualifier_list_filter(&captures[2], face_name)
                .or_else(|| parse_permanent_criteria(&captures[2], face_name))?,
            1,
            "battlefield",
            false,
        ));
        return Some((
            effects,
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    let exile_then_conditional_counter_re = Regex::new(
        r"(?i)^Exile target card from a graveyard\. (?:If it was (?:a|an) (.+?) card|When (?:a|an) (.+?) card is exiled this way), put (?:a|an|one) ([^ ]+) counter on (this (?:artifact|creature|enchantment|permanent)|target .+?)\.$",
    )
    .expect("graveyard exile and conditional counter regex compiles");
    if let Some(captures) = exile_then_conditional_counter_re.captures(instruction) {
        let matching_criteria = captures.get(1).or_else(|| captures.get(2))?.as_str();
        let recipient_text = captures.get(4)?.as_str();
        let mut decisions = vec![target_decision(
            "targetGraveyardCard",
            json!({
                "kind": "cards",
                "zone": { "kind": "anyGraveyard" },
                "where": Value::Null,
            }),
            1,
            1,
        )];
        let recipient = if recipient_text.to_ascii_lowercase().starts_with("this ") {
            self_ref()
        } else {
            let criteria = recipient_text.get("target ".len()..).map(str::trim)?;
            decisions.push(target_decision(
                "counterRecipient",
                permanent_target_candidates(criteria, face_name)?,
                1,
                1,
            ));
            chosen_target("counterRecipient")
        };
        return Some((
            vec![
                json!({
                    "kind": "exileTargetCardWithSource",
                    "card": chosen_target("targetGraveyardCard"),
                    "source": self_ref(),
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "objectMatchesFilter",
                        "object": chosen_target("targetGraveyardCard"),
                        "where": parse_permanent_criteria(matching_criteria, face_name)?,
                    },
                    "then": [{
                        "kind": "putCounters",
                        "permanent": recipient,
                        "counter": captures.get(3)?.as_str(),
                        "count": integer(1),
                    }],
                    "else": [],
                }),
            ],
            decisions,
        ));
    }
    let targeted_graveyard_return_re = Regex::new(
        r"(?i)^Return target (.+?) from your graveyard to the battlefield\. That (?:creature|permanent) gains (.+?)(?: until end of turn)?\. Exile it at the beginning of the next end step\.$",
    )
    .expect("targeted graveyard return with delayed exile regex compiles");
    if let Some(captures) = targeted_graveyard_return_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetGraveyardCard"),
                "to": "battlefield",
                "controller": controller(),
                "tapped": false,
                "grantKeywords": oracle_keyword_list(&captures[2])?,
                "exileAtNextEndStep": true,
            })],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": parse_permanent_criteria(&captures[1], face_name)?,
                }),
                1,
                1,
            )],
        ));
    }
    let create_and_attach_re = Regex::new(
        r"(?i)^(Create .+? tokens?(?: with .+?)?)(\. You may|, then) attach this (?:Aura|Equipment|permanent) to it\.$",
    )
    .expect("create-token and source-attachment sequence regex compiles");
    if let Some(captures) = create_and_attach_re.captures(instruction) {
        let mut create = create_token_effect(&format!("{}.", &captures[1]))?;
        create["bind"] = Value::String("createdTokens".to_string());
        let attach = json!({
            "kind": "attachPermanent",
            "attachment": self_ref(),
            "to": bound_objects("createdTokens"),
        });
        let attach = if captures[2].eq_ignore_ascii_case(". You may") {
            json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [attach],
            })
        } else {
            attach
        };
        return Some((vec![create, attach], Vec::new()));
    }
    let delayed_blink_with_source_counter_re = Regex::new(
        r"(?i)^Exile (up to one )?(?:(another|other) )?target (.+?)\. (?:Return that card to the battlefield under its owner's control at the beginning of the next end step|At the beginning of the next end step, return that card to the battlefield under its owner's control)\. If it entered under your control, put (?:a|an|one) ([^ ]+) counter on (.+)\.$",
    )
    .expect("delayed blink with a conditional source counter regex compiles");
    if let Some(captures) = delayed_blink_with_source_counter_re.captures(instruction)
        && source_reference_matches(&captures[5], face_name)
    {
        let mut candidates = permanent_target_candidates(&captures[3], face_name)?;
        if captures.get(2).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": "exileUntilNextEndStep",
                "objects": { "kind": "chosenTargets", "id": "delayedBlinkTargets" },
                "returnUnderOwnerControl": true,
                "creatureCounter": "",
                "planeswalkerCounter": "",
                "sourceCounterIfReturnedUnderController": captures[4].trim(),
            })],
            vec![target_decision(
                "delayedBlinkTargets",
                candidates,
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }
    let bounded_delayed_blink_re = Regex::new(&format!(
        r"(?i)^Exile up to ({}) (other )?target (.+?) you control\. Return those cards to the battlefield(?: under their owner's control)? at the beginning of the next end step\.$",
        count_word_pattern(),
    ))
    .expect("bounded delayed mass-blink regex compiles");
    if let Some(captures) = bounded_delayed_blink_re.captures(instruction) {
        let mut candidates = permanent_target_candidates(&captures[3], face_name)?;
        candidates["controller"] = controller();
        if captures.get(2).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": "exileUntilNextEndStep",
                "objects": { "kind": "chosenTargets", "id": "delayedBlinkTargets" },
                "returnUnderOwnerControl": true,
                "creatureCounter": "",
                "planeswalkerCounter": "",
            })],
            vec![target_decision(
                "delayedBlinkTargets",
                candidates,
                0,
                parse_number_word(&captures[1])?,
            )],
        ));
    }
    let delayed_mass_blink_re = Regex::new(
        r"(?i)^Exile any number of (target )?(other )?(.+?) (you own and control|you control)\. Return those cards to the battlefield(?: under their owner's control)? at the beginning of the next end step\.$",
    )
    .expect("delayed mass-blink regex compiles");
    if let Some(captures) = delayed_mass_blink_re.captures(instruction) {
        let where_filter = parse_permanent_criteria(&captures[3], face_name)?;
        let mut candidates = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": where_filter.clone(),
        });
        if captures[4].eq_ignore_ascii_case("you own and control") {
            candidates["owner"] = controller();
        }
        if captures.get(2).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        if captures.get(1).is_some() {
            return Some((
                vec![json!({
                    "kind": "exileUntilNextEndStep",
                    "objects": { "kind": "chosenTargets", "id": "delayedBlinkTargets" },
                    "returnUnderOwnerControl": true,
                    "creatureCounter": "",
                    "planeswalkerCounter": "",
                })],
                vec![target_decision("delayedBlinkTargets", candidates, 0, 64)],
            ));
        }
        return Some((
            vec![
                json!({
                    "kind": "choosePermanents",
                    "id": "delayedBlinkPermanents",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": where_filter,
                    },
                    "candidates": candidates,
                }),
                json!({
                    "kind": "exileUntilNextEndStep",
                    "objects": decision_result("delayedBlinkPermanents"),
                    "returnUnderOwnerControl": true,
                    "creatureCounter": "",
                    "planeswalkerCounter": "",
                }),
            ],
            Vec::new(),
        ));
    }

    let phase_out_targets_re =
        Regex::new(r"(?i)^Any number of target (.+?) you control phase out\.$")
            .expect("variable-count phase-out target regex compiles");
    if let Some(captures) = phase_out_targets_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "phaseOutPermanent",
                "permanent": { "kind": "chosenTargets", "id": "phaseOutTargets" },
            })],
            vec![target_decision(
                "phaseOutTargets",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                }),
                0,
                64,
            )],
        ));
    }
    let one_per_card_type_re = Regex::new(&format!(
        r"(?i)^Reveal the top ({}) cards of your library\. For each card type, you may put a card of that type from among the revealed cards into your hand\. Put the rest on the bottom of your library in a random order\.(?: \(.+\))?$",
        count_word_pattern(),
    ))
    .expect("one-revealed-card-per-card-type sequence regex compiles");
    if let Some(captures) = one_per_card_type_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "revealTopAndChooseOnePerCardType",
                "player": controller(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }
    let reveal_top_random_matching_to_battlefield_re = Regex::new(&format!(
        r"(?i)^Reveal the top ({}) cards? of your library\. Put (?:a|one) random (.+?) card from among them onto the battlefield\. Put the rest on the bottom of your library in (?:a )?random order\.$",
        count_word_pattern(),
    ))
    .expect("fixed reveal with random matching battlefield card regex compiles");
    if let Some(captures) = reveal_top_random_matching_to_battlefield_re.captures(instruction) {
        let selected = decision_result("randomRevealedCard");
        return Some((
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(parse_number_word(captures.get(1)?.as_str())?),
                    "bind": "revealedTopCards",
                }),
                json!({
                    "kind": "revealCards",
                    "cards": bound_objects("revealedTopCards"),
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "randomRevealedCard",
                    "player": controller(),
                    "from": bound_objects("revealedTopCards"),
                    "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
                    "minimum": integer(0),
                    "maximum": integer(1),
                    "selection": { "kind": "random" },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": selected.clone(),
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "setDifference",
                        "left": bound_objects("revealedTopCards"),
                        "right": selected,
                    },
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "bottom",
                    },
                    "order": { "kind": "random" },
                }),
            ],
            Vec::new(),
        ));
    }
    let reveal_until_conditional_mana_value_destination_re = Regex::new(
        r"(?i)^Reveal cards from the top of your library until you reveal (?:an? )?(.+?) card\. If its mana value is less than or equal to the number of (.+?) you control, put it onto the battlefield( tapped)?\. Otherwise, put it into your hand\. Put the rest on the bottom of your library in (?:a )?random order\.$",
    )
    .expect("reveal-until with conditional mana-value destination regex compiles");
    if let Some(captures) = reveal_until_conditional_mana_value_destination_re.captures(instruction)
    {
        return Some((
            vec![json!({
                "kind": "revealUntilAndPutOntoBattlefield",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                "tapped": captures.get(3).is_some(),
                "attacking": false,
                "maximumManaValue": {
                    "kind": "countPermanents",
                    "player": controller(),
                    "where": parse_permanent_criteria(
                        &singular_card_term(captures.get(2)?.as_str()),
                        face_name,
                    )?,
                },
                "otherwise": "hand",
            })],
            Vec::new(),
        ));
    }
    let reveal_until_battlefield_re = Regex::new(
        r"(?i)^Reveal cards from the top of your library until you reveal (?:an? )?(.+?) card\. Put that card onto the battlefield( tapped)?( and attacking)? and the rest on the bottom of your library in a random order\.$",
    )
    .expect("reveal-until battlefield sequence regex compiles");
    if let Some(captures) = reveal_until_battlefield_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "revealUntilAndPutOntoBattlefield",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[1], face_name)?,
                "tapped": captures.get(2).is_some(),
                "attacking": captures.get(3).is_some(),
            })],
            Vec::new(),
        ));
    }
    let token_reflexive_attach_re =
        Regex::new(r"(?i)^(Create .+\.) When you do, attach it to target (.+?)\.$")
            .expect("token creation with reflexive attachment regex compiles");
    if let Some(captures) = token_reflexive_attach_re.captures(instruction) {
        let mut create = create_token_effect(captures.get(1)?.as_str())?;
        create["bind"] = Value::String("createdTokens".to_string());
        return Some((
            vec![
                create,
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
                                "targetPermanent",
                                permanent_target_candidates(
                                    captures.get(2)?.as_str(),
                                    face_name,
                                )?,
                                1,
                                1,
                            )],
                        },
                        "effects": [{
                            "kind": "attachPermanent",
                            "attachment": bound_objects("createdTokens"),
                            "to": chosen_target("targetPermanent"),
                        }],
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let mut depth = 0_i32;
    let mut quoted = false;
    let mut start = 0;
    let mut sentences = Vec::new();
    for (index, character) in instruction.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth = depth.saturating_sub(1),
            '.' if !quoted
                && depth == 0
                && instruction[index + character.len_utf8()..].starts_with(' ') =>
            {
                sentences.push(instruction[start..=index].trim().to_string());
                start = index + character.len_utf8() + 1;
            }
            _ => {}
        }
    }
    let tail = instruction[start..].trim();
    if !tail.is_empty() {
        sentences.push(if tail.ends_with('.') {
            tail.to_string()
        } else {
            format!("{tail}.")
        });
    }
    if sentences.len() < 2 {
        return None;
    }

    let mut effects = Vec::new();
    let mut decisions = Vec::new();
    let mut decision_ids = BTreeSet::new();
    for sentence in sentences {
        let sentence = sentence
            .strip_prefix("Then ")
            .or_else(|| sentence.strip_prefix("then "))
            .unwrap_or(&sentence);
        let lower = sentence.to_ascii_lowercase();
        if [
            "if ",
            "otherwise",
            "that ",
            "those ",
            "it ",
            "its ",
            "they ",
            "them ",
            "he ",
            "she ",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        {
            return None;
        }
        let (sentence_effects, sentence_decisions) =
            parse_general_effect_instruction(sentence, face_name)?;
        for decision in &sentence_decisions {
            let id = decision["id"].as_str()?;
            if !decision_ids.insert(id.to_string()) {
                return None;
            }
        }
        effects.extend(sentence_effects);
        decisions.extend(sentence_decisions);
    }
    Some((effects, decisions))
}

pub(in crate::oracle::canonical) fn parse_destroy_then_controller_amass_sequence(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let destroy_amass_re = Regex::new(
        r"(?i)^Destroy (up to one )?(other )?target (.+?)\. Its controller amasses ([A-Za-z][A-Za-z '-]+) X, where X is that (?:creature|permanent)'s power\. If you controlled that (?:creature|permanent), (.+)$",
    )
    .expect("destroy then controller amass sequence regex compiles");
    let captures = destroy_amass_re.captures(instruction)?;
    let target = chosen_target("targetPermanent");
    let mut candidates = permanent_target_candidates(captures.get(3)?.as_str(), face_name)?;
    if captures.get(2).is_some() {
        candidates["excludeSource"] = Value::Bool(true);
    }
    let follow_up = captures.get(5)?.as_str();
    let (follow_up_effects, follow_up_decisions) =
        parse_general_effect_instruction(follow_up, face_name)?;
    if !follow_up_decisions.is_empty() {
        return None;
    }
    Some((
        vec![
            json!({
                "kind": "bind",
                "id": "targetController",
                "value": { "kind": "controllerOf", "object": target.clone() },
            }),
            json!({
                "kind": "bind",
                "id": "targetPower",
                "value": { "kind": "powerOf", "object": target.clone() },
            }),
            json!({ "kind": "destroyPermanent", "permanent": target }),
            json!({
                "kind": "amass",
                "player": { "kind": "boundValue", "id": "targetController" },
                "armySubtype": singular_card_term(captures.get(4)?.as_str()),
                "count": { "kind": "boundValue", "id": "targetPower" },
            }),
            json!({
                "kind": "conditionalEffect",
                "condition": {
                    "kind": "playersEqual",
                    "left": { "kind": "boundValue", "id": "targetController" },
                    "right": controller(),
                },
                "then": follow_up_effects,
                "else": [],
            }),
        ],
        vec![target_decision(
            "targetPermanent",
            candidates,
            if captures.get(1).is_some() { 0 } else { 1 },
            1,
        )],
    ))
}
