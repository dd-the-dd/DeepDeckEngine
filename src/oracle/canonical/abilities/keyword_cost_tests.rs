//! `Keyword—COST` reads the shared activation-cost sequence and lowers it
//! through one keyword-cost lowering (Flashback, Equip, Kicker, Escalate);
//! parameterized keyword lines that reuse shared readers (disguise, ward
//! life costs, `hexproof from`, `Equip [quality]`).
use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, text: &str) -> Result<Value, String> {
    let result = parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let [ability] = result.abilities.as_slice() else {
        panic!("{name}: expected one ability");
    };
    ability.rule.clone().ok_or_else(|| {
        ability.diagnostics[0]
            .state
            .clone()
            .unwrap_or_else(|| ability.diagnostics[0].code.clone())
    })
}

fn keyword(name: &str, type_line: &str, text: &str) -> Value {
    let rule = parse(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
    assert_eq!(rule["kind"], "keywordAbility", "{name}");
    rule["ability"].clone()
}

#[test]
fn flashback_reads_nonmana_and_composite_costs_after_its_em_dash() {
    let composite = keyword(
        "Deep Analysis",
        "Sorcery",
        "Flashback—{1}{U}, Pay 3 life. (You may cast this card from your graveyard for its flashback cost. Then exile it.)",
    );
    assert_eq!(composite["kind"], "flashback");
    assert_eq!(composite["cost"]["kind"], "compositeCost");
    assert_eq!(
        composite["cost"]["costs"][0],
        json!({ "kind": "payMana", "manaCost": "{1}{U}" })
    );
    assert_eq!(composite["cost"]["costs"][1]["kind"], "payLife");
    assert_eq!(composite["cost"]["costs"][1]["amount"]["value"], 3);

    let sacrifice = keyword(
        "Cabal Therapy",
        "Sorcery",
        "Flashback—Sacrifice a creature. (You may cast this card from your graveyard for its flashback cost. Then exile it.)",
    );
    assert_eq!(sacrifice["cost"]["kind"], "sacrificePermanent");
    assert!(sacrifice["cost"]["where"].is_object());

    // Unused combination of supported concepts: a life payment and an
    // artifact sacrifice as one Flashback cost.
    let unused = keyword(
        "Probe Recollection",
        "Instant",
        "Flashback—Pay 2 life, Sacrifice an artifact.",
    );
    assert_eq!(unused["cost"]["kind"], "compositeCost");
    assert_eq!(unused["cost"]["costs"][1]["kind"], "sacrificePermanent");
}

#[test]
fn keyword_costs_the_runtime_cannot_pay_fail_explicitly() {
    for (type_line, text, reason) in [
        (
            "Sorcery",
            "Flashback—Sacrifice three creatures.",
            "several chosen objects in one keyword payment",
        ),
        (
            "Sorcery",
            "Flashback—Tap an untapped white creature you control.",
            "a lone tap payment the graveyard-cast runtime does not pay",
        ),
        (
            "Sorcery",
            "Flashback Sacrifice a creature.",
            "a nonmana cost without the em dash",
        ),
        (
            "Artifact — Equipment",
            "Equip—Pay 3 life.",
            "a life payment the attach runtime does not pay",
        ),
        (
            "Artifact — Equipment",
            "Equip—{2}, Pay 2 life.",
            "a composite Equip payment",
        ),
        (
            "Creature — Human Warrior",
            "Kicker—{1}{G}, Sacrifice a land. (You may pay {1}{G} and sacrifice a land in addition to any other costs as you cast this spell.)",
            "a composite nonmana kicker payment",
        ),
        (
            "Creature — Human Warrior",
            "Kicker—Sacrifice a land. (You may sacrifice a creature in addition to any other costs as you cast this spell.)",
            "a kicker reminder restating another cost",
        ),
    ] {
        assert!(
            parse("Near Miss", type_line, text).is_err(),
            "{reason}: {text}"
        );
    }
}

#[test]
fn equip_reads_a_sacrifice_cost_and_a_restricted_quality() {
    let sacrifice = keyword(
        "Dissection Tools",
        "Artifact — Equipment",
        "Equip—Sacrifice a creature.",
    );
    assert_eq!(sacrifice["kind"], "equip");
    assert_eq!(sacrifice["costs"][0]["kind"], "sacrificePermanent");

    let halfling = keyword(
        "Bilbo's Ring",
        "Legendary Artifact — Equipment",
        "Equip Halfling {1} ({1}: Attach to target Halfling you control. Equip only as a sorcery.)",
    );
    assert_eq!(
        halfling["costs"],
        json!([{ "kind": "payMana", "manaCost": "{1}" }])
    );
    assert!(halfling["where"].is_object());

    // The reminder must restate the printed quality.
    assert!(
        parse(
            "Near Miss",
            "Artifact — Equipment",
            "Equip Halfling {1} ({1}: Attach to target Knight you control. Equip only as a sorcery.)",
        )
        .is_err()
    );
    // A sentence about equip abilities is not the keyword.
    assert!(
        parse(
            "Fervent Champion",
            "Creature — Human Knight",
            "Equip abilities you activate that target this creature cost {3} less to activate.",
        )
        .map_or(true, |rule| rule["ability"]["kind"] != "equip")
    );
}

#[test]
fn kicker_and_escalate_accept_one_nonmana_keyword_payment() {
    let kicker = keyword(
        "Gnarlid Pack Raider",
        "Creature — Goblin Warrior",
        "Kicker—Sacrifice an artifact or creature. (You may sacrifice an artifact or creature in addition to any other costs as you cast this spell.)",
    );
    assert_eq!(kicker["kind"], "kicker");
    assert_eq!(kicker["cost"]["kind"], "sacrificePermanent");
    assert_eq!(kicker["repeatable"], false);

    let escalate = keyword(
        "Collective Brutality",
        "Sorcery",
        "Escalate—Discard a card. (Pay this cost for each mode chosen beyond the first.)",
    );
    assert_eq!(escalate["kind"], "escalate");
    assert_eq!(escalate["cost"]["kind"], "discardCard");
}

#[test]
fn ward_life_disguise_and_hexproof_from_reuse_shared_readers() {
    let ward = keyword(
        "Sedgemoor Witch",
        "Creature — Human Warlock",
        "Ward—Pay 3 life. (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays 3 life.)",
    );
    assert_eq!(ward["kind"], "ward");
    assert_eq!(ward["cost"]["kind"], "payLife");

    let disguise = keyword(
        "Coveted Falcon",
        "Artifact Creature — Bird",
        "Disguise {1}{U} (You may cast this card face down for {3} as a 2/2 creature with ward {2}. Turn it face up any time for its disguise cost.)",
    );
    assert_eq!(
        disguise,
        json!({
            "kind": "disguise",
            "cost": { "kind": "payMana", "manaCost": "{1}{U}" },
            "putCounterWhenTurnedFaceUp": false,
        })
    );

    let hexproof = keyword(
        "Jaheira, Harper Emissary",
        "Legendary Creature — Human Ranger",
        "Hexproof from artifacts and enchantments",
    );
    assert_eq!(
        hexproof,
        json!({ "kind": "hexproofFrom", "from": ["Artifact", "Enchantment"] })
    );
    let group = parse(
        "Sporeweb Weaver",
        "Creature — Spider",
        "Reach, hexproof from blue",
    )
    .expect("a keyword list ending with hexproof from");
    assert_eq!(group["kind"], "keywordAbilityGroup");
    assert_eq!(group["abilities"][0]["kind"], "reach");
    assert_eq!(group["abilities"][1]["from"], json!(["blue"]));
    let reminder = keyword(
        "Knight of Grace",
        "Creature — Human Knight",
        "Hexproof from black (This creature can't be the target of black spells or abilities your opponents control.)",
    );
    assert_eq!(reminder["from"], json!(["black"]));
    assert!(
        parse(
            "Near Miss",
            "Creature — Human",
            "Hexproof from multicolored"
        )
        .is_err()
    );
}
