//! The one alternative-cast permission sentence: shared cost sequence,
//! shared condition program, one lowering (CR 118.9).
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

fn permission(name: &str, type_line: &str, text: &str) -> Value {
    let rule = parse(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
    assert_eq!(rule["kind"], "keywordAbility", "{name}");
    assert_eq!(rule["ability"]["kind"], "alternativeCost", "{name}");
    rule["ability"].clone()
}

#[test]
fn printed_alternative_costs_read_the_shared_cost_sequence() {
    let bringer = permission(
        "Bringer of the Red Dawn",
        "Creature — Bringer",
        "You may pay {W}{U}{B}{R}{G} rather than pay this spell's mana cost.",
    );
    assert_eq!(
        bringer["costs"],
        json!([{ "kind": "payMana", "manaCost": "{W}{U}{B}{R}{G}" }])
    );
    assert!(bringer.get("condition").is_none());

    let flare = permission(
        "Flare of Denial",
        "Instant",
        "You may sacrifice a nontoken blue creature rather than pay this spell's mana cost.",
    );
    assert_eq!(flare["costs"][0]["kind"], "sacrificePermanent");
    assert_eq!(flare["costs"][0]["count"]["value"], 1);

    let borderpost = permission(
        "Fieldmist Borderpost",
        "Artifact",
        "You may pay {1} and return a basic land you control to its owner's hand rather than pay this spell's mana cost.",
    );
    assert_eq!(borderpost["costs"][0]["manaCost"], "{1}");
    assert_eq!(
        borderpost["costs"][1]["kind"],
        "returnPermanentToOwnersHand"
    );

    let fireblast = permission(
        "Fireblast",
        "Instant",
        "You may sacrifice two Mountains rather than pay this spell's mana cost.",
    );
    assert_eq!(fireblast["costs"][0]["count"]["value"], 2);

    let abolish = permission(
        "Abolish",
        "Instant",
        "You may discard a Plains card rather than pay this spell's mana cost.",
    );
    assert_eq!(abolish["costs"][0]["kind"], "discardCard");

    // Unused combination: a mana payment and a tapped artifact.
    let zahid = permission(
        "Zahid, Djinn of the Lamp",
        "Legendary Creature — Djinn",
        "You may pay {3}{U} and tap an untapped artifact you control rather than pay this spell's mana cost.",
    );
    assert_eq!(zahid["costs"][1]["kind"], "tap");
}

#[test]
fn conditional_permissions_read_the_shared_condition_program() {
    let dark_triumph = permission(
        "Dark Triumph",
        "Instant",
        "If you control a Swamp, you may sacrifice a creature rather than pay this spell's mana cost.",
    );
    assert!(dark_triumph["condition"].is_object());
    assert_eq!(dark_triumph["costs"][0]["kind"], "sacrificePermanent");

    let snuff_out = permission(
        "Snuff Out",
        "Instant",
        "If you control a Swamp, you may pay 4 life rather than pay this spell's mana cost.",
    );
    assert_eq!(snuff_out["costs"][0]["kind"], "payLife");

    let legate = permission(
        "Cho-Arrim Legate",
        "Creature — Human Soldier",
        "If an opponent controls a Swamp and you control a Plains, you may cast this spell without paying its mana cost.",
    );
    assert_eq!(legate["costs"], json!([]));
    assert!(legate["condition"].is_object());

    let trap = permission(
        "Archive Trap",
        "Instant — Trap",
        "If an opponent searched their library this turn, you may pay {0} rather than pay this spell's mana cost.",
    );
    assert_eq!(trap["costs"][0]["manaCost"], "{0}");
    assert!(trap["condition"].is_object());
}

#[test]
fn alternative_cast_near_misses_fail_explicitly() {
    for (name, type_line, text, reason) in [
        (
            "Security Rhox",
            "Creature — Rhino",
            "You may pay {R}{G} rather than pay this spell's mana cost. Spend only mana produced by Treasures to cast it this way.",
            "a payment restriction sentence",
        ),
        (
            "Ensnare",
            "Instant",
            "You may return two Islands you control to their owner's hand rather than pay this spell's mana cost.",
            "several returned permanents",
        ),
        (
            "Near Miss",
            "Instant",
            "You may pay {X} rather than pay this spell's mana cost.",
            "a variable alternative mana cost",
        ),
        (
            "Near Miss",
            "Instant",
            "If you control a Swamp, you may pay 4 life rather than pay this spell's mana cost if an opponent lost life this turn.",
            "two conditions on one permission",
        ),
    ] {
        assert!(parse(name, type_line, text).is_err(), "{reason}: {text}");
    }
}
