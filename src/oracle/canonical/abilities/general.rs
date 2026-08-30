use super::super::*;

pub(in crate::oracle::canonical) fn parse_generalized_zone_and_combat_ability(
    text: &str,
    ability_kind: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let normalized;
    let text = if let Some((main, reminder)) = text.rsplit_once(". (")
        && reminder.ends_with(')')
    {
        normalized = format!("{main}.");
        normalized.as_str()
    } else {
        text
    };
    let recent_opponent_graveyard_mass_return_re = Regex::new(
        r#"(?i)^Put onto the battlefield under your control all (.+?) cards in your opponents' graveyards that were put there from the battlefield this turn\. They are ([A-Za-z][A-Za-z '-]+) (artifacts|creatures|enchantments|lands) with \"(.+)\" \(They lose all other types and subtypes\.\)$"#,
    )
    .expect("recent opponent graveyard mass return regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = recent_opponent_graveyard_mass_return_re.captures(text)
    {
        let card_type = match captures.get(3)?.as_str().to_ascii_lowercase().as_str() {
            "artifacts" => "Artifact",
            "creatures" => "Creature",
            "enchantments" => "Enchantment",
            "lands" => "Land",
            _ => return None,
        };
        let granted = parse_mana_ability(captures.get(4)?.as_str())
            .map(promote_activated_mana_ability)
            .or_else(|| parse_simple_activated_ability(captures.get(4)?.as_str()))
            .or_else(|| parse_common_activated_ability(captures.get(4)?.as_str()))?;
        if granted.rule["kind"].as_str() != Some("activatedAbility") {
            return None;
        }
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "returnRecentGraveyardCards",
                    "player": { "kind": "opponentsOf", "player": controller() },
                    "controller": controller(),
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                    "all": true,
                    "tapped": false,
                    "enterAs": {
                        "types": [card_type],
                        "subtypes": [singular_card_term(captures.get(2)?.as_str())],
                        "retainSupertypes": true,
                        "grantAbilities": [granted.rule],
                    },
                }],
            }),
            &[
                "Select opponent graveyard cards that left the battlefield this turn",
                "Move every matching card under the spell controller's control",
                "Replace card types and subtypes before battlefield entry",
                "Parse and grant the quoted activated ability",
            ],
        ));
    }
    let recent_graveyard_return_re = Regex::new(&format!(
        r"(?i)^Choose up to ({}) target (.+?) cards? in your graveyard that were put there from the battlefield this turn\. Return them to the battlefield( tapped)?\.$",
        count_word_pattern(),
    ))
    .expect("recent graveyard return regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = recent_graveyard_return_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "returnRecentGraveyardCards",
                    "player": controller(),
                    "where": parse_permanent_criteria(&captures[2], face_name)?,
                    "maximum": integer(parse_number_word(&captures[1])?),
                    "tapped": captures.get(3).is_some(),
                }],
            }),
            &[
                "Filter cards that moved from the battlefield this turn",
                "Choose up to the parsed maximum",
                "Return the chosen cards to the battlefield",
            ],
        ));
    }

    let shuffle_permanent_draw_re = Regex::new(&format!(
        r"(?i)^The owner of target (.+?) shuffles it into their library, then draws ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("shuffle target permanent and draw regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = shuffle_permanent_draw_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        permanent_target_candidates(&captures[1], face_name)?,
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "shufflePermanentIntoOwnersLibraryThenDraw",
                    "permanent": chosen_target("targetPermanent"),
                    "count": integer(parse_number_word(&captures[2])?),
                }],
            }),
            &[
                "Extract the permanent target",
                "Move it into and shuffle its owner's library",
                "Make that owner draw the parsed number of cards",
            ],
        ));
    }

    let chosen_color_protection_re = Regex::new(
        r"(?i)^Target (.+?) gains protection from the color of your choice until end of turn\.$",
    )
    .expect("chosen-color protection regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = chosen_color_protection_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        permanent_target_candidates(&captures[1], face_name)?,
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "grantChosenColorProtection",
                    "object": chosen_target("targetPermanent"),
                    "player": controller(),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Extract the permanent target",
                "Ask the resolving player to choose a color",
                "Grant protection for the turn",
            ],
        ));
    }

    let prevent_attacker_damage_re = Regex::new(
        r"(?i)^Prevent all combat damage that would be dealt by target attacking creature this turn\.$",
    )
    .expect("target attacking creature damage prevention regex compiles");
    if matches!(ability_kind, "activatedAbility" | "staticAbility")
        && let Some((cost_text, _)) = text.split_once(':')
        && prevent_attacker_damage_re.is_match(text.split_once(':')?.1.trim())
    {
        let (costs, cost_decisions) = parse_activation_costs(cost_text.trim())?;
        let mut decisions = cost_decisions;
        decisions.push(target_decision(
            "targetAttacker",
            json!({
                "kind": "permanents",
                "where": and(vec![
                    card_type("Creature"),
                    json!({ "kind": "isAttacking" }),
                ]),
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
                    "kind": "preventCombatDamageFromPermanent",
                    "permanent": chosen_target("targetAttacker"),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Parse the activated costs",
                "Require an attacking creature target",
                "Prevent its combat damage for the turn",
            ],
        ));
    }

    let upkeep_reanimate_re = Regex::new(&format!(
        r"(?i)^At the beginning of your upkeep, if you control ({}) or more (.+?), you may return target (.+?) card from your graveyard to the battlefield\.$",
        count_word_pattern(),
    ))
    .expect("upkeep threshold reanimation regex compiles");
    if matches!(ability_kind, "triggeredAbility" | "staticAbility")
        && let Some(captures) = upkeep_reanimate_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "upkeep",
                    "player": controller(),
                },
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": parse_permanent_criteria(&captures[2], face_name)?,
                    }),
                    integer(parse_number_word(&captures[1])?),
                ),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetGraveyardCard",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": parse_permanent_criteria(&captures[3], face_name)?,
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "moveTargetCard",
                    "card": chosen_target("targetGraveyardCard"),
                    "to": "battlefield",
                    "tapped": false,
                    "controller": controller(),
                }],
            }),
            &[
                "Recognize the controller's upkeep",
                "Count the qualifying controlled permanents",
                "Optionally reanimate the legal target",
            ],
        ));
    }

    let angel_tokens_and_indestructible_re = Regex::new(&format!(
        r"(?i)^Create ({}) (\d+)/(\d+) (.+?) creature tokens? with (.+?)\. Non-(.+?) creatures you control gain (.+?) until your next turn\.$",
        count_word_pattern(),
    ))
    .expect("token creation and temporary controlled keyword regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = angel_tokens_and_indestructible_re.captures(text)
    {
        let token_words = captures[4].split_whitespace().collect::<Vec<_>>();
        let color = token_words.first()?.to_ascii_lowercase();
        let subtypes = token_words
            .iter()
            .skip(1)
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let token_keyword = oracle_keyword_kind(&captures[5])?;
        let granted_keyword = oracle_keyword_kind(&captures[7])?;
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [
                    {
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": integer(parse_number_word(&captures[1])?),
                        "token": {
                            "colors": [color],
                            "types": ["Creature"],
                            "subtypes": subtypes,
                            "power": captures[2].parse::<i64>().ok()?,
                            "toughness": captures[3].parse::<i64>().ok()?,
                            "abilities": [{ "kind": token_keyword }],
                        },
                    },
                    {
                        "kind": "grantKeywordToPermanents",
                        "player": controller(),
                        "where": and(vec![
                            card_type("Creature"),
                            not(subtype(&captures[6])),
                        ]),
                        "keyword": granted_keyword,
                        "duration": { "kind": "untilNextTurn" },
                    },
                ],
            }),
            &[
                "Parse the creature-token characteristics",
                "Create the tokens",
                "Grant the temporary keyword to the complementary creature set",
            ],
        ));
    }

    let x_artifacts_or_life_re = Regex::new(
        r"(?is)^Choose one [^\n]*\n[\x{2022}\x{00E2}\x{00C2}\x{00E9}\x{20AC}\x{201D}\x{2014}\x{0080}\x{0094}\s-]*Destroy X target artifacts and/or enchantments\.\s*[\x{2022}\x{00E2}\x{00C2}\x{00E9}\x{20AC}\x{201D}\x{2014}\x{0080}\x{0094}\s-]*Target player gains twice X life\.$",
    )
    .expect("X modal destruction or life regex compiles");
    if ability_kind == "spellAbility" && x_artifacts_or_life_re.is_match(text) {
        let mut destroy_targets = target_decision(
            "targetPermanents",
            json!({
                "kind": "permanents",
                "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
            }),
            0,
            0,
        );
        destroy_targets["maximum"] = decision_result("xValue");
        destroy_targets["condition"] = selection("chosenModes", "destroy");
        let mut life_target = target_decision("targetPlayer", json!({ "kind": "players" }), 1, 1);
        life_target["condition"] = selection("chosenModes", "gainLife");
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
                            "options": ["destroy", "gainLife"],
                        },
                        x_value(),
                        destroy_targets,
                        life_target,
                    ],
                },
                "effects": [
                    {
                        "kind": "conditionalEffect",
                        "condition": selection("chosenModes", "destroy"),
                        "then": [{
                            "kind": "destroyChosenPermanents",
                            "objects": { "kind": "chosenTargets", "id": "targetPermanents" },
                        }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": selection("chosenModes", "gainLife"),
                        "then": [{
                            "kind": "gainLife",
                            "player": chosen_target("targetPlayer"),
                            "amount": {
                                "kind": "multiply",
                                "left": decision_result("xValue"),
                                "right": integer(2),
                            },
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Choose one modal branch",
                "Bind X to the selected targets or life amount",
                "Resolve the selected branch",
            ],
        ));
    }

    let per_opponent_destroy_re = Regex::new(
        r"(?i)^For each opponent, destroy up to one target (.+?) that player controls\.$",
    )
    .expect("per-opponent targeted destruction regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = per_opponent_destroy_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "destroyUpToOneForEachOpponent",
                    "player": controller(),
                    "where": parse_permanent_criteria(&captures[1], face_name)?,
                }],
            }),
            &[
                "Iterate over each opponent independently",
                "Offer up to one matching permanent that opponent controls",
                "Destroy every chosen permanent",
            ],
        ));
    }

    let per_opponent_gain_control_re = Regex::new(
        r"(?i)^For each opponent, gain control of up to one target (.+?) that player controls\.$",
    )
    .expect("per-opponent targeted control-change regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = per_opponent_gain_control_re.captures(text)
    {
        let candidates = json!({
            "kind": "permanents",
            "controller": { "kind": "opponentsOf", "player": controller() },
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
        });
        let mut decision = target_decision("controlTargets", candidates, 0, 0);
        decision["maximum"] = json!({
            "kind": "countOpponents",
            "player": controller(),
        });
        decision["selectionConstraint"] = json!({
            "kind": "distinctPermanentControllers",
        });
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [decision],
                },
                "effects": [{
                    "kind": "gainControlPermanent",
                    "permanent": { "kind": "chosenTargets", "id": "controlTargets" },
                    "controller": controller(),
                }],
            }),
            &[
                "Select no more than one permanent for each opponent",
                "Require distinct current controllers",
                "Gain control of every selected permanent",
            ],
        ));
    }

    let cycle_destroy_all_re = Regex::new(r"(?i)^When you cycle this card, destroy all (.+?)\.$")
        .expect("cycling global destruction regex compiles");
    if matches!(ability_kind, "triggeredAbility" | "staticAbility")
        && let Some(captures) = cycle_destroy_all_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "sourceCycled", "object": self_ref() },
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": card_qualifier_list_filter(&captures[1], face_name)
                            .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
                    },
                }],
            }),
            &[
                "Recognize a source cycling event",
                "Parse the global permanent criteria",
                "Destroy every matching permanent",
            ],
        ));
    }

    let opponent_land_entry_re = Regex::new(
        r"(?i)^Whenever a land an opponent controls enters, if that player controls more lands than you, you may put a land card from your hand onto the battlefield\.$",
    )
    .expect("opponent land-entry catch-up regex compiles");
    if matches!(ability_kind, "triggeredAbility" | "staticAbility")
        && opponent_land_entry_re.is_match(text)
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": { "kind": "opponentsOf", "player": controller() },
                    "where": card_type("Land"),
                },
                "effects": [{
                    "kind": "putLandFromHandIfTriggeringPlayerAhead",
                    "player": controller(),
                    "triggeringPermanent": { "kind": "triggeringPermanent" },
                }],
            }),
            &[
                "Watch lands entering under an opponent's control",
                "Compare that player's land count with the controller's",
                "Optionally put a land from hand onto the battlefield",
            ],
        ));
    }

    let opponent_draw_tax_re = Regex::new(
        r"(?i)^Whenever an opponent draws a card, that player may pay (\{[^}]+\})\. If the player doesn't, you create a (.+?) token\.(?: \(.+\))?$",
    )
    .expect("opponent draw payment-or-token regex compiles");
    if matches!(ability_kind, "triggeredAbility" | "staticAbility")
        && let Some(captures) = opponent_draw_tax_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "cardDrawn",
                    "opponentOfSourceController": true,
                },
                "effects": [{
                    "kind": "opponentMayPayManaOrCreateToken",
                    "player": { "kind": "triggeringPlayer" },
                    "beneficiary": controller(),
                    "manaCost": captures[1].to_string(),
                    "tokenName": captures[2].trim(),
                }],
            }),
            &[
                "Watch each opponent draw",
                "Offer the triggering player the parsed mana payment",
                "Create the named token for the controller if unpaid",
            ],
        ));
    }

    let landfall_destination_re = Regex::new(
        r"(?i)^(?:Landfall [^\p{L}]+ )?Whenever a land you control enters, you may return target (.+?) card from your graveyard to your hand\. If that land is a (.+?), you may return that (.+?) card to the battlefield instead\.$",
    )
    .expect("landfall graveyard destination regex compiles");
    if matches!(ability_kind, "triggeredAbility" | "staticAbility")
        && let Some(captures) = landfall_destination_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetGraveyardCard",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": parse_permanent_criteria(&captures[1], face_name)?,
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "returnGraveyardCardByTriggerSubtype",
                    "card": chosen_target("targetGraveyardCard"),
                    "triggeringPermanent": { "kind": "triggeringPermanent" },
                    "subtype": captures[2].trim(),
                    "defaultDestination": "hand",
                    "matchingDestination": "battlefield",
                    "controller": controller(),
                }],
            }),
            &[
                "Watch controlled land entries",
                "Extract the optional graveyard target",
                "Choose its destination from the triggering land subtype",
            ],
        ));
    }

    let approach_re = Regex::new(&format!(
        r"(?i)^If this spell was cast from your hand and you've cast another spell named (.+?) this game, you win the game\. Otherwise, put (.+?) into its owner's library ([A-Za-z0-9-]+) from the top and you gain ({}) life\.$",
        count_word_pattern(),
    ))
    .expect("second named cast alternate resolution regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = approach_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "winOnRepeatedNamedCastOtherwiseLibrary",
                    "player": controller(),
                    "name": captures[1].trim(),
                    "requiredCastZone": "hand",
                    "libraryPositionFromTop": integer(parse_ordinal_word(&captures[3])?),
                    "gainLife": integer(parse_number_word(&captures[4])?),
                }],
            }),
            &[
                "Inspect prior same-name casts from the required zone",
                "Win after the repeated qualifying cast",
                "Otherwise gain life and replace the spell's normal destination",
            ],
        ));
    }

    let chapter = text
        .split_whitespace()
        .next()
        .and_then(|value| match value {
            "I" => Some(1),
            "II" => Some(2),
            "III" => Some(3),
            _ => None,
        });
    if matches!(ability_kind, "staticAbility" | "triggeredAbility")
        && let Some(chapter) = chapter
    {
        if let Some((_, quoted_ability)) = text.split_once("This Saga gains \"")
            && let Some(granted_text) = quoted_ability.strip_suffix('"')
        {
            let granted = parse_mana_ability(granted_text)
                .map(promote_activated_mana_ability)
                .or_else(|| parse_simple_activated_ability(granted_text))
                .or_else(|| parse_common_activated_ability(granted_text))
                .or_else(|| parse_expansion_triggered(granted_text, face_name))?;
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "sagaChapterReached",
                        "object": self_ref(),
                        "chapters": [integer(chapter)],
                    },
                    "effects": [{
                        "kind": "grantAbility",
                        "object": self_ref(),
                        "ability": granted.rule,
                        "duration": { "kind": "permanent" },
                    }],
                }),
                &[
                    "Trigger the parsed Saga chapter",
                    "Parse the quoted ability through the shared activated-ability grammar",
                    "Grant that canonical ability to the Saga permanently",
                ],
            ));
        }
        let chapter_exact_mana_cost_search_re = Regex::new(
            r"(?i)^[IVX]+\s+.+?Search your library for (?:a|an) (.+?) card with mana cost ((?:\{[^}]+\})(?: or (?:\{[^}]+\}))+), put it onto the battlefield, then shuffle\.$",
        )
        .expect("Saga chapter exact-mana-cost search regex compiles");
        if let Some(captures) = chapter_exact_mana_cost_search_re.captures(text) {
            let exact_costs = Regex::new(r"\{[^}]+\}")
                .expect("mana symbol sequence regex compiles")
                .find_iter(&captures[2])
                .map(|matched| {
                    json!({
                        "kind": "manaCostEquals",
                        "value": matched.as_str(),
                    })
                })
                .collect::<Vec<_>>();
            if exact_costs.len() < 2 {
                return None;
            }
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "sagaChapterReached",
                        "object": self_ref(),
                        "chapters": [integer(chapter)],
                    },
                    "effects": [{
                        "kind": "searchLibrary",
                        "player": controller(),
                        "where": and(vec![
                            parse_permanent_criteria(&captures[1], face_name)?,
                            or(exact_costs),
                        ]),
                        "maximum": integer(1),
                        "destination": "battlefield",
                        "tapped": false,
                    }],
                }),
                &[
                    "Trigger the parsed Saga chapter",
                    "Filter the library by reusable card criteria and exact printed mana costs",
                    "Put the selected card onto the battlefield and shuffle",
                ],
            ));
        }
        if text
            .contains("Exile target permanent an opponent controls with mana value 3 or greater.")
        {
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "sagaChapterReached",
                        "object": self_ref(),
                        "chapters": [integer(chapter)],
                    },
                    "declaration": {
                        "kind": "castingDeclaration",
                        "decisions": [target_decision(
                            "targetPermanent",
                            json!({
                                "kind": "permanents",
                                "controller": { "kind": "opponentsOf", "player": controller() },
                                "where": compare(
                                    ">=",
                                    json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                                    integer(3),
                                ),
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
                    "Trigger the parsed Saga chapter",
                    "Target an opposing permanent by mana value",
                    "Exile it",
                ],
            ));
        }
        if text.contains(
            "Noncreature spells your opponents cast cost {2} more to cast until your next turn.",
        ) {
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "sagaChapterReached",
                        "object": self_ref(),
                        "chapters": [integer(chapter)],
                    },
                    "effects": [{
                        "kind": "increaseOpponentSpellCostUntilNextTurn",
                        "player": controller(),
                        "where": not(card_type("Creature")),
                        "amount": integer(2),
                    }],
                }),
                &[
                    "Trigger the parsed Saga chapter",
                    "Select opposing noncreature spells",
                    "Increase their cost until the controller's next turn",
                ],
            ));
        }
        if text.contains(
            "Return target creature or planeswalker card from your graveyard to the battlefield.",
        ) && text.contains("Put a +1/+1 counter or a loyalty counter on it.")
        {
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "sagaChapterReached",
                        "object": self_ref(),
                        "chapters": [integer(chapter)],
                    },
                    "declaration": {
                        "kind": "castingDeclaration",
                        "decisions": [target_decision(
                            "targetGraveyardCard",
                            json!({
                                "kind": "cards",
                                "zone": graveyard(controller()),
                                "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                            }),
                            1,
                            1,
                        )],
                    },
                    "effects": [{
                        "kind": "reanimateWithTypeCounter",
                        "card": chosen_target("targetGraveyardCard"),
                        "controller": controller(),
                        "creatureCounter": "+1/+1",
                        "planeswalkerCounter": "loyalty",
                    }],
                }),
                &[
                    "Trigger the parsed Saga chapter",
                    "Target a creature or planeswalker card",
                    "Return it with the type-appropriate counter",
                ],
            ));
        }
    }

    let saga_chapter_instruction_re =
        Regex::new(r"(?is)^([IVX]+(?:\s*,\s*[IVX]+)*)\s+(?:\x{2014}|-)\s+(.+)$")
            .expect("generic Saga chapter instruction regex compiles");
    if matches!(ability_kind, "staticAbility" | "triggeredAbility")
        && let Some(captures) = saga_chapter_instruction_re.captures(text)
    {
        let chapters = captures
            .get(1)?
            .as_str()
            .split(',')
            .map(|chapter| match chapter.trim() {
                "I" => Some(1),
                "II" => Some(2),
                "III" => Some(3),
                "IV" => Some(4),
                "V" => Some(5),
                "VI" => Some(6),
                "VII" => Some(7),
                "VIII" => Some(8),
                "IX" => Some(9),
                "X" => Some(10),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let instruction = captures.get(2)?.as_str().trim();
        let (effects, decisions) = parse_general_effect_instruction(instruction, face_name)
            .or_else(|| parse_general_effect_sequence(instruction, face_name))?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "sagaChapterReached",
                "object": self_ref(),
                "chapters": chapters.into_iter().map(integer).collect::<Vec<_>>(),
            },
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
                "Parse one or more Saga chapter numerals",
                "Resolve the chapter text through shared effect grammar",
                "Attach shared target declarations to the chapter trigger",
            ],
        ));
    }

    let gift_text = text
        .split_once(" (")
        .map(|(keyword, _)| keyword)
        .unwrap_or(text)
        .trim_end_matches('.');
    let gift_re = Regex::new(r"(?i)^Gift (?:a|an) (.+)$").expect("gift keyword regex compiles");
    if let Some(captures) = gift_re.captures(gift_text) {
        let gift = captures.get(1)?.as_str().trim();
        let effects = if gift.eq_ignore_ascii_case("card") {
            vec![json!({
                "kind": "drawCards",
                "player": { "kind": "boundValue", "id": "giftRecipient" },
                "count": integer(1),
            })]
        } else {
            let mut create = create_token_effect(&format!("Create a {gift} token."))?;
            create["controller"] = json!({ "kind": "boundValue", "id": "giftRecipient" });
            vec![create]
        };
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "gift",
                    "optional": true,
                    "effects": effects,
                },
            }),
            &[
                "Offer a gift recipient while declaring the spell",
                "Bind the chosen opponent as the gift recipient",
                "Resolve the parsed gift effect before the spell's other effects",
            ],
        ));
    }

    if ability_kind == "spellAbility"
        && text
            == "Separate all creature cards in your graveyard into two piles. Exile the pile of an opponent's choice and return the other to the battlefield."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "separateGraveyardCardsIntoPiles",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "chosenPileDestination": "exile",
                    "otherPileDestination": "battlefield",
                }],
            }),
            &[
                "Partition every matching graveyard card into two piles",
                "Let a chosen opponent select the pile to exile",
                "Return the other pile to the battlefield",
            ],
        ));
    }

    let emblem_bonus_re = Regex::new(
        r#"(?i)^([^:]+): You get an emblem with \"(.+?) you control get ([+\-]\d+)/([+\-]\d+)(?: and have ([A-Za-z ,]+))?\.\"$"#,
    )
    .expect("permanent emblem bonus regex compiles");
    if matches!(ability_kind, "activatedAbility" | "staticAbility")
        && let Some(captures) = emblem_bonus_re.captures(text)
    {
        let loyalty = captures[1].replace("âˆ’", "-").replace("Ã¢Ë†â€™", "-");
        let (costs, _) = parse_activation_costs(&loyalty)?;
        let keywords = captures
            .get(5)
            .map(|keywords| {
                keywords
                    .as_str()
                    .split(" and ")
                    .flat_map(|part| part.split(','))
                    .filter_map(|keyword| normalized_keyword_name(keyword.trim()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "effects": [{
                    "kind": "createEmblemPermanentModifier",
                    "player": controller(),
                    "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
                    "power": integer(captures[3].parse::<i64>().ok()?),
                    "toughness": integer(captures[4].parse::<i64>().ok()?),
                    "keywords": keywords,
                }],
            }),
            &[
                "Parse the loyalty payment",
                "Create a persistent emblem modifier",
                "Apply its bonus and keywords to controlled creatures",
            ],
        ));
    }

    if text == "You may sacrifice a nontoken white creature rather than pay this spell's mana cost."
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "sacrificeCreatureAlternativeCost",
                    "where": and(vec![
                        card_type("Creature"),
                        color_filter("white")?,
                        not(json!({ "kind": "isToken" })),
                    ]),
                },
            }),
            &[
                "Find controlled nontoken white creatures",
                "Offer one as an alternative to the mana cost",
            ],
        ));
    }

    if ability_kind == "spellAbility"
        && text
            == "Until end of turn, your life total can't change, and permanents you control gain hexproof and indestructible."
    {
        return Some(draft(
            json!({
                "kind": "spellAbility",
                "source": self_ref(),
                "effects": [{
                    "kind": "lockLifeAndGrantKeywords",
                    "player": controller(),
                    "where": Value::Null,
                    "keywords": ["hexproof", "indestructible"],
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Prevent the controller's life total from changing",
                "Grant both keywords to their current permanents for the turn",
            ],
        ));
    }

    let most_life_attack_re = Regex::new(
        r"(?i)^.+? attacks an opponent with the most life among your opponents each combat if able unless you control a creature named (.+?)\.$",
    )
    .expect("most-life attack restriction regex compiles");
    if matches!(ability_kind, "staticAbility" | "keywordAbility")
        && let Some(captures) = most_life_attack_re.captures(text)
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "attackMostLifeOpponentUnlessControlNamed",
                    "object": self_ref(),
                    "exceptionName": captures[1].trim(),
                }],
            }),
            &[
                "Check the named-creature exception",
                "Require the source to attack if able",
                "Restrict its defender to an opponent tied for most life",
            ],
        ));
    }

    let equipped_bundle_re = Regex::new(
        r#"(?i)^Equipped creature gets ([+\-]\d+)/([+\-]\d+) and has (.+), and \"Whenever this creature deals combat damage to a creature, exile that creature\.\"$"#,
    )
    .expect("equipped keyword and damage-exile bundle regex compiles");
    if matches!(ability_kind, "staticAbility" | "triggeredAbility")
        && let Some(captures) = equipped_bundle_re.captures(text)
    {
        let keywords = captures[3]
            .split(',')
            .flat_map(|part| part.split(" and "))
            .filter_map(|keyword| normalized_keyword_name(keyword.trim()))
            .collect::<Vec<_>>();
        let attached = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        let mut modifiers = vec![json!({
            "kind": "modifyPowerToughness",
            "objects": attached.clone(),
            "power": integer(captures[1].parse::<i64>().ok()?),
            "toughness": integer(captures[2].parse::<i64>().ok()?),
        })];
        modifiers.extend(keywords.into_iter().map(|keyword| {
            json!({
                "kind": "grantKeyword",
                "objects": attached.clone(),
                "keyword": keyword,
            })
        }));
        modifiers.push(json!({
            "kind": "attachedCombatDamageExileDamagedCreature",
            "attachment": self_ref(),
        }));
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": modifiers,
            }),
            &[
                "Apply the attached creature's power/toughness bonus",
                "Grant every parsed keyword",
                "Create a combat-damage exile trigger through the attachment",
            ],
        ));
    }

    let karmic_justice_re = Regex::new(
        r"(?i)^Whenever a spell or ability an opponent controls destroys a noncreature permanent you control, you may destroy target permanent that opponent controls\.$",
    )
    .expect("opponent-caused destruction retaliation regex compiles");
    if matches!(ability_kind, "triggeredAbility" | "staticAbility")
        && karmic_justice_re.is_match(text)
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "opponentDestroyedControlledPermanent",
                    "player": controller(),
                    "where": not(card_type("Creature")),
                },
                "effects": [{
                    "kind": "destroyOptionalPermanentOfTriggeringPlayer",
                    "player": controller(),
                }],
            }),
            &[
                "Recognize destruction caused by an opposing stack object",
                "Offer an optional permanent controlled by that opponent",
                "Destroy the chosen permanent",
            ],
        ));
    }

    let graveyard_copy_re = Regex::new(
        r"(?i)^Return target (.+?) from your graveyard to the battlefield\. If this spell was cast from a graveyard, you may copy this spell and may choose a new target for the copy\.$",
    )
    .expect("graveyard self-copy reanimation regex compiles");
    if ability_kind == "spellAbility"
        && let Some(captures) = graveyard_copy_re.captures(text)
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
                            "where": parse_permanent_criteria(&captures[1], face_name)?,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "moveTargetCard",
                        "card": chosen_target("targetGraveyardCard"),
                        "to": "battlefield",
                        "controller": controller(),
                        "tapped": false,
                    },
                    {
                        "kind": "copyResolvingSpellIfCastFromZone",
                        "zone": "graveyard",
                        "optional": true,
                        "chooseNewTargets": true,
                    },
                ],
            }),
            &[
                "Return the legal graveyard target",
                "Inspect the original cast zone",
                "Optionally create a spell copy and retarget it",
            ],
        ));
    }

    if text
        == "+1: Exile the top card of your library face down and look at it. Create a 2/2 colorless Spirit creature token. When that token leaves the battlefield, put the exiled card into your hand."
    {
        let (costs, _) = parse_activation_costs("+1")?;
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "effects": [{
                    "kind": "exileTopCardFaceDownAndCreateLinkedToken",
                    "player": controller(),
                    "token": {
                        "name": "Spirit Token",
                        "colors": ["colorless"],
                        "types": ["Creature"],
                        "subtypes": ["Spirit"],
                        "power": 2,
                        "toughness": 2,
                    },
                    "returnDestination": "hand",
                }],
            }),
            &[
                "Exile the top card face down",
                "Create the parsed creature token",
                "Link the exiled card's return to that token leaving",
            ],
        ));
    }

    let colored_destroy_re =
        Regex::new(r"(?i)^([^:]+): Destroy target permanent that's one or more colors\.$")
            .expect("colored permanent loyalty destruction regex compiles");
    if matches!(ability_kind, "activatedAbility" | "staticAbility")
        && let Some(captures) = colored_destroy_re.captures(text)
    {
        let loyalty = captures[1].replace("âˆ’", "-").replace("Ã¢Ë†â€™", "-");
        let (costs, _) = parse_activation_costs(&loyalty)?;
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetColoredPermanent",
                        json!({
                            "kind": "permanents",
                            "where": compare(
                                ">=",
                                json!({ "kind": "colorCountOf", "object": { "kind": "candidate" } }),
                                integer(1),
                            ),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetColoredPermanent"),
                }],
            }),
            &[
                "Parse the loyalty payment",
                "Require a permanent with at least one color",
                "Destroy the target",
            ],
        ));
    }

    let mageta_re = Regex::new(
        r"(?i)^(.+?): Destroy all creatures except for (.+?)\. Those creatures can't be regenerated\.$",
    )
    .expect("activated global creature destruction regex compiles");
    if matches!(ability_kind, "activatedAbility" | "staticAbility")
        && let Some(captures) = mageta_re.captures(text)
        && source_reference_matches(&captures[2], face_name)
    {
        let (costs, cost_decisions) = parse_activation_costs(captures[1].trim())?;
        let distinct_discard_ids = cost_decisions
            .iter()
            .filter_map(|decision| {
                decision["id"]
                    .as_str()
                    .filter(|id| id.starts_with("discardCost"))
                    .map(Value::from)
            })
            .collect::<Vec<_>>();
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": cost_decisions,
                },
                "distinctTargets": [distinct_discard_ids],
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": card_type("Creature"),
                        "excludeSource": true,
                    },
                    "cannotRegenerate": true,
                }],
            }),
            &[
                "Parse all activated costs including multiple distinct discards",
                "Exclude the source from the global creature set",
                "Bypass regeneration for this destruction",
            ],
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_dungeon_room(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let (room_name, body) = [" — ", " â€” ", " Ã¢â‚¬â€ ", " ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â "]
        .into_iter()
        .find_map(|separator| text.split_once(separator))?;
    let leads_re = Regex::new(r"(?s)^(.*?)(?: \(Leads to: (.+)\))?$")
        .expect("dungeon room route regex compiles");
    let captures = leads_re.captures(body.trim())?;
    let instruction = captures.get(1)?.as_str().trim();
    let (effects, decisions) = parse_general_effect_sequence(instruction, face_name)
        .or_else(|| parse_general_effect_instruction(instruction, face_name))
        .unwrap_or_else(|| {
            (
                vec![json!({
                    "kind": "unsupportedDungeonRoomInstruction",
                    "text": instruction,
                })],
                Vec::new(),
            )
        });
    let next_rooms = captures
        .get(2)
        .map(|rooms| {
            rooms
                .as_str()
                .split(", ")
                .map(str::trim)
                .filter(|room| !room.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut rule = json!({
        "kind": "dungeonRoom",
        "room": room_name.trim(),
        "nextRooms": next_rooms,
        "effects": effects,
    });
    if !decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        });
    }
    Some(draft(
        rule,
        &[
            "Recognize a Dungeon room",
            "Parse its reusable effect instruction",
            "Preserve its outgoing room routes",
        ],
    ))
}
