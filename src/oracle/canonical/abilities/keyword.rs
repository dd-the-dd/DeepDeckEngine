use super::super::*;

pub(in crate::oracle::canonical) fn parse_keyword_ability(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let ninjutsu_re = Regex::new(
        r"(?i)^Ninjutsu ((?:\{[^}]+\})+) \(((?:\{[^}]+\})+), Return an unblocked attacker you control to hand: Put this card onto the battlefield from your hand tapped and attacking\.\)$",
    )
    .expect("ninjutsu keyword regex compiles");
    if let Some(captures) = ninjutsu_re.captures(text)
        && captures.get(1)?.as_str() == captures.get(2)?.as_str()
    {
        let (mut costs, _) = parse_activation_costs(captures.get(1)?.as_str())?;
        costs.push(json!({
            "kind": "returnPermanentToOwnersHand",
            "permanent": chosen_target("unblockedAttacker"),
        }));
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "hand",
                "activationCondition": { "kind": "ninjutsuTiming" },
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "unblockedAttacker",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": and(vec![
                                card_type("Creature"),
                                json!({ "kind": "isUnblockedAttacker" }),
                            ]),
                            "ignoreTargetingRestrictions": true,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "putAbilitySourceOntoBattlefieldTappedAndAttacking",
                    "replacingAttacker": chosen_target("unblockedAttacker"),
                }],
            }),
            &[
                "Parse ninjutsu through the shared mana-cost grammar",
                "Choose an unblocked controlled attacker as the return cost",
                "Enter from hand tapped and attacking the same defender",
            ],
        ));
    }
    if text.eq_ignore_ascii_case(
        "As each game begins, you can place one card with companion here if your starting deck meets its condition. You may cast it once from here.",
    ) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "companionGameRule" },
            }),
            &[
                "Recognize the shared Companion game rule",
                "Defer eligibility to each companion's canonical deck condition",
            ],
        ));
    }
    let companion_re = Regex::new(&format!(
        r"(?i)^Companion\s*(?:ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â|ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â|Ã¢â‚¬â€|â€”|—|-)\s*Your starting deck contains at least ({}) cards more than the minimum deck size\. \(If this card is your chosen companion, you may put it into your hand from outside the game for ((?:\{{[^}}]+\}})+) as a sorcery\.\)$",
        count_word_pattern(),
    ))
    .expect("companion deck-condition regex compiles");
    if let Some(captures) = companion_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "companion",
                    "deckCondition": {
                        "kind": "minimumDeckSizeExtra",
                        "count": integer(parse_number_word(&captures[1])?),
                    },
                    "acquisitionCost": {
                        "kind": "payMana",
                        "manaCost": &captures[2],
                    },
                    "timing": { "kind": "sorceryTiming" },
                },
            }),
            &[
                "Recognize the Companion keyword",
                "Parse its starting-deck condition",
                "Parse its acquisition payment with the shared mana-cost grammar",
            ],
        ));
    }
    let symbol_glossary_re =
        Regex::new(r"^\(\{[^}]+\} (?:represents .+|can be paid with either .+)\.\)$")
            .expect("mana-symbol glossary regex compiles");
    if symbol_glossary_re.is_match(text) {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Preserve a standalone mana-symbol glossary without creating an ability"],
        ));
    }
    let counter_area_marker_re = Regex::new(r"^\(Place your .+ counters in this area\.\)$")
        .expect("counter area marker regex compiles");
    if counter_area_marker_re.is_match(text) {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Preserve a counter-area marker without creating a game action"],
        ));
    }
    if (text.starts_with("(After you foretell a card,")
        || text.starts_with("(A player with ten or more poison counters loses the game."))
        && text.ends_with(')')
    {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Preserve a rules-zone or game-loss reminder without creating an action"],
        ));
    }
    let transform_origin_marker_re = Regex::new(r"(?i)^\(Transforms from .+\.\)$")
        .expect("transform-origin reminder regex compiles");
    if transform_origin_marker_re.is_match(text) {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Preserve a transform-origin reminder without creating a second action"],
        ));
    }
    let labeled_partner_marker_re =
        Regex::new(r"(?i)^Partner[^\r\n]+ \(You can have two commanders if .+\)$")
            .expect("labeled partner reminder regex compiles");
    if labeled_partner_marker_re.is_match(text) {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Preserve a labeled Partner deck-construction designation"],
        ));
    }
    let theme_color_re =
        Regex::new(r"^\(Theme color: \{[WUBRG]\}\)$").expect("theme color marker regex compiles");
    if theme_color_re.is_match(text) {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Preserve non-gameplay theme-color metadata"],
        ));
    }
    let partner_re = Regex::new(
        r"(?i)^Partner(?:[—–-][A-Za-z][A-Za-z '-]*)?(?: \(You can have two commanders if .+\))?$",
    )
    .expect("partner keyword regex compiles");
    if partner_re.is_match(text) {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Preserve the commander deck-construction partner designation"],
        ));
    }
    let dredge_re = Regex::new(&format!(
        r"(?i)^Dredge (\d+) \(If you would draw a card, you may mill ({}) cards instead\. If you do, return this card from your graveyard to your hand\.\)$",
        count_word_pattern(),
    ))
    .expect("dredge keyword regex compiles");
    if let Some(captures) = dredge_re.captures(text) {
        let keyword_count = captures[1].parse::<i64>().ok()?;
        let reminder_count = parse_number_word(&captures[2])?;
        if keyword_count <= 0 || keyword_count != reminder_count {
            return None;
        }
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "dredge",
                    "count": integer(keyword_count),
                },
            }),
            &[
                "Recognize the Dredge draw replacement",
                "Preserve and cross-check its mill quantity",
                "Return the source from graveyard only after the replacement succeeds",
            ],
        ));
    }
    let delve_re = Regex::new(
        r"(?i)^Delve(?: \(Each card you exile from your graveyard while casting this spell pays for \{1\}\.\))?$",
    )
    .expect("delve keyword regex compiles");
    if delve_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "delve" },
            }),
            &[
                "Recognize delve as an optional casting cost payment",
                "Choose cards from the caster's graveyard",
                "Reduce only the generic component of the spell cost",
            ],
        ));
    }
    let split_second_re = Regex::new(
        r"(?i)^Split second(?: \(As long as this spell is on the stack, players can't cast spells or activate abilities that aren't mana abilities\.\))?$",
    )
    .expect("split second keyword regex compiles");
    if split_second_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "splitSecond" },
            }),
            &[
                "Recognize Split second",
                "Restrict casts and nonmana activations while the spell is on the stack",
            ],
        ));
    }
    let splice_text = text
        .split_once(" (")
        .filter(|(_, reminder)| reminder.ends_with(')'))
        .map(|(keyword_text, _)| keyword_text)
        .unwrap_or(text)
        .trim_end_matches('.');
    let splice_remainder = splice_text.get("Splice onto ".len()..).filter(|_| {
        splice_text
            .get(.."Splice onto ".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Splice onto "))
    });
    if let Some(splice_remainder) = splice_remainder {
        let split_by_dash = [
            "ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â",
            "ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â",
            "Ã¢â‚¬â€",
            "â€”",
            "â€“",
            "—",
            "–",
        ]
        .into_iter()
        .find_map(|separator| splice_remainder.split_once(separator));
        let (receiving_criteria, cost_text) = split_by_dash.or_else(|| {
            splice_remainder
                .find(" {")
                .map(|separator| splice_remainder.split_at(separator))
        })?;
        let cost = parse_keyword_cost(&format!("Cost—{}", cost_text.trim_start()), "Cost")?;
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "splice",
                    "onto": parse_permanent_criteria(receiving_criteria.trim(), "")?,
                    "cost": cost,
                },
            }),
            &[
                "Recognize Splice",
                "Parse the receiving spell subtype as a reusable criterion",
                "Parse the splice payment with the shared cost grammar",
            ],
        ));
    }
    if text == "This creature can block only creatures with flying." {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "canBlockOnlyFlying" },
            }),
            &["Restrict this creature to blocking creatures with flying"],
        ));
    }
    let toxic_re =
        Regex::new(r"(?i)^Toxic (\d+)(?: \(.+\))?\.?$").expect("toxic keyword regex compiles");
    if let Some(captures) = toxic_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "toxic",
                    "count": integer(captures[1].parse::<i64>().ok()?),
                },
            }),
            &[
                "Recognize toxic combat-damage poison",
                "Preserve the poison-counter quantity",
            ],
        ));
    }
    let prototype_re = Regex::new(r"^Prototype ((?:\{[^}]+\})+).+?(\d+)/(\d+) \(.+\)$")
        .expect("prototype keyword regex compiles");
    if let Some(captures) = prototype_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "prototype",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": captures[1].to_string(),
                    },
                    "power": integer(captures[2].parse::<i64>().ok()?),
                    "toughness": integer(captures[3].parse::<i64>().ok()?),
                },
            }),
            &[
                "Recognize prototype alternative characteristics",
                "Store its mana cost, power, and toughness",
            ],
        ));
    }
    let morph_re = Regex::new(r"(?i)^(mega)?morph ((?:\{[^}]+\})+) \(.+\)$")
        .expect("morph keyword regex compiles");
    if let Some(captures) = morph_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "morph",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": captures[2].to_string(),
                    },
                    "putCounterWhenTurnedFaceUp": captures.get(1).is_some(),
                },
            }),
            &[
                "Recognize morph alternative face-down casting",
                "Store the face-up cost",
                "Preserve the megamorph counter instruction",
            ],
        ));
    }
    if text.starts_with("Fuse (") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "fuse" },
            }),
            &[
                "Recognize fuse",
                "Allow both split halves to be cast together from hand",
            ],
        ));
    }
    if text.starts_with("Convoke (") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "convoke" },
            }),
            &[
                "Recognize convoke",
                "Let untapped controlled creatures pay the spell's total cost",
            ],
        ));
    }
    let teamwork_re =
        Regex::new(r"(?i)^Teamwork (\d+)(?: \(.+\))?$").expect("teamwork keyword regex compiles");
    if let Some(captures) = teamwork_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "teamwork",
                    "minimumPower": integer(captures[1].parse::<i64>().ok()?),
                },
            }),
            &[
                "Recognize teamwork as an optional additional cost",
                "Choose untapped controlled creatures",
                "Require their total power to meet the threshold",
            ],
        ));
    }
    let miracle_re =
        Regex::new(r"^Miracle ((?:\{[^}]+\})+) \(.+\)$").expect("miracle keyword regex compiles");
    if let Some(captures) = miracle_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "miracle",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": captures[1].to_string(),
                    },
                },
            }),
            &[
                "Recognize miracle alternative cost",
                "Offer it only for the first card drawn during the turn",
            ],
        ));
    }
    if text == "You may cast this spell as though it had flash if it targets a commander." {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "flashIfTargetsCommander" },
            }),
            &[
                "Recognize commander-conditional flash",
                "Restrict instant-speed declarations to commander targets",
            ],
        ));
    }
    let conditional_flash_control_re = Regex::new(
        r"(?i)^You may cast this spell as though it had flash if you control (?:a|an) (.+?)\.$",
    )
    .expect("conditional flash controlled-permanent regex compiles");
    if let Some(captures) = conditional_flash_control_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "flashIfControls",
                    "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                },
            }),
            &[
                "Recognize a conditional flash permission",
                "Parse the controlled permanent criterion",
            ],
        ));
    }
    if text
        == "Morbid — You may put that card onto the battlefield instead of putting it into your hand if a creature died this turn."
        || text
            == "Morbid â€” You may put that card onto the battlefield instead of putting it into your hand if a creature died this turn."
    {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "morbidSearchDestination": true,
            }),
            &[
                "Recognize the morbid search-destination replacement",
                "Offer the battlefield destination after a creature dies",
            ],
        ));
    }
    let awaken_re = Regex::new(r"^Awaken (\d+)(?:—|â€”|Ã¢â‚¬â€)((?:\{[^}]+\})+) \(.+\)$")
        .expect("awaken keyword regex compiles");
    if let Some(captures) = awaken_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "awaken",
                    "count": integer(captures[1].parse::<i64>().ok()?),
                    "cost": {
                        "kind": "payMana",
                        "manaCost": captures[2].to_string(),
                    },
                },
            }),
            &[
                "Recognize awaken alternative cost",
                "Declare a controlled land target",
                "Animate it with counters and haste",
            ],
        ));
    }
    if text.starts_with("(As this Saga enters and after your draw step, add a lore counter.") {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Recognize intrinsic Saga lore-counter progression"],
        ));
    }
    if text.starts_with("Evolve (Whenever a creature you control enters,") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "evolve" },
            }),
            &[
                "Recognize evolve",
                "Compare entering power and toughness",
                "Apply one +1/+1 counter",
            ],
        ));
    }
    if text.starts_with("Exploit (When this creature enters, you may sacrifice a creature.)") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "exploit" },
            }),
            &[
                "Recognize exploit",
                "Offer an optional controlled-creature sacrifice",
            ],
        ));
    }
    let annihilator_re = Regex::new(r"(?i)^Annihilator (\d+)(?: \(.+\))?$")
        .expect("annihilator keyword regex compiles");
    if let Some(captures) = annihilator_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "declaredAttacker",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "sacrificePermanents",
                    "player": { "kind": "triggeringPlayer" },
                    "where": Value::Null,
                    "count": integer(captures[1].parse::<i64>().ok()?),
                }],
            }),
            &[
                "Recognize annihilator",
                "Make the defending player sacrifice permanents",
            ],
        ));
    }
    let graft_re = Regex::new(r"^Graft (\d+) \(This .+ enters with a \+1/\+1 counter on it\.")
        .expect("graft keyword regex compiles");
    if let Some(captures) = graft_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "graft",
                    "count": integer(captures[1].parse::<i64>().ok()?),
                },
            }),
            &[
                "Recognize graft",
                "Apply entering counters",
                "Offer counter movement to entering creatures",
            ],
        ));
    }
    let additional_waterbend_re = Regex::new(
        r"^As an additional cost to cast this spell, (you may )?waterbend (\{(?:\d+|X)\})(?:\..*)?$",
    )
    .expect("additional waterbend regex compiles");
    if let Some(captures) = additional_waterbend_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "waterbend",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[2],
                    },
                    "optional": captures.get(1).is_some(),
                },
            }),
            &[
                "Recognize waterbend additional cost",
                "Allow artifacts and creatures to pay generic mana",
            ],
        ));
    }
    let simple_keywords = [
        ("Changeling", "changeling"),
        ("Deathtouch", "deathtouch"),
        ("Defender", "defender"),
        ("Devoid", "devoid"),
        ("Double strike", "doubleStrike"),
        ("First strike", "firstStrike"),
        ("Fear", "fear"),
        ("Flanking", "flanking"),
        ("Flash", "flash"),
        ("Flying", "flying"),
        ("Haste", "haste"),
        ("Hexproof", "hexproof"),
        ("Horsemanship", "horsemanship"),
        ("Indestructible", "indestructible"),
        ("Infect", "infect"),
        ("Intimidate", "intimidate"),
        ("Islandwalk", "islandwalk"),
        ("Lifelink", "lifelink"),
        ("Menace", "menace"),
        ("Myriad", "myriad"),
        ("Prowess", "prowess"),
        ("Plainswalk", "plainswalk"),
        ("Reach", "reach"),
        ("Shadow", "shadow"),
        ("Shroud", "shroud"),
        ("Skulk", "skulk"),
        ("Swampwalk", "swampwalk"),
        ("Mountainwalk", "mountainwalk"),
        ("Forestwalk", "forestwalk"),
        ("Exalted", "exalted"),
        ("Umbra armor", "umbraArmor"),
        ("Trample", "trample"),
        ("Vigilance", "vigilance"),
        ("Wither", "wither"),
    ];
    let simple_text = text
        .strip_suffix(" (This card has no color.)")
        .or_else(|| {
            text.rsplit_once(" (")
                .filter(|(_, reminder)| reminder.ends_with(')'))
                .map(|(keyword, _)| keyword)
        })
        .unwrap_or(text);
    let keyword_text = simple_text.trim_end_matches('.');
    if let Some((_, kind)) = simple_keywords
        .iter()
        .find(|(label, _)| keyword_text == *label)
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": kind },
            }),
            &["Recognize evergreen keyword"],
        ));
    }
    if keyword_text
        .split(", ")
        .any(|label| label.to_ascii_lowercase().starts_with("annihilator "))
    {
        let abilities = keyword_text
            .split(", ")
            .map(|label| {
                if let Some((_, kind)) = simple_keywords
                    .iter()
                    .find(|(candidate, _)| label.eq_ignore_ascii_case(candidate))
                {
                    return Some(json!({ "kind": kind }));
                }
                if let Some(qualities) = label
                    .strip_prefix("protection from ")
                    .or_else(|| label.strip_prefix("Protection from "))
                {
                    return Some(json!({
                        "kind": "protection",
                        "from": qualities
                            .split(" and ")
                            .map(|quality| quality.trim().trim_start_matches("from ").to_ascii_lowercase())
                            .collect::<Vec<_>>(),
                    }));
                }
                Regex::new(r"(?i)^annihilator (\d+)$")
                    .expect("grouped annihilator regex compiles")
                    .captures(label)
                    .and_then(|captures| {
                        Some(json!({
                            "kind": "annihilator",
                            "quantity": integer(captures[1].parse::<i64>().ok()?),
                        }))
                    })
            })
            .collect::<Option<Vec<_>>>()?;
        return Some(draft(
            json!({
                "kind": "keywordAbilityGroup",
                "source": self_ref(),
                "abilities": abilities,
            }),
            &[
                "Partition the keyword list",
                "Preserve structured protection qualities",
                "Preserve the annihilator quantity",
            ],
        ));
    }
    let mut protection_modifiers = Vec::new();
    let mut contains_protection = false;
    for label in keyword_text.split(", ") {
        if let Some((_, kind)) = simple_keywords
            .iter()
            .find(|(candidate, _)| label.eq_ignore_ascii_case(candidate))
        {
            protection_modifiers.push(json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": kind,
            }));
            continue;
        }
        let Some(qualities) = label
            .strip_prefix("protection from ")
            .or_else(|| label.strip_prefix("Protection from "))
        else {
            protection_modifiers.clear();
            break;
        };
        let qualities = qualities
            .split(" and ")
            .map(|quality| quality.trim().trim_start_matches("from "))
            .filter(|quality| !quality.is_empty())
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        if qualities.is_empty() {
            protection_modifiers.clear();
            break;
        }
        contains_protection = true;
        protection_modifiers.push(json!({
            "kind": "grantProtection",
            "objects": self_ref(),
            "from": qualities,
        }));
    }
    if contains_protection && !protection_modifiers.is_empty() {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": protection_modifiers,
            }),
            &[
                "Partition keyword and protection clauses",
                "Grant evergreen keywords",
                "Apply protection qualities",
            ],
        ));
    }
    let keyword_group = keyword_text
        .split(", ")
        .filter_map(|label| {
            simple_keywords
                .iter()
                .find(|(candidate, _)| label.eq_ignore_ascii_case(candidate))
                .map(|(_, kind)| json!({ "kind": kind }))
                .or_else(|| {
                    Regex::new(r"(?i)^firebending (\d+)$")
                        .expect("grouped firebending regex compiles")
                        .captures(label)
                        .and_then(|captures| {
                            Some(firebending_ability(integer(
                                captures[1].parse::<i64>().ok()?,
                            )))
                        })
                })
                .or_else(|| {
                    parse_keyword_cost(label, "Ward").map(|cost| {
                        json!({
                            "kind": "ward",
                            "cost": cost,
                        })
                    })
                })
                .or_else(|| {
                    Regex::new(r"(?i)^blitz ((?:\{[^}]+\})+)$")
                        .expect("grouped blitz regex compiles")
                        .captures(label)
                        .map(|captures| {
                            json!({
                                "kind": "blitz",
                                "cost": {
                                    "kind": "payMana",
                                    "manaCost": &captures[1],
                                },
                            })
                        })
                })
        })
        .collect::<Vec<_>>();
    if keyword_group.len() > 1 && keyword_group.len() == keyword_text.split(", ").count() {
        return Some(draft(
            json!({
                "kind": "keywordAbilityGroup",
                "source": self_ref(),
                "abilities": keyword_group,
            }),
            &["Partition keyword list", "Recognize evergreen keywords"],
        ));
    }
    let firebending_re =
        Regex::new(r"(?i)^Firebending (.+?)(?: \(.+\))?$").expect("firebending regex compiles");
    if let Some(captures) = firebending_re.captures(keyword_text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": firebending_ability(avatar_quantity(&captures[1])?),
            }),
            &[
                "Recognize firebending",
                "Resolve produced red-mana quantity",
            ],
        ));
    }
    let flying_firebending_re =
        Regex::new(r"(?i)^Flying, firebending (\d+)$").expect("flying firebending regex compiles");
    if let Some(captures) = flying_firebending_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbilityGroup",
                "source": self_ref(),
                "abilities": [
                    { "kind": "flying" },
                    firebending_ability(integer(captures[1].parse::<i64>().ok()?)),
                ],
            }),
            &["Partition keyword list", "Recognize flying and firebending"],
        ));
    }
    let mobilize_re = Regex::new(r"^Mobilize (\d+) \(Whenever this creature attacks, create .+\)$")
        .expect("mobilize regex compiles");
    if let Some(captures) = mobilize_re.captures(text) {
        let quantity = captures[1].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "mobilize",
                    "quantity": quantity,
                },
            }),
            &[
                "Recognize mobilize keyword",
                "Extract Warrior token quantity",
                "Register attack trigger",
            ],
        ));
    }
    if let Some(cost) = parse_keyword_cost(text, "Ward") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "ward",
                    "cost": cost,
                },
            }),
            &["Recognize ward keyword", "Resolve ward cost"],
        ));
    }
    if let Some(cost) = parse_keyword_cost(text, "Flashback") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "flashback",
                    "cost": cost,
                },
            }),
            &[
                "Recognize flashback keyword",
                "Resolve graveyard casting cost",
            ],
        ));
    }
    let impending_re =
        Regex::new(r"(?i)^Impending (\d+)(?:\x{2014}|-)((?:\{[^}]+\})+)(?: \(.+\))?$")
            .expect("impending keyword regex compiles");
    if let Some(captures) = impending_re.captures(text) {
        let time_counters = captures[1].parse::<i64>().ok()?;
        if time_counters <= 0 {
            return None;
        }
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "impending",
                    "timeCounters": integer(time_counters),
                    "cost": parse_keyword_cost(
                        &format!("Cost\u{2014}{}", captures.get(2)?.as_str()),
                        "Cost",
                    )?,
                },
            }),
            &[
                "Recognize Impending as a reusable alternative-casting mechanic",
                "Parse its alternative payment through the shared cost grammar",
                "Preserve the entering time-counter count",
                "Defer the temporary noncreature state and end-step trigger to the keyword executor",
            ],
        ));
    }
    let escape_re = Regex::new(&format!(
        r"(?i)^Escape(?:ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â|ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â|Ã¢â‚¬â€|â€”|—|-)((?:\{{[^}}]+\}})+), Exile any number of (other )?cards from your graveyard with ({}) or more card types among them\.(?: \(.+\))?$",
        count_word_pattern(),
    ))
    .expect("escape aggregate graveyard cost regex compiles");
    if let Some(captures) = escape_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "escape",
                    "costs": [
                        parse_keyword_cost(&format!("Cost—{}", &captures[1]), "Cost")?,
                        {
                            "kind": "exileGraveyardCards",
                            "player": controller(),
                            "where": Value::Null,
                            "excludeSource": captures.get(2).is_some(),
                            "quantity": { "kind": "anyNumber" },
                            "aggregateCondition": {
                                "kind": "distinctCardTypesAtLeast",
                                "count": integer(parse_number_word(&captures[3])?),
                            },
                            "decisionId": "escapeExileCards",
                        },
                    ],
                },
            }),
            &[
                "Recognize Escape as graveyard casting permission",
                "Parse its mana payment with the shared cost grammar",
                "Select any number of other graveyard cards",
                "Validate their aggregate distinct card-type count",
            ],
        ));
    }
    if let Some(cost) = parse_keyword_cost(text, "Escalate") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "escalate",
                    "cost": cost,
                },
            }),
            &[
                "Recognize the Escalate keyword",
                "Parse its payment with the shared cost grammar",
                "Repeat that cost for each selected mode beyond the first",
            ],
        ));
    }
    let unearth_re =
        Regex::new(r"^Unearth ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("unearth regex compiles");
    if let Some(captures) = unearth_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "activationZone": "graveyard",
                "activationCondition": { "kind": "sorceryTiming" },
                "costs": [{
                    "kind": "payMana",
                    "manaCost": &captures[1],
                }],
                "effects": [{
                    "kind": "returnAbilitySourceFromGraveyard",
                    "tapped": false,
                    "grantKeywords": ["haste"],
                    "exileAtNextEndStep": true,
                    "exileIfLeavesBattlefield": true,
                }],
            }),
            &[
                "Recognize the unearth keyword and its mana cost",
                "Activate from the graveyard only at sorcery timing",
                "Return the source with haste",
                "Install its delayed and replacement exile rules",
            ],
        ));
    }
    let echo_re =
        Regex::new(r"^Echo ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("echo keyword regex compiles");
    if let Some(captures) = echo_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "upkeep",
                    "player": controller(),
                },
                "condition": { "kind": "sourceNeedsEcho" },
                "effects": [{
                    "kind": "payManaOrSacrificeSource",
                    "manaCost": &captures[1],
                }],
            }),
            &[
                "Recognize the echo mana cost",
                "Wait for the controller's next upkeep after gaining control",
                "Pay the cost or sacrifice the source",
            ],
        ));
    }
    let blitz_re =
        Regex::new(r"^Blitz ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("blitz regex compiles");
    if let Some(captures) = blitz_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "blitz",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                },
            }),
            &[
                "Recognize blitz alternative cost",
                "Grant haste and death draw when paid",
                "Register next-end-step sacrifice when paid",
            ],
        ));
    }
    let dash_re = Regex::new(r"^Dash ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("dash regex compiles");
    if let Some(captures) = dash_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "dash",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                },
            }),
            &[
                "Recognize dash alternative cost",
                "Grant haste when the dash cost was paid",
                "Return the permanent at the next end step",
            ],
        ));
    }
    let cycling_re =
        Regex::new(r"^Cycling ((?:\{[^}]+\})+) \(.+Discard this card: Draw a card\.\)$")
            .expect("cycling regex compiles");
    if let Some(captures) = cycling_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "cycling",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                    "activationZone": "hand",
                    "costs": [{
                        "kind": "discardCard",
                        "card": self_ref(),
                    }],
                    "effects": [{
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    }],
                },
            }),
            &[
                "Recognize cycling keyword",
                "Resolve hand activation cost",
                "Discard source card",
                "Draw one card",
            ],
        ));
    }
    let typecycling_re = Regex::new(
        r"(?i)^([A-Za-z][A-Za-z '-]+)cycling ((?:\{[^}]+\})+) \(.+Discard this card: Search your library for (?:a|an) ([A-Za-z][A-Za-z '-]+) card, reveal it, put it into your hand, then shuffle\.\)$",
    )
    .expect("typed cycling regex compiles");
    if let Some(captures) = typecycling_re.captures(text) {
        let label_type = singular_card_term(captures.get(1)?.as_str());
        let searched_type = singular_card_term(captures.get(3)?.as_str());
        if !label_type.eq_ignore_ascii_case(&searched_type) {
            return None;
        }
        let where_filter = parse_permanent_criteria(&searched_type, face_name)?;
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "typecycling",
                    "cost": { "kind": "payMana", "manaCost": &captures[2] },
                    "activationZone": "hand",
                    "costs": [{ "kind": "discardCard", "card": self_ref() }],
                    "where": where_filter.clone(),
                    "effects": search_library_effects(where_filter, 1, "hand", false),
                },
            }),
            &[
                "Recognize a typed cycling keyword",
                "Cross-check the label against the searched card criterion",
                "Resolve the shared hand activation and library search",
            ],
        ));
    }
    let myriad_text = text
        .strip_suffix(
            " (Whenever this creature attacks, for each opponent other than defending player, you may create a token copy that's tapped and attacking that player or a planeswalker they control. Exile the tokens at end of combat.)",
        )
        .unwrap_or(text);
    if myriad_text == "Myriad" {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "myriad" },
            }),
            &[
                "Recognize myriad keyword",
                "Register multiplayer attack trigger",
            ],
        ));
    }
    if myriad_text == "Flying, myriad" {
        return Some(draft(
            json!({
                "kind": "keywordAbilityGroup",
                "source": self_ref(),
                "abilities": [
                    { "kind": "flying" },
                    { "kind": "myriad" },
                ],
            }),
            &["Partition keyword list", "Recognize flying and myriad"],
        ));
    }
    if let Some(cost) = parse_keyword_cost(text, "Warp") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "warp",
                    "cost": cost,
                },
            }),
            &[
                "Recognize warp keyword",
                "Resolve alternative hand casting cost",
                "Register next-end-step exile",
                "Grant later exile casting permission",
            ],
        ));
    }
    let basic_landcycling_re = Regex::new(
        r"^Basic landcycling ((?:\{[^}]+\})+) \(.+Discard this card: Search your library for a basic land card, reveal it, put it into your hand, then shuffle\.\)$",
    )
    .expect("basic landcycling regex compiles");
    if let Some(captures) = basic_landcycling_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "basicLandcycling",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                    "activationZone": "hand",
                    "costs": [{
                        "kind": "discardCard",
                        "card": self_ref(),
                    }],
                    "effects": search_library_effects(
                        json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                        1,
                        "hand",
                        false,
                    ),
                },
            }),
            &[
                "Recognize basic landcycling keyword",
                "Resolve hand activation cost",
                "Search for a basic land",
                "Reveal, move, and shuffle",
            ],
        ));
    }
    if text
        == "Ascend (If you control ten or more permanents, you get the city's blessing for the rest of the game.)"
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "ascend" },
            }),
            &[
                "Recognize ascend keyword",
                "Register city-blessing state action",
            ],
        ));
    }
    if text
        == "Conspire (As you cast this spell, you may tap two untapped creatures you control that share a color with it. When you do, copy it and you may choose a new target for the copy.)"
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "conspire" },
            }),
            &[
                "Recognize conspire keyword",
                "Register optional cast cost and spell copy",
            ],
        ));
    }
    if text.eq_ignore_ascii_case("Living weapon")
        || text.starts_with("Living weapon (When this Equipment enters,")
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "livingWeapon" },
            }),
            &[
                "Recognize living weapon",
                "Register Equipment enter trigger",
                "Create and attach a Germ token",
            ],
        ));
    }
    if text
        == "Undying (When this creature dies, if it had no +1/+1 counters on it, return it to the battlefield under its owner's control with a +1/+1 counter on it.)"
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "undying" },
            }),
            &[
                "Recognize undying keyword",
                "Register qualified dies trigger",
            ],
        ));
    }
    let kicker_sacrifice_re = Regex::new(
        r"^Kicker(?:â€”|—)Sacrifice a creature\. \(You may sacrifice a creature in addition to any other costs as you cast this spell\.\)$",
    )
    .expect("sacrifice kicker regex compiles");
    if kicker_sacrifice_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "kicker",
                    "cost": {
                        "kind": "sacrificePermanent",
                        "where": card_type("Creature"),
                    },
                    "repeatable": false,
                },
            }),
            &[
                "Recognize kicker keyword",
                "Resolve optional creature-sacrifice cost",
            ],
        ));
    }
    if let Some(cost_text) = text
        .strip_prefix("Kicker ")
        .filter(|cost_text| cost_text.contains(" and/or "))
    {
        let costs = cost_text
            .split(" and/or ")
            .map(|cost| parse_keyword_cost(&format!("Cost—{}", cost.trim()), "Cost"))
            .collect::<Option<Vec<_>>>()?;
        if costs.len() < 2 {
            return None;
        }
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "kicker",
                    "costs": costs,
                    "repeatable": false,
                },
            }),
            &[
                "Recognize independent kicker alternatives",
                "Parse every kicker through the shared cost grammar",
                "Allow any subset of the listed kicker costs",
            ],
        ));
    }
    let kicker_re = Regex::new(r"^Kicker (\{[^)]+\}) \(You may pay an additional .+\)$")
        .expect("kicker regex compiles");
    if let Some(captures) = kicker_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "kicker",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                    "repeatable": false,
                },
            }),
            &["Recognize kicker keyword", "Resolve additional cost"],
        ));
    }
    let replicate_re =
        Regex::new(r"^Replicate ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("replicate regex compiles");
    if let Some(captures) = replicate_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "replicate",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                    "repeatable": true,
                },
            }),
            &[
                "Recognize replicate keyword",
                "Resolve repeatable additional mana cost",
                "Register cast trigger for spell copies",
            ],
        ));
    }
    let plot_re = Regex::new(r"^Plot ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("plot regex compiles");
    if let Some(captures) = plot_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "plot",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                },
            }),
            &[
                "Recognize plot keyword",
                "Register sorcery-speed hand special action",
                "Grant a future free cast from exile",
            ],
        ));
    }
    let suspend_re = Regex::new(r"^Suspend (\d+)[^{]*((?:\{[^}]+\})+)(?: \(.+\))?$")
        .expect("suspend regex compiles");
    if let Some(captures) = suspend_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "suspend",
                    "timeCounters": captures[1].parse::<i64>().ok()?,
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[2],
                    },
                },
            }),
            &[
                "Recognize suspend keyword",
                "Exile the card with time counters",
                "Remove counters during its owner's upkeep and offer the free cast",
            ],
        ));
    }
    let foretell_re =
        Regex::new(r"^Foretell ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("foretell regex compiles");
    if let Some(captures) = foretell_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "foretell",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                    "exileCost": {
                        "kind": "payMana",
                        "manaCost": "{2}",
                    },
                },
            }),
            &[
                "Recognize foretell keyword",
                "Register the two-mana turn special action",
                "Grant a later cast from exile for the foretell cost",
            ],
        ));
    }
    let repeated_keyword_text = text
        .split_once(" (")
        .map(|(keywords, _)| keywords)
        .unwrap_or(text);
    if repeated_keyword_text.contains(',') {
        let abilities = repeated_keyword_text
            .split(',')
            .map(str::trim)
            .map(oracle_keyword_kind)
            .collect::<Option<Vec<_>>>();
        if let Some(abilities) = abilities.filter(|abilities| abilities.len() > 1) {
            return Some(draft(
                json!({
                    "kind": "keywordAbilityGroup",
                    "source": self_ref(),
                    "abilities": abilities
                        .into_iter()
                        .map(|kind| json!({ "kind": kind }))
                        .collect::<Vec<_>>(),
                }),
                &[
                    "Partition the comma-delimited keyword instances",
                    "Preserve repeated keyword instances independently",
                ],
            ));
        }
    }
    let cascade_re = Regex::new(r"^Cascade(?: \(.+\))?$").expect("cascade regex compiles");
    if cascade_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "cascade" },
            }),
            &[
                "Recognize cascade keyword",
                "Exile through the first cheaper nonland card",
                "Offer the card as a free cast and randomize the rest onto the bottom",
            ],
        ));
    }
    let rebound_re = Regex::new(r"^Rebound(?: \(.+\))?$").expect("rebound regex compiles");
    if rebound_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "rebound" },
            }),
            &[
                "Recognize rebound keyword",
                "Exile a hand-cast spell as it resolves",
                "Offer a free cast at its controller's next upkeep",
            ],
        ));
    }
    let storm_re = Regex::new(r"^Storm(?: \(.+\))?$").expect("storm regex compiles");
    if storm_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "storm",
                },
            }),
            &[
                "Recognize storm keyword",
                "Count spells cast before this spell this turn",
                "Register cast trigger for spell copies",
            ],
        ));
    }
    let gravestorm_re = Regex::new(r"^Gravestorm(?: \(.+\))?$").expect("gravestorm regex compiles");
    if gravestorm_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "gravestorm" },
            }),
            &[
                "Recognize gravestorm keyword",
                "Count permanents put into graveyards this turn",
                "Register cast trigger for spell copies",
            ],
        ));
    }
    let offspring_re = Regex::new(
        r"^Offspring ((?:\{[^}]+\})+) \(You may pay an additional .+ as you cast this spell\. If you do, when this creature enters, create a 1/1 token copy of it\.\)$",
    )
    .expect("offspring regex compiles");
    if let Some(captures) = offspring_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "offspring",
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    },
                    "repeatable": false,
                    "trigger": {
                        "event": {
                            "kind": "enterBattlefield",
                            "object": self_ref(),
                        },
                        "condition": {
                            "kind": "offspringCostWasPaid",
                            "spell": self_ref(),
                        },
                        "effects": [{
                            "kind": "createTokenCopy",
                            "controller": controller(),
                            "object": self_ref(),
                            "basePower": integer(1),
                            "baseToughness": integer(1),
                        }],
                    },
                },
            }),
            &[
                "Recognize offspring keyword",
                "Resolve optional additional cost",
                "Register paid-cost enter trigger",
                "Resolve 1/1 token-copy characteristics",
            ],
        ));
    }
    let paradigm_re =
        Regex::new(r"^Paradigm \(Then exile this spell\..+\)$").expect("paradigm regex compiles");
    if paradigm_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "paradigm",
                    "spellName": face_name,
                },
            }),
            &["Recognize paradigm keyword", "Bind spell-name identity"],
        ));
    }
    None
}
