use super::KEYWORDS;
use crate::oracle::canonical::MachineFailure;
use crate::oracle::canonical::{claims_parameterized_keyword_family, parse_parameterized_keyword};
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_oracle_entities};
use serde_json::{Value, json};

fn claimed(text: &str, type_line: &str) -> bool {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Keyword Bearer",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    claims_parameterized_keyword_family(&input)
}

fn lowered(text: &str, type_line: &str) -> Result<Value, MachineFailure> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Keyword Bearer",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    assert!(claims_parameterized_keyword_family(&input), "{text}");
    parse_parameterized_keyword(&input).map(|draft| draft.rule)
}

const CREATURE: &str = "Creature \u{2014} Phyrexian Rat";
const INSTANT: &str = "Instant";
const ENCHANTMENT: &str = "Enchantment";

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn count_keywords_read_the_shared_numeral_and_validate_their_reminder() {
    // Karumonix (toxic 1), Ribskiff (toxic 2), Artisan of Kozilek (annihilator 2).
    for (text, kind, field, value) in [
        (
            "Toxic 1 (Players dealt combat damage by this creature also get a poison counter.)",
            "toxic",
            "count",
            1,
        ),
        (
            "Toxic 2 (Players dealt combat damage by this creature also get two poison counters.)",
            "toxic",
            "count",
            2,
        ),
        // A reminder may restate the poison loss rule (Bloodroot Apothecary).
        (
            "Toxic 2 (Players dealt combat damage by this creature also get two poison counters. A player with ten or more poison counters loses the game.)",
            "toxic",
            "count",
            2,
        ),
        ("Toxic 6", "toxic", "count", 6),
        (
            "Annihilator 2 (Whenever this creature attacks, defending player sacrifices two permanents of their choice.)",
            "annihilator",
            "quantity",
            2,
        ),
        (
            "Annihilator 1 (Whenever this creature attacks, defending player sacrifices a permanent of their choice.)",
            "annihilator",
            "quantity",
            1,
        ),
    ] {
        let rule = lowered(text, CREATURE).expect(text);
        assert_eq!(rule["kind"], "keywordAbility", "{text}");
        assert_eq!(rule["ability"]["kind"], kind, "{text}");
        assert_eq!(
            rule["ability"][field],
            json!({ "kind": "integer", "value": value }),
            "{text}"
        );
    }
}

#[test]
fn reminder_quantity_must_match_the_printed_parameter() {
    for text in [
        "Toxic 2 (Players dealt combat damage by this creature also get a poison counter.)",
        "Annihilator 3 (Whenever this creature attacks, defending player sacrifices two permanents of their choice.)",
        "Buyback {3} (You may pay an additional {2} as you cast this spell. If you do, put this card into your hand as it resolves.)",
    ] {
        let failure = lowered(text, CREATURE).expect_err(text);
        assert_eq!(
            failure.state, "keyword.parameterized.reminder.body",
            "{text}"
        );
    }
}

#[test]
fn ability_words_and_variable_parameters_are_not_silently_accepted() {
    // Vrock: an ability word sharing the lexeme is not the keyword.
    assert!(!claimed(
        "Toxic Spores \u{2014} At the beginning of your end step, if a permanent you controlled left the battlefield this turn, each opponent loses 3 life.",
        CREATURE
    ));
    // Memory Crystal names buyback costs; it does not have buyback.
    assert!(!claimed("Buyback costs cost {2} less.", "Artifact"));
    // That Which Was Compleated: a variable count is claimed and refused.
    let failure = lowered(
        "Annihilator X, where X is the total number of poison counters your opponents have.",
        CREATURE,
    )
    .expect_err("variable annihilator");
    assert!(
        failure
            .trace
            .iter()
            .any(|transition| transition.state == "keyword.parameterized.parameter"),
        "{failure:?}"
    );
}

#[test]
fn optional_additional_cost_keywords_keep_their_typed_costs() {
    // Clockspinning (mana buyback) and Slaughter (life buyback).
    let mana = lowered(
        "Buyback {3} (You may pay an additional {3} as you cast this spell. If you do, put this card into your hand as it resolves.)",
        INSTANT,
    )
    .expect("mana buyback");
    assert_eq!(mana["kind"], "spellAbility");
    assert_eq!(
        mana["declaration"]["decisions"][0],
        json!({
            "id": "buybackMode",
            "kind": "chooseModes",
            "minimum": 1,
            "maximum": 1,
            "options": ["decline", "buyback"],
        })
    );
    assert_eq!(
        mana["declaration"]["additionalCosts"][0]["then"],
        json!([{ "kind": "payMana", "manaCost": "{3}" }])
    );
    let life = lowered(
        "Buyback\u{2014}Pay 4 life. (You may pay 4 life in addition to any other costs as you cast this spell. If you do, put this card into your hand as it resolves.)",
        INSTANT,
    )
    .expect("life buyback");
    assert_eq!(
        life["declaration"]["additionalCosts"][0]["then"],
        json!([{ "kind": "payLife", "player": controller(), "amount": { "kind": "integer", "value": 4 } }])
    );
    // Reaping the Rewards: nonmana, nonlife costs are refused explicitly.
    let failure = lowered(
        "Buyback\u{2014}Sacrifice a land. (You may sacrifice a land in addition to any other costs as you cast this spell. If you do, put this card into your hand as it resolves.)",
        INSTANT,
    )
    .expect_err("sacrifice buyback");
    assert!(failure.state.starts_with("cost.") || failure.state.starts_with("keyword."));

    // Agate Instigator: the runtime offspring contract.
    let offspring = lowered(
        "Offspring {1}{R} (You may pay an additional {1}{R} as you cast this spell. If you do, when this creature enters, create a 1/1 token copy of it.)",
        CREATURE,
    )
    .expect("offspring");
    assert_eq!(
        offspring["ability"],
        json!({
            "kind": "offspring",
            "cost": { "kind": "payMana", "manaCost": "{1}{R}" },
            "repeatable": false,
            "trigger": {
                "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
                "condition": { "kind": "offspringCostWasPaid", "spell": { "kind": "self" } },
                "effects": [{
                    "kind": "createTokenCopy",
                    "controller": controller(),
                    "object": { "kind": "self" },
                    "basePower": { "kind": "integer", "value": 1 },
                    "baseToughness": { "kind": "integer", "value": 1 },
                }],
            },
        })
    );
}

#[test]
fn cumulative_upkeep_repeats_its_cost_once_per_age_counter() {
    let reminder = "(At the beginning of your upkeep, put an age counter on this permanent, then sacrifice it unless you pay its upkeep cost for each age counter on it.)";
    let age_counters =
        json!({ "kind": "countCounters", "object": { "kind": "self" }, "counter": "age" });
    // Ancestral Knowledge ({1}) and Reality Twist ({1}{U}{U}).
    for cost in ["{1}", "{1}{U}{U}"] {
        let rule =
            lowered(&format!("Cumulative upkeep {cost} {reminder}"), ENCHANTMENT).expect(cost);
        assert_eq!(rule["event"]["step"], "upkeep");
        assert_eq!(rule["effects"][0]["counter"], "age");
        assert_eq!(
            rule["effects"][1]["cost"],
            json!({ "kind": "payMana", "manaCost": cost, "times": age_counters })
        );
        assert_eq!(
            rule["effects"][2]["then"],
            json!([{ "kind": "sacrificePermanent", "permanent": { "kind": "self" } }])
        );
    }
    // Glacial Chasm.
    let life = lowered(
        &format!("Cumulative upkeep\u{2014}Pay 2 life. {reminder}"),
        "Land",
    )
    .expect("life cumulative upkeep");
    assert_eq!(
        life["effects"][1]["cost"]["amount"],
        json!({ "kind": "multiply", "left": { "kind": "integer", "value": 2 }, "right": age_counters })
    );
    // Alternative costs (Jotun Owl Keeper) and other nonmana costs stay
    // explicit failures (Wall of Shards).
    for text in [
        format!("Cumulative upkeep {{W}} or {{U}} {reminder}"),
        format!("Cumulative upkeep\u{2014}An opponent gains 1 life. {reminder}"),
    ] {
        assert!(lowered(&text, ENCHANTMENT).is_err(), "{text}");
    }
}

#[test]
fn affinity_expands_to_the_shared_reduction_and_checks_its_reminder_node() {
    // Thought Monitor, Gate Colossus, Icebreaker Kraken.
    for (text, value) in [
        (
            "Affinity for artifacts (This spell costs {1} less to cast for each artifact you control.)",
            "Artifact",
        ),
        ("Affinity for artifacts", "Artifact"),
        (
            "Affinity for Gates (This spell costs {1} less to cast for each Gate you control.)",
            "Gate",
        ),
    ] {
        let rule = lowered(text, "Artifact Creature \u{2014} Golem").expect(text);
        assert_eq!(rule["kind"], "staticAbility", "{text}");
        assert_eq!(rule["activeWhile"]["zone"]["kind"], "stackOrCast", "{text}");
        let modifier = &rule["modifiers"][0];
        assert_eq!(modifier["kind"], "reduceOwnGenericCastingCost", "{text}");
        assert_eq!(modifier["amount"]["kind"], "countPermanents", "{text}");
        assert_eq!(modifier["amount"]["player"], controller(), "{text}");
        assert!(
            modifier["amount"]["where"].to_string().contains(value),
            "{text}: {}",
            modifier["amount"]["where"]
        );
    }
    // Spire Golem: a land subtype quality.
    lowered(
        "Affinity for Islands (This spell costs {1} less to cast for each Island you control.)",
        "Artifact Creature \u{2014} Golem",
    )
    .expect("land subtype quality");
    // The reminder must count the printed quality.
    let failure = lowered(
        "Affinity for artifacts (This spell costs {1} less to cast for each creature you control.)",
        "Artifact",
    )
    .expect_err("mismatched affinity reminder");
    assert_eq!(
        failure.state,
        "keyword.parameterized.reminder.reduction-match"
    );
}

#[test]
fn riot_expands_to_one_typed_entry_choice() {
    // Zhur-Taa Goblin and Arcbound Slasher's "additional" wording.
    for text in [
        "Riot (This creature enters with your choice of a +1/+1 counter or haste.)",
        "Riot (This creature enters with your choice of an additional +1/+1 counter or haste.)",
        "Riot",
    ] {
        let rule = lowered(text, CREATURE).expect(text);
        assert_eq!(rule["kind"], "replacementEffect", "{text}");
        assert_eq!(
            rule["decisions"][0]["options"],
            json!(["+1/+1 counter", "haste"]),
            "{text}"
        );
        assert_eq!(rule["replacement"][0]["kind"], "applyRiotChoice", "{text}");
    }
    // A keyword list containing riot stays with the shared keyword-list parent.
    assert!(!claimed("Trample, riot", CREATURE));
}

#[test]
fn afflict_triggers_when_its_source_becomes_blocked() {
    // Khenra Eternal-style afflict 2.
    let afflict = lowered(
        "Afflict 2 (Whenever this creature becomes blocked, defending player loses 2 life.)",
        CREATURE,
    )
    .expect("afflict");
    assert_eq!(
        afflict["event"],
        json!({ "kind": "permanentBecameBlocked", "object": { "kind": "self" } })
    );
    assert_eq!(
        afflict["effects"][0],
        json!({
            "kind": "loseLife",
            "player": { "kind": "triggeringPlayer" },
            "amount": { "kind": "integer", "value": 2 },
        })
    );
}

#[test]
fn every_keyword_entry_names_its_official_comprehensive_rules_section() {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../../../data/ability-catalog/official-ability-snapshot-v2.json"
    ))
    .expect("official ability snapshot");
    let abilities = snapshot["keywordAbilities"]
        .as_array()
        .expect("keyword abilities");
    for entry in KEYWORDS {
        let section = entry.keyword.rules_section();
        let official = abilities
            .iter()
            .find(|ability| ability["section"] == section)
            .unwrap_or_else(|| panic!("CR {section} is an official keyword ability"));
        assert_eq!(
            official["name"].as_str().unwrap().to_ascii_lowercase(),
            entry.lexeme.join(" "),
            "CR {section}"
        );
        assert!(
            entry.keyword.entry().keyword == entry.keyword,
            "one vocabulary entry per keyword"
        );
    }
}

#[test]
fn evoke_is_an_alternative_cost_in_the_engines_evoke_mode() {
    // Mulldrifter.
    let rule = lowered(
        "Evoke {2}{U} (You may cast this spell for its evoke cost. If you do, it's sacrificed when it enters.)",
        "Creature \u{2014} Elemental",
    )
    .expect("evoke");
    assert_eq!(
        rule["ability"],
        json!({
            "kind": "alternativeCost",
            "mode": "evoke",
            "costs": [{ "kind": "payMana", "manaCost": "{2}{U}" }],
        })
    );
    // Nonmana evoke costs (Solitude) are not claimed by this mana family.
    assert!(!claimed(
        "Evoke\u{2014}Exile a white card from your hand.",
        "Creature \u{2014} Elemental Incarnation"
    ));
}
