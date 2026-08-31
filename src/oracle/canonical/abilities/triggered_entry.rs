use super::super::*;

pub(in crate::oracle::canonical) fn entry_trigger_rule(
    declaration: Option<Value>,
    effects: Vec<Value>,
) -> CanonicalRuleDraft {
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": { "kind": "enterBattlefield", "object": self_ref() },
        "effects": effects,
    });
    if let Some(declaration) = declaration {
        rule["declaration"] = declaration;
    }
    draft(
        rule,
        &[
            "Recognize the entering-permanent event",
            "Declare semantic choices and targets",
            "Resolve the composed effects in Oracle order",
        ],
    )
}

pub(in crate::oracle::canonical) fn parse_source_entry_trigger<'a>(
    text: &'a str,
    face_name: &str,
) -> Option<(Value, &'a str)> {
    let (event_text, instruction) = text.split_once(", ")?;
    let subject_and_event = strip_prefix_ascii_case(event_text, "When ")
        .or_else(|| strip_prefix_ascii_case(event_text, "Whenever "))?;
    let subject = [" enters the battlefield", " comes into play", " enters"]
        .into_iter()
        .find_map(|suffix| strip_suffix_ascii_case(subject_and_event, suffix))?
        .trim();
    let source_kind = strip_prefix_ascii_case(subject, "this ");
    let is_source = source_kind.is_some_and(|kind| {
        [
            "artifact",
            "aura",
            "creature",
            "enchantment",
            "equipment",
            "land",
            "permanent",
            "room",
            "spacecraft",
            "token",
            "vehicle",
        ]
        .iter()
        .any(|candidate| kind.eq_ignore_ascii_case(candidate))
    }) || source_reference_matches(subject, face_name);
    is_source.then(|| {
        (
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            instruction,
        )
    })
}

pub(in crate::oracle::canonical) fn mana_value_candidate_filter(
    operator: &str,
    value: i64,
) -> Value {
    compare(
        operator,
        json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
        integer(value),
    )
}

pub(in crate::oracle::canonical) fn entry_search_filter(description: &str) -> Option<Value> {
    let normalized = strip_leading_article(description.trim())
        .trim_end_matches(" cards")
        .trim_end_matches(" card")
        .trim();
    if normalized.eq_ignore_ascii_case("nonlegendary") {
        return Some(not(json!({ "kind": "isLegendary" })));
    }
    if let Some(subtype_name) = normalized
        .strip_prefix("basic ")
        .or_else(|| normalized.strip_prefix("Basic "))
    {
        return Some(and(vec![
            json!({ "kind": "typeLineContains", "value": "Basic" }),
            known_card_type_filter(subtype_name)
                .unwrap_or_else(|| subtype(&singular_card_term(subtype_name))),
        ]));
    }
    if let Some(card_kind) = normalized.strip_prefix("nonlegendary ") {
        return Some(and(vec![
            not(json!({ "kind": "isLegendary" })),
            known_card_type_filter(card_kind)?,
        ]));
    }
    if description == "a legendary creature card" {
        return Some(and(vec![
            card_type("Creature"),
            json!({ "kind": "isLegendary" }),
        ]));
    }
    let creature_mana_value =
        Regex::new(r"^a creature card with mana value (\d+) or (greater|less)$")
            .expect("entry creature search filter regex compiles");
    if let Some(captures) = creature_mana_value.captures(description) {
        return Some(and(vec![
            card_type("Creature"),
            mana_value_candidate_filter(
                if &captures[2] == "greater" {
                    ">="
                } else {
                    "<="
                },
                captures[1].parse::<i64>().ok()?,
            ),
        ]));
    }
    let basic_or_small_creature =
        Regex::new(r"^a basic ([A-Za-z]+) card or a creature card with mana value (\d+) or less$")
            .expect("entry basic-or-creature search filter regex compiles");
    if let Some(captures) = basic_or_small_creature.captures(description) {
        return Some(or(vec![
            and(vec![
                json!({ "kind": "typeLineContains", "value": "Basic" }),
                subtype(&captures[1]),
            ]),
            and(vec![
                card_type("Creature"),
                mana_value_candidate_filter("<=", captures[2].parse::<i64>().ok()?),
            ]),
        ]));
    }
    let description = description.trim().trim_end_matches(" card");
    let alternatives = Regex::new(r"\s+or\s+")
        .expect("entry search alternatives regex compiles")
        .split(description)
        .map(|criterion| {
            let criterion = strip_leading_article(criterion);
            if criterion.eq_ignore_ascii_case("basic land") {
                return Some(and(vec![
                    json!({ "kind": "typeLineContains", "value": "Basic" }),
                    card_type("Land"),
                ]));
            }
            if let Some(filter) = known_card_type_filter(criterion) {
                return Some(filter);
            }
            let subtype_name = singular_card_term(criterion);
            (subtype_name.chars().next().is_some_and(char::is_uppercase)
                && subtype_name
                    .chars()
                    .all(|character| character.is_alphabetic() || character == '-'))
            .then(|| subtype(&subtype_name))
        })
        .collect::<Option<Vec<_>>>()?;
    match alternatives.as_slice() {
        [filter] => Some(filter.clone()),
        _ => Some(or(alternatives)),
    }
}

pub(in crate::oracle::canonical) fn linked_exile_candidates(
    description: &str,
    exclude_source: bool,
) -> Option<Value> {
    let mut candidates = permanent_target_candidates(description, "")?;
    if exclude_source {
        candidates["excludeSource"] = Value::Bool(true);
    }
    Some(candidates)
}

pub(in crate::oracle::canonical) fn parse_composed_entry_triggered(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let normalized;
    let text = if let Some((instruction, _)) = text.split_once(". (") {
        normalized = format!("{instruction}.");
        normalized.as_str()
    } else {
        text
    };

    let aura_reanimate_re = Regex::new(
        r#"(?i)^When this Aura enters, if it's on the battlefield, it loses \"(Enchant (.+?) card in (?:a|your|an opponent's) graveyard)\" and gains \"(Enchant (.+?) put onto the battlefield with this Aura)\.\" Return enchanted (.+?) card to the battlefield under your control and attach this Aura to it\. When this Aura leaves the battlefield, that (?:creature|permanent)'s controller sacrifices it\.$"#,
    )
    .expect("linked Aura reanimation regex compiles");
    if let Some(captures) = aura_reanimate_re.captures(text) {
        let old_ability_text = captures.get(1)?.as_str();
        let old_criteria = captures.get(2)?.as_str();
        let new_criteria = captures.get(4)?.as_str();
        let returned_criteria = captures.get(5)?.as_str();
        let old_filter = parse_permanent_criteria(old_criteria, "")?;
        let new_filter = parse_permanent_criteria(new_criteria, "")?;
        let returned_filter = parse_permanent_criteria(returned_criteria, "")?;
        if old_filter != returned_filter || new_filter != returned_filter {
            return None;
        }
        let old_rule = parse_common_static_ability(old_ability_text, "")?.rule;
        let new_rule = json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": {
                "kind": "enchant",
                "where": returned_filter,
                "linkedToSource": true,
            },
        });
        let mut parsed = entry_trigger_rule(
            None,
            vec![
                json!({
                    "kind": "replaceAbility",
                    "object": self_ref(),
                    "from": old_rule,
                    "to": new_rule,
                    "duration": { "kind": "permanent" },
                }),
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "attachedObject",
                        "attachment": self_ref(),
                    },
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                }),
                json!({
                    "kind": "grantAbility",
                    "object": self_ref(),
                    "duration": { "kind": "permanent" },
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": self_ref(),
                        "event": {
                            "kind": "permanentLeftBattlefield",
                            "object": self_ref(),
                        },
                        "effects": [{
                            "kind": "sacrificePermanent",
                            "permanent": {
                                "kind": "attachedPermanent",
                                "attachment": self_ref(),
                            },
                        }],
                    },
                }),
            ],
        );
        parsed.rule["condition"] = json!({ "kind": "sourceOnBattlefield" });
        return Some(parsed);
    }

    if text
        == "When this creature enters, look at the top five cards of your library. You may reveal a land card or a card with {X} in its mana cost from among them and put it into your hand. Put the rest on the bottom of your library in a random order."
    {
        return Some(entry_trigger_rule(
            None,
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(5),
                    "bind": "entryLookedCards",
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "entryCardForHand",
                    "player": controller(),
                    "from": bound_objects("entryLookedCards"),
                    "where": or(vec![
                        card_type("Land"),
                        json!({ "kind": "manaCostContainsX" }),
                    ]),
                    "minimum": 0,
                    "maximum": 1,
                }),
                json!({
                    "kind": "revealCards",
                    "cards": decision_result("entryCardForHand"),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("entryCardForHand"),
                    "to": hand(controller()),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "setDifference",
                        "left": bound_objects("entryLookedCards"),
                        "right": decision_result("entryCardForHand"),
                    },
                    "to": {
                        "kind": "library",
                        "player": controller(),
                        "position": "bottom",
                    },
                    "order": { "kind": "random" },
                }),
            ],
        ));
    }

    if text
        == "When this land enters, you may look at the top five cards of your library. If you do, reveal up to one basic land card from among them, then put that card on top of your library and the rest on the bottom in any order."
    {
        return Some(entry_trigger_rule(
            None,
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [
                    {
                        "kind": "lookAtTopCards",
                        "zone": library(controller()),
                        "count": integer(5),
                        "bind": "entryLookedCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "entryCardForTop",
                        "player": controller(),
                        "from": bound_objects("entryLookedCards"),
                        "where": json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                        "minimum": 0,
                        "maximum": 1,
                    },
                    {
                        "kind": "revealCards",
                        "cards": decision_result("entryCardForTop"),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("entryCardForTop"),
                        "to": {
                            "kind": "library",
                            "player": controller(),
                            "position": "top",
                        },
                    },
                    {
                        "kind": "chooseOrder",
                        "id": "entryBottomOrder",
                        "player": controller(),
                        "objects": {
                            "kind": "setDifference",
                            "left": bound_objects("entryLookedCards"),
                            "right": decision_result("entryCardForTop"),
                        },
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("entryBottomOrder"),
                        "to": {
                            "kind": "library",
                            "player": controller(),
                            "position": "bottom",
                        },
                    },
                ],
            })],
        ));
    }

    if text
        == "When this enchantment leaves the battlefield, return the exiled card to the battlefield under its owner's control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentLeftBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "returnCardsExiledWithSource",
                }],
            }),
            &[
                "Recognize the source leaving",
                "Return cards linked to that source",
            ],
        ));
    }

    let linked_return_to_hand_re = Regex::new(
        r"^When this (?:artifact|creature|enchantment|permanent) leaves the battlefield, return the exiled card to its owner's hand\.$",
    )
    .expect("linked exile return-to-hand regex compiles");
    if linked_return_to_hand_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentLeftBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "returnCardsExiledWithSourceToOwnersHand",
                }],
            }),
            &[
                "Recognize the source leaving",
                "Return cards linked to that source to their owners' hands",
            ],
        ));
    }

    if text
        == "When this creature leaves the battlefield, put its counters on target creature you control."
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
                    "decisions": [target_decision(
                        "counterRecipient",
                        candidates.clone(),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "putSameCountersAs",
                    "permanent": chosen_target("counterRecipient"),
                    "source": self_ref(),
                    "candidates": candidates,
                }],
            }),
            &[
                "Recognize the source leaving",
                "Choose a controlled creature",
                "Move each counter kind",
            ],
        ));
    }

    let per_opponent_linked_exile_re = Regex::new(
        r"(?i)^When this (Aura|artifact|creature|enchantment) enters, for each opponent, exile up to one target (.+?) that player controls until this (Aura|artifact|creature|enchantment) leaves the battlefield\.$",
    )
    .expect("per-opponent linked exile regex compiles");
    if let Some(captures) = per_opponent_linked_exile_re.captures(text)
        && captures[1].eq_ignore_ascii_case(&captures[3])
    {
        let mut candidates = linked_exile_candidates(&captures[2], false)?;
        candidates["controller"] = json!({
            "kind": "opponentsOf",
            "player": controller(),
        });
        let mut decision = target_decision("exileTargets", candidates.clone(), 0, 0);
        decision["maximum"] = json!({
            "kind": "countOpponents",
            "player": controller(),
        });
        decision["selectionConstraint"] = json!({
            "kind": "distinctPermanentControllers",
        });
        return Some(entry_trigger_rule(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [decision],
            })),
            vec![json!({
                "kind": "exilePermanentWithSource",
                "permanent": {
                    "kind": "chosenTargets",
                    "id": "exileTargets",
                },
                "candidates": candidates,
                "minimum": 0,
                "maximum": {
                    "kind": "countOpponents",
                    "player": controller(),
                },
                "returnWhenSourceLeaves": true,
            })],
        ));
    }

    let search_re = Regex::new(
        r"^When .+ enters, (?:you may )?search your library for (.+), reveal it, put it into your hand, then shuffle\.$",
    )
    .expect("entry search trigger regex compiles");
    if let Some(captures) = search_re.captures(text) {
        let filter = entry_search_filter(&captures[1])?;
        let mut effects = search_library_effects(filter, 1, "hand", false);
        effects.insert(
            1,
            json!({ "kind": "revealCards", "cards": decision_result("searchedCards") }),
        );
        return Some(entry_trigger_rule(None, effects));
    }

    let linked_exile_with_followup_re = Regex::new(
        r"(?i)^When this (Aura|artifact|creature|enchantment) enters, exile (up to one )?(another )?target (.+?) until this (Aura|artifact|creature|enchantment) leaves the battlefield\. (.+)$",
    )
    .expect("linked exile followed by another entry effect regex compiles");
    if let Some(captures) = linked_exile_with_followup_re.captures(text)
        && captures[1].eq_ignore_ascii_case(&captures[5])
    {
        let minimum = i64::from(captures.get(2).is_none());
        let candidates = linked_exile_candidates(&captures[4], captures.get(3).is_some())?;
        let (mut trailing_effects, trailing_decisions) =
            parse_general_effect_instruction(&captures[6], "")?;
        if !trailing_decisions.is_empty() {
            return None;
        }
        let mut effects = vec![json!({
            "kind": "exilePermanentWithSource",
            "permanent": chosen_target("exileTarget"),
            "candidates": candidates.clone(),
            "minimum": minimum,
            "maximum": 1,
            "returnWhenSourceLeaves": true,
        })];
        effects.append(&mut trailing_effects);
        return Some(entry_trigger_rule(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision("exileTarget", candidates, minimum, 1)],
            })),
            effects,
        ));
    }
    let linked_exile_re = Regex::new(
        r"^When this (?:Aura|enchantment|creature) enters, exile (up to one )?(another )?target (.+) until this (?:Aura|enchantment|creature) leaves the battlefield(?:\. \(.+\))?\.$",
    )
    .expect("linked exile trigger regex compiles");
    if let Some(captures) = linked_exile_re.captures(text) {
        let minimum = i64::from(captures.get(1).is_none());
        let candidates = linked_exile_candidates(&captures[3], captures.get(2).is_some())?;
        return Some(entry_trigger_rule(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "exileTarget",
                    candidates.clone(),
                    minimum,
                    1,
                )],
            })),
            vec![json!({
                "kind": "exilePermanentWithSource",
                "permanent": chosen_target("exileTarget"),
                "candidates": candidates,
                "minimum": minimum,
                "maximum": 1,
                "returnWhenSourceLeaves": true,
            })],
        ));
    }

    let independent_exile_re = Regex::new(
        r"^When this (?:enchantment|creature) enters, exile (up to one )?(another )?target (.+)\.$",
    )
    .expect("independent exile trigger regex compiles");
    if let Some(captures) = independent_exile_re.captures(text) {
        let minimum = i64::from(captures.get(1).is_none());
        let candidates = linked_exile_candidates(&captures[3], captures.get(2).is_some())?;
        return Some(entry_trigger_rule(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "exileTarget",
                    candidates.clone(),
                    minimum,
                    1,
                )],
            })),
            vec![json!({
                "kind": "exilePermanentWithSource",
                "permanent": chosen_target("exileTarget"),
                "candidates": candidates,
                "minimum": minimum,
                "maximum": 1,
                "returnWhenSourceLeaves": false,
            })],
        ));
    }

    let counters_for_target_player_re = Regex::new(
        r"^When .+ enters, put a ([^ ]+) counter on each creature target player controls\.$",
    )
    .expect("entry target-player counters regex compiles");
    if let Some(captures) = counters_for_target_player_re.captures(text) {
        return Some(entry_trigger_rule(
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPlayer",
                    json!({ "kind": "players" }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "putCountersOnCreaturesOfTargetPlayer",
                "player": chosen_target("targetPlayer"),
                "counter": &captures[1],
                "count": integer(1),
            })],
        ));
    }

    if text
        == "When this creature enters, choose a creature type. Other permanents you control of that type gain hexproof and indestructible until end of turn."
    {
        let selected_type = json!({
            "kind": "chosenCreatureType",
            "decisionId": "entryCreatureType",
        });
        let affected = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": selected_type,
            "excludeSource": true,
        });
        return Some(entry_trigger_rule(
            None,
            vec![
                json!({
                    "kind": "chooseCreatureType",
                    "id": "entryCreatureType",
                    "player": controller(),
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": affected.clone(),
                    "keyword": "hexproof",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": affected,
                    "keyword": "indestructible",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
        ));
    }

    if text
        == "When Slinn Voda enters, if it was kicked, return all creatures to their owners' hands except for Merfolk, Krakens, Leviathans, Octopuses, and Serpents."
    {
        return Some(entry_trigger_rule(
            None,
            vec![json!({
                "kind": "conditionalEffect",
                "condition": { "kind": "wasKicked", "spell": self_ref() },
                "then": [{
                    "kind": "returnPermanentsToOwnersHands",
                    "where": and(vec![
                        card_type("Creature"),
                        not(or(vec![
                            subtype("Merfolk"),
                            subtype("Kraken"),
                            subtype("Leviathan"),
                            subtype("Octopus"),
                            subtype("Serpent"),
                        ])),
                    ]),
                }],
                "else": [],
            })],
        ));
    }

    if text
        == "When this creature enters, you may exile target historic permanent you control. If you do, return that card to the battlefield under its owner's control at the beginning of the next end step."
    {
        return Some(entry_trigger_rule(
            None,
            vec![
                json!({
                    "kind": "choosePermanents",
                    "id": "historicPermanent",
                    "player": controller(),
                    "minimum": integer(0),
                    "maximum": integer(1),
                    "candidates": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": { "kind": "historic" },
                    },
                }),
                json!({
                    "kind": "exileUntilNextEndStep",
                    "objects": decision_result("historicPermanent"),
                    "returnUnderOwnerControl": true,
                    "creatureCounter": "",
                    "planeswalkerCounter": "",
                }),
            ],
        ));
    }

    if text
        == "When this creature exploits a creature, return to their owners' hands all creatures your opponents control with toughness less than the exploited creature's toughness."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "creatureExploited", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "exploitReturnSmallerCreatures",
                }],
            }),
            &[
                "Bind the exploited creature's toughness",
                "Return smaller opposing creatures",
            ],
        ));
    }

    if text
        == "Whenever another creature you control enters, you may have this creature become a copy of that creature until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                },
                "effects": [{
                    "kind": "copyTriggeringPermanentUntilEndOfTurn",
                    "object": self_ref(),
                    "optional": true,
                }],
            }),
            &[
                "Recognize another controlled creature entering",
                "Offer the copy choice",
                "Restore at cleanup",
            ],
        ));
    }

    None
}
