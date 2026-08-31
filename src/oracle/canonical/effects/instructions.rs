use super::super::*;

pub(in crate::oracle::canonical) fn parse_general_effect_instruction(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let normalized_instruction;
    let instruction = if let Some((main, _)) = instruction.split_once(". (") {
        normalized_instruction = format!("{}.", main.trim_end_matches('.'));
        normalized_instruction.as_str()
    } else {
        instruction
    };

    if instruction.eq_ignore_ascii_case("The Ring tempts you.") {
        return Some((
            vec![json!({
                "kind": "ringTemptsPlayer",
                "player": controller(),
            })],
            Vec::new(),
        ));
    }

    let controlled_permanents_counter_re = Regex::new(
        r"(?i)^Put (?:a|an|one) ([A-Za-z0-9+/ -]+?) counter on each (other )?(.+?) you control\.$",
    )
    .expect("controlled permanents counter regex compiles");
    if let Some(captures) = controlled_permanents_counter_re.captures(instruction) {
        let mut permanents = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?,
        });
        if captures.get(2).is_some() {
            permanents["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": permanents,
                "counter": captures.get(1)?.as_str().to_ascii_lowercase(),
                "count": integer(1),
            })],
            Vec::new(),
        ));
    }

    let life_per_controlled_permanent_re = Regex::new(&format!(
        r"(?i)^You gain ({}) life for each (other )?(.+?) you control\.$",
        count_word_pattern(),
    ))
    .expect("life per controlled permanent regex compiles");
    if let Some(captures) = life_per_controlled_permanent_re.captures(instruction) {
        let life_permanent = parse_number_word(captures.get(1)?.as_str())?;
        let mut permanent_count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?,
        });
        if captures.get(2).is_some() {
            permanent_count["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": if life_permanent == 1 {
                    permanent_count
                } else {
                    json!({
                        "kind": "multiply",
                        "left": permanent_count,
                        "right": integer(life_permanent),
                    })
                },
            })],
            Vec::new(),
        ));
    }

    let defending_player_least_power_sacrifice_re = Regex::new(
        r"(?i)^Defending player sacrifices (?:a|an|one) (.+?) with the least power among (.+?) they control\.$",
    )
    .expect("defending-player least-power sacrifice regex compiles");
    if let Some(captures) = defending_player_least_power_sacrifice_re.captures(instruction) {
        let sacrificed = parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?;
        let compared =
            parse_permanent_criteria(&singular_card_term(captures.get(2)?.as_str()), face_name)?;
        if sacrificed != compared {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "sacrificePermanents",
                "player": { "kind": "triggeringPlayer" },
                "where": sacrificed,
                "count": integer(1),
                "minimumPowerAmongCandidates": true,
            })],
            Vec::new(),
        ));
    }

    let opponent_sacrifices_damage_source_re = Regex::new(
        r"(?i)^Each opponent sacrifices (?:a|an|one) (.+?) of their choice that dealt combat damage to you this turn\.$",
    )
    .expect("opponent sacrifice among combat-damage sources regex compiles");
    if let Some(captures) = opponent_sacrifices_damage_source_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "sacrificePermanentsEachPlayer",
                "controller": controller(),
                "scope": "opponents",
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                "count": integer(1),
                "candidateIdsDecision": "combatDamageSourceIds",
            })],
            Vec::new(),
        ));
    }

    let source_keyword_until_end_re = Regex::new(r"(?i)^(.+?) gains (.+?) until end of turn\.$")
        .expect("source keyword until end of turn regex compiles");
    if let Some(captures) = source_keyword_until_end_re.captures(instruction) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            let keywords = oracle_keyword_list(captures.get(2)?.as_str())?;
            return Some((
                keywords
                    .into_iter()
                    .map(|keyword| {
                        json!({
                            "kind": "grantKeyword",
                            "object": self_ref(),
                            "keyword": keyword,
                            "duration": { "kind": "untilEndOfCurrentTurn" },
                        })
                    })
                    .collect(),
                Vec::new(),
            ));
        }
    }
    let tap_gendered_source_re =
        Regex::new(r"(?i)^Tap (him|her)\.$").expect("tap gendered source regex compiles");
    if tap_gendered_source_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "tapPermanent",
                "permanent": self_ref(),
            })],
            Vec::new(),
        ));
    }

    let player_protection_until_next_turn_re =
        Regex::new(r"(?i)^You gain protection from (.+?) until your next turn\.$")
            .expect("player protection until next turn regex compiles");
    if let Some(captures) = player_protection_until_next_turn_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "grantPlayerProtection",
                "player": controller(),
                "from": parse_protection_qualities(captures.get(1)?.as_str())?,
                "duration": {
                    "kind": "untilNextTurn",
                    "player": controller(),
                },
            })],
            Vec::new(),
        ));
    }

    let attach_source_re =
        Regex::new(r"(?i)^Attach (?:it|this (?:Aura|Equipment|permanent)) to target (.+?)\.?$")
            .expect("source attachment instruction regex compiles");
    if let Some(captures) = attach_source_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "attachPermanent",
                "attachment": self_ref(),
                "to": chosen_target("attachmentTarget"),
            })],
            vec![target_decision(
                "attachmentTarget",
                permanent_target_candidates(captures.get(1)?.as_str(), face_name)?,
                1,
                1,
            )],
        ));
    }

    let self_return_attached_re = Regex::new(
        r"(?i)^Return this card from your graveyard to the battlefield attached to target (.+?)\.?$",
    )
    .expect("graveyard self-return attached regex compiles");
    if let Some(captures) = self_return_attached_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "moveAbilitySourceToBattlefield",
                    "from": "graveyard",
                    "tapped": false,
                }),
                json!({
                    "kind": "attachPermanent",
                    "attachment": self_ref(),
                    "to": chosen_target("attachmentTarget"),
                }),
            ],
            vec![target_decision(
                "attachmentTarget",
                permanent_target_candidates(captures.get(1)?.as_str(), face_name)?,
                1,
                1,
            )],
        ));
    }

    if instruction.eq_ignore_ascii_case("Return this card from your graveyard to your hand.") {
        return Some((
            vec![json!({ "kind": "moveAbilitySourceToHand" })],
            Vec::new(),
        ));
    }
    let self_return_battlefield_re =
        Regex::new(r"(?i)^Return this card from your graveyard to the battlefield( tapped)?\.?$")
            .expect("graveyard self-return regex compiles");
    if let Some(captures) = self_return_battlefield_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "moveAbilitySourceToBattlefield",
                "from": "graveyard",
                "tapped": captures.get(1).is_some(),
            })],
            Vec::new(),
        ));
    }
    let triggering_player_mill_re = Regex::new(r"(?i)^That player mills that many cards\.$")
        .expect("triggering-player variable mill regex compiles");
    if triggering_player_mill_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "mill",
                "player": { "kind": "triggeringPlayer" },
                "count": { "kind": "decisionResult", "decisionId": "lifeLostAmount" },
            })],
            Vec::new(),
        ));
    }
    let draw_per_sized_player_zone_re = Regex::new(&format!(
        r"(?i)^Draw a card for each (graveyard|hand|library) with ({}) or (more|fewer) cards in it\.$",
        count_word_pattern(),
    ))
    .expect("draw per sized player zone regex compiles");
    if let Some(captures) = draw_per_sized_player_zone_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": {
                    "kind": "countPlayerZonesByCardCount",
                    "zone": captures[1].to_ascii_lowercase(),
                    "operator": if captures[3].eq_ignore_ascii_case("more") { ">=" } else { "<=" },
                    "count": integer(parse_number_word(&captures[2])?),
                },
            })],
            Vec::new(),
        ));
    }
    let each_opponent_discards_re = Regex::new(&format!(
        r"(?i)^Each opponent discards (?:a|({})) cards?\.$",
        count_word_pattern(),
    ))
    .expect("each-opponent discard regex compiles");
    if let Some(captures) = each_opponent_discards_re.captures(instruction) {
        let count = captures
            .get(1)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        return Some((
            vec![json!({
                "kind": "discardCards",
                "player": { "kind": "opponentsOf", "player": controller() },
                "count": integer(count),
            })],
            Vec::new(),
        ));
    }
    let source_power_amass_re = Regex::new(
        r"(?i)^Amass ([A-Za-z][A-Za-z '-]+) X, where X is this (?:creature|permanent)'s power\.$",
    )
    .expect("source last-known power amass regex compiles");
    if let Some(captures) = source_power_amass_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "amass",
                "player": controller(),
                "armySubtype": singular_card_term(captures.get(1)?.as_str()),
                "count": { "kind": "abilitySourceLastKnownPower" },
            })],
            Vec::new(),
        ));
    }

    let inspect_top_re = Regex::new(&format!(
        r"(?i)^Look at the top ({}) cards? of your library, then put (?:it|them) back in any order\.$",
        count_word_pattern(),
    ))
    .expect("look at and reorder top library cards regex compiles");
    if let Some(captures) = inspect_top_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(parse_number_word(&captures[1])?),
                    "bind": "lookedCards",
                }),
                json!({
                    "kind": "chooseOrder",
                    "id": "topOrder",
                    "player": controller(),
                    "objects": bound_objects("lookedCards"),
                }),
                json!({
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
                }),
            ],
            Vec::new(),
        ));
    }
    if let Some(effect) = parse_direct_add_mana_effect(instruction) {
        return Some((vec![effect], Vec::new()));
    }
    let global_destroy_re =
        Regex::new(r"(?i)^Destroy (?:all|each) (.+?)(?: (you control|your opponents control))?\.$")
            .expect("global filtered destruction instruction regex compiles");
    if let Some(captures) = global_destroy_re.captures(instruction) {
        let mut permanent = json!({
            "kind": "eachPermanent",
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
        });
        if let Some(scope) = captures.get(2) {
            permanent["player"] = if scope.as_str().eq_ignore_ascii_case("you control") {
                controller()
            } else {
                json!({ "kind": "opponentsOf", "player": controller() })
            };
        }
        return Some((
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": permanent,
            })],
            Vec::new(),
        ));
    }
    let optional_free_hand_cast_re = Regex::new(
        r"(?i)^You may cast (?:a|an|one) (.+?) spell with mana value X or less from your hand without paying its mana cost, where X is (.+?)\.?$",
    )
    .expect("optional bounded free cast from hand regex compiles");
    if let Some(captures) = optional_free_hand_cast_re.captures(instruction) {
        let spell_filter = parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?;
        let maximum_mana_value = parse_numeric_expression_text(captures.get(2)?.as_str())?;
        return Some((
            vec![json!({
                "kind": "castAnyNumber",
                "player": controller(),
                "cards": {
                    "kind": "cardsInZone",
                    "zone": hand(controller()),
                    "where": and(vec![
                        spell_filter,
                        compare(
                            "<=",
                            json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                            maximum_mana_value,
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
            })],
            Vec::new(),
        ));
    }
    let each_permanent_keyword_re = Regex::new(
        r"(?i)^Each (.+?) gains (flying|deathtouch|double strike|first strike|haste|lifelink|reach|trample|vigilance) until end of turn\.$",
    )
    .expect("filtered permanent keyword regex compiles");
    if let Some(captures) = each_permanent_keyword_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "grantKeyword",
                "object": {
                    "kind": "eachPermanent",
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                },
                "keyword": oracle_keyword_kind(captures.get(2)?.as_str())?,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    let search_reveal_top_re = Regex::new(
        r"(?i)^Search your library for (?:a|an) (.+?) card, reveal it, then shuffle and put the card on top\.$",
    )
    .expect("library search, reveal, shuffle, and top regex compiles");
    if let Some(captures) = search_reveal_top_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "searchedCard",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "cards",
                        "zone": library(controller()),
                        "where": card_qualifier_filter(&captures[1], face_name)
                            .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
                    },
                }),
                json!({
                    "kind": "revealCards",
                    "cards": decision_result("searchedCard"),
                }),
                json!({ "kind": "shuffleZone", "zone": library(controller()) }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("searchedCard"),
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "top",
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let source_to_owner_library_re = Regex::new(
        r"(?i)^Put this (?:artifact|creature|enchantment|land|permanent) on top of its owner's library\.$",
    )
    .expect("put ability source on owner's library regex compiles");
    if source_to_owner_library_re.is_match(instruction) {
        return Some((
            vec![json!({ "kind": "putAbilitySourceOnOwnersLibrary" })],
            Vec::new(),
        ));
    }
    let source_each_opponent_damage_re = Regex::new(&format!(
        r"(?i)^(.+?) deals ({}) damage to each opponent\.$",
        count_word_pattern(),
    ))
    .expect("source damage to each opponent regex compiles");
    if let Some(captures) = source_each_opponent_damage_re.captures(instruction)
        && source_reference_matches(&captures[1], face_name)
    {
        return Some((
            vec![json!({
                "kind": "dealDamageToEachOpponent",
                "amount": integer(parse_number_word(&captures[2])?),
            })],
            Vec::new(),
        ));
    }
    let reveal_until_hand_re = Regex::new(
        r"(?i)^Reveal cards from the top of your library until you reveal (?:a|an) (.+?) card\. Put that card into your hand and the rest on the bottom of your library in a random order\.$",
    )
    .expect("reveal until matching card into hand regex compiles");
    if let Some(captures) = reveal_until_hand_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "revealUntilAndPutIntoHand",
                "player": controller(),
                "where": card_qualifier_list_filter(&captures[1], face_name)
                    .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
            })],
            Vec::new(),
        ));
    }
    if instruction.eq_ignore_ascii_case("You may play an additional land this turn.") {
        return Some((
            vec![json!({
                "kind": "grantAdditionalLandPlays",
                "player": controller(),
                "count": integer(1),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    let graveyard_copy_until_end_re = Regex::new(
        r"(?i)^This (?:artifact|creature|enchantment|land|permanent) becomes a copy of target (.+?) card in your graveyard until end of turn\.$",
    )
    .expect("temporary copy of targeted graveyard card regex compiles");
    if let Some(captures) = graveyard_copy_until_end_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "becomeCopyOfCardUntilEndOfTurn",
                "object": self_ref(),
                "copy": chosen_target("copyCard"),
            })],
            vec![target_decision(
                "copyCard",
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
    let kicked_target_instead_re = Regex::new(
        r"(?i)^Exile target (.+?)\. If this spell was kicked, exile target (.+?) instead\.$",
    )
    .expect("kicked alternate exile target regex compiles");
    if let Some(captures) = kicked_target_instead_re.captures(instruction) {
        let kicked = json!({ "kind": "wasKicked", "spell": self_ref() });
        let mut base_target = target_decision(
            "baseTarget",
            permanent_target_candidates(&captures[1], face_name)?,
            1,
            1,
        );
        base_target["condition"] = not(kicked.clone());
        let mut kicked_target = target_decision(
            "kickedTarget",
            permanent_target_candidates(&captures[2], face_name)?,
            1,
            1,
        );
        kicked_target["condition"] = kicked.clone();
        return Some((
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": not(kicked.clone()),
                    "then": [{
                        "kind": "exilePermanent",
                        "permanent": chosen_target("baseTarget"),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": kicked,
                    "then": [{
                        "kind": "exilePermanent",
                        "permanent": chosen_target("kickedTarget"),
                    }],
                    "else": [],
                }),
            ],
            vec![base_target, kicked_target],
        ));
    }
    let kicked_phase_out_instead_re = Regex::new(
        r"(?i)^Target (.+?) phases out\. If this spell was kicked, each (.+?) target player controls phases out instead\.(?: \(.+\))?$",
    )
    .expect("kicked alternate phase-out scope regex compiles");
    if let Some(captures) = kicked_phase_out_instead_re.captures(instruction) {
        let kicked = json!({ "kind": "wasKicked", "spell": self_ref() });
        let mut base_target = target_decision(
            "baseTarget",
            permanent_target_candidates(captures.get(1)?.as_str(), face_name)?,
            1,
            1,
        );
        base_target["condition"] = not(kicked.clone());
        let mut kicked_target =
            target_decision("kickedTargetPlayer", json!({ "kind": "players" }), 1, 1);
        kicked_target["condition"] = kicked.clone();
        return Some((
            vec![
                json!({
                    "kind": "conditionalEffect",
                    "condition": not(kicked.clone()),
                    "then": [{
                        "kind": "phaseOutPermanent",
                        "permanent": chosen_target("baseTarget"),
                    }],
                    "else": [],
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": kicked,
                    "then": [{
                        "kind": "phaseOutPermanent",
                        "permanent": {
                            "kind": "eachPermanent",
                            "player": chosen_target("kickedTargetPlayer"),
                            "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
                        },
                    }],
                    "else": [],
                }),
            ],
            vec![base_target, kicked_target],
        ));
    }
    if instruction.eq_ignore_ascii_case(
        "You may cast spells from your graveyard this turn, and if a card would be put into your graveyard from anywhere this turn, exile it instead.",
    ) {
        return Some((
            vec![
                json!({
                    "kind": "grantZoneCastingUntilEndOfTurn",
                    "player": controller(),
                    "sourceZone": "graveyard",
                    "where": Value::Null,
                }),
                json!({
                    "kind": "replaceGraveyardWithExileUntilEndOfTurn",
                    "player": controller(),
                }),
            ],
            Vec::new(),
        ));
    }
    let retarget_stack_item_re = Regex::new(
        r"(?i)^You may choose new targets for target (spell|ability|spell or ability)\.$",
    )
    .expect("optional stack-item retarget regex compiles");
    if let Some(captures) = retarget_stack_item_re.captures(instruction) {
        let where_filter = match captures.get(1)?.as_str().to_ascii_lowercase().as_str() {
            "spell" => json!({ "kind": "isSpell" }),
            "ability" => or(vec![
                json!({ "kind": "isActivatedAbility" }),
                json!({ "kind": "isTriggeredAbility" }),
            ]),
            _ => or(vec![
                json!({ "kind": "isSpell" }),
                json!({ "kind": "isActivatedAbility" }),
                json!({ "kind": "isTriggeredAbility" }),
            ]),
        };
        return Some((
            vec![json!({
                "kind": "changeTargets",
                "object": chosen_target("targetStackObject"),
                "player": controller(),
                "optional": true,
            })],
            vec![target_decision(
                "targetStackObject",
                json!({
                    "kind": "stackItems",
                    "where": and(vec![
                        where_filter,
                        compare(
                            ">=",
                            json!({
                                "kind": "targetCountOf",
                                "object": { "kind": "candidate" },
                            }),
                            integer(1),
                        ),
                    ]),
                }),
                1,
                1,
            )],
        ));
    }
    let controller_sacrifice_re =
        Regex::new(r"(?i)^(You may )?[Ss]acrifice (another|a|an|one) (.+?)(?: of your choice)?\.$")
            .expect("controller permanent sacrifice regex compiles");
    if let Some(captures) = controller_sacrifice_re.captures(instruction) {
        let action = json!({
            "kind": "sacrificePermanents",
            "player": controller(),
            "where": parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?,
            "count": integer(1),
            "excludeSource": captures[2].eq_ignore_ascii_case("another"),
        });
        return Some((
            vec![if captures.get(1).is_some() {
                json!({
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": action,
                    "onPerformed": [],
                })
            } else {
                action
            }],
            Vec::new(),
        ));
    }
    let linked_cast_destination_re = Regex::new(
        r"(?i)^If (?:a|an) (.+?) spell cast this way would be put into your graveyard, exile it instead\.$",
    )
    .expect("linked cast-destination replacement regex compiles");
    if let Some(captures) = linked_cast_destination_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "replaceCastSpellDestination",
                "spells": bound_objects("castSpell"),
                "where": card_qualifier_list_filter(&captures[1], face_name)
                    .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
                "destination": "exile",
            })],
            Vec::new(),
        ));
    }
    let top_cards_two_piles_re = Regex::new(&format!(
        r"(?is)^Look at the top ({}) cards? of your library and separate them into a (face-down|face-up) pile and a (face-down|face-up) pile\. An opponent chooses one of the piles\. Put that pile (into your hand|into your graveyard) and the other (into your hand|into your graveyard)\.$",
        count_word_pattern(),
    ))
    .expect("top cards separated into two visibility piles regex compiles");
    if let Some(captures) = top_cards_two_piles_re.captures(instruction) {
        let first_visibility = captures.get(2)?.as_str().to_ascii_lowercase();
        let second_visibility = captures.get(3)?.as_str().to_ascii_lowercase();
        let chosen_destination = captures.get(4)?.as_str().to_ascii_lowercase();
        let other_destination = captures.get(5)?.as_str().to_ascii_lowercase();
        if first_visibility == second_visibility || chosen_destination == other_destination {
            return None;
        }
        let destination = |text: &str| {
            if text.ends_with("your hand") {
                "hand"
            } else {
                "graveyard"
            }
        };
        return Some((
            vec![json!({
                "kind": "separateTopCardsIntoPiles",
                "player": controller(),
                "count": integer(parse_number_word(captures.get(1)?.as_str())?),
                "firstPileVisibility": first_visibility,
                "secondPileVisibility": second_visibility,
                "pileChooser": { "kind": "chosenOpponent" },
                "chosenPileDestination": destination(&chosen_destination),
                "otherPileDestination": destination(&other_destination),
            })],
            Vec::new(),
        ));
    }
    if let Some(parsed) = parse_conditional_effect_amendment(instruction, face_name) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_mana_value_guarded_destroy_instruction(instruction, face_name) {
        return Some(parsed);
    }
    let conditional_instruction_re = Regex::new(r"(?i)^If (.+?), (?:also )?(.+)$")
        .expect("generic conditional instruction regex compiles");
    if let Some(captures) = conditional_instruction_re.captures(instruction) {
        let condition = parse_condition_text(captures.get(1)?.as_str())?;
        let (effects, decisions) =
            parse_general_effect_sequence(captures.get(2)?.as_str(), face_name).or_else(|| {
                parse_general_effect_instruction(captures.get(2)?.as_str(), face_name)
            })?;
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": condition,
                "then": effects,
                "else": [],
            })],
            decisions,
        ));
    }
    let attack_this_turn_re = Regex::new(r"(?i)^Whenever you attack this turn, (.+)$")
        .expect("temporary attack-trigger instruction regex compiles");
    if let Some(captures) = attack_this_turn_re.captures(instruction) {
        let (effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        let mut install = json!({
            "kind": "installAttackTrigger",
            "player": controller(),
            "duration": { "kind": "untilEndOfCurrentTurn" },
            "effects": effects,
        });
        if !decisions.is_empty() {
            install["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some((vec![install], Vec::new()));
    }
    if instruction.eq_ignore_ascii_case("Players can't cast spells this turn.") {
        return Some((
            vec![json!({
                "kind": "restrictPlayerActions",
                "player": { "kind": "eachPlayer" },
                "castSpells": true,
                "activateAbilities": false,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    let transform_source_re = Regex::new(r"(?i)^Transform (.+?)\.$")
        .expect("transform source instruction regex compiles");
    if let Some(captures) = transform_source_re.captures(instruction)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            vec![json!({
                "kind": "transformPermanent",
                "object": { "kind": "abilitySource" },
            })],
            Vec::new(),
        ));
    }
    if instruction.eq_ignore_ascii_case("Investigate.") {
        return Some((
            vec![create_token_effect("Create a Clue token.")?],
            Vec::new(),
        ));
    }

    let exile_return_transformed_re = Regex::new(
        r"(?i)^(you may )?exile (.+?), then return (?:it|him|her|them) to the battlefield transformed under (?:its|his|her|their) owner's control\.$",
    )
    .expect("source exile-return transformed instruction regex compiles");
    if let Some(captures) = exile_return_transformed_re.captures(instruction)
        && source_reference_matches(captures.get(2)?.as_str(), face_name)
    {
        let transform = json!({
            "kind": "exileThenReturnTransformed",
            "object": self_ref(),
            "controller": { "kind": "ownerOf", "object": self_ref() },
        });
        return Some((
            vec![if captures.get(1).is_some() {
                json!({
                    "kind": "optionalEffects",
                    "player": controller(),
                    "effects": [transform],
                })
            } else {
                transform
            }],
            Vec::new(),
        ));
    }

    let draw_equal_re = Regex::new(r"(?i)^Draw cards equal to (.+)\.$")
        .expect("variable card-draw instruction regex compiles");
    if let Some(captures) = draw_equal_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": parse_numeric_expression_text(captures.get(1)?.as_str())?,
            })],
            Vec::new(),
        ));
    }

    if instruction.eq_ignore_ascii_case("Draw a card for each opponent who lost life this turn.") {
        return Some((
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": {
                    "kind": "countOpponentsWhoLostLifeThisTurn",
                    "player": controller(),
                },
            })],
            Vec::new(),
        ));
    }

    let draw_for_each_player_counter_re =
        Regex::new(r"(?i)^Draw a card for each ([A-Za-z][A-Za-z '-]+) counter you have\.$")
            .expect("draw for each player counter regex compiles");
    if let Some(captures) = draw_for_each_player_counter_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": {
                    "kind": "countPlayerCounters",
                    "player": controller(),
                    "counter": singular_card_term(captures.get(1)?.as_str()).to_ascii_lowercase(),
                },
            })],
            Vec::new(),
        ));
    }

    let emblem_text = instruction
        .strip_prefix("You get an emblem with \"")
        .and_then(|text| text.strip_suffix("\".").or_else(|| text.strip_suffix('\"')));
    if let Some(emblem_text) = emblem_text {
        let modifiers = parse_player_static_modifiers(emblem_text).or_else(|| {
            let granted = parse_common_static_ability(emblem_text, face_name)?;
            (granted.rule["kind"].as_str() == Some("staticAbility"))
                .then(|| granted.rule["modifiers"].as_array().cloned())
                .flatten()
        })?;
        return Some((
            vec![json!({
                "kind": "createEmblem",
                "player": controller(),
                "modifiers": modifiers,
            })],
            Vec::new(),
        ));
    }
    let source_power_damage_re = Regex::new(
        r"(?i)^(?:It|This creature|This permanent) deals damage equal to its power to (up to one )?target (.+?)\.$",
    )
    .expect("source-power damage regex compiles");
    if let Some(captures) = source_power_damage_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "recipient": chosen_target("targetDamageRecipient"),
                "amount": { "kind": "powerOf", "object": self_ref() },
            })],
            vec![target_decision(
                "targetDamageRecipient",
                permanent_target_candidates(&captures[2], face_name)?,
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }
    let source_add_type_subtype_and_grant_re = Regex::new(
        r#"(?i)^This (?:artifact|enchantment|permanent) becomes (?:a|an) ([A-Za-z][A-Za-z '-]+) creature in addition to its other types and gains \"(.+?)\"\.?$"#,
    )
    .expect("source permanent animation with granted ability regex compiles");
    if let Some(captures) = source_add_type_subtype_and_grant_re.captures(instruction) {
        let granted = parse_common_static_ability(captures.get(2)?.as_str(), face_name)?;
        return Some((
            vec![
                json!({
                    "kind": "addCardType",
                    "object": self_ref(),
                    "cardType": "Creature",
                    "retainExistingTypes": true,
                    "duration": { "kind": "permanent" },
                }),
                json!({
                    "kind": "addSubtypeToPermanent",
                    "object": self_ref(),
                    "subtype": singular_card_term(captures.get(1)?.as_str()),
                    "duration": { "kind": "permanent" },
                }),
                json!({
                    "kind": "grantAbility",
                    "object": self_ref(),
                    "ability": granted.rule,
                    "duration": { "kind": "permanent" },
                }),
            ],
            Vec::new(),
        ));
    }
    let untap_and_grant_quoted_trigger_re = Regex::new(
        r#"(?i)^Untap target (.+?)\. Until end of turn, it gains (.+?), and "(.+)"\.?$"#,
    )
    .expect("untap and temporary granted-trigger regex compiles");
    if let Some(captures) = untap_and_grant_quoted_trigger_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        let keywords = oracle_keyword_list(&captures[2])?;
        let granted = parse_expansion_triggered(&captures[3], face_name)?;
        if granted.rule["kind"].as_str() != Some("triggeredAbility") {
            return None;
        }
        let mut effects = vec![json!({
            "kind": "untapPermanent",
            "permanent": target.clone(),
        })];
        effects.extend(keywords.into_iter().map(|keyword| {
            json!({
                "kind": "grantKeyword",
                "object": target.clone(),
                "keyword": keyword,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })
        }));
        effects.push(json!({
            "kind": "grantAbility",
            "object": target,
            "ability": granted.rule,
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
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
    let temporary_grant_quoted_trigger_re =
        Regex::new(r#"(?i)^Until end of turn, target (.+?) gains "(.+)"\.?$"#)
            .expect("temporary granted-trigger regex compiles");
    if let Some(captures) = temporary_grant_quoted_trigger_re.captures(instruction) {
        let granted = parse_expansion_triggered(captures.get(2)?.as_str(), face_name)?;
        if granted.rule["kind"].as_str() != Some("triggeredAbility") {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "grantAbility",
                "object": chosen_target("targetPermanent"),
                "ability": granted.rule,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(captures.get(1)?.as_str(), face_name)?,
                1,
                1,
            )],
        ));
    }
    let targeted_hand_disruption_re = Regex::new(
        r"(?i)^Target (player|opponent) reveals their hand\. You choose (?:a|an) (.+?) card from it\. That player discards that card\.$",
    )
    .expect("general targeted hand disruption regex compiles");
    if let Some(captures) = targeted_hand_disruption_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        return Some((
            vec![
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
                        "where": parse_permanent_criteria(&captures[2], face_name)?,
                    },
                }),
                json!({
                    "kind": "discardCards",
                    "player": target_player,
                    "cards": decision_result("discardedCard"),
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
    let paired_counters_and_subtype_re = Regex::new(&format!(
        r"(?i)^Put ({}) ([A-Za-z0-9+/-]+) counters? and (?:a|an) ([A-Za-z0-9+/-]+) counter on target (.+?)\. It becomes (?:a|an) ([A-Za-z][A-Za-z '-]+) in addition to its other types\.$",
        count_word_pattern(),
    ))
    .expect("paired counters and added subtype regex compiles");
    if let Some(captures) = paired_counters_and_subtype_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        return Some((
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": target.clone(),
                    "counter": captures[2].to_ascii_lowercase(),
                    "count": integer(parse_number_word(&captures[1])?),
                }),
                json!({
                    "kind": "putCounters",
                    "permanent": target.clone(),
                    "counter": captures[3].to_ascii_lowercase(),
                    "count": integer(1),
                }),
                json!({
                    "kind": "addSubtypeToPermanent",
                    "object": target,
                    "subtype": captures[5].trim(),
                    "duration": { "kind": "permanent" },
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": parse_permanent_criteria(&captures[4], face_name)?,
                }),
                1,
                1,
            )],
        ));
    }
    let source_counter_re = Regex::new(&format!(
        r"(?i)^Put (a|an|{}) ([A-Za-z0-9+/-]+) counters? on (.+?)\.$",
        count_word_pattern(),
    ))
    .expect("counter on source reference regex compiles");
    if let Some(captures) = source_counter_re.captures(instruction) {
        let subject = captures.get(3)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            let count = parse_number_word(captures.get(1)?.as_str()).unwrap_or(1);
            return Some((
                vec![json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": captures.get(2)?.as_str().to_ascii_lowercase(),
                    "count": integer(count),
                })],
                Vec::new(),
            ));
        }
    }
    let conditional_draw_for_opponent_colors_re = Regex::new(
        r"(?i)^Draw (?:a|one) card if an opponent has cast a (white|blue|black|red|green) or (white|blue|black|red|green) spell this turn\.$",
    )
    .expect("conditional draw for opponent spell colors regex compiles");
    if let Some(captures) = conditional_draw_for_opponent_colors_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": {
                    "kind": "opponentCastSpellWithAnyColor",
                    "colors": [captures[1].to_ascii_lowercase(), captures[2].to_ascii_lowercase()],
                },
                "then": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
                "else": [],
            })],
            Vec::new(),
        ));
    }
    let controlled_spells_uncounterable_re =
        Regex::new(r"(?i)^Spells you control can't be countered this turn\.$")
            .expect("temporary controlled-spell counter prohibition regex compiles");
    if controlled_spells_uncounterable_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "installTemporaryCantBeCountered",
                "player": controller(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    let player_and_permanents_hexproof_re = Regex::new(
        r"(?i)^You and permanents you control gain hexproof from (white|blue|black|red|green) and from (white|blue|black|red|green) until end of turn\.$",
    )
    .expect("temporary player and controlled-permanent hexproof regex compiles");
    if let Some(captures) = player_and_permanents_hexproof_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "grantTemporaryHexproofFrom",
                "player": controller(),
                "includePlayer": true,
                "includeControlledPermanents": true,
                "from": [captures[1].to_ascii_lowercase(), captures[2].to_ascii_lowercase()],
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    let reveal_until_chosen_name_re = Regex::new(&format!(
        r"(?i)^Choose a card name\. Reveal cards from the top of your library until you reveal a card with that name, then put that card into your hand\. Exile all other cards revealed this way, and you lose ({}) life for each of the exiled cards\.$",
        count_word_pattern(),
    ))
    .expect("reveal-until-chosen-name sequence regex compiles");
    if let Some(captures) = reveal_until_chosen_name_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "chooseCardName",
                    "id": "chosenCardName",
                    "player": controller(),
                    "where": Value::Null,
                }),
                json!({
                    "kind": "revealUntilChosenName",
                    "player": controller(),
                    "decisionId": "chosenCardName",
                    "matchingDestination": "hand",
                    "otherDestination": "exile",
                    "lifePerOtherCard": integer(parse_number_word(&captures[1])?),
                }),
            ],
            Vec::new(),
        ));
    }
    let optional_discard_hand_draw_re = Regex::new(&format!(
        r"(?i)^You may discard your hand\. If you do, draw ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("optional discard-hand then draw regex compiles");
    if let Some(captures) = optional_discard_hand_draw_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": { "kind": "discardHand", "player": controller() },
                "onPerformed": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(parse_number_word(&captures[1])?),
                }],
            })],
            Vec::new(),
        ));
    }
    let simultaneous_hand_choice_lowest_mana_re = Regex::new(
        r"(?i)^Each player chooses a card in their hand\. Then each player reveals their chosen card\. The owner of each (.+?) card revealed this way with the lowest mana value puts it onto the battlefield\.$",
    )
    .expect("simultaneous hand choice and lowest-mana movement regex compiles");
    if let Some(captures) = simultaneous_hand_choice_lowest_mana_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "eachPlayerChoosesHandCardThenMoveLowestManaValue",
                "players": { "kind": "eachPlayer" },
                "where": parse_permanent_criteria(&captures[1], face_name)?,
                "destination": "battlefield",
            })],
            Vec::new(),
        ));
    }
    let linked_exiled_card_variable_token_re = Regex::new(
        r"(?i)^The exiled card's owner creates an X/X (.+?) creature token, where X is the mana value of the exiled card\.$",
    )
    .expect("linked exiled-card variable token regex compiles");
    if let Some(captures) = linked_exiled_card_variable_token_re.captures(instruction) {
        let (colors, subtypes) = parse_color_prefix(captures.get(1)?.as_str())?;
        let linked_card = json!({ "kind": "cardExiledWithSource" });
        let mana_value = json!({
            "kind": "manaValueOf",
            "object": linked_card.clone(),
        });
        return Some((
            vec![json!({
                "kind": "createTokens",
                "controller": {
                    "kind": "ownerOf",
                    "object": linked_card,
                },
                "quantity": integer(1),
                "token": {
                    "colors": colors,
                    "types": ["Creature"],
                    "subtypes": subtypes,
                    "power": mana_value.clone(),
                    "toughness": mana_value,
                    "abilities": [],
                },
            })],
            Vec::new(),
        ));
    }
    let colors_spent_exile_re = Regex::new(
        r"(?i)^(?:Converge\s*(?:Ã¢â‚¬â€|â€”|—|-)?\s*)?Exile target (.+?) if its mana value is less than or equal to the number of colors of mana spent to cast this spell\.$",
    )
    .expect("colors-spent conditional exile regex compiles");
    if let Some(captures) = colors_spent_exile_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "conditional",
                "condition": compare(
                    "<=",
                    json!({
                        "kind": "manaValueOf",
                        "object": chosen_target("targetPermanent"),
                    }),
                    json!({ "kind": "colorsOfManaSpentToCastSource" }),
                ),
                "then": [{
                    "kind": "exilePermanent",
                    "permanent": chosen_target("targetPermanent"),
                }],
                "else": [],
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    let each_opponent_cost_or_controller_effect_re = Regex::new(
        r"(?i)^For each opponent, you (create .+?) unless that player (sacrifices|discards) (.+)\.$",
    )
    .expect("each-opponent cost-or-controller-effect regex compiles");
    if let Some(captures) = each_opponent_cost_or_controller_effect_re.captures(instruction) {
        let cost_verb = if captures[2].eq_ignore_ascii_case("sacrifices") {
            "sacrifice"
        } else {
            "discard"
        };
        let cost_text = format!("{cost_verb} {}", &captures[3]);
        let create_text = format!("Create {}.", captures[1].trim_start_matches("create "));
        return Some((
            vec![json!({
                "kind": "forEachOpponentPaysCostOrControllerEffect",
                "cost": parse_resolution_cost_text(&cost_text)?,
                "otherwise": [create_token_effect(&create_text)?],
            })],
            Vec::new(),
        ));
    }
    let targeted_sacrifice_re = Regex::new(&format!(
        r"(?i)^Target (player|opponent) sacrifices ({}) (.+?) of their choice\.$",
        count_word_pattern(),
    ))
    .expect("targeted sacrifice instruction regex compiles");
    if let Some(captures) = targeted_sacrifice_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        return Some((
            vec![json!({
                "kind": "sacrificePermanents",
                "player": target_player,
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(3)?.as_str()),
                    face_name,
                )?,
                "count": integer(parse_number_word(captures.get(2)?.as_str())?),
                "excludeSource": false,
            })],
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
    let targeted_sacrifice_discard_life_re = Regex::new(&format!(
        r"(?i)^Target (player|opponent) sacrifices ({}) (.+?) of their choice, discards ({}) cards?, (?:and|then) loses ({}) life\.$",
        count_word_pattern(),
        count_word_pattern(),
        numeric_expression_pattern(),
    ))
    .expect("targeted sacrifice-discard-life sequence regex compiles");
    if let Some(captures) = targeted_sacrifice_discard_life_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        let life_amount = parse_numeric_expression_text(&captures[5])?;
        let mut decisions = Vec::new();
        if contains_rule_kind(&life_amount, "decisionResult") {
            decisions.push(x_value());
        }
        decisions.push(target_decision(
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
        ));
        return Some((
            vec![
                json!({
                    "kind": "sacrificePermanents",
                    "player": target_player.clone(),
                    "where": parse_permanent_criteria(
                        &singular_card_term(&captures[3]),
                        face_name,
                    )?,
                    "count": integer(parse_number_word(&captures[2])?),
                    "excludeSource": false,
                }),
                json!({
                    "kind": "discardCards",
                    "player": target_player.clone(),
                    "count": integer(parse_number_word(&captures[4])?),
                }),
                json!({
                    "kind": "loseLife",
                    "player": target_player,
                    "amount": life_amount,
                }),
            ],
            decisions,
        ));
    }
    let each_player_unless_cost_re = Regex::new(&format!(
        r"(?i)^Each player loses ({}) life unless they (.+)\.$",
        count_word_pattern(),
    ))
    .expect("each-player unless-cost life-loss regex compiles");
    if let Some(captures) = each_player_unless_cost_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "eachPlayerPaysCostOrLosesLife",
                "cost": parse_resolution_cost_text(&captures[2])?,
                "amount": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }
    let discard_then_sacrifice_list_re = Regex::new(&format!(
        r"(?i)^Discard (a|an|{}) cards? and sacrifice (.+)\.$",
        count_word_pattern(),
    ))
    .expect("discard then sacrifice-list regex compiles");
    if let Some(captures) = discard_then_sacrifice_list_re.captures(instruction) {
        let discard_count = if matches!(captures[1].to_ascii_lowercase().as_str(), "a" | "an") {
            1
        } else {
            parse_number_word(&captures[1])?
        };
        let mut effects = vec![json!({
            "kind": "discardCards",
            "player": controller(),
            "count": integer(discard_count),
        })];
        let list_separator_re = Regex::new(r"(?i)\s*,\s*(?:and\s+)?|\s+and\s+")
            .expect("sacrifice-list separator regex compiles");
        let sacrifice_effects = list_separator_re
            .split(&captures[2])
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .map(|term| {
                Some(json!({
                    "kind": "sacrificePermanents",
                    "player": controller(),
                    "where": parse_permanent_criteria(term, face_name)?,
                    "count": integer(1),
                    "excludeSource": false,
                }))
            })
            .collect::<Option<Vec<_>>>()?;
        if sacrifice_effects.len() < 2 {
            return None;
        }
        effects.extend(sacrifice_effects);
        return Some((effects, Vec::new()));
    }
    let pay_cost_or_move_source_re = Regex::new(
        r"(?i)^(Sacrifice|Destroy) this (?:artifact|creature|enchantment|land|permanent) unless you pay (.+)\.$",
    )
    .expect("pay-cost or move-source regex compiles");
    if let Some(captures) = pay_cost_or_move_source_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "payCostOrMoveSource",
                "cost": parse_resolution_cost_text(&captures[2])?,
                "otherwise": if captures[1].eq_ignore_ascii_case("destroy") {
                    "destroy"
                } else {
                    "sacrifice"
                },
            })],
            Vec::new(),
        ));
    }
    let cost_or_sacrifice_named_source_re =
        Regex::new(r"(?i)^Sacrifice (.+?) unless you (sacrifice .+?)\.$")
            .expect("resolution cost or sacrifice named source regex compiles");
    if let Some(captures) = cost_or_sacrifice_named_source_re.captures(instruction)
        && (source_reference_matches(&captures[1], face_name)
            || captures[1].to_ascii_lowercase().starts_with("this "))
    {
        return Some((
            vec![json!({
                "kind": "payCostOrMoveSource",
                "cost": parse_resolution_cost_text(&captures[2])?,
                "otherwise": "sacrifice",
            })],
            Vec::new(),
        ));
    }
    let target_cant_attack_until_next_turn_re =
        Regex::new(r"(?i)^Target (.+?) can't attack until your next turn\.$")
            .expect("target attack restriction until next turn regex compiles");
    if let Some(captures) = target_cant_attack_until_next_turn_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "grantKeyword",
                "object": chosen_target("targetPermanent"),
                "keyword": "cantAttack",
                "duration": { "kind": "untilNextTurn", "player": controller() },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    let target_stats_until_next_turn_re =
        Regex::new(r"(?i)^Target (.+?) gets ([+-]\d+)/([+-]\d+) until your next turn\.$")
            .expect("target stats modification until next turn regex compiles");
    if let Some(captures) = target_stats_until_next_turn_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": chosen_target("targetPermanent"),
                "power": integer(captures[2].parse::<i64>().ok()?),
                "toughness": integer(captures[3].parse::<i64>().ok()?),
                "duration": { "kind": "untilNextTurn", "player": controller() },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    let hand_to_battlefield_re = Regex::new(
        r"(?i)^(You may )?put (?:a|an) (.+?) from your hand onto the battlefield( tapped)?\.$",
    )
    .expect("generic hand-to-battlefield instruction regex compiles");
    if let Some(captures) = hand_to_battlefield_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("handCard"),
                "to": "battlefield",
                "controller": controller(),
                "tapped": captures.get(3).is_some(),
            })],
            vec![target_decision(
                "handCard",
                json!({
                    "kind": "cards",
                    "zone": hand(controller()),
                    "where": parse_permanent_criteria(&captures[2], face_name)?,
                }),
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }
    let any_hand_cards_to_battlefield_re =
        Regex::new(r"(?i)^Put any number of (.+?) cards from your hand onto the battlefield\.$")
            .expect("any-number hand cards to battlefield regex compiles");
    if let Some(captures) = any_hand_cards_to_battlefield_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "putCardsFromHandOntoBattlefield",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            })],
            Vec::new(),
        ));
    }
    let targeted_hand_exile_re = Regex::new(
        r"(?i)^Target (player|opponent) reveals their hand\. You choose a (.+?) card from it and exile that card\.$",
    )
    .expect("targeted hand reveal and exile regex compiles");
    if let Some(captures) = targeted_hand_exile_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        let player_candidates = if captures[1].eq_ignore_ascii_case("opponent") {
            json!({
                "kind": "players",
                "where": { "kind": "isOpponentOf", "player": controller() },
            })
        } else {
            json!({ "kind": "players" })
        };
        return Some((
            vec![
                json!({
                    "kind": "revealHand",
                    "player": target_player.clone(),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "exiledHandCard",
                    "player": controller(),
                    "minimum": integer(1),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "cards",
                        "zone": hand(target_player.clone()),
                        "where": parse_permanent_criteria(&captures[2], face_name)?,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("exiledHandCard"),
                    "to": { "kind": "exile", "player": target_player, "faceDown": false },
                }),
            ],
            vec![target_decision("targetPlayer", player_candidates, 1, 1)],
        ));
    }
    let player_exiles_graveyard_card_re =
        Regex::new(r"(?i)^Target (player|opponent) exiles (?:a|one) card from their graveyard\.$")
            .expect("target-player graveyard choice regex compiles");
    if let Some(captures) = player_exiles_graveyard_card_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "chosenGraveyardCard",
                    "player": target_player.clone(),
                    "minimum": integer(1),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "cards",
                        "zone": graveyard(target_player.clone()),
                        "where": Value::Null,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("chosenGraveyardCard"),
                    "to": { "kind": "exile", "player": target_player, "faceDown": false },
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
    if let Some(optional_instruction) = instruction
        .strip_prefix("You may ")
        .or_else(|| instruction.strip_prefix("you may "))
    {
        let normalized = format!("{}.", optional_instruction.trim().trim_end_matches('.'));
        if let Some((effects, decisions)) = parse_general_effect_instruction(&normalized, face_name)
            && decisions.is_empty()
        {
            if optional_instruction
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("search your library")
            {
                return Some((effects, Vec::new()));
            }
            return Some((
                vec![json!({
                    "kind": "optionalEffects",
                    "player": controller(),
                    "effects": effects,
                })],
                Vec::new(),
            ));
        }
    }
    if instruction.eq_ignore_ascii_case("Take an extra turn after this one.") {
        return Some((
            vec![json!({
                "kind": "grantExtraTurn",
                "player": controller(),
            })],
            Vec::new(),
        ));
    }
    let target_extra_turn_re =
        Regex::new(r"(?i)^Target (player|opponent) takes an extra turn after this one\.$")
            .expect("target-player extra-turn regex compiles");
    if let Some(captures) = target_extra_turn_re.captures(instruction) {
        let opponent_only = captures[1].eq_ignore_ascii_case("opponent");
        return Some((
            vec![json!({
                "kind": "grantExtraTurn",
                "player": chosen_target("targetPlayer"),
            })],
            vec![target_decision(
                "targetPlayer",
                if opponent_only {
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
    if instruction.eq_ignore_ascii_case("Its owner shuffles their graveyard into their library.") {
        return Some((
            vec![json!({
                "kind": "shuffleGraveyardIntoLibrary",
                "player": controller(),
            })],
            Vec::new(),
        ));
    }
    let source_owner_shuffle_draw_re = Regex::new(&format!(
        r"(?i)^(.+?)(?:'s|’s) owner shuffles (?:it|him|her|them) into their library and draws ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("source-owner shuffle and draw instruction regex compiles");
    if let Some(captures) = source_owner_shuffle_draw_re.captures(instruction) {
        let subject = captures.get(1)?.as_str().trim();
        let lower_subject = subject.to_ascii_lowercase();
        let inferred_named_source = face_name.is_empty()
            && subject.chars().next().is_some_and(char::is_uppercase)
            && subject
                .chars()
                .all(|character| character.is_alphanumeric() || " ,'-".contains(character))
            && !lower_subject.contains("target ")
            && !["another ", "enchanted ", "equipped ", "that "]
                .iter()
                .any(|prefix| lower_subject.starts_with(prefix));
        if source_reference_matches(subject, face_name) || inferred_named_source {
            return Some((
                vec![json!({
                    "kind": "shufflePermanentIntoOwnersLibraryThenDraw",
                    "permanent": self_ref(),
                    "count": integer(parse_number_word(&captures[2])?),
                })],
                Vec::new(),
            ));
        }
    }
    let each_player_put_re = Regex::new(
        r"(?i)^Each player may put (?:an? )?(.+?) card from their hand onto the battlefield\.$",
    )
    .expect("each-player hand-to-battlefield regex compiles");
    if let Some(captures) = each_player_put_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "eachPlayerMayPutCardFromHand",
                "where": card_qualifier_list_filter(&captures[1], face_name)
                    .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
            })],
            Vec::new(),
        ));
    }
    let linked_hand_exile_re = Regex::new(r"(?i)^You may exile (?:an? )?(.+?) from your hand\.$")
        .expect("optional linked hand exile regex compiles");
    if let Some(captures) = linked_hand_exile_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "exileTargetCardWithSource",
                "card": chosen_target("handCardToExile"),
                "source": self_ref(),
                "fromZone": "hand",
            })],
            vec![target_decision(
                "handCardToExile",
                json!({
                    "kind": "cards",
                    "zone": hand(controller()),
                    "where": parse_permanent_criteria(&captures[1], face_name)?,
                }),
                0,
                1,
            )],
        ));
    }
    let source_x_counters_then_half_draw_re = Regex::new(
        r"(?i)^Put X ([A-Za-z0-9+/-]+) counters? on (?:him|her|it|this creature|this permanent)\. Then draw half X cards, rounded down\.$",
    )
    .expect("source X counters followed by half-X draw regex compiles");
    if let Some(captures) = source_x_counters_then_half_draw_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": captures[1].to_string(),
                    "count": { "kind": "sourceCastXValue" },
                }),
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": {
                        "kind": "divide",
                        "left": { "kind": "sourceCastXValue" },
                        "right": integer(2),
                        "round": "down",
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    let distinct_land_names_re = Regex::new(&format!(
        r"(?i)^If you control ({}) or more lands with different names, (.+)$",
        count_word_pattern(),
    ))
    .expect("distinct controlled land names conditional regex compiles");
    if let Some(captures) = distinct_land_names_re.captures(instruction) {
        let (effects, decisions) = parse_general_effect_instruction(&captures[2], face_name)?;
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countDistinctPermanentNames",
                        "player": controller(),
                        "where": card_type("Land"),
                    }),
                    integer(parse_number_word(&captures[1])?),
                ),
                "then": effects,
                "else": [],
            })],
            decisions,
        ));
    }
    let half_token_count_re = Regex::new(
        r"(?i)^(Create X .+? tokens?), where X is half the number of (.+?) you control, rounded down\.$",
    )
    .expect("half controlled count token regex compiles");
    if let Some(captures) = half_token_count_re.captures(instruction) {
        let mut effect = create_token_effect(&format!("{}.", &captures[1]))?;
        effect["quantity"] = json!({
            "kind": "divide",
            "left": {
                "kind": "countPermanents",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[2], face_name)?,
            },
            "right": integer(2),
            "round": "down",
        });
        return Some((vec![effect], Vec::new()));
    }
    let source_subject = if face_name.is_empty() {
        r"(?:This creature|This permanent)".to_string()
    } else {
        format!(
            r"(?:This creature|This permanent|{})",
            regex::escape(face_name),
        )
    };
    let source_damage_and_draw_re = Regex::new(&format!(
        r"(?i)^{} deals ({}) damage to you and you draw a card\.$",
        source_subject,
        count_word_pattern(),
    ))
    .expect("source damage controller and draw regex compiles");
    if let Some(captures) = source_damage_and_draw_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(parse_number_word(&captures[1])?),
                    "recipient": controller(),
                }),
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }),
            ],
            Vec::new(),
        ));
    }
    let devotion_drain_re = Regex::new(
        r"(?i)^Each opponent loses X life, where X is your devotion to (white|blue|black|red|green)\. You gain life equal to the life lost this way\.$",
    )
    .expect("devotion drain regex compiles");
    if let Some(captures) = devotion_drain_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drainEachOpponent",
                "amount": {
                    "kind": "devotion",
                    "player": controller(),
                    "color": captures[1].to_ascii_lowercase(),
                },
            })],
            Vec::new(),
        ));
    }
    let opponent_loss_and_scry_re = Regex::new(
        r"(?i)^Each opponent loses X life and you scry X, where X is the number of (.+?) you control\.$",
    )
    .expect("opponent loss and variable scry regex compiles");
    if let Some(captures) = opponent_loss_and_scry_re.captures(instruction) {
        let amount = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[1], face_name)?,
        });
        return Some((
            vec![
                json!({ "kind": "loseLifeEachOpponent", "amount": amount.clone() }),
                json!({ "kind": "scry", "player": controller(), "count": amount }),
            ],
            Vec::new(),
        ));
    }
    let return_graveyard_then_destroy_re = Regex::new(
        r"(?i)^Return all (.+?) cards from your graveyard to the battlefield( tapped)?, then destroy all (.+?)\.$",
    )
    .expect("graveyard return then destroy regex compiles");
    if let Some(captures) = return_graveyard_then_destroy_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "cardsInZone",
                        "zone": graveyard(controller()),
                        "where": card_qualifier_list_filter(&captures[1], face_name)
                            .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
                    },
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": captures.get(2).is_some(),
                    },
                }),
                json!({
                    "kind": "destroyPermanent",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": parse_permanent_criteria(&captures[3], face_name)?,
                    },
                }),
            ],
            Vec::new(),
        ));
    }
    if instruction.eq_ignore_ascii_case("Create a token that's a copy of that creature.") {
        return Some((
            vec![json!({
                "kind": "createTokenCopyOfPermanent",
                "object": { "kind": "triggeringPermanent" },
                "grantKeywords": [],
                "exileAtNextEndStep": false,
            })],
            Vec::new(),
        ));
    }
    let opponent_land_advantage_re =
        Regex::new(r"(?i)^If an opponent controls more lands than you, (.+)$")
            .expect("opponent land advantage conditional regex compiles");
    if let Some(captures) = opponent_land_advantage_re.captures(instruction) {
        let (effects, decisions) = parse_general_effect_instruction(&captures[1], face_name)?;
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": parse_controlled_permanent_condition(
                    "an opponent controls more lands than you",
                    face_name,
                )?,
                "then": effects,
                "else": [],
            })],
            decisions,
        ));
    }
    let draw_to_hand_size_re = Regex::new(&format!(
        r"(?i)^If you have fewer than ({}) cards in hand, draw cards equal to the difference\.$",
        count_word_pattern(),
    ))
    .expect("draw to hand size regex compiles");
    if let Some(captures) = draw_to_hand_size_re.captures(instruction) {
        let target_size = parse_number_word(&captures[1])?;
        let hand_count = json!({
            "kind": "countCards",
            "zone": { "kind": "hand", "player": controller() },
            "where": Value::Null,
        });
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": compare("<", hand_count.clone(), integer(target_size)),
                "then": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": {
                        "kind": "subtract",
                        "left": integer(target_size),
                        "right": hand_count,
                    },
                }],
                "else": [],
            })],
            Vec::new(),
        ));
    }
    if instruction.eq_ignore_ascii_case("Each player draws a card, then discards a card.") {
        return Some((
            vec![json!({ "kind": "drawEachPlayerThenDiscard", "count": integer(1) })],
            Vec::new(),
        ));
    }
    if instruction
        .eq_ignore_ascii_case("That player discards a card and you untap all lands you control.")
    {
        return Some((
            vec![
                json!({
                    "kind": "discardCards",
                    "player": { "kind": "triggeringPlayer" },
                    "count": integer(1),
                }),
                json!({
                    "kind": "untapPermanentsMatching",
                    "player": controller(),
                    "where": card_type("Land"),
                }),
            ],
            Vec::new(),
        ));
    }
    if let Some(criteria) = instruction
        .strip_prefix("Destroy target ")
        .and_then(|text| text.strip_suffix(". A creature destroyed this way can't be regenerated."))
    {
        return Some((
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": chosen_target("targetPermanent"),
                "cannotRegenerate": true,
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(criteria, face_name)?,
                1,
                1,
            )],
        ));
    }
    let global_zone_change_re =
        Regex::new(r"(?i)^(Destroy|Exile) all (.+?)( target player controls)?\.$")
            .expect("global permanent zone-change regex compiles");
    if let Some(captures) = global_zone_change_re.captures(instruction)
        && !captures[2].eq_ignore_ascii_case("graveyards")
    {
        let where_filter = card_qualifier_list_filter(&captures[2], face_name)
            .or_else(|| parse_permanent_criteria(&captures[2], face_name))?;
        let targets_player = captures.get(3).is_some();
        let mut permanent = json!({
            "kind": "eachPermanent",
            "where": where_filter,
        });
        if targets_player {
            permanent["player"] = chosen_target("targetPlayer");
        }
        let mut effect = json!({
            "kind": if captures[1].eq_ignore_ascii_case("destroy") {
                "destroyPermanent"
            } else {
                "exilePermanent"
            },
            "permanent": permanent,
        });
        if targets_player && captures[1].eq_ignore_ascii_case("exile") {
            effect["bind"] = Value::String("exiledPermanents".to_string());
        }
        return Some((
            vec![effect],
            targets_player
                .then(|| target_decision("targetPlayer", json!({ "kind": "players" }), 1, 1))
                .into_iter()
                .collect(),
        ));
    }
    let exchange_control_re =
        Regex::new(r"(?i)^Exchange control of two target (.+?) that share a card type\.$")
            .expect("shared-card-type control exchange regex compiles");
    if let Some(captures) = exchange_control_re.captures(instruction) {
        let mut decision = target_decision(
            "targetPermanents",
            permanent_target_candidates(&singular_card_term(captures.get(1)?.as_str()), face_name)?,
            2,
            2,
        );
        decision["selectionConstraint"] = json!({ "kind": "shareCardType" });
        return Some((
            vec![json!({
                "kind": "exchangeControl",
                "objects": { "kind": "chosenTargets", "id": "targetPermanents" },
            })],
            vec![decision],
        ));
    }
    if instruction.eq_ignore_ascii_case("Exile all graveyards.") {
        return Some((vec![json!({ "kind": "exileAllGraveyards" })], Vec::new()));
    }
    let destroy_then_controller_token_re =
        Regex::new(r"(?i)^Destroy target (.+?)\. Its controller creates (.+?)\.$")
            .expect("destroy then controller token regex compiles");
    if let Some(captures) = destroy_then_controller_token_re.captures(instruction) {
        let mut token_effect = create_token_effect(&format!("Create {}.", &captures[2]))?;
        token_effect["controller"] = json!({
            "kind": "boundValue",
            "id": "destroyedPermanentController",
        });
        return Some((
            vec![
                json!({
                    "kind": "bind",
                    "id": "destroyedPermanentController",
                    "value": {
                        "kind": "controllerOf",
                        "object": chosen_target("targetPermanent"),
                    },
                }),
                json!({
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetPermanent"),
                }),
                token_effect,
            ],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    let destroy_then_mana_value_loss_re = Regex::new(
        r"(?i)^Destroy target (.+?)\. You lose life equal to that permanent's mana value\.$",
    )
    .expect("destroy then mana-value life loss regex compiles");
    if let Some(captures) = destroy_then_mana_value_loss_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "bind",
                    "id": "destroyedManaValue",
                    "value": {
                        "kind": "manaValueOf",
                        "object": chosen_target("targetPermanent"),
                    },
                }),
                json!({
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetPermanent"),
                }),
                json!({
                    "kind": "loseLife",
                    "player": controller(),
                    "amount": { "kind": "boundValue", "id": "destroyedManaValue" },
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    if instruction.eq_ignore_ascii_case("Exile target player's graveyard.") {
        return Some((
            vec![json!({
                "kind": "exilePlayerGraveyard",
                "player": chosen_target("targetPlayer"),
            })],
            vec![target_decision(
                "targetPlayer",
                json!({ "kind": "players" }),
                1,
                1,
            )],
        ));
    }
    if instruction.eq_ignore_ascii_case("Exile each opponent's graveyard.") {
        return Some((
            vec![json!({
                "kind": "exileAllGraveyards",
                "exceptPlayer": controller(),
            })],
            Vec::new(),
        ));
    }
    let exile_graveyard_card_re = Regex::new(
        r"(?i)^Exile (up to one )?target (?:(.+?) )?card from (a|any|your|an opponent's) graveyard\.$",
    )
    .expect("qualified graveyard target exile regex compiles");
    if let Some(captures) = exile_graveyard_card_re.captures(instruction) {
        let where_filter = captures
            .get(2)
            .map(|criteria| parse_permanent_criteria(criteria.as_str(), face_name))
            .unwrap_or(Some(Value::Null))?;
        let zone = match captures.get(3)?.as_str().to_ascii_lowercase().as_str() {
            "a" | "any" => json!({ "kind": "anyGraveyard" }),
            "your" => graveyard(controller()),
            "an opponent's" => json!({
                "kind": "graveyard",
                "player": { "kind": "opponentsOf", "player": controller() },
            }),
            _ => return None,
        };
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetGraveyardCard"),
                "to": "exile",
                "tapped": false,
            })],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": zone,
                    "where": where_filter,
                }),
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }
    let return_graveyard_targets_re = Regex::new(&format!(
        r"(?i)^Return (up to )?({}) target (.+?) cards? from your graveyard to your hand\.$",
        count_word_pattern(),
    ))
    .expect("multiple graveyard targets to hand regex compiles");
    if let Some(captures) = return_graveyard_targets_re.captures(instruction) {
        let maximum = parse_number_word(&captures[2])?;
        return Some((
            vec![json!({
                "kind": "moveCards",
                "cards": { "kind": "chosenTargets", "id": "targetGraveyardCards" },
                "to": hand(controller()),
            })],
            vec![target_decision(
                "targetGraveyardCards",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": parse_permanent_criteria(&captures[3], face_name)?,
                }),
                if captures.get(1).is_some() {
                    0
                } else {
                    maximum
                },
                maximum,
            )],
        ));
    }
    let split_library_search_re = Regex::new(
        r"(?i)^Search your library for two cards\. Put one into your (hand|graveyard) and the other into your (hand|graveyard)\. Then shuffle\.$",
    )
    .expect("split-destination library search regex compiles");
    if let Some(captures) = split_library_search_re.captures(instruction)
        && !captures[1].eq_ignore_ascii_case(&captures[2])
    {
        let destinations = [&captures[1], &captures[2]];
        let mut effects = Vec::new();
        for (index, destination) in destinations.into_iter().enumerate() {
            let decision_id = format!("splitSearchCard{}", index + 1);
            effects.push(json!({
                "kind": "chooseCards",
                "id": decision_id.clone(),
                "player": controller(),
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "cards",
                    "zone": library(controller()),
                    "where": Value::Null,
                },
            }));
            effects.push(json!({
                "kind": "moveCards",
                "cards": decision_result(&decision_id),
                "to": {
                    "kind": destination.to_ascii_lowercase(),
                    "player": controller(),
                },
            }));
        }
        effects.push(json!({
            "kind": "shuffleZone",
            "zone": library(controller()),
        }));
        return Some((effects, Vec::new()));
    }
    if instruction.eq_ignore_ascii_case("Venture into the dungeon.") {
        return Some((
            vec![json!({
                "kind": "ventureDungeon",
                "player": controller(),
            })],
            Vec::new(),
        ));
    }
    let described_artifact_token_re = Regex::new(
        r#"(?i)^Create (?:a|an) ([A-Za-z0-9 ',.-]+) token\. \(It's (.+?) artifact with \"\{T\}: Add (\{[WUBRGC]\}(?: or \{[WUBRGC]\})+)\.\"\)$"#,
    )
    .expect("described artifact mana-token regex compiles");
    if let Some(captures) = described_artifact_token_re.captures(instruction.trim()) {
        let colors = ["white", "blue", "black", "red", "green"]
            .into_iter()
            .filter(|color| captures[2].to_ascii_lowercase().contains(color))
            .collect::<Vec<_>>();
        let options = captures[3]
            .split(" or ")
            .map(str::to_string)
            .collect::<Vec<_>>();
        return Some((
            vec![json!({
                "kind": "createTokens", "controller": controller(), "quantity": integer(1),
                "token": {
                    "name": captures[1].trim(), "colors": colors, "types": ["Artifact"], "subtypes": [], "power": 0, "toughness": 0,
                    "abilities": [{
                        "kind": "activatedAbility", "source": self_ref(),
                        "costs": [{ "kind": "tap", "object": self_ref() }],
                        "effects": [{ "kind": "addMana", "player": controller(), "mana": { "kind": "chooseOne", "options": options } }],
                    }],
                },
            })],
            Vec::new(),
        ));
    }
    let normalized;
    let instruction = if let Some((main, _)) = instruction.split_once(". (") {
        normalized = format!("{main}.");
        normalized.as_str()
    } else {
        instruction.trim()
    };

    let energy_re = Regex::new(r"(?i)^You get ((?:\{E\})+)(?: \([^)]*\))?\.$")
        .expect("energy-counter instruction regex compiles");
    if let Some(captures) = energy_re.captures(instruction) {
        let count = captures[1].matches("{E}").count() as i64;
        return Some((
            vec![json!({
                "kind": "addPlayerCounters",
                "player": controller(),
                "counter": "energy",
                "count": integer(count),
            })],
            Vec::new(),
        ));
    }

    let attached_tap_and_clear_counters_re = Regex::new(
        r"(?i)^(Tap|Untap) enchanted (creature|permanent) and remove all counters from it\.$",
    )
    .expect("attached permanent tap and clear-counters regex compiles");
    if let Some(captures) = attached_tap_and_clear_counters_re.captures(instruction) {
        let permanent = json!({
            "kind": "attachedPermanent",
            "attachment": self_ref(),
        });
        let tap_kind = if captures[1].eq_ignore_ascii_case("tap") {
            "tapPermanent"
        } else {
            "untapPermanent"
        };
        return Some((
            vec![
                json!({ "kind": tap_kind, "permanent": permanent.clone() }),
                json!({ "kind": "removeAllCounters", "permanent": permanent }),
            ],
            Vec::new(),
        ));
    }

    let attached_tap_re = Regex::new(r"(?i)^(Tap|Untap) enchanted (creature|permanent)\.$")
        .expect("attached permanent tap regex compiles");
    if let Some(captures) = attached_tap_re.captures(instruction) {
        let kind = if captures[1].eq_ignore_ascii_case("tap") {
            "tapPermanent"
        } else {
            "untapPermanent"
        };
        return Some((
            vec![json!({
                "kind": kind,
                "permanent": {
                    "kind": "attachedPermanent",
                    "attachment": self_ref(),
                },
            })],
            Vec::new(),
        ));
    }

    if Regex::new(r"(?i)^Sacrifice (?:this (?:creature|permanent|artifact|enchantment|land)|it)\.$")
        .expect("sacrifice source instruction regex compiles")
        .is_match(instruction)
    {
        return Some((
            vec![json!({
                "kind": "sacrificePermanent",
                "permanent": self_ref(),
            })],
            Vec::new(),
        ));
    }

    let becomes_prepared_re =
        Regex::new(r"(?i)^(?:This creature|This permanent|It|[A-Z][^.,]+) becomes prepared\.$")
            .expect("becomes-prepared instruction regex compiles");
    if becomes_prepared_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "setPrepared",
                "object": self_ref(),
                "value": true,
            })],
            Vec::new(),
        ));
    }

    let target_player_draw_re = Regex::new(&format!(
        r"(?i)^Target (player|opponent) draws ({}) cards?(?: and loses ({}) life)?\.$",
        numeric_expression_pattern(),
        count_word_pattern(),
    ))
    .expect("target-player draw instruction regex compiles");
    if let Some(captures) = target_player_draw_re.captures(instruction) {
        let count = parse_numeric_expression_text(&captures[2])?;
        let variable = contains_rule_kind(&count, "decisionResult");
        let mut effects = vec![json!({
            "kind": "drawCards",
            "player": chosen_target("targetPlayer"),
            "count": count,
        })];
        if let Some(life) = captures.get(3) {
            effects.push(json!({
                "kind": "loseLife",
                "player": chosen_target("targetPlayer"),
                "amount": integer(parse_number_word(life.as_str())?),
            }));
        }
        let mut decisions = Vec::new();
        if variable {
            decisions.push(x_value());
        }
        decisions.push(target_decision(
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
        ));
        return Some((effects, decisions));
    }

    let target_player_life_re = Regex::new(&format!(
        r"(?i)^Target player gains ({}) life\.$",
        numeric_expression_pattern(),
    ))
    .expect("target-player life instruction regex compiles");
    if let Some(captures) = target_player_life_re.captures(instruction) {
        let amount = parse_numeric_expression_text(&captures[1])?;
        let variable = contains_rule_kind(&amount, "decisionResult");
        let mut decisions = Vec::new();
        if variable {
            decisions.push(x_value());
        }
        decisions.push(target_decision(
            "targetPlayer",
            json!({ "kind": "players" }),
            1,
            1,
        ));
        return Some((
            vec![json!({
                "kind": "gainLife",
                "player": chosen_target("targetPlayer"),
                "amount": amount,
            })],
            decisions,
        ));
    }

    let draw_and_gain_re = Regex::new(&format!(
        r"(?i)^You draw ({}) cards? and gain ({}) life\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("draw-and-gain instruction regex compiles");
    if let Some(captures) = draw_and_gain_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(parse_number_word(&captures[1])?),
                }),
                json!({
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(parse_number_word(&captures[2])?),
                }),
            ],
            Vec::new(),
        ));
    }

    let target_protection_then_explore_re = Regex::new(
        r"(?i)^Target (.+?) gains protection from (.+?) until end of turn\. (?:It|That (?:creature|permanent)) explores\.$",
    )
    .expect("target protection followed by explore regex compiles");
    if let Some(captures) = target_protection_then_explore_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        return Some((
            vec![
                json!({
                    "kind": "grantProtection",
                    "object": target.clone(),
                    "from": parse_protection_qualities(&captures[2])?,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "explore",
                    "object": target,
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }

    let optional_damage_mill_then_hand_re = Regex::new(
        r"(?i)^You may mill that many cards\. If you do, you may put (?:a|an) (.+?) card from among them into your hand\.$",
    )
    .expect("optional damage-count mill followed by a hand selection regex compiles");
    if let Some(captures) = optional_damage_mill_then_hand_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "mill",
                    "player": controller(),
                    "count": { "kind": "decisionResult", "decisionId": "damageAmount" },
                    "bind": "milledCards",
                },
                "onPerformed": [
                    {
                        "kind": "chooseCards",
                        "id": "milledCardForHand",
                        "player": controller(),
                        "from": bound_objects("milledCards"),
                        "where": parse_permanent_criteria(&captures[1], face_name)?,
                        "minimum": integer(0),
                        "maximum": integer(1),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("milledCardForHand"),
                        "to": hand(controller()),
                    },
                ],
            })],
            Vec::new(),
        ));
    }

    let self_mill_re = Regex::new(&format!(r"(?i)^Mill ({}) cards?\.$", count_word_pattern(),))
        .expect("self-mill instruction regex compiles");
    if let Some(captures) = self_mill_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "mill",
                "player": controller(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let each_opponent_mill_re = Regex::new(&format!(
        r"(?i)^Each opponent mills ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("each-opponent mill instruction regex compiles");
    if let Some(captures) = each_opponent_mill_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "millEachPlayer",
                "players": {
                    "kind": "opponentsOf",
                    "player": controller(),
                },
                "count": integer(parse_number_word(captures.get(1)?.as_str())?),
            })],
            Vec::new(),
        ));
    }

    let each_player_loses_re = Regex::new(&format!(
        r"(?i)^Each player loses ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("each-player life-loss regex compiles");
    if let Some(captures) = each_player_loses_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "loseLifeEachPlayer",
                "amount": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let each_player_draws_re = Regex::new(&format!(
        r"(?i)^Each player draws ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("each-player draw regex compiles");
    if let Some(captures) = each_player_draws_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drawEachPlayer",
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let each_player_hand_and_graveyard_shuffle_draw_re = Regex::new(&format!(
        r"(?i)^Each player shuffles their hand and graveyard into their library, then draws ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("each-player hand-and-graveyard shuffle-draw regex compiles");
    if let Some(captures) = each_player_hand_and_graveyard_shuffle_draw_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "shuffleHandAndGraveyardIntoLibraryEachPlayerThenDraw",
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let each_opponent_draws_re = Regex::new(&format!(
        r"(?i)^Each opponent draws ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("each-opponent draw regex compiles");
    if let Some(captures) = each_opponent_draws_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drawEachOpponent",
                "player": controller(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }
    let each_opponent_then_controller_draw_re = Regex::new(
        r"(?i)^Each opponent draws a card, then you draw a card for each opponent who drew a card this way\.$",
    )
    .expect("opponents then controller draw regex compiles");
    if each_opponent_then_controller_draw_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "drawEachOpponentThenControllerForEach",
                "player": controller(),
            })],
            Vec::new(),
        ));
    }

    let discard_hand_draw_re = Regex::new(&format!(
        r"(?i)^Each player discards their hand, then draws ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("discard-hands-then-draw regex compiles");
    if let Some(captures) = discard_hand_draw_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "discardHandThenDrawEachPlayer",
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    if instruction.eq_ignore_ascii_case(
        "Each player discards their hand, then draws cards equal to the greatest number of cards a player discarded this way.",
    ) {
        return Some((
            vec![json!({
                "kind": "discardHandThenDrawEachPlayer",
                "count": {
                    "kind": "greatestHandSize",
                },
            })],
            Vec::new(),
        ));
    }

    let drain_opponents_re = Regex::new(
        r"(?i)^Each opponent loses X life\. You gain life equal to the life lost this way\.$",
    )
    .expect("drain-opponents regex compiles");
    if drain_opponents_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "drainEachOpponent",
                "amount": decision_result("xValue"),
            })],
            vec![x_value()],
        ));
    }

    let draw_then_top_re = Regex::new(&format!(
        r"(?i)^Draw ({}) cards?, then put ({}) cards? from your hand on top of your library in any order\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("draw-then-put-on-top regex compiles");
    if let Some(captures) = draw_then_top_re.captures(instruction) {
        let draw_count = integer(parse_number_word(&captures[1])?);
        let put_count = integer(parse_number_word(&captures[2])?);
        return Some((
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": draw_count,
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "cardsForLibrary",
                    "player": controller(),
                    "minimum": put_count.clone(),
                    "maximum": put_count,
                    "candidates": {
                        "kind": "cards",
                        "zone": hand(controller()),
                        "where": Value::Null,
                    },
                }),
                json!({
                    "kind": "chooseOrder",
                    "id": "cardsForLibraryOrder",
                    "player": controller(),
                    "objects": decision_result("cardsForLibrary"),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("cardsForLibraryOrder"),
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "top",
                    },
                    "order": {
                        "kind": "decisionOrder",
                        "decisionId": "cardsForLibraryOrder",
                    },
                }),
            ],
            Vec::new(),
        ));
    }

    let target_copy_re = Regex::new(
        r"(?i)^Create a token that's a copy of target (.+?)(?:\. That token gains (haste) until end of turn)?\.$",
    )
    .expect("target token-copy regex compiles");
    if let Some(captures) = target_copy_re.captures(instruction) {
        let target_description = captures[1].trim();
        return Some((
            vec![json!({
                "kind": "createTokenCopyOfPermanent",
                "object": chosen_target("targetPermanent"),
                "grantKeywords": captures.get(2).map(|_| vec!["haste"]).unwrap_or_default(),
                "exileAtNextEndStep": false,
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(target_description, face_name)?,
                1,
                1,
            )],
        ));
    }

    let exile_gain_power_re = Regex::new(
        r"(?i)^Exile (up to one )?(other )?target (.+?)\. (?:Its|That creature's) controller gains life equal to its power\.$",
    )
    .expect("exile-and-gain-power regex compiles");
    if let Some(captures) = exile_gain_power_re.captures(instruction) {
        let mut candidates = permanent_target_candidates(&captures[3], face_name)?;
        if captures.get(2).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": "exilePermanentAndControllerGainsPower",
                "permanent": chosen_target("targetPermanent"),
            })],
            vec![target_decision(
                "targetPermanent",
                candidates,
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }

    let graveyard_to_library_bottom_re = Regex::new(
        r"(?i)^(up to one )?target player puts all the cards from their graveyard on the bottom of their library in (?:a )?random order\.$",
    )
    .expect("target graveyard-to-library-bottom regex compiles");
    if let Some(captures) = graveyard_to_library_bottom_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "moveGraveyardToLibraryBottomRandom",
                "player": chosen_target("targetPlayer"),
            })],
            vec![target_decision(
                "targetPlayer",
                json!({ "kind": "players" }),
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }

    let blink_re = Regex::new(
        r"(?i)^(You may )?exile (another )?target (.+?), then return (?:that card|it) to the battlefield( tapped)? under (its owner's|your) control\.$",
    )
    .expect("blink-permanent regex compiles");
    if let Some(captures) = blink_re.captures(instruction) {
        let mut candidates = permanent_target_candidates(&captures[3], face_name)?;
        if captures.get(2).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        let mut effect = json!({
            "kind": "blinkPermanent",
            "permanent": chosen_target("targetPermanent"),
        });
        if captures.get(4).is_some() {
            effect["tapped"] = Value::Bool(true);
        }
        if captures[5].eq_ignore_ascii_case("your") {
            effect["controller"] = controller();
        }
        return Some((
            vec![effect],
            vec![target_decision(
                "targetPermanent",
                candidates,
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }

    let temporary_gain_control_re =
        Regex::new(r"(?i)^Gain control of target (.+?) until end of turn\.$")
            .expect("temporary gain-control instruction regex compiles");
    if let Some(captures) = temporary_gain_control_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "gainControlPermanent",
                "permanent": chosen_target("targetPermanent"),
                "controller": controller(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    let gain_control_re = Regex::new(r"(?i)^Gain control of target (.+?)\.$")
        .expect("gain-control instruction regex compiles");
    if let Some(captures) = gain_control_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "gainControlPermanent",
                "permanent": chosen_target("targetPermanent"),
                "controller": controller(),
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }

    let each_player_sacrifices_all_re =
        Regex::new(r"(?i)^Each player sacrifices all (.+?) they control(?: (that .+))?\.$")
            .expect("each-player sacrifice-all instruction regex compiles");
    if let Some(captures) = each_player_sacrifices_all_re.captures(instruction) {
        let criteria = captures
            .get(2)
            .map(|suffix| format!("{} {}", &captures[1], suffix.as_str()))
            .unwrap_or_else(|| captures[1].to_string());
        return Some((
            vec![json!({
                "kind": "sacrificePermanentsEachPlayer",
                "scope": "allPlayers",
                "controller": controller(),
                "where": parse_permanent_criteria(&criteria, face_name)?,
                "all": true,
            })],
            Vec::new(),
        ));
    }

    let each_player_sacrifices_re = Regex::new(&format!(
        r"(?i)^(Each player|Each opponent|Each other player) sacrifices ({}) (.+?) of their choice\.$",
        count_word_pattern(),
    ))
    .expect("each-player sacrifice instruction regex compiles");
    if let Some(captures) = each_player_sacrifices_re.captures(instruction) {
        let description = captures[3].trim();
        let singular = singular_card_term(description);
        return Some((
            vec![json!({
                "kind": "sacrificePermanentsEachPlayer",
                "scope": if captures[1].eq_ignore_ascii_case("Each player") { "allPlayers" } else { "opponents" },
                "controller": controller(),
                "where": parse_permanent_criteria(&singular, face_name)?,
                "count": integer(parse_number_word(&captures[2])?),
            })],
            Vec::new(),
        ));
    }

    let reanimate_re = Regex::new(
        r"(?i)^Put target (.+?) card from a graveyard onto the battlefield under your control\. You lose life equal to that card's mana value\.$",
    )
    .expect("reanimate-target regex compiles");
    if let Some(captures) = reanimate_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "reanimateTargetCard",
                "card": chosen_target("targetGraveyardCard"),
                "controller": controller(),
                "loseManaValue": true,
            })],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": { "kind": "anyGraveyard" },
                    "where": parse_permanent_criteria(&captures[1], face_name)?,
                }),
                1,
                1,
            )],
        ));
    }

    let return_all_graveyard_re = Regex::new(
        r"(?i)^Return all (.+?) cards from your graveyard to the battlefield( tapped)?\.$",
    )
    .expect("return-all-from-graveyard regex compiles");
    if let Some(captures) = return_all_graveyard_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "returnCardsFromGraveyard",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[1], face_name)?,
                "controller": controller(),
                "tapped": captures.get(2).is_some(),
            })],
            Vec::new(),
        ));
    }

    let linked_exiled_card_to_hand_re = Regex::new(
        r"(?i)^Put a card exiled with (?:this (?:artifact|creature|enchantment|permanent|Saga)|.+?) into its owner's hand\.$",
    )
    .expect("linked exiled-card choice to owner's hand regex compiles");
    if linked_exiled_card_to_hand_re.is_match(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "chosenLinkedExiledCard",
                    "player": controller(),
                    "from": { "kind": "cardsExiledWithSource" },
                    "minimum": integer(1),
                    "maximum": integer(1),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("chosenLinkedExiledCard"),
                    "to": { "kind": "ownersHand" },
                }),
            ],
            Vec::new(),
        ));
    }

    let choose_graveyard_to_battlefield_re = Regex::new(
        r"(?i)^Return (up to one|a|one) (.+?\bcard(?: with .+?)?) from your graveyard to the battlefield( tapped)?\.$",
    )
    .expect("chosen graveyard-to-battlefield regex compiles");
    if let Some(captures) = choose_graveyard_to_battlefield_re.captures(instruction) {
        let tapped = captures.get(3).is_some();
        let criteria = captures[2]
            .replace(" card with ", " with ")
            .trim_end_matches(" card")
            .to_string();
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "chosenGraveyardCards",
                    "player": controller(),
                    "minimum": if captures[1].eq_ignore_ascii_case("up to one") { 0 } else { 1 },
                    "maximum": 1,
                    "candidates": {
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": parse_permanent_criteria(&criteria, face_name)?,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("chosenGraveyardCards"),
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": tapped,
                    },
                }),
            ],
            Vec::new(),
        ));
    }

    let graveyard_to_library_re = Regex::new(
        r"(?i)^(You may )?put target\s+(?:(.+?)\s+)?card from your graveyard on (?:the )?(top|bottom) of your library\.$",
    )
    .expect("graveyard-to-library regex compiles");
    if let Some(captures) = graveyard_to_library_re.captures(instruction) {
        let where_filter = captures
            .get(2)
            .map(|criteria| parse_permanent_criteria(criteria.as_str().trim(), face_name))
            .unwrap_or(Some(Value::Null))?;
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetGraveyardCard"),
                "to": if captures[3].eq_ignore_ascii_case("top") { "libraryTop" } else { "libraryBottom" },
                "tapped": false,
            })],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": where_filter,
                }),
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }

    let target_library_search_exile_re = Regex::new(
        r"(?i)^Search target (player|opponent)'s library for a card and exile it\. Then that player shuffles\.$",
    )
    .expect("target-player library-search exile regex compiles");
    if let Some(captures) = target_library_search_exile_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        let opponent_only = captures[1].eq_ignore_ascii_case("opponent");
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "searchedCard",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "cards",
                        "zone": library(target_player.clone()),
                        "where": Value::Null,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("searchedCard"),
                    "to": {
                        "kind": "exile",
                        "player": target_player.clone(),
                        "faceDown": false,
                    },
                }),
                json!({ "kind": "shuffleZone", "zone": library(target_player) }),
            ],
            vec![target_decision(
                "targetPlayer",
                if opponent_only {
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
    let any_graveyard_to_owner_library_re = Regex::new(
        r"(?i)^Put target\s+(?:(.+?)\s+)?card from a graveyard on (?:the )?(top|bottom) of its owner's library\.$",
    )
    .expect("any-graveyard to owner-library regex compiles");
    if let Some(captures) = any_graveyard_to_owner_library_re.captures(instruction) {
        let where_filter = captures
            .get(1)
            .map(|criteria| parse_permanent_criteria(criteria.as_str().trim(), face_name))
            .unwrap_or(Some(Value::Null))?;
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetGraveyardCard"),
                "to": if captures[2].eq_ignore_ascii_case("top") { "libraryTop" } else { "libraryBottom" },
                "tapped": false,
            })],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": { "kind": "anyGraveyard" },
                    "where": where_filter,
                }),
                1,
                1,
            )],
        ));
    }

    let target_owner_library_choice_re = Regex::new(
        r"(?i)^Target (.+?)'s owner puts it on their choice of the top or bottom of their library\.$",
    )
    .expect("target owner library-end choice regex compiles");
    if let Some(captures) = target_owner_library_choice_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "putPermanentOnOwnerLibrary",
                "permanent": chosen_target("targetPermanent"),
                "position": "ownerChoiceTopOrBottom",
            })],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                }),
                1,
                1,
            )],
        ));
    }

    let graveyard_cast_permission_re = Regex::new(
        r"(?i)^Choose target (.+?) card in your graveyard\. You may cast that card this turn\.$",
    )
    .expect("target graveyard cast-permission regex compiles");
    if let Some(captures) = graveyard_cast_permission_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "grantPermission",
                "player": controller(),
                "action": {
                    "kind": "play",
                    "card": chosen_target("targetGraveyardCard"),
                    "normalTimingApplies": true,
                    "normalCostsApply": true,
                },
                "duration": { "kind": "untilEndOfCurrentTurn" },
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

    let cast_one_from_zone_re = Regex::new(
        r"(?i)^(You may )?cast (?:a|an) (.+?) spell from your (graveyard|hand|exile)\.$",
    )
    .expect("resolution-time zone casting regex compiles");
    if let Some(captures) = cast_one_from_zone_re.captures(instruction) {
        let source_zone = captures[3].to_ascii_lowercase();
        return Some((
            vec![json!({
                "kind": "castOneCard",
                "player": controller(),
                "cards": {
                    "kind": "cardsInZone",
                    "zone": { "kind": source_zone, "player": controller() },
                    "where": card_qualifier_list_filter(&captures[2], face_name)
                        .or_else(|| parse_permanent_criteria(&captures[2], face_name))?,
                },
                "sourceZone": source_zone,
                "normalCostsApply": true,
                "optional": captures.get(1).is_some(),
                "bind": "castSpell",
            })],
            Vec::new(),
        ));
    }

    let return_and_attach_re = Regex::new(
        r"(?i)^Return target (.+?\bcard(?: with .+?)?) from your graveyard to the battlefield and attach this (?:Aura|Equipment|permanent) to it\.$",
    )
    .expect("graveyard return-and-attach regex compiles");
    if let Some(captures) = return_and_attach_re.captures(instruction) {
        let target = chosen_target("targetGraveyardCard");
        let criteria = captures[1]
            .replace(" card with ", " with ")
            .trim_end_matches(" card")
            .to_string();
        return Some((
            vec![
                json!({
                    "kind": "moveTargetCard",
                    "card": target.clone(),
                    "to": "battlefield",
                    "tapped": false,
                    "controller": controller(),
                }),
                json!({
                    "kind": "attachPermanent",
                    "attachment": self_ref(),
                    "to": target,
                }),
            ],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": parse_permanent_criteria(&criteria, face_name)?,
                }),
                1,
                1,
            )],
        ));
    }

    let top_graveyard_haste_exile_re = Regex::new(
        r"(?i)^Return the top (.+?) card of your graveyard to the battlefield\. That (?:creature|permanent) gains haste(?: until end of turn)?\. Exile it at the beginning of the next end step\.$",
    )
    .expect("top graveyard return with haste and delayed exile regex compiles");
    if let Some(captures) = top_graveyard_haste_exile_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "moveTopMatchingCard",
                "zone": graveyard(controller()),
                "where": parse_permanent_criteria(&captures[1], face_name)?,
                "to": {
                    "kind": "battlefield",
                    "player": controller(),
                    "tapped": false,
                },
                "grantKeywords": ["haste"],
                "exileAtNextEndStep": true,
            })],
            Vec::new(),
        ));
    }

    let return_graveyard_to_battlefield_re = Regex::new(
        r"(?i)^(You may )?return (up to one )?target (.+?\bcard(?: with .+?)?) from your graveyard to the battlefield( tapped)?(?: with (?:a|an) ([^ ]+) counter on it)?\.$",
    )
    .expect("graveyard-to-battlefield regex compiles");
    if let Some(captures) = return_graveyard_to_battlefield_re.captures(instruction) {
        let criteria = captures[3]
            .replace(" card with ", " with ")
            .trim_end_matches(" card")
            .to_string();
        let mut effect = json!({
            "kind": "moveTargetCard",
            "card": chosen_target("targetGraveyardCard"),
            "to": "battlefield",
            "tapped": captures.get(4).is_some(),
            "controller": controller(),
        });
        if let Some(counter) = captures.get(5) {
            effect["counter"] = Value::String(counter.as_str().to_string());
        }
        return Some((
            vec![effect],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": parse_permanent_criteria(&criteria, face_name)?,
                }),
                if captures.get(1).is_some() || captures.get(2).is_some() {
                    0
                } else {
                    1
                },
                1,
            )],
        ));
    }

    let mill_counter_pronoun_keyword_re = Regex::new(&format!(
        r"(?i)^Mill ({}) cards?\. Put ({}) ([A-Za-z0-9+/ -]+) counters? on target (.+?)\. It gains (flying|deathtouch|double strike|first strike|haste|lifelink|reach|trample|vigilance) until end of turn\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("mill-counter-pronoun-keyword regex compiles");
    if let Some(captures) = mill_counter_pronoun_keyword_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        let keyword = oracle_keyword_list(&captures[5])?.into_iter().next()?;
        return Some((
            vec![
                json!({ "kind": "mill", "player": controller(), "count": integer(parse_number_word(&captures[1])?) }),
                json!({ "kind": "putCounters", "permanent": target.clone(), "counter": captures[3].trim(), "count": integer(parse_number_word(&captures[2])?) }),
                json!({ "kind": "grantKeyword", "object": target, "keyword": keyword, "duration": { "kind": "untilEndOfCurrentTurn" } }),
            ],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[4], face_name)?,
                1,
                1,
            )],
        ));
    }

    let up_to_x_counter_keyword_draw_re = Regex::new(
        r"(?i)^Put (?:a|an) ([^ ]+) counter on each of up to X target (.+?)\. Those creatures gain (flying|deathtouch|double strike|first strike|haste|lifelink|reach|trample|vigilance) until end of turn\. Draw a card\.$",
    )
    .expect("up-to-X counter-keyword-draw regex compiles");
    if let Some(captures) = up_to_x_counter_keyword_draw_re.captures(instruction) {
        let keyword = oracle_keyword_list(&captures[3])?.into_iter().next()?;
        return Some((
            vec![
                json!({ "kind": "putCounters", "permanent": { "kind": "chosenTargets", "id": "targetPermanents" }, "counter": captures[1].to_string(), "count": integer(1) }),
                json!({ "kind": "grantKeywordToTargets", "objects": { "kind": "chosenTargets", "id": "targetPermanents" }, "keyword": keyword, "duration": { "kind": "untilEndOfCurrentTurn" } }),
                json!({ "kind": "drawCards", "player": controller(), "count": integer(1) }),
            ],
            vec![
                x_value(),
                json!({
                    "id": "targetPermanents", "kind": "chooseTargets", "minimum": 0,
                    "maximum": decision_result("xValue"),
                    "candidates": permanent_target_candidates(&captures[2], face_name)?,
                }),
            ],
        ));
    }

    let life_for_mana_re = Regex::new(
        r"(?i)^Until end of turn, any time you could activate a mana ability, you may pay (\d+) life\. If you do, add \{([WUBRGC])\}\.$",
    )
    .expect("temporary life-for-mana permission regex compiles");
    if let Some(captures) = life_for_mana_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "installLifeForManaPermission",
                "player": controller(),
                "life": integer(captures[1].parse::<i64>().ok()?),
                "symbol": captures[2].to_ascii_uppercase(),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }

    let conjure_re = Regex::new(
        r"(?i)^Conjure a card named ([A-Za-z0-9 ',.-]+) (onto the battlefield|into your hand)\.$",
    )
    .expect("conjure named card regex compiles");
    if let Some(captures) = conjure_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "conjureNamedCard",
                "player": controller(),
                "name": captures[1].trim(),
                "destination": if captures[2].eq_ignore_ascii_case("onto the battlefield") { "battlefield" } else { "hand" },
            })],
            Vec::new(),
        ));
    }

    let mill_then_choose_re = Regex::new(&format!(
        r"(?i)^Mill ({}) cards?[.,] then put (?:a|an) (.+?) card from among them (onto the battlefield|into your hand)\.$",
        count_word_pattern(),
    ))
    .expect("mill-then-choose regex compiles");
    if let Some(captures) = mill_then_choose_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "mill",
                    "player": controller(),
                    "count": integer(parse_number_word(&captures[1])?),
                    "bind": "milledCards",
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "chosenMilledCard",
                    "player": controller(),
                    "from": bound_objects("milledCards"),
                    "where": parse_permanent_criteria(&captures[2], face_name)?,
                    "minimum": 1,
                    "maximum": 1,
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("chosenMilledCard"),
                    "to": if captures[3].eq_ignore_ascii_case("into your hand") {
                        json!({ "kind": "hand", "player": controller() })
                    } else {
                        json!({ "kind": "battlefield", "player": controller(), "tapped": false })
                    },
                }),
            ],
            Vec::new(),
        ));
    }

    let mill_then_move_all_matching_re = Regex::new(&format!(
        r"(?i)^Mill ({}) cards?, then put all (.+?) cards from among them into your hand\.$",
        count_word_pattern(),
    ))
    .expect("mill then move all matching cards regex compiles");
    if let Some(captures) = mill_then_move_all_matching_re.captures(instruction) {
        let where_filter = card_qualifier_list_filter(captures.get(2)?.as_str(), face_name)
            .or_else(|| parse_permanent_criteria(captures.get(2)?.as_str(), face_name))?;
        return Some((
            vec![
                json!({
                    "kind": "mill",
                    "player": controller(),
                    "count": integer(parse_number_word(captures.get(1)?.as_str())?),
                    "bind": "milledCards",
                }),
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "filterObjects",
                        "objects": bound_objects("milledCards"),
                        "where": where_filter,
                    },
                    "to": { "kind": "hand", "player": controller() },
                }),
            ],
            Vec::new(),
        ));
    }

    let mill_then_choose_up_to_matching_re = Regex::new(&format!(
        r"(?i)^Mill ({}) cards?, then put up to ({}) (.+?) cards from among them into your hand\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("mill then choose up to matching cards regex compiles");
    if let Some(captures) = mill_then_choose_up_to_matching_re.captures(instruction) {
        let maximum = parse_number_word(captures.get(2)?.as_str())?;
        let where_filter = card_qualifier_list_filter(captures.get(3)?.as_str(), face_name)
            .or_else(|| parse_permanent_criteria(captures.get(3)?.as_str(), face_name))?;
        return Some((
            vec![
                json!({
                    "kind": "mill",
                    "player": controller(),
                    "count": integer(parse_number_word(captures.get(1)?.as_str())?),
                    "bind": "milledCards",
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "chosenMilledCards",
                    "player": controller(),
                    "from": bound_objects("milledCards"),
                    "where": where_filter,
                    "minimum": 0,
                    "maximum": maximum,
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("chosenMilledCards"),
                    "to": { "kind": "hand", "player": controller() },
                }),
            ],
            Vec::new(),
        ));
    }

    let exile_grave_copy_re = Regex::new(
        r"(?i)^Exile target (.+?) card from your graveyard\. Create a token that's a copy of it\.$",
    )
    .expect("exile-graveyard-and-copy regex compiles");
    if let Some(captures) = exile_grave_copy_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "exileTargetCardThenCreateTokenCopy",
                "card": chosen_target("targetGraveyardCard"),
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

    let each_subtype_counter_keyword_re = Regex::new(
        r"(?i)^Put (?:a|an) ([^ ]+) counter on each ([A-Za-z][A-Za-z '-]+) you control\. They gain (flying|deathtouch|double strike|first strike|haste|lifelink|reach|trample|vigilance) until end of turn\.$",
    )
    .expect("mass subtype counter-and-keyword regex compiles");
    if let Some(captures) = each_subtype_counter_keyword_re.captures(instruction) {
        let objects = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": subtype(&captures[2]),
        });
        let keyword = oracle_keyword_list(&captures[3])?.into_iter().next()?;
        return Some((
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": objects.clone(),
                    "counter": captures[1].to_string(),
                    "count": integer(1),
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": objects,
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
            Vec::new(),
        ));
    }

    let fractal_x_re = Regex::new(
        r"(?i)^Create X 0/0 green and blue Fractal creature tokens, then put X \+1/\+1 counters on each Fractal you control\.$",
    )
    .expect("fractal-X regex compiles");
    if fractal_x_re.is_match(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": { "kind": "sourceCastXValue" },
                    "token": {
                        "name": "Fractal Token", "colors": ["green", "blue"],
                        "types": ["Creature"], "subtypes": ["Fractal"], "power": 0, "toughness": 0,
                    },
                }),
                json!({
                    "kind": "putCounters",
                    "permanent": { "kind": "eachPermanent", "player": controller(), "where": subtype("Fractal") },
                    "counter": "+1/+1", "count": { "kind": "sourceCastXValue" },
                }),
            ],
            vec![x_value()],
        ));
    }

    let for_each_opponent_token_re = Regex::new(r"(?i)^For each opponent, you (create .+)$")
        .expect("per-opponent token regex compiles");
    if let Some(captures) = for_each_opponent_token_re.captures(instruction) {
        let token_instruction = format!("{}", &captures[1]);
        let mut effect = create_token_effect(&token_instruction)?;
        effect["quantity"] = json!({ "kind": "countOpponents", "player": controller() });
        return Some((vec![effect], Vec::new()));
    }

    let target_keyword_re = Regex::new(
        r"(?i)^(Another )?target (.+?) gains? (flying|deathtouch|double strike|first strike|haste|indestructible|lifelink|reach|trample|vigilance) until end of turn\.$",
    )
    .expect("target keyword instruction regex compiles");
    if let Some(captures) = target_keyword_re.captures(instruction)
        && !captures[2].to_ascii_lowercase().contains(" gets ")
    {
        let keyword = oracle_keyword_list(&captures[3])?.into_iter().next()?;
        let description = if captures.get(1).is_some() {
            format!("another {}", captures.get(2)?.as_str())
        } else {
            captures.get(2)?.as_str().to_string()
        };
        return Some((
            vec![json!({
                "kind": "grantKeyword",
                "object": chosen_target("targetPermanent"),
                "keyword": keyword,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&description, face_name)?,
                1,
                1,
            )],
        ));
    }

    let tap_or_untap_re = Regex::new(r"(?i)^You may tap or untap target (.+)\.$")
        .expect("tap-or-untap instruction regex compiles");
    if let Some(captures) = tap_or_untap_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "tapOrUntapPermanent",
                "permanent": chosen_target("targetPermanent"),
                "player": controller(),
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }

    let target_opponent_damage_re = Regex::new(&format!(
        r"(?i)^(?:This spell|[A-Z][^.,]+) deals ({}) damage to target opponent\.$",
        count_word_pattern(),
    ))
    .expect("target-opponent damage instruction regex compiles");
    if let Some(captures) = target_opponent_damage_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "amount": integer(parse_number_word(&captures[1])?),
                "recipient": chosen_target("targetOpponent"),
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

    let return_graveyard_card_re = Regex::new(
        r"(?i)^(You may )?Return (up to one )?target (?:(.+?) )?card from your graveyard to your hand\.$",
    )
    .expect("graveyard return instruction regex compiles");
    if let Some(captures) = return_graveyard_card_re.captures(instruction) {
        let filter = captures
            .get(3)
            .map(|criteria| parse_permanent_criteria(criteria.as_str(), face_name))
            .unwrap_or(Some(Value::Null))?;
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetGraveyardCard"),
                "to": "hand",
                "tapped": false,
            })],
            vec![target_decision(
                "targetGraveyardCard",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": filter,
                }),
                if captures.get(1).is_some() || captures.get(2).is_some() {
                    0
                } else {
                    1
                },
                1,
            )],
        ));
    }
    let choose_graveyard_card_re = Regex::new(
        r"(?i)^(You may )?Return (?:an? )?(.+?) card from your graveyard to your hand\.$",
    )
    .expect("chosen graveyard return instruction regex compiles");
    if let Some(captures) = choose_graveyard_card_re.captures(instruction) {
        let where_filter = card_qualifier_list_filter(&captures[2], face_name)
            .or_else(|| parse_permanent_criteria(&captures[2], face_name))?;
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "graveyardCardForHand",
                    "player": controller(),
                    "minimum": integer(if captures.get(1).is_some() { 0 } else { 1 }),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": where_filter,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("graveyardCardForHand"),
                    "to": hand(controller()),
                }),
            ],
            Vec::new(),
        ));
    }

    let search_opponent_to_battlefield_re = Regex::new(
        r"(?i)^Search target opponent's library for (?:an? )?(.+?) card and put that card onto the battlefield under your control\. Then that player shuffles\.$",
    )
    .expect("opponent-library search-to-battlefield instruction regex compiles");
    if let Some(captures) = search_opponent_to_battlefield_re.captures(instruction) {
        let target_opponent = chosen_target("targetOpponent");
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "searchedOpponentCard",
                    "player": controller(),
                    "minimum": 0,
                    "maximum": 1,
                    "candidates": {
                        "kind": "cards",
                        "zone": library(target_opponent.clone()),
                        "where": parse_permanent_criteria(&captures[1], face_name)?,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("searchedOpponentCard"),
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                }),
                json!({
                    "kind": "shuffleZone",
                    "zone": library(target_opponent),
                }),
            ],
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

    let search_to_library_top_re = Regex::new(
        r"(?i)^Search your library for (?:an? )?(?:(.+?) )?card, then shuffle and put that card on top\.$",
    )
    .expect("library search-to-top instruction regex compiles");
    if let Some(captures) = search_to_library_top_re.captures(instruction) {
        let filter = captures
            .get(1)
            .map(|criteria| parse_permanent_criteria(criteria.as_str(), face_name))
            .unwrap_or(Some(Value::Null))?;
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "searchedCards",
                    "player": controller(),
                    "minimum": 0,
                    "maximum": 1,
                    "candidates": {
                        "kind": "cards",
                        "zone": library(controller()),
                        "where": filter,
                    },
                }),
                json!({
                    "kind": "shuffleZone",
                    "zone": library(controller()),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("searchedCards"),
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "top",
                    },
                }),
            ],
            Vec::new(),
        ));
    }

    if let Some(effects) =
        split_library_search_between_battlefield_and_hand_effects(instruction, face_name)
    {
        return Some((effects, Vec::new()));
    }

    let search_opponent_to_exile_re = Regex::new(
        r"(?i)^Search target opponent's library for (?:an? )?(?:(.+?) )?card and exile it face down\. Then that player shuffles\. You may play that card for as long as it remains exiled\.$",
    )
    .expect("opponent-library search-to-exile instruction regex compiles");
    if let Some(captures) = search_opponent_to_exile_re.captures(instruction) {
        let filter = captures
            .get(1)
            .map(|criteria| parse_permanent_criteria(criteria.as_str(), face_name))
            .unwrap_or(Some(Value::Null))?;
        let target_opponent = chosen_target("targetOpponent");
        return Some((
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": "searchedOpponentCard",
                    "player": controller(),
                    "minimum": 0,
                    "maximum": 1,
                    "candidates": {
                        "kind": "cards",
                        "zone": library(target_opponent.clone()),
                        "where": filter,
                    },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("searchedOpponentCard"),
                    "to": {
                        "kind": "exile",
                        "player": target_opponent.clone(),
                        "faceDown": true,
                    },
                }),
                json!({
                    "kind": "shuffleZone",
                    "zone": library(target_opponent),
                }),
                json!({
                    "kind": "grantCardPermission",
                    "player": controller(),
                    "cards": decision_result("searchedOpponentCard"),
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

    let search_then_top_re = Regex::new(
        r"(?i)^Search your library for (?:an? )?(.+?), (reveal (?:it|that card), then )?shuffle and put (?:it|that card) on top\.$",
    )
    .expect("library search followed by shuffle and top placement regex compiles");
    if let Some(captures) = search_then_top_re.captures(instruction) {
        return Some((
            search_library_then_put_on_top_effects(
                entry_search_filter(captures.get(1)?.as_str())?,
                captures.get(2).is_some(),
            ),
            Vec::new(),
        ));
    }

    let search_to_hand_re = Regex::new(
        r"(?i)^Search your library for (?:an? )?(.+?), (?:reveal it, )?put (?:that card|it) into your hand, then shuffle\.$",
    )
    .expect("library search-to-hand instruction regex compiles");
    if let Some(captures) = search_to_hand_re.captures(instruction) {
        let criteria = captures.get(1)?.as_str();
        let filter = if criteria.eq_ignore_ascii_case("card") {
            Value::Null
        } else {
            parse_permanent_criteria(criteria, face_name)?
        };
        let mut effects = search_library_effects(filter, 1, "hand", false);
        if instruction.to_ascii_lowercase().contains("reveal it") {
            effects.insert(
                1,
                json!({ "kind": "revealCards", "cards": decision_result("searchedCards") }),
            );
        }
        return Some((effects, Vec::new()));
    }
    let counted_search_to_hand_re = Regex::new(&format!(
        r"(?i)^Search your library for (?:up to )?({}) (.+?) cards?, (?:reveal (?:it|them|that card|those cards), )?put (?:it|them|that card|those cards) into your hand, then shuffle\.$",
        count_word_pattern(),
    ))
    .expect("counted library search-to-hand regex compiles");
    if let Some(captures) = counted_search_to_hand_re.captures(instruction) {
        let maximum = parse_number_word(&captures[1])?;
        let criteria = format!("a {} card", captures[2].trim());
        let mut effects =
            search_library_effects(entry_search_filter(&criteria)?, maximum, "hand", false);
        if instruction.to_ascii_lowercase().contains("reveal") {
            effects.insert(
                1,
                json!({ "kind": "revealCards", "cards": decision_result("searchedCards") }),
            );
        }
        return Some((effects, Vec::new()));
    }

    let search_to_graveyard_re = Regex::new(&format!(
        r"(?i)^(?:You may )?search your library for (?:up to )?(?:({}) )?(?:(nonlegendary|artifact|creature|enchantment|instant|sorcery|land) )?cards?, put (?:that card|those cards|it|them) into your graveyard, then shuffle\.$",
        count_word_pattern(),
    ))
    .expect("library search-to-graveyard instruction regex compiles");
    if let Some(captures) = search_to_graveyard_re.captures(instruction) {
        let maximum = captures
            .get(1)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        let filter = captures
            .get(2)
            .map(|criterion| entry_search_filter(criterion.as_str()))
            .unwrap_or(Some(Value::Null))?;
        return Some((
            search_library_effects(filter, maximum, "graveyard", false),
            Vec::new(),
        ));
    }

    let multi_target_bonus_re = Regex::new(
        r"(?i)^(One or two|Up to X) target creatures each get ([+-]\d+)/([+-]\d+) until end of turn\.$",
    )
    .expect("multi-target bonus instruction regex compiles");
    if let Some(captures) = multi_target_bonus_re.captures(instruction) {
        let variable = captures[1].eq_ignore_ascii_case("Up to X");
        let mut decisions = Vec::new();
        if variable {
            decisions.push(json!({
                "id": "xValue",
                "kind": "chooseNumber",
                "minimum": 0,
            }));
        }
        decisions.push(target_decision(
            "targetCreatures",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            if variable { 0 } else { 1 },
            if variable { i64::MAX } else { 2 },
        ));
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": { "kind": "chosenTargets", "id": "targetCreatures" },
                "power": integer(captures[2].parse::<i64>().ok()?),
                "toughness": integer(captures[3].parse::<i64>().ok()?),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            decisions,
        ));
    }

    if instruction.eq_ignore_ascii_case("Create a token that's a copy of this creature.") {
        return Some((
            vec![json!({
                "kind": "createTokenCopyOfPermanent",
                "object": self_ref(),
                "grantKeywords": [],
                "exileAtNextEndStep": false,
            })],
            Vec::new(),
        ));
    }
    let untap_up_to_re = Regex::new(&format!(
        r"(?i)^Untap up to ({}) (.+?)\.$",
        count_word_pattern(),
    ))
    .expect("untap-up-to instruction regex compiles");
    if let Some(captures) = untap_up_to_re.captures(instruction) {
        let maximum = parse_number_word(&captures[1])?;
        return Some((
            vec![json!({
                "kind": "untapPermanents",
                "objects": { "kind": "chosenTargets", "id": "untapPermanents" },
            })],
            vec![target_decision(
                "untapPermanents",
                permanent_target_candidates(&singular_card_term(&captures[2]), face_name)?,
                0,
                maximum,
            )],
        ));
    }
    let x_tokens_then_destroy_re = Regex::new(
        r"(?i)^(Create X .+? tokens?)\. If X is (\d+) or more, destroy all other creatures\.$",
    )
    .expect("variable tokens then global destruction regex compiles");
    if let Some(captures) = x_tokens_then_destroy_re.captures(instruction) {
        let mut create = create_token_effect(&format!("{}.", &captures[1]))?;
        create["bind"] = Value::String("createdTokens".to_string());
        return Some((
            vec![
                create,
                json!({
                    "kind": "conditionalEffect",
                    "condition": compare(
                        ">=",
                        json!({ "kind": "sourceCastXValue" }),
                        integer(captures[2].parse::<i64>().ok()?),
                    ),
                    "then": [{
                        "kind": "destroyPermanent",
                        "permanent": {
                            "kind": "eachPermanent",
                            "where": card_type("Creature"),
                            "exclude": bound_objects("createdTokens"),
                        },
                    }],
                    "else": [],
                }),
            ],
            Vec::new(),
        ));
    }
    if (instruction.to_ascii_lowercase().starts_with("create ")
        || instruction.to_ascii_lowercase().starts_with("you create "))
        && let Some(effect) = create_token_effect(instruction)
    {
        return Some((vec![effect], Vec::new()));
    }

    let draw_then_discard_re = Regex::new(&format!(
        r"(?i)^Draw ({}) cards?, then discard ({}) cards?\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("draw then discard instruction regex compiles");
    if let Some(captures) = draw_then_discard_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drawThenDiscard",
                "player": controller(),
                "drawCount": integer(parse_number_word(&captures[1])?),
                "discardCount": integer(parse_number_word(&captures[2])?),
            })],
            Vec::new(),
        ));
    }

    let begins_with_draw_then_discard = Regex::new(&format!(
        r"(?i)^Draw ({}) cards?, then discard ({}) cards?\.",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("draw then discard sentence prefix regex compiles")
    .is_match(instruction);
    for separator in [", then ", ". Then ", ". "] {
        if separator == ", then " && begins_with_draw_then_discard {
            continue;
        }
        if let Some((left, right)) = instruction.trim_end_matches('.').split_once(separator) {
            let left = format!("{}.", left.trim_end_matches('.'));
            let right = format!("{}.", right.trim_end_matches('.'));
            if let (
                Some((mut left_effects, mut left_decisions)),
                Some((right_effects, right_decisions)),
            ) = (
                parse_general_effect_instruction(&left, face_name),
                parse_general_effect_instruction(&right, face_name),
            ) {
                let mut ids = left_decisions
                    .iter()
                    .filter_map(|decision| decision["id"].as_str())
                    .collect::<BTreeSet<_>>();
                if right_decisions
                    .iter()
                    .all(|decision| decision["id"].as_str().is_some_and(|id| ids.insert(id)))
                {
                    left_effects.extend(right_effects);
                    left_decisions.extend(right_decisions);
                    return Some((left_effects, left_decisions));
                }
            }
        }
    }

    let land_search_instruction_re = Regex::new(&format!(
        r"(?i)^Search your library for (?:(?:a|one)|up to ({})) (basic land|land|[A-Za-z-]+) cards?, (?:reveal (?:that card|those cards|it|them), )?put (?:that card|those cards|it|them) onto the battlefield( tapped)?, then shuffle\.$",
        count_word_pattern(),
    ))
    .expect("general land-search instruction regex compiles");
    if let Some(captures) = land_search_instruction_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "searchLibrary",
                "player": controller(),
                "where": entry_search_filter(&format!("a {} card", &captures[2]))?,
                "maximum": integer(
                    captures
                        .get(1)
                        .and_then(|value| parse_number_word(value.as_str()))
                        .unwrap_or(1),
                ),
                "destination": "battlefield",
                "tapped": captures.get(3).is_some(),
            })],
            Vec::new(),
        ));
    }

    let linked_exile_search_instruction_re = Regex::new(&format!(
        r"(?i)^Search your library for up to ({}) (.+?) cards?, exile (?:those cards|them), then shuffle\.$",
        count_word_pattern(),
    ))
    .expect("general linked-exile library-search instruction regex compiles");
    if let Some(captures) = linked_exile_search_instruction_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "searchLibrary",
                "player": controller(),
                "where": entry_search_filter(&format!("a {} card", &captures[2]))?,
                "maximum": integer(parse_number_word(&captures[1])?),
                "destination": "exile",
                "tapped": false,
                "linkToSource": true,
            })],
            Vec::new(),
        ));
    }

    let paired_land_search_re = Regex::new(
        r"(?i)^Search your library for a ([A-Za-z]+) card and a ([A-Za-z]+) card, put them onto the battlefield( tapped)?, then shuffle\.$",
    )
    .expect("paired land-search instruction regex compiles");
    if let Some(captures) = paired_land_search_re.captures(instruction) {
        let tapped = captures.get(3).is_some();
        let effects = [&captures[1], &captures[2]]
            .into_iter()
            .map(|land_type| {
                json!({
                    "kind": "searchLibrary",
                    "player": controller(),
                    "where": subtype(land_type),
                    "maximum": integer(1),
                    "destination": "battlefield",
                    "tapped": tapped,
                })
            })
            .collect();
        return Some((effects, Vec::new()));
    }

    let land_threshold_instead_re =
        Regex::new(r"(?i)^(.+) If you control (\w+) or more lands, (.+) instead\.$")
            .expect("land threshold replacement instruction regex compiles");
    if let Some(captures) = land_threshold_instead_re.captures(instruction) {
        let base_instruction = format!("{}.", captures[1].trim_end_matches('.'));
        let alternative_instruction = format!("{}.", captures[3].trim_end_matches('.'));
        let (base_effects, base_decisions) =
            parse_general_effect_instruction(&base_instruction, face_name)?;
        let (alternative_effects, alternative_decisions) =
            parse_general_effect_instruction(&alternative_instruction, face_name)?;
        if base_decisions != alternative_decisions {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": card_type("Land"),
                    }),
                    integer(parse_number_word(&captures[2])?),
                ),
                "then": alternative_effects,
                "else": base_effects,
            })],
            base_decisions,
        ));
    }

    let controlled_land_condition_re =
        Regex::new(r"(?i)^If you control (\w+) or more lands, (.+)$")
            .expect("controlled land conditional instruction regex compiles");
    if let Some(captures) = controlled_land_condition_re.captures(instruction) {
        let minimum = parse_number_word(&captures[1])?;
        let (effects, decisions) = parse_general_effect_instruction(&captures[2], face_name)?;
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": card_type("Land"),
                    }),
                    integer(minimum),
                ),
                "then": effects,
                "else": [],
            })],
            decisions,
        ));
    }

    let controlled_not_owned_condition_re = Regex::new(&format!(
        r"(?i)^If you control ({}) or more (.+?) you don't own, (.+)$",
        count_word_pattern(),
    ))
    .expect("controlled-but-not-owned conditional instruction regex compiles");
    if let Some(captures) = controlled_not_owned_condition_re.captures(instruction) {
        let minimum = parse_number_word(&captures[1])?;
        let (effects, decisions) = parse_general_effect_instruction(&captures[3], face_name)?;
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": parse_permanent_criteria(&singular_card_term(&captures[2]), face_name)?,
                        "ownership": "notOwned",
                    }),
                    integer(minimum),
                ),
                "then": effects,
                "else": [],
            })],
            decisions,
        ));
    }

    let exile_top_permission_re = Regex::new(&format!(
        r"(?i)^Exile the top (?:(card)|({}) cards) of your library\.(?: Until (?:the )?end of (your next turn|turn), you may play (?:that card|those cards)| You may play (?:it|them) until (?:the )?end of (your next turn|turn))\.$",
        count_word_pattern(),
    ))
    .expect("top-card play permission regex compiles");
    if let Some(captures) = exile_top_permission_re.captures(instruction) {
        let count = captures
            .get(2)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        let duration = captures.get(3).or_else(|| captures.get(4))?.as_str();
        let next_turn = duration.eq_ignore_ascii_case("your next turn");
        return Some((
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(controller()),
                    "count": integer(count),
                    "faceDown": false,
                    "bind": "exiledTopCards",
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
                    "duration": if next_turn {
                        json!({ "kind": "untilEndOfNextTurn", "player": controller() })
                    } else {
                        json!({ "kind": "untilEndOfCurrentTurn" })
                    },
                }),
            ],
            Vec::new(),
        ));
    }

    let look_at_top_re = Regex::new(&format!(
        r"(?i)^Look at the top (?:card|({}) cards) of (your|target player's) library\.$",
        count_word_pattern(),
    ))
    .expect("look-at-top library instruction regex compiles");
    if let Some(captures) = look_at_top_re.captures(instruction) {
        let count = captures
            .get(1)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        let target_player = captures[2].eq_ignore_ascii_case("target player's");
        let player = if target_player {
            chosen_target("targetPlayer")
        } else {
            controller()
        };
        let decisions = target_player
            .then(|| {
                vec![target_decision(
                    "targetPlayer",
                    json!({ "kind": "players" }),
                    1,
                    1,
                )]
            })
            .unwrap_or_default();
        return Some((
            vec![json!({
                "kind": "lookAtTopCards",
                "zone": library(player),
                "count": integer(count),
                "bind": "lookedTopCards",
            })],
            decisions,
        ));
    }

    let look_at_random_hand_card_re =
        Regex::new(r"(?i)^Look at (?:a|one) card at random in target player's hand\.$")
            .expect("random hand-card inspection regex compiles");
    if look_at_random_hand_card_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "lookAtRandomCardInHand",
                "viewer": controller(),
                "handOwner": chosen_target("targetPlayer"),
            })],
            vec![target_decision(
                "targetPlayer",
                json!({ "kind": "players" }),
                1,
                1,
            )],
        ));
    }

    let delayed_step_draw_re = Regex::new(&format!(
        r"(?i)^(?:You )?draw (?:a|({})) cards? at the beginning of the next turn's (upkeep|end step)\.$",
        count_word_pattern(),
    ))
    .expect("next-turn delayed draw instruction regex compiles");
    if let Some(captures) = delayed_step_draw_re.captures(instruction) {
        let count = captures
            .get(1)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        let step = if captures[2].eq_ignore_ascii_case("upkeep") {
            "upkeep"
        } else {
            "endStep"
        };
        return Some((
            vec![json!({
                "kind": "installDelayedStepTrigger",
                "step": step,
                "controller": controller(),
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(count),
                }],
            })],
            Vec::new(),
        ));
    }

    let sentences = instruction
        .strip_suffix('.')
        .unwrap_or(instruction)
        .split(". ")
        .map(|sentence| format!("{}.", sentence.trim()))
        .collect::<Vec<_>>();
    if sentences.len() > 1
        && sentences.iter().skip(1).all(|sentence| {
            !matches!(
                sentence.split_whitespace().next(),
                Some("It" | "Its" | "It's" | "That" | "Those" | "They" | "Them")
            )
        })
    {
        let parsed = sentences
            .iter()
            .map(|sentence| parse_general_effect_instruction(sentence, face_name))
            .collect::<Option<Vec<_>>>()?;
        let mut effects = Vec::new();
        let mut decisions = Vec::new();
        let mut decision_ids = BTreeSet::new();
        for (sentence_effects, sentence_decisions) in parsed {
            for decision in &sentence_decisions {
                let id = decision["id"].as_str()?;
                if !decision_ids.insert(id.to_string()) {
                    return None;
                }
            }
            effects.extend(sentence_effects);
            decisions.extend(sentence_decisions);
        }
        return Some((effects, decisions));
    }

    if instruction.eq_ignore_ascii_case("Recruit.")
        || instruction.eq_ignore_ascii_case("You recruit.")
    {
        return Some((
            vec![json!({ "kind": "recruit", "player": controller() })],
            Vec::new(),
        ));
    }

    let target_connive_re = Regex::new(r"(?i)^Target (.+) connives\.$")
        .expect("target connive instruction regex compiles");
    if let Some(captures) = target_connive_re.captures(instruction) {
        let permanent = chosen_target("conniveTarget");
        return Some((
            vec![json!({
                "kind": "connive",
                "permanent": permanent.clone(),
                "player": { "kind": "controllerOf", "object": permanent },
            })],
            vec![target_decision(
                "conniveTarget",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }

    let self_connive_subject = if face_name.is_empty() {
        r"(?:This creature|It)".to_string()
    } else {
        format!(r"(?:This creature|It|{})", regex::escape(face_name))
    };
    let self_connive_re = Regex::new(&format!(r"(?i)^{} connives\.$", self_connive_subject))
        .expect("self connive instruction regex compiles");
    if self_connive_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "connive",
                "permanent": self_ref(),
                "player": controller(),
            })],
            Vec::new(),
        ));
    }

    let targeted_token_creation_re = Regex::new(r"(?i)^Target (player|opponent) creates (.+)$")
        .expect("targeted token creation regex compiles");
    if let Some(captures) = targeted_token_creation_re.captures(instruction) {
        let mut effect = create_token_effect(&format!("Create {}", &captures[2]))?;
        effect["controller"] = chosen_target("targetPlayer");
        return Some((
            vec![effect],
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

    let token_per_triggering_player_permanent_re =
        Regex::new(r"(?i)^You create (.+? tokens?) for each (.+?) that player controls\.$")
            .expect("token per triggering-player permanent regex compiles");
    if let Some(captures) = token_per_triggering_player_permanent_re.captures(instruction) {
        let mut effect = create_token_effect(&format!("Create {}.", captures.get(1)?.as_str()))?;
        effect["quantity"] = json!({
            "kind": "countPermanents",
            "player": { "kind": "triggeringPlayer" },
            "where": parse_permanent_criteria(
                &singular_card_term(captures.get(2)?.as_str()),
                face_name,
            )?,
        });
        return Some((vec![effect], Vec::new()));
    }

    let targeted_scry_then_draw_re = Regex::new(&format!(
        r"(?i)^Target (player|opponent) scries ({}), then draws? (?:a|one) card\.$",
        numeric_expression_pattern(),
    ))
    .expect("targeted scry then draw regex compiles");
    if let Some(captures) = targeted_scry_then_draw_re.captures(instruction) {
        let target_player = chosen_target("targetPlayer");
        let count = parse_numeric_expression_text(&captures[2])?;
        let mut decisions = Vec::new();
        if contains_rule_kind(&count, "decisionResult") {
            decisions.push(x_value());
        }
        decisions.push(target_decision(
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
        ));
        return Some((
            vec![
                json!({
                    "kind": "scry",
                    "player": target_player.clone(),
                    "count": count,
                }),
                json!({
                    "kind": "drawCards",
                    "player": target_player,
                    "count": integer(1),
                }),
            ],
            decisions,
        ));
    }

    if let Some(token_effect) = create_token_effect(instruction) {
        return Some((vec![token_effect], Vec::new()));
    }

    for (separator_index, _) in instruction.match_indices(" and ") {
        let left = format!("{}.", instruction[..separator_index].trim_end_matches('.'));
        let right = format!(
            "{}.",
            instruction[separator_index + " and ".len()..].trim_end_matches('.')
        );
        let Some((mut left_effects, mut left_decisions)) =
            parse_general_effect_instruction(&left, face_name)
        else {
            continue;
        };
        let shared_subject_right = instruction[..separator_index]
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("you ")
            .then(|| format!("You {}", right.trim_start()));
        let Some((right_effects, right_decisions)) =
            parse_general_effect_instruction(&right, face_name).or_else(|| {
                shared_subject_right
                    .as_deref()
                    .and_then(|right| parse_general_effect_instruction(right, face_name))
            })
        else {
            continue;
        };
        let mut decision_ids = left_decisions
            .iter()
            .filter_map(|decision| decision["id"].as_str().map(ToOwned::to_owned))
            .collect::<BTreeSet<_>>();
        if right_decisions.iter().any(|decision| {
            decision["id"]
                .as_str()
                .is_none_or(|id| !decision_ids.insert(id.to_string()))
        }) {
            continue;
        }
        left_effects.extend(right_effects);
        left_decisions.extend(right_decisions);
        return Some((left_effects, left_decisions));
    }

    let amass_re = Regex::new(&format!(
        r"(?i)^Amass ([A-Za-z][A-Za-z '-]+) ({})\.$",
        count_word_pattern(),
    ))
    .expect("amass instruction regex compiles");
    if let Some(captures) = amass_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "amass",
                "player": controller(),
                "armySubtype": singular_card_term(&captures[1]),
                "count": integer(parse_number_word(&captures[2])?),
            })],
            Vec::new(),
        ));
    }

    let library_selection_re = Regex::new(&format!(
        r"(?i)^(Scry|Surveil) ({})\.$",
        count_word_pattern()
    ))
    .expect("general library-selection instruction regex compiles");
    if let Some(captures) = library_selection_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": captures[1].to_ascii_lowercase(),
                "player": controller(),
                "count": integer(parse_number_word(&captures[2])?),
            })],
            Vec::new(),
        ));
    }

    let draw_and_lose_re = Regex::new(&format!(
        r"(?i)^You draw ({}) cards? and (?:you )?lose ({}) life\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("draw and lose life instruction regex compiles");
    if let Some(captures) = draw_and_lose_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(parse_number_word(&captures[1])?),
                }),
                json!({
                    "kind": "loseLife",
                    "player": controller(),
                    "amount": integer(parse_number_word(&captures[2])?),
                }),
            ],
            Vec::new(),
        ));
    }

    let mutual_draw_re = Regex::new(&format!(
        r"(?i)^You and target opponent each draw ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("mutual draw instruction regex compiles");
    if let Some(captures) = mutual_draw_re.captures(instruction) {
        let count = integer(parse_number_word(&captures[1])?);
        return Some((
            vec![
                json!({ "kind": "drawCards", "player": controller(), "count": count.clone() }),
                json!({
                    "kind": "drawCards",
                    "player": chosen_target("targetOpponent"),
                    "count": count,
                }),
            ],
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

    let draw_re = Regex::new(&format!(
        r"(?i)^(?:You )?[Dd]raw ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("general draw instruction regex compiles");
    if let Some(captures) = draw_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }
    let discard_re = Regex::new(&format!(
        r"(?i)^(?:You )?[Dd]iscard ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("general discard instruction regex compiles");
    if let Some(captures) = discard_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "discardCards",
                "player": controller(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }
    let optional_draw_re = Regex::new(&format!(
        r"(?i)^You may draw (?:a|({})) cards?\.$",
        count_word_pattern(),
    ))
    .expect("optional draw instruction regex compiles");
    if let Some(captures) = optional_draw_re.captures(instruction) {
        let count = captures
            .get(1)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        return Some((
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(count),
                }],
            })],
            Vec::new(),
        ));
    }

    let player_counter_re = Regex::new(&format!(
        r"(?i)^You get ({}) ([A-Za-z][A-Za-z '-]+) counters?\.$",
        count_word_pattern(),
    ))
    .expect("player counter instruction regex compiles");
    if let Some(captures) = player_counter_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "addPlayerCounters",
                "player": controller(),
                "counter": singular_card_term(captures.get(2)?.as_str()).to_ascii_lowercase(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let put_land_re = Regex::new(
        r"(?i)^(?:You may )?put a land card from your (hand|graveyard|hand or graveyard) onto the battlefield( tapped)?\.$",
    )
    .expect("optional land placement regex compiles");
    if let Some(captures) = put_land_re.captures(instruction) {
        let zone = |kind: &str| json!({ "kind": kind, "player": controller() });
        let candidates = match &captures[1].to_ascii_lowercase()[..] {
            "hand" => json!({ "kind": "cards", "zone": zone("hand"), "where": card_type("Land") }),
            "graveyard" => {
                json!({ "kind": "cards", "zone": zone("graveyard"), "where": card_type("Land") })
            }
            "hand or graveyard" => json!({
                "kind": "union",
                "sets": [
                    { "kind": "cards", "zone": zone("hand"), "where": card_type("Land") },
                    { "kind": "cards", "zone": zone("graveyard"), "where": card_type("Land") },
                ],
            }),
            _ => return None,
        };
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("landToBattlefield"),
                "to": "battlefield",
                "tapped": captures.get(2).is_some(),
                "controller": controller(),
            })],
            vec![target_decision("landToBattlefield", candidates, 0, 1)],
        ));
    }

    let gain_life_re = Regex::new(&format!(
        r"(?i)^You gain ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("general gain life instruction regex compiles");
    if let Some(captures) = gain_life_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let sacrifice_source_gain_life_re = Regex::new(&format!(
        r"(?i)^Sacrifice (.+?) and you gain ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("source sacrifice and life gain regex compiles");
    if let Some(captures) = sacrifice_source_gain_life_re.captures(instruction)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            vec![
                json!({
                    "kind": "sacrificePermanent",
                    "permanent": self_ref(),
                }),
                json!({
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(parse_number_word(captures.get(2)?.as_str())?),
                }),
            ],
            Vec::new(),
        ));
    }

    let lose_life_re = Regex::new(&format!(
        r"(?i)^You lose ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("general lose life instruction regex compiles");
    if let Some(captures) = lose_life_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "loseLife",
                "player": controller(),
                "amount": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let lose_life_per_source_counter_re = Regex::new(&format!(
        r"(?i)^You lose ({}) life for each ([^ ]+) counter on (.+?)\.$",
        count_word_pattern(),
    ))
    .expect("life loss per source counter regex compiles");
    if let Some(captures) = lose_life_per_source_counter_re.captures(instruction)
        && source_reference_matches(captures.get(3)?.as_str(), face_name)
    {
        let count = json!({
            "kind": "countCounters",
            "object": self_ref(),
            "counter": captures.get(2)?.as_str().to_ascii_lowercase(),
        });
        let multiplier = parse_number_word(captures.get(1)?.as_str())?;
        return Some((
            vec![json!({
                "kind": "loseLife",
                "player": controller(),
                "amount": if multiplier == 1 {
                    count
                } else {
                    json!({
                        "kind": "multiply",
                        "left": count,
                        "right": integer(multiplier),
                    })
                },
            })],
            Vec::new(),
        ));
    }

    let opponent_life_re = Regex::new(&format!(
        r"(?i)^Each opponent loses ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("general opponent life loss instruction regex compiles");
    if let Some(captures) = opponent_life_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "loseLifeEachOpponent",
                "amount": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let target_opponent_life_re = Regex::new(&format!(
        r"(?i)^Target opponent loses ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("target-opponent life loss regex compiles");
    if let Some(captures) = target_opponent_life_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "loseLife",
                "player": chosen_target("targetOpponent"),
                "amount": integer(parse_number_word(&captures[1])?),
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

    let target_player_life_re = Regex::new(&format!(
        r"(?i)^Target player loses ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("target-player life loss regex compiles");
    if let Some(captures) = target_player_life_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "loseLife",
                "player": chosen_target("targetPlayer"),
                "amount": integer(parse_number_word(&captures[1])?),
            })],
            vec![target_decision(
                "targetPlayer",
                json!({ "kind": "players" }),
                1,
                1,
            )],
        ));
    }

    if instruction.eq_ignore_ascii_case("Goad target creature.") {
        return Some((
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "goadTargetCreature",
            })],
            vec![target_decision(
                "targetCreature",
                json!({ "kind": "permanents", "where": card_type("Creature") }),
                1,
                1,
            )],
        ));
    }

    let target_mill_re = Regex::new(&format!(
        r"(?i)^Target (player|opponent) mills ({}) cards?\.$",
        numeric_expression_pattern(),
    ))
    .expect("general target mill instruction regex compiles");
    if let Some(captures) = target_mill_re.captures(instruction) {
        let opponent_only = captures[1].eq_ignore_ascii_case("opponent");
        let count = parse_numeric_expression_text(&captures[2])?;
        let variable = contains_rule_kind(&count, "decisionResult");
        return Some((
            vec![json!({
                "kind": "mill",
                "player": chosen_target("targetPlayer"),
                "count": count,
            })],
            {
                let mut decisions = Vec::new();
                if variable {
                    decisions.push(x_value());
                }
                decisions.push(target_decision(
                    "targetPlayer",
                    if opponent_only {
                        json!({
                            "kind": "players",
                            "where": { "kind": "isOpponentOf", "player": controller() },
                        })
                    } else {
                        json!({ "kind": "players" })
                    },
                    1,
                    1,
                ));
                decisions
            },
        ));
    }

    let each_controlled_counter_re = Regex::new(&format!(
        r"(?i)^Put ({}) ([A-Za-z0-9+/ -]+) counters? on each (other )?(.+?) you control\.$",
        count_word_pattern(),
    ))
    .expect("general each-controlled-permanent counter instruction regex compiles");
    if let Some(captures) = each_controlled_counter_re.captures(instruction) {
        let mut permanents = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": parse_permanent_criteria(captures.get(4)?.as_str(), face_name)?,
        });
        if captures.get(3).is_some() {
            permanents["excludeSource"] = json!(true);
            permanents["exclude"] = json!({ "kind": "chosenTargets", "id": "targetPermanent" });
        }
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": permanents,
                "counter": captures[2].trim(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let self_counter_re = Regex::new(&format!(
        r"(?i)^Put ({}) ([A-Za-z0-9+/ -]+) counters? on (.+)\.$",
        count_word_pattern(),
    ))
    .expect("general self counter instruction regex compiles");
    if let Some(captures) = self_counter_re.captures(instruction)
        && (matches!(
            captures[3].to_ascii_lowercase().as_str(),
            "this artifact"
                | "this creature"
                | "this enchantment"
                | "this permanent"
                | "this equipment"
                | "it"
        ) || source_reference_matches(&captures[3], face_name))
    {
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": captures[2].trim(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
            Vec::new(),
        ));
    }

    let target_counter_re = Regex::new(&format!(
        r"(?i)^Put (X|{}) ([A-Za-z0-9+/ -]+) counters? on (?:up to one )?target (.+?)(?:, where X is ({}))?\.$",
        count_word_pattern(),
        variable_clause_pattern(),
    ))
    .expect("general target counter instruction regex compiles");
    if let Some(captures) = target_counter_re.captures(instruction) {
        let optional = instruction
            .to_ascii_lowercase()
            .contains("up to one target");
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": chosen_target("targetPermanent"),
                "counter": captures[2].trim(),
                "count": if captures[1].eq_ignore_ascii_case("X") {
                    x_variable_expression(captures.get(4)?.as_str())?
                } else {
                    integer(parse_number_word(&captures[1])?)
                },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[3], face_name)?,
                i64::from(!optional),
                1,
            )],
        ));
    }

    let source_subject = if face_name.is_empty() {
        r"(?:This (?:artifact|creature|enchantment|permanent|token)|It|He|She|They|[A-Z][^.,]+)"
            .to_string()
    } else {
        format!(
            r"(?:This (?:artifact|creature|enchantment|permanent|token)|It|He|She|They|[A-Z][^.,]+|{})",
            regex::escape(face_name),
        )
    };
    let target_keyword_while_source_remains_re = Regex::new(
        r"(?i)^Target (.+?) you control gains? (.+?) for as long as this (?:Saga|permanent) remains on the battlefield\.$",
    )
    .expect("target keyword while source remains regex compiles");
    if let Some(captures) = target_keyword_while_source_remains_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "installLinkedKeyword",
                "object": chosen_target("targetPermanent"),
                "keyword": oracle_keyword_kind(captures.get(2)?.as_str())?,
                "duration": { "kind": "whileSourceOnBattlefield" },
            })],
            vec![target_decision(
                "targetPermanent",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                }),
                1,
                1,
            )],
        ));
    }
    let prevent_damage_by_target_while_source_remains_re = Regex::new(
        r"(?i)^Prevent all damage that would be dealt by (up to one )?target (.+?) for as long as this (?:Saga|permanent) remains on the battlefield\.$",
    )
    .expect("linked damage-source prevention regex compiles");
    if let Some(captures) = prevent_damage_by_target_while_source_remains_re.captures(instruction) {
        let optional = captures.get(1).is_some();
        return Some((
            vec![json!({
                "kind": "installLinkedDamagePrevention",
                "source": chosen_target("targetPermanent"),
                "duration": { "kind": "whileSourceOnBattlefield" },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(captures.get(2)?.as_str(), face_name)?,
                i64::from(!optional),
                1,
            )],
        ));
    }
    let opposing_permanent_damage_by_quantity_re = Regex::new(&format!(
        r"(?i)^({}) deals damage to each (.+?) your opponents control equal to (.+)\.$",
        source_subject,
    ))
    .expect("opposing permanent damage by numeric quantity regex compiles");
    if let Some(captures) = opposing_permanent_damage_by_quantity_re.captures(instruction) {
        let subject = captures.get(1)?.as_str();
        if face_name.is_empty() || source_reference_matches(subject, face_name) {
            return Some((
                vec![json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": parse_numeric_expression_text(captures.get(3)?.as_str())?,
                    "recipient": {
                        "kind": "eachPermanent",
                        "player": { "kind": "opponentsOf", "player": controller() },
                        "where": parse_permanent_criteria(
                            &singular_card_term(captures.get(2)?.as_str()),
                            face_name,
                        )?,
                    },
                })],
                Vec::new(),
            ));
        }
    }
    let per_player_permanent_damage_re = Regex::new(
        r"(?i)^(.+?) deals damage to each player equal to (twice )?the number of (.+?) that player controls\.$",
    )
    .expect("per-player permanent-count damage regex compiles");
    if let Some(captures) = per_player_permanent_damage_re.captures(instruction)
        && (face_name.is_empty() || source_reference_matches(&captures[1], face_name))
    {
        return Some((
            vec![json!({
                "kind": "dealDamageByPermanentCountEachPlayer",
                "where": parse_permanent_criteria(&captures[3], face_name)?,
                "factor": integer(if captures.get(2).is_some() { 2 } else { 1 }),
            })],
            Vec::new(),
        ));
    }

    let opposing_keyword_loss_re =
        Regex::new(r"(?i)^Permanents your opponents control lose (.+?) until end of turn\.$")
            .expect("opposing permanent keyword-loss regex compiles");
    if let Some(captures) = opposing_keyword_loss_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "removeKeywordsFromPermanents",
                "objects": {
                    "kind": "permanents",
                    "controller": { "kind": "opponentsOf", "player": controller() },
                    "where": Value::Null,
                },
                "keywords": oracle_keyword_list(&captures[1])?,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    let source_damage_re = Regex::new(&format!(
        r"(?i)^{} deals ({}) damage to (each opponent|target opponent|any target)\.$",
        source_subject,
        count_word_pattern(),
    ))
    .expect("general source damage instruction regex compiles");
    if let Some(captures) = source_damage_re.captures(instruction) {
        let amount = integer(parse_number_word(&captures[1])?);
        if captures[2].eq_ignore_ascii_case("each opponent") {
            return Some((
                vec![json!({ "kind": "dealDamageToEachOpponent", "amount": amount })],
                Vec::new(),
            ));
        }
        if captures[2].eq_ignore_ascii_case("target opponent") {
            return Some((
                vec![json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": amount,
                    "recipient": chosen_target("targetOpponent"),
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
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "amount": amount,
                "recipient": chosen_target("targetDamageable"),
            })],
            vec![target_decision(
                "targetDamageable",
                json!({ "kind": "anyTarget" }),
                1,
                1,
            )],
        ));
    }
    let attach_controlled_permanents_re = Regex::new(
        r"(?i)^Attach target (.+?) you control to (up to one )?target (.+?) you control\.$",
    )
    .expect("controlled permanent attachment instruction regex compiles");
    if let Some(captures) = attach_controlled_permanents_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "attachPermanent",
                "attachment": chosen_target("targetAttachment"),
                "to": chosen_target("targetAttachmentRecipient"),
            })],
            vec![
                target_decision(
                    "targetAttachment",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": parse_permanent_criteria(&captures[1], face_name)?,
                    }),
                    1,
                    1,
                ),
                target_decision(
                    "targetAttachmentRecipient",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": parse_permanent_criteria(&captures[3], face_name)?,
                    }),
                    if captures.get(2).is_some() { 0 } else { 1 },
                    1,
                ),
            ],
        ));
    }
    let divided_damage_re = Regex::new(&format!(
        r"(?i)^{} deals ({}) damage divided as you choose among (.+?) targets\.$",
        source_subject,
        count_word_pattern(),
    ))
    .expect("divided damage instruction regex compiles");
    if let Some(captures) = divided_damage_re.captures(instruction) {
        let amount = parse_number_word(&captures[1])?;
        let target_counts = parse_consecutive_number_choices(&captures[2])?;
        let maximum = *target_counts.last()?;
        if maximum > amount {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "dealDividedDamage",
                "targetsDecisionId": "dividedDamageTargets",
                "divisionDecisionId": "damageDivision",
            })],
            vec![
                target_decision(
                    "dividedDamageTargets",
                    json!({ "kind": "anyTarget" }),
                    1,
                    maximum,
                ),
                json!({
                    "kind": "divideQuantityAmongTargets",
                    "id": "damageDivision",
                    "quantity": integer(amount),
                    "targetsDecisionId": "dividedDamageTargets",
                    "minimumPerTarget": integer(1),
                }),
            ],
        ));
    }
    let source_target_permanent_damage_re = Regex::new(&format!(
        r"(?i)^{} deals ({}) damage to target (.+)\.$",
        source_subject,
        count_word_pattern(),
    ))
    .expect("source damage to qualified permanent regex compiles");
    if let Some(captures) = source_target_permanent_damage_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "amount": integer(parse_number_word(&captures[1])?),
                "recipient": chosen_target("targetPermanent"),
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[2], face_name)?,
                1,
                1,
            )],
        ));
    }
    let source_count_damage_re = Regex::new(&format!(
        r"(?i)^{} deals damage to (each opponent|any target) equal to the number of (.+) you control\.$",
        source_subject,
    ))
    .expect("general source count damage instruction regex compiles");
    if let Some(captures) = source_count_damage_re.captures(instruction) {
        let amount = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[2], face_name)?,
        });
        if captures[1].eq_ignore_ascii_case("each opponent") {
            return Some((
                vec![json!({ "kind": "dealDamageToEachOpponent", "amount": amount })],
                Vec::new(),
            ));
        }
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "amount": amount,
                "recipient": chosen_target("targetDamageable"),
            })],
            vec![target_decision(
                "targetDamageable",
                json!({ "kind": "anyTarget" }),
                1,
                1,
            )],
        ));
    }
    let source_equal_count_damage_re = Regex::new(&format!(
        r"(?i)^{} deals damage equal to the number of (.+?) you control to (each opponent|any target)\.$",
        source_subject,
    ))
    .expect("general source equal-to-count damage instruction regex compiles");
    if let Some(captures) = source_equal_count_damage_re.captures(instruction) {
        let amount = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
        });
        if captures[2].eq_ignore_ascii_case("each opponent") {
            return Some((
                vec![json!({ "kind": "dealDamageToEachOpponent", "amount": amount })],
                Vec::new(),
            ));
        }
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "amount": amount,
                "recipient": chosen_target("targetDamageable"),
            })],
            vec![target_decision(
                "targetDamageable",
                json!({ "kind": "anyTarget" }),
                1,
                1,
            )],
        ));
    }
    let source_damage_to_opposing_permanents_re = Regex::new(&format!(
        r"(?i)^{} deals ({}) damage to each (.+?) your opponents control\.$",
        source_subject,
        count_word_pattern(),
    ))
    .expect("source damage to each opposing permanent regex compiles");
    if let Some(captures) = source_damage_to_opposing_permanents_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "amount": integer(parse_number_word(captures.get(1)?.as_str())?),
                "recipient": {
                    "kind": "eachPermanent",
                    "player": { "kind": "opponentsOf", "player": controller() },
                    "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
                },
            })],
            Vec::new(),
        ));
    }
    let self_counted_bonus_re = Regex::new(&format!(
        r"(?i)^{} gets ([+-]\d+)/([+-]\d+) until end of turn for each (other )?(.+?) you control\.$",
        source_subject,
    ))
    .expect("temporary self bonus per controlled permanent regex compiles");
    let target_counted_bonus_re = Regex::new(
        r"(?i)^Target (.+?) gets ([+-]\d+)/([+-]\d+) until end of turn for each (.+?) you control\.$",
    )
    .expect("temporary target bonus per controlled permanent regex compiles");
    if let Some(captures) = target_counted_bonus_re.captures(instruction) {
        let count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[4], face_name)?,
        });
        let scaled = |amount: i64| {
            if amount == 0 {
                integer(0)
            } else if amount == 1 {
                count.clone()
            } else {
                json!({ "kind": "multiply", "value": count.clone(), "factor": integer(amount) })
            }
        };
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": chosen_target("targetPermanent"),
                "power": scaled(captures[2].parse::<i64>().ok()?),
                "toughness": scaled(captures[3].parse::<i64>().ok()?),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }
    if let Some(captures) = self_counted_bonus_re.captures(instruction) {
        let mut count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[4], face_name)?,
        });
        if captures.get(3).is_some() {
            count["excludeSource"] = Value::Bool(true);
        }
        let scaled = |amount: i64| {
            if amount == 0 {
                integer(0)
            } else if amount == 1 {
                count.clone()
            } else {
                json!({ "kind": "multiply", "value": count.clone(), "factor": integer(amount) })
            }
        };
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": self_ref(),
                "power": scaled(captures[1].parse::<i64>().ok()?),
                "toughness": scaled(captures[2].parse::<i64>().ok()?),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }
    let optional_source_base_power_toughness_re = Regex::new(
        r"(?i)^You may have (.+?)(?:'s|\x{2019}s) base power and toughness become ([+-]?\d+)/([+-]?\d+) until end of turn\.$",
    )
    .expect("optional temporary source base power toughness regex compiles");
    if let Some(captures) = optional_source_base_power_toughness_re.captures(instruction)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [{
                    "kind": "setBasePowerToughness",
                    "object": self_ref(),
                    "power": integer(captures[2].parse::<i64>().ok()?),
                    "toughness": integer(captures[3].parse::<i64>().ok()?),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            })],
            Vec::new(),
        ));
    }
    let self_bonus_re = Regex::new(&format!(
        r"(?i)^{} gets ([+-]\d+)/([+-]\d+)(?: and gains? (.+))? until end of turn\.$",
        source_subject,
    ))
    .expect("general self power toughness instruction regex compiles");
    if let Some(captures) = self_bonus_re.captures(instruction)
        && !instruction.to_ascii_lowercase().starts_with("target ")
        && !instruction
            .to_ascii_lowercase()
            .starts_with("another target ")
    {
        let mut effects = vec![json!({
            "kind": "modifyPowerToughness",
            "object": self_ref(),
            "power": integer(captures[1].parse::<i64>().ok()?),
            "toughness": integer(captures[2].parse::<i64>().ok()?),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        })];
        if let Some(keyword_text) = captures.get(3) {
            for keyword in oracle_keyword_list(keyword_text.as_str())? {
                effects.push(json!({
                    "kind": "grantKeyword",
                    "object": self_ref(),
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }));
            }
        }
        return Some((effects, Vec::new()));
    }

    let double_self_counters_re = Regex::new(
        r"(?i)^Double the number of ([A-Za-z0-9+/ -]+) counters on (?:this creature|this permanent|it)\.$",
    )
    .expect("double self counters regex compiles");
    if let Some(captures) = double_self_counters_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "doubleCounters",
                "permanent": self_ref(),
                "counter": captures[1].trim(),
            })],
            Vec::new(),
        ));
    }

    let animate_source_land_re = Regex::new(
        r"(?i)^This land becomes a (\d+)/(\d+) (.+?) creature(?: with (.+?))? until end of turn\. It's still a land\.$",
    )
    .expect("animate source land regex compiles");
    if let Some(captures) = animate_source_land_re.captures(instruction) {
        let characteristics = captures[3].split_whitespace().collect::<Vec<_>>();
        let mut add_types = vec!["Creature"];
        if characteristics
            .iter()
            .any(|part| part.eq_ignore_ascii_case("artifact"))
        {
            add_types.push("Artifact");
        }
        let add_subtypes = characteristics
            .into_iter()
            .filter(|part| !part.eq_ignore_ascii_case("artifact"))
            .collect::<Vec<_>>();
        let mut effects = vec![json!({
            "kind": "becomeCreature",
            "object": self_ref(),
            "addTypes": add_types,
            "addSubtypes": add_subtypes,
            "basePower": integer(captures[1].parse::<i64>().ok()?),
            "baseToughness": integer(captures[2].parse::<i64>().ok()?),
            "retainExistingTypes": true,
            "duration": { "kind": "untilEndOfCurrentTurn" },
        })];
        if let Some(keyword_text) = captures.get(4) {
            for keyword in oracle_keyword_list(keyword_text.as_str())? {
                effects.push(json!({
                    "kind": "grantKeyword",
                    "object": self_ref(),
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }));
            }
        }
        return Some((effects, Vec::new()));
    }

    let animate_land_re = Regex::new(
        r"(?i)^Up to one target land you control becomes a (\d+)/(\d+) ([A-Za-z -]+) creature with (.+)\. It's still a land\.$",
    )
    .expect("animate target land regex compiles");
    if let Some(captures) = animate_land_re.captures(instruction) {
        let mut effects = vec![json!({
            "kind": "becomeCreature",
            "object": chosen_target("targetLand"),
            "addTypes": ["Creature"],
            "addSubtypes": captures[3]
                .split_whitespace()
                .collect::<Vec<_>>(),
            "basePower": integer(captures[1].parse::<i64>().ok()?),
            "baseToughness": integer(captures[2].parse::<i64>().ok()?),
            "retainExistingTypes": true,
            "duration": { "kind": "permanent" },
        })];
        for keyword in oracle_keyword_list(&captures[4])? {
            effects.push(json!({
                "kind": "grantKeyword",
                "object": chosen_target("targetLand"),
                "keyword": keyword,
                "duration": { "kind": "permanent" },
            }));
        }
        return Some((
            effects,
            vec![target_decision(
                "targetLand",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Land"),
                }),
                0,
                1,
            )],
        ));
    }

    let target_bonus_re = Regex::new(
        r"(?i)^(?:(another) target|target) (.+?) gets ([+-]\d+)/([+-]\d+)(?: and gains? (.+))? until end of turn\.$",
    )
    .expect("general target power toughness instruction regex compiles");

    let target_count_bonus_re = Regex::new(
        r"(?i)^Target (.+?) gains? (.+?) and gets \+X/([+-]\d+) until end of turn, where X is the number of (.+?) you control\.$",
    )
    .expect("target variable power and keyword instruction regex compiles");
    if let Some(captures) = target_count_bonus_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        let mut effects = oracle_keyword_list(&captures[2])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "object": target,
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                })
            })
            .collect::<Vec<_>>();
        effects.push(json!({
            "kind": "modifyPowerToughness",
            "object": target,
            "power": {
                "kind": "countPermanents",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[4], face_name)?,
            },
            "toughness": integer(captures[3].parse::<i64>().ok()?),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
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

    if let Some(captures) = target_bonus_re.captures(instruction) {
        let mut effects = vec![json!({
            "kind": "modifyPowerToughness",
            "object": chosen_target("targetPermanent"),
            "power": integer(captures[3].parse::<i64>().ok()?),
            "toughness": integer(captures[4].parse::<i64>().ok()?),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        })];
        if let Some(keyword_text) = captures.get(5) {
            for keyword in oracle_keyword_list(keyword_text.as_str())? {
                effects.push(json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetPermanent"),
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }));
            }
        }
        let target_description = if captures.get(1).is_some() {
            format!("another {}", &captures[2])
        } else {
            captures[2].to_string()
        };
        return Some((
            effects,
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&target_description, face_name)?,
                1,
                1,
            )],
        ));
    }

    let controlled_variable_bonus_re = Regex::new(&format!(
        r"(?i)^(.+?) you control get \+X/\+X until end of turn, where X is ({})\.$",
        variable_clause_pattern(),
    ))
    .expect("controlled variable power toughness instruction regex compiles");
    if let Some(captures) = controlled_variable_bonus_re.captures(instruction) {
        let amount = x_variable_expression(captures.get(2)?.as_str())?;
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                },
                "power": amount.clone(),
                "toughness": amount,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }

    let controlled_parameterized_keyword_re = Regex::new(&format!(
        r"(?i)^(.+?) you control gain (.+?) and annihilator ({}) until end of turn\.$",
        count_word_pattern(),
    ))
    .expect("controlled parameterized keyword instruction regex compiles");
    if let Some(captures) = controlled_parameterized_keyword_re.captures(instruction) {
        let objects = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": card_qualifier_list_filter(captures.get(1)?.as_str(), face_name)
                .or_else(|| parse_permanent_criteria(captures.get(1)?.as_str(), face_name))?,
        });
        let mut effects = oracle_keyword_list(captures.get(2)?.as_str())?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "object": objects.clone(),
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                })
            })
            .collect::<Vec<_>>();
        effects.push(json!({
            "kind": "grantKeyword",
            "object": objects,
            "keyword": "annihilator",
            "quantity": integer(parse_number_word(&captures[3])?),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }));
        return Some((effects, Vec::new()));
    }

    let controlled_bonus_re = Regex::new(
        r"(?i)^(Other )?(.+?) you control get ([+-]\d+)/([+-]\d+)(?: and gains? (.+))? until end of turn\.$",
    )
    .expect("general controlled power toughness instruction regex compiles");
    if let Some(captures) = controlled_bonus_re.captures(instruction) {
        let mut objects = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": card_qualifier_list_filter(&captures[2], face_name)
                .or_else(|| parse_permanent_criteria(&captures[2], face_name))?,
        });
        if captures.get(1).is_some() {
            objects["excludeSource"] = Value::Bool(true);
        }
        let mut effects = vec![json!({
            "kind": "modifyPowerToughness",
            "object": objects.clone(),
            "power": integer(captures[3].parse::<i64>().ok()?),
            "toughness": integer(captures[4].parse::<i64>().ok()?),
            "duration": { "kind": "untilEndOfCurrentTurn" },
        })];
        if let Some(keyword_text) = captures.get(5) {
            for keyword in oracle_keyword_list(keyword_text.as_str())? {
                effects.push(json!({
                    "kind": "grantKeyword",
                    "object": objects.clone(),
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }));
            }
        }
        return Some((effects, Vec::new()));
    }

    let group_cant_block_re = Regex::new(r"(?i)^(.+?) can't block this turn\.$")
        .expect("temporary group blocking prohibition regex compiles");
    if let Some(captures) = group_cant_block_re.captures(instruction) {
        let description = captures.get(1)?.as_str();
        if description.to_ascii_lowercase().starts_with("target ")
            || source_reference_matches(description, face_name)
        {
            return None;
        }
        return Some((
            vec![json!({
                "kind": "grantKeyword",
                "object": {
                    "kind": "eachPermanent",
                    "where": parse_permanent_criteria(description, face_name)?,
                },
                "keyword": "cantBlock",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }

    let opponents_controlled_bonus_re = Regex::new(
        r"(?i)^(.+?) your opponents control get ([+-]\d+)/([+-]\d+) until end of turn\.$",
    )
    .expect("general opponents' power toughness instruction regex compiles");
    if let Some(captures) = opponents_controlled_bonus_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": {
                    "kind": "eachPermanent",
                    "excludeSourceController": true,
                    "where": card_qualifier_list_filter(&captures[1], face_name)
                        .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
                },
                "power": integer(captures[2].parse::<i64>().ok()?),
                "toughness": integer(captures[3].parse::<i64>().ok()?),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }

    let target_player_controlled_bonus_re = Regex::new(
        r"(?i)^(.+?) target player controls get ([+-]\d+)/([+-]\d+) until end of turn\.$",
    )
    .expect("target-player controlled power toughness instruction regex compiles");
    if let Some(captures) = target_player_controlled_bonus_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": {
                    "kind": "eachPermanent",
                    "player": chosen_target("targetPlayer"),
                    "where": card_qualifier_list_filter(&captures[1], face_name)
                        .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
                },
                "power": integer(captures[2].parse::<i64>().ok()?),
                "toughness": integer(captures[3].parse::<i64>().ok()?),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            vec![target_decision(
                "targetPlayer",
                json!({ "kind": "players" }),
                1,
                1,
            )],
        ));
    }

    let global_bonus_re = Regex::new(
        r"(?i)^(?:All|Each) (.+?) get(?:s)? ([+-](?:\d+|X))/([+-](?:\d+|X)) until end of turn\.$",
    )
    .expect("general global power toughness instruction regex compiles");
    if let Some(captures) = global_bonus_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": {
                    "kind": "eachPermanent",
                    "where": card_qualifier_list_filter(&captures[1], face_name)
                        .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
                },
                "power": parse_signed_stat_expression(&captures[2])?,
                "toughness": parse_signed_stat_expression(&captures[3])?,
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            Vec::new(),
        ));
    }

    let attached_keywords_re =
        Regex::new(r"(?i)^(?:Enchanted (?:creature|permanent)|Equipped creature) gains? (.+) until end of turn\.$")
            .expect("attached permanent keyword instruction regex compiles");
    if let Some(captures) = attached_keywords_re.captures(instruction) {
        let effects = oracle_keyword_list(&captures[1])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "object": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                })
            })
            .collect();
        return Some((effects, Vec::new()));
    }

    let source_keywords_re = Regex::new(r"(?i)^(.+?) gains? (.+) until end of turn\.$")
        .expect("source keyword instruction regex compiles");
    if let Some(captures) = source_keywords_re.captures(instruction) {
        let subject = captures[1].trim();
        if subject.eq_ignore_ascii_case("this creature")
            || subject.eq_ignore_ascii_case("this permanent")
            || source_reference_matches(subject, face_name)
            || face_name.is_empty()
                && subject.chars().next().is_some_and(char::is_uppercase)
                && subject
                    .chars()
                    .all(|character| character.is_alphanumeric() || " ,'-".contains(character))
        {
            let effects = oracle_keyword_list(&captures[2])?
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "object": self_ref(),
                        "keyword": keyword,
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    })
                })
                .collect();
            return Some((effects, Vec::new()));
        }
    }

    let controlled_keywords_re =
        Regex::new(r"(?i)^(.+?) you control gains? (.+) until end of turn\.$")
            .expect("general controlled keyword instruction regex compiles");
    if let Some(captures) = controlled_keywords_re.captures(instruction) {
        let objects = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": card_qualifier_list_filter(&captures[1], face_name)
                .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
        });
        let effects = oracle_keyword_list(&captures[2])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "object": objects.clone(),
                    "keyword": keyword,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                })
            })
            .collect();
        return Some((effects, Vec::new()));
    }

    let temporary_add_type_and_keyword_re = Regex::new(
        r"(?i)^Until end of turn, target (.+?) becomes (?:a|an) (artifact|battle|creature|enchantment|land|planeswalker) in addition to its other types and gains? (.+)\.$",
    )
    .expect("temporary additional card-type and keyword regex compiles");
    if let Some(captures) = temporary_add_type_and_keyword_re.captures(instruction) {
        let card_type = match captures[2].to_ascii_lowercase().as_str() {
            "artifact" => "Artifact",
            "battle" => "Battle",
            "creature" => "Creature",
            "enchantment" => "Enchantment",
            "land" => "Land",
            "planeswalker" => "Planeswalker",
            _ => return None,
        };
        let target = chosen_target("targetPermanent");
        let mut effects = vec![json!({
            "kind": "addCardType",
            "object": target.clone(),
            "cardType": card_type,
            "retainExistingTypes": true,
            "duration": { "kind": "untilEndOfCurrentTurn" },
        })];
        effects.extend(
            oracle_keyword_list(&captures[3])?
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "object": target.clone(),
                        "keyword": keyword,
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    })
                }),
        );
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

    let destroy_then_controller_search_re = Regex::new(
        r"(?i)^Destroy (up to one )?target (.+?)\. (?:Its controller|That player) may search their library for (?:a|an) (.+?) card, put (?:it|that card) onto the battlefield( tapped)?, then shuffle\.$",
    )
    .expect("destroy then target-controller library search regex compiles");
    if let Some(captures) = destroy_then_controller_search_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        let affected_player = json!({ "kind": "boundValue", "id": "targetController" });
        let search_effects = search_library_effects_for(
            affected_player.clone(),
            parse_permanent_criteria(&captures[3], face_name)?,
            1,
            "battlefield",
            captures.get(4).is_some(),
        );
        return Some((
            vec![
                json!({
                    "kind": "bind",
                    "id": "targetController",
                    "value": { "kind": "controllerOf", "object": target.clone() },
                }),
                json!({
                    "kind": "destroyPermanent",
                    "permanent": target,
                }),
                json!({
                    "kind": "optionalEffects",
                    "player": affected_player,
                    "effects": search_effects,
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[2], face_name)?,
                if captures.get(1).is_some() { 0 } else { 1 },
                1,
            )],
        ));
    }

    let multi_target_zone_change_re = Regex::new(&format!(
        r"(?i)^(Destroy|Exile) up to ({}) target (.+)\.$",
        count_word_pattern(),
    ))
    .expect("multi-target zone change instruction regex compiles");
    if let Some(captures) = multi_target_zone_change_re.captures(instruction) {
        if let Some(candidates) = permanent_target_candidates(&captures[3], face_name) {
            let maximum = parse_number_word(&captures[2])?;
            return Some((
                vec![json!({
                    "kind": if captures[1].eq_ignore_ascii_case("destroy") {
                        "destroyPermanent"
                    } else {
                        "exilePermanent"
                    },
                    "permanent": { "kind": "chosenTargets", "id": "targetPermanents" },
                })],
                vec![target_decision("targetPermanents", candidates, 0, maximum)],
            ));
        }
    }

    let target_zone_change_re = Regex::new(r"(?i)^(Destroy|Exile) (up to one )?target (.+)\.$")
        .expect("general target zone change instruction regex compiles");
    if let Some(captures) = target_zone_change_re
        .captures(instruction)
        .filter(|captures| !captures[3].contains(" if it's "))
    {
        if let Some(candidates) = permanent_target_candidates(&captures[3], face_name) {
            let effect = json!({
                "kind": if captures[1].eq_ignore_ascii_case("destroy") {
                    "destroyPermanent"
                } else {
                    "exilePermanent"
                },
                "permanent": chosen_target("targetPermanent"),
            });
            return Some((
                vec![effect],
                vec![target_decision(
                    "targetPermanent",
                    candidates,
                    i64::from(captures.get(2).is_none()),
                    1,
                )],
            ));
        }
    }

    let counter_unless_re =
        Regex::new(r"(?i)^Counter target (.+?) unless its controller pays ((?:\{[^}]+\})+)\.$")
            .expect("general counter-unless criteria regex compiles");
    if let Some(captures) = counter_unless_re.captures(instruction) {
        let description = captures[1].trim();
        let spell_description = if description.eq_ignore_ascii_case("spell") {
            ""
        } else {
            description.strip_suffix(" spell")?
        };
        let where_filter = if spell_description.is_empty() {
            Value::Null
        } else {
            card_qualifier_list_filter(spell_description, face_name)
                .or_else(|| parse_permanent_criteria(spell_description, face_name))?
        };
        return Some((
            vec![json!({
                "kind": "counterStackObjectUnlessPays",
                "spell": chosen_target("targetStackObject"),
                "manaCost": &captures[2],
            })],
            vec![target_decision(
                "targetStackObject",
                json!({ "kind": "spells", "where": where_filter }),
                1,
                1,
            )],
        ));
    }

    let exile_target_spells_re = Regex::new(r"(?i)^Exile any number of target spells\.$")
        .expect("variable-count target spell exile regex compiles");
    if exile_target_spells_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "exileTargetCards",
                "cards": { "kind": "chosenTargets", "id": "targetSpells" },
            })],
            vec![target_decision(
                "targetSpells",
                json!({ "kind": "spells" }),
                0,
                64,
            )],
        ));
    }

    let exile_graveyard_cards_re = Regex::new(&format!(
        r"(?i)^Exile (up to )?({}) target cards? from (graveyards|a single graveyard)\.$",
        numeric_expression_pattern(),
    ))
    .expect("target graveyard card exile regex compiles");
    if let Some(captures) = exile_graveyard_cards_re.captures(instruction) {
        let maximum = parse_numeric_expression_text(&captures[2])?;
        let minimum = if captures.get(1).is_some() {
            integer(0)
        } else if parse_number_word(&captures[2]).is_some() {
            maximum.clone()
        } else {
            return None;
        };
        let mut decision = json!({
            "id": "targetCards",
            "kind": "chooseTargets",
            "minimum": minimum,
            "maximum": maximum,
            "candidates": {
                "kind": "cards",
                "zone": { "kind": "anyGraveyard" },
                "where": Value::Null,
            },
        });
        if captures[3].eq_ignore_ascii_case("a single graveyard") {
            decision["selectionConstraint"] = json!({
                "kind": "sameZoneOwner",
                "zone": "graveyard",
            });
        }
        return Some((
            vec![json!({
                "kind": "exileTargetCards",
                "cards": { "kind": "chosenTargets", "id": "targetCards" },
            })],
            vec![decision],
        ));
    }

    let counter_re =
        Regex::new(r"(?i)^Counter target (.+?)(?: if it's (white|blue|black|red|green))?\.$")
            .expect("general counter stack-object criteria regex compiles");
    if let Some(captures) = counter_re.captures(instruction) {
        let description = captures[1].trim();
        let ability_description = description
            .strip_suffix(" ability")
            .or_else(|| description.strip_suffix(" abilities"));
        let ability_filters = ability_description.and_then(|description| {
            Regex::new(r"(?i)\s+or\s+")
                .expect("stack ability alternatives regex compiles")
                .split(description)
                .map(|term| match term.trim().to_ascii_lowercase().as_str() {
                    "activated" => Some(json!({ "kind": "isActivatedAbility" })),
                    "triggered" => Some(json!({ "kind": "isTriggeredAbility" })),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
        });
        let candidates = if let Some(filters) = ability_filters {
            json!({
                "kind": "stackItems",
                "where": if filters.len() == 1 {
                    filters[0].clone()
                } else {
                    or(filters)
                },
            })
        } else if description.eq_ignore_ascii_case("triggered ability or colorless spell") {
            json!({
                "kind": "stackItems",
                "where": {
                    "kind": "or",
                    "operands": [
                        { "kind": "isTriggeredAbility" },
                        {
                            "kind": "and",
                            "operands": [
                                { "kind": "isSpell" },
                                { "kind": "isColorless" },
                            ],
                        },
                    ],
                },
            })
        } else {
            let spell_description = if description.eq_ignore_ascii_case("spell") {
                ""
            } else {
                description.strip_suffix(" spell")?
            };
            let mut candidates = json!({ "kind": "spells" });
            if !spell_description.is_empty() {
                candidates["where"] = card_qualifier_list_filter(spell_description, face_name)
                    .or_else(|| parse_permanent_criteria(spell_description, face_name))?;
            }
            candidates
        };
        let counter = json!({
            "kind": "counterStackObject",
            "object": chosen_target("targetStackObject"),
        });
        let effects = if let Some(color) = captures.get(2) {
            vec![json!({
                "kind": "conditional",
                "condition": {
                    "kind": "objectMatchesFilter",
                    "object": chosen_target("targetStackObject"),
                    "where": color_filter(color.as_str())?,
                },
                "then": [counter],
            })]
        } else {
            vec![counter]
        };
        return Some((
            effects,
            vec![target_decision("targetStackObject", candidates, 1, 1)],
        ));
    }
    let counter_linked_spell_re = Regex::new(r"(?i)^Counter (?:that spell|it)\.$")
        .expect("linked spell counter regex compiles");
    let counter_linked_unless_discard_re = Regex::new(
        r"(?i)^Counter (?:that spell|it) unless its controller discards (?:a|one) (.+?)?card\.$",
    )
    .expect("linked spell counter unless discard regex compiles");
    if let Some(captures) = counter_linked_unless_discard_re.captures(instruction) {
        let qualifier = captures
            .get(1)
            .map(|value| value.as_str().trim())
            .unwrap_or_default();
        return Some((
            vec![json!({
                "kind": "counterStackObjectUnlessPays",
                "spell": { "kind": "triggeringStackObject" },
                "cost": {
                    "kind": "discardCard",
                    "where": if qualifier.is_empty() {
                        Value::Null
                    } else {
                        parse_permanent_criteria(qualifier, face_name)?
                    },
                },
            })],
            Vec::new(),
        ));
    }
    if counter_linked_spell_re.is_match(instruction) {
        return Some((
            vec![json!({
                "kind": "counterStackObject",
                "object": { "kind": "triggeringStackObject" },
            })],
            Vec::new(),
        ));
    }

    let destroy_if_color_re =
        Regex::new(r"(?i)^Destroy target (.+?) if it's (white|blue|black|red|green)\.$")
            .expect("general conditional destroy regex compiles");
    if let Some(captures) = destroy_if_color_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "conditional",
                "condition": {
                    "kind": "objectMatchesFilter",
                    "object": chosen_target("targetPermanent"),
                    "where": color_filter(&captures[2])?,
                },
                "then": [{
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetPermanent"),
                }],
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }

    let global_damage_re = Regex::new(&format!(
        r"(?i)^.+? deals ({}) damage to each (.+)\.$",
        count_word_pattern(),
    ))
    .expect("global permanent damage regex compiles");
    if let Some(captures) = global_damage_re.captures(instruction) {
        let criteria = captures[2].replace("each ", "");
        return Some((
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "amount": integer(parse_number_word(&captures[1])?),
                "recipient": {
                    "kind": "eachPermanent",
                    "where": card_qualifier_list_filter(&criteria, face_name)
                        .or_else(|| parse_permanent_criteria(&criteria, face_name))?,
                },
            })],
            Vec::new(),
        ));
    }

    let global_destroy_re = Regex::new(r"(?i)^Destroy all (.+)\.$")
        .expect("global permanent destruction regex compiles");
    if let Some(captures) = global_destroy_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "destroyPermanent",
                "permanent": {
                    "kind": "eachPermanent",
                    "where": parse_permanent_criteria(&captures[1], face_name)?,
                },
            })],
            Vec::new(),
        ));
    }

    if instruction.eq_ignore_ascii_case("Return target spell to its owner's hand.") {
        return Some((
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": chosen_target("targetSpell"),
            })],
            vec![target_decision(
                "targetSpell",
                json!({ "kind": "stackItems", "where": { "kind": "isSpell" } }),
                1,
                1,
            )],
        ));
    }

    let target_spell_or_permanent_return_re = Regex::new(
        r"(?i)^Return target spell or (.+?) permanent(?: (.+?))? to its owner's hand\.$",
    )
    .expect("spell-or-permanent return regex compiles");
    if let Some(captures) = target_spell_or_permanent_return_re.captures(instruction) {
        let mut permanent_description = format!("{} permanent", captures[1].trim());
        if let Some(qualifier) = captures.get(2) {
            permanent_description.push(' ');
            permanent_description.push_str(qualifier.as_str().trim());
        }
        return Some((
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": chosen_target("targetSpellOrPermanent"),
            })],
            vec![target_decision(
                "targetSpellOrPermanent",
                json!({
                    "kind": "union",
                    "sets": [
                        { "kind": "stackItems", "where": { "kind": "isSpell" } },
                        permanent_target_candidates(&permanent_description, face_name)?,
                    ],
                }),
                1,
                1,
            )],
        ));
    }

    let target_players_draw_re = Regex::new(&format!(
        r"(?i)^(Up to )?({}) target players each draw (a|{}) cards?\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("multiple target players draw regex compiles");
    if let Some(captures) = target_players_draw_re.captures(instruction) {
        let maximum = parse_number_word(&captures[2])?;
        let draw_count = if captures[3].eq_ignore_ascii_case("a") {
            1
        } else {
            parse_number_word(&captures[3])?
        };
        return Some((
            vec![json!({
                "kind": "drawCards",
                "player": { "kind": "chosenTargets", "id": "targetPlayers" },
                "count": integer(draw_count),
            })],
            vec![target_decision(
                "targetPlayers",
                json!({ "kind": "players" }),
                if captures.get(1).is_some() {
                    0
                } else {
                    maximum
                },
                maximum,
            )],
        ));
    }

    let target_return_re = Regex::new(
        r"(?i)^Return (up to one )?(?:(other|another) )?target (.+) to its owner's hand\.$",
    )
    .expect("general target return instruction regex compiles");
    if let Some(captures) = target_return_re.captures(instruction) {
        let description = captures
            .get(2)
            .map(|qualifier| format!("{} {}", qualifier.as_str(), &captures[3]))
            .unwrap_or_else(|| captures[3].to_string());
        return Some((
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": chosen_target("targetPermanent"),
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&description, face_name)?,
                i64::from(captures.get(1).is_none()),
                1,
            )],
        ));
    }

    let destroy_then_life_re = Regex::new(&format!(
        r"(?i)^Destroy target (.+)\. Its controller loses ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("general destroy then controller life loss regex compiles");
    if let Some(captures) = destroy_then_life_re.captures(instruction) {
        return Some((
            vec![
                json!({
                    "kind": "bind",
                    "id": "destroyedPermanentController",
                    "value": {
                        "kind": "controllerOf",
                        "object": chosen_target("targetPermanent"),
                    },
                }),
                json!({
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetPermanent"),
                }),
                json!({
                    "kind": "loseLife",
                    "player": { "kind": "boundValue", "id": "destroyedPermanentController" },
                    "amount": integer(parse_number_word(&captures[2])?),
                }),
            ],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(&captures[1], face_name)?,
                1,
                1,
            )],
        ));
    }

    let multi_target_tap_re =
        Regex::new(r"(?i)^Tap (one|two|one or two|up to one|up to two) target (.+)\.$")
            .expect("bounded multi-target tap instruction regex compiles");
    if let Some(captures) = multi_target_tap_re.captures(instruction) {
        let cardinality = captures[1].to_ascii_lowercase();
        let maximum = if cardinality.contains("two") { 2 } else { 1 };
        let minimum = if cardinality.starts_with("up to") {
            0
        } else if cardinality == "two" {
            2
        } else {
            1
        };
        return Some((
            vec![json!({
                "kind": "tapPermanent",
                "permanent": { "kind": "chosenTargets", "id": "targetPermanents" },
            })],
            vec![target_decision(
                "targetPermanents",
                permanent_target_candidates(&singular_card_term(&captures[2]), face_name)?,
                minimum,
                maximum,
            )],
        ));
    }

    let target_tap_re = Regex::new(r"(?i)^(Tap|Untap) (another )?target (.+)\.$")
        .expect("general target tap instruction regex compiles");
    if let Some(captures) = target_tap_re.captures(instruction) {
        let mut candidates = permanent_target_candidates(&captures[3], face_name)?;
        if captures.get(2).is_some() {
            candidates["excludeSource"] = Value::Bool(true);
        }
        return Some((
            vec![json!({
                "kind": if captures[1].eq_ignore_ascii_case("tap") {
                    "tapPermanent"
                } else {
                    "untapPermanent"
                },
                "permanent": chosen_target("targetPermanent"),
            })],
            vec![target_decision("targetPermanent", candidates, 1, 1)],
        ));
    }

    let untap_controlled_re = Regex::new(r"(?i)^Untap all (.+?) you control\.$")
        .expect("untap all controlled permanents regex compiles");
    if let Some(captures) = untap_controlled_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "untapPermanentsMatching",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            })],
            Vec::new(),
        ));
    }

    let untap_source_re = Regex::new(
        r"(?i)^Untap (this (?:artifact|creature|enchantment|equipment|land|permanent)|it)\.$",
    )
    .expect("untap ability source regex compiles");
    if untap_source_re.is_match(instruction) {
        return Some((
            vec![json!({ "kind": "untapPermanent", "permanent": self_ref() })],
            Vec::new(),
        ));
    }

    None
}
