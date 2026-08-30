use super::super::*;

pub(in crate::oracle::canonical) fn parse_composed_entry_replacement(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let parity_choice_re =
        Regex::new(r"(?i)^As (.+?) enters, choose odd or even\.(?: \(Zero is even\.\))?$")
            .expect("as-enters mana-value parity choice regex compiles");
    if let Some(captures) = parity_choice_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            return Some(draft(
                json!({
                    "kind": "replacementEffect",
                    "source": self_ref(),
                    "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                    "decisions": [{
                        "id": "chosenManaValueParity",
                        "kind": "chooseOption",
                        "options": ["odd", "even"],
                    }],
                    "replacement": [{
                        "kind": "storeDecision",
                        "decisionId": "chosenManaValueParity",
                    }],
                }),
                &[
                    "Choose a mana-value parity while entering",
                    "Persist that choice on the game object",
                ],
            ));
        }
    }
    let optional_discard_or_graveyard_re = Regex::new(
        r"(?i)^If this (artifact|creature|enchantment|land|permanent) would enter, you may discard (?:a|an) (.+?) card instead\. If you do, put this (artifact|creature|enchantment|land|permanent) onto the battlefield\. If you don't, put it into its owner's graveyard\.$",
    )
    .expect("optional entry-discard replacement regex compiles");
    if let Some(captures) = optional_discard_or_graveyard_re.captures(text)
        && captures[1].eq_ignore_ascii_case(&captures[3])
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "entryDiscard",
                    "kind": "chooseCardForReplacement",
                    "where": parse_permanent_criteria(&captures[2], "")?,
                    "minimum": integer(0),
                    "maximum": integer(1),
                }],
                "replacement": [{
                    "kind": "discardChosenCardOrReplaceEntry",
                    "decisionId": "entryDiscard",
                    "destination": "graveyard",
                }],
            }),
            &[
                "Parse the optional discard through shared card criteria",
                "Resolve the choice as an entry replacement",
                "Replace the battlefield entry only when the cost is declined",
            ],
        ));
    }
    let choose_card_name_re = Regex::new(
        r"(?i)^As (?:this|[A-Z][^.]+) (?:artifact|creature|enchantment|land|permanent) enters, (?:(?:look at|you may look at) an opponent's hand, then )?choose (?:any |a )card name\.$",
    )
    .expect("entering card-name choice regex compiles");
    if choose_card_name_re.is_match(text) {
        let looked_at_opponent_hand = text.to_ascii_lowercase().contains("opponent's hand");
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "chosenCardName",
                    "kind": "chooseCardName",
                    "where": Value::Null,
                    "lookAtOpponentHand": looked_at_opponent_hand,
                }],
                "replacement": [{
                    "kind": "storeDecision",
                    "decisionId": "chosenCardName",
                }],
            }),
            &[
                "Choose any card name while entering",
                "Persist the chosen name on the source",
            ],
        ));
    }
    let entering_color_choice_re = Regex::new(
        r"(?i)^As (this (?:artifact|aura|creature|enchantment|equipment|land|permanent)|[A-Z][^.]+) enters, choose a color\.$",
    )
    .expect("entering color-choice regex compiles");
    if let Some(captures) = entering_color_choice_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "chosenColor",
                    "kind": "chooseColor",
                    "options": ["W", "U", "B", "R", "G"],
                }],
                "replacement": [{ "kind": "storeDecision", "decisionId": "chosenColor" }],
            }),
            &[
                "Choose a color as the source enters",
                "Persist the chosen color",
            ],
        ));
    }
    if text == "As this Aura enters, choose a creature." {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "chosenCreature",
                    "kind": "chooseBattlefieldCreature",
                    "optional": false,
                }],
                "replacement": [{ "kind": "storeDecision", "decisionId": "chosenCreature" }],
            }),
            &[
                "Choose a battlefield creature as the Aura enters",
                "Persist the chosen object",
            ],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn strip_expansion_landfall_prefix(text: &str) -> Option<&str> {
    text.strip_prefix("Landfall â€” ")
        .or_else(|| text.strip_prefix("Landfall — "))
        .or_else(|| text.strip_prefix("Landfall Ã¢â‚¬â€ "))
}
