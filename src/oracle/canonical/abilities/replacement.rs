use super::super::*;

pub(in crate::oracle::canonical) fn parse_conditional_enter_tapped(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let controlled_count_re = Regex::new(&format!(
        r"^(?:This land|[A-Z][^.]+) enters tapped unless you control ({}) or (more|fewer) (?:other )?(.+)\.$",
        count_word_pattern(),
    ))
    .expect("generic controlled-count enter tapped regex compiles");
    if let Some(captures) = controlled_count_re.captures(text) {
        let threshold = integer(parse_number_word(&captures[1])?);
        let operator = if captures[2].eq_ignore_ascii_case("more") {
            ">="
        } else {
            "<="
        };
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "condition": not(compare(
                    operator,
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": parse_permanent_criteria(&captures[3], "")?,
                    }),
                    threshold,
                )),
                "replacement": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": true,
                }],
            }),
            &[
                "Parse the controlled-permanent quantity",
                "Compare it with the unless threshold",
                "Apply the tapped entering state",
            ],
        ));
    }
    let opponent_count_re = Regex::new(&format!(
        r"^(?:This land|[A-Z][^.]+) enters tapped unless you have ({}) or (more|fewer) opponents\.$",
        count_word_pattern(),
    ))
    .expect("generic opponent-count enter tapped regex compiles");
    if let Some(captures) = opponent_count_re.captures(text) {
        let operator = if captures[2].eq_ignore_ascii_case("more") {
            ">="
        } else {
            "<="
        };
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "condition": not(compare(
                    operator,
                    json!({ "kind": "countOpponents", "player": controller() }),
                    integer(parse_number_word(&captures[1])?),
                )),
                "replacement": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": true,
                }],
            }),
            &[
                "Count the controller's opponents",
                "Compare them with the unless threshold",
                "Apply the tapped entering state",
            ],
        ));
    }
    let opponent_permanent_count_re = Regex::new(&format!(
        r"^(?:This land|[A-Z][^.]+) enters tapped unless your opponents control ({}) or (more|fewer) (.+)\.$",
        count_word_pattern(),
    ))
    .expect("generic opponent permanent-count enter tapped regex compiles");
    if let Some(captures) = opponent_permanent_count_re.captures(text) {
        let operator = if captures[2].eq_ignore_ascii_case("more") {
            ">="
        } else {
            "<="
        };
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "condition": not(compare(
                    operator,
                    json!({
                        "kind": "greatestOpponentPermanentCount",
                        "player": controller(),
                        "where": parse_permanent_criteria(&captures[3], "")?,
                    }),
                    integer(parse_number_word(&captures[1])?),
                )),
                "replacement": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": true,
                }],
            }),
            &[
                "Count each opponent's matching permanents",
                "Compare the greatest count with the unless threshold",
                "Apply the tapped entering state",
            ],
        ));
    }
    let controlled_criteria_re =
        Regex::new(r"^(?:This land|[A-Z][^.]+) enters tapped unless you control (.+)\.$")
            .expect("generic controlled-criteria enter tapped regex compiles");
    if let Some(captures) = controlled_criteria_re.captures(text)
        && let Some(condition) =
            parse_controlled_permanent_condition(&format!("you control {}", &captures[1]), "")
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "condition": not(condition),
                "replacement": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": true,
                }],
            }),
            &[
                "Parse the controlled-permanent unless criterion",
                "Negate the unless condition",
                "Apply the tapped entering state",
            ],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_shock_land_replacement(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let captures = Regex::new(
        r"(?i)^As (?:this land|(?-i:[A-Z])[A-Za-z0-9 ',.-]+) enters, you may pay (\d+) life\. If you don't, it enters tapped\.$",
    )
    .expect("optional life payment entry regex compiles")
    .captures(text)?;
    let life = captures[1].parse::<i64>().ok()?;
    let decision_id = format!("pay{life}Life");
    Some(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": self_ref(),
            },
            "decisions": [{
                "id": decision_id,
                "kind": "chooseWhetherToPay",
                "player": controller(),
                "cost": {
                    "kind": "payLife",
                    "amount": integer(life),
                },
            }],
            "replacement": [{
                "kind": "conditional",
                "condition": not(json!({
                    "kind": "costWasPaid",
                    "decisionId": decision_id,
                })),
                "then": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": true,
                }],
            }],
        }),
        &[
            "Classify as-entering replacement",
            "Extract optional life payment",
            "Resolve unpaid-cost condition",
            "Attach entering-state replacement",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_multiversal_passage_replacement(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text
        != "As this land enters, choose a basic land type. Then you may pay 2 life. If you don't, it enters tapped."
    {
        return None;
    }
    Some(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": self_ref(),
            },
            "decisions": [
                {
                    "id": "basicLandType",
                    "kind": "chooseBasicLandType",
                    "player": controller(),
                    "options": ["Plains", "Island", "Swamp", "Mountain", "Forest"],
                },
                {
                    "id": "payTwoLife",
                    "kind": "chooseWhetherToPay",
                    "player": controller(),
                    "cost": {
                        "kind": "payLife",
                        "amount": integer(2),
                    },
                },
            ],
            "replacement": [
                {
                    "kind": "setBasicLandType",
                    "object": self_ref(),
                    "decisionId": "basicLandType",
                },
                {
                    "kind": "conditional",
                    "condition": not(json!({
                        "kind": "costWasPaid",
                        "decisionId": "payTwoLife",
                    })),
                    "then": [{
                        "kind": "setEnteringState",
                        "object": self_ref(),
                        "tapped": true,
                    }],
                },
            ],
        }),
        &[
            "Classify as-entering replacement",
            "Extract basic land type choice",
            "Extract optional life payment",
            "Apply chosen land type",
            "Attach entering-state replacement",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_prepared_replacement(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let prepared_re = Regex::new(
        r"^(?:This creature|[A-Z][^.]+) enters prepared\.(?: \(While it's prepared,.*\))?$",
    )
    .expect("prepared replacement regex compiles");
    if !prepared_re.is_match(text) {
        return None;
    }
    Some(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": self_ref(),
            },
            "replacement": [{
                "kind": "setPrepared",
                "object": self_ref(),
                "value": true,
            }],
        }),
        &[
            "Resolve source reference",
            "Recognize prepared vocabulary",
            "Attach prepared entering replacement",
        ],
    ))
}

pub(in crate::oracle::canonical) fn prepared_effect() -> Value {
    json!({
        "kind": "setPrepared",
        "object": self_ref(),
        "value": true,
    })
}
