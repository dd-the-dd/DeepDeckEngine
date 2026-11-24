//! Casting keyword lines (CR 702): one shared line reader, typed lowering.
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

fn ability(name: &str, type_line: &str, text: &str) -> Value {
    let rule = parse(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
    assert_eq!(rule["kind"], "keywordAbility", "{name}");
    rule["ability"].clone()
}

fn pay(cost: &str) -> Value {
    json!({ "kind": "payMana", "manaCost": cost })
}

/// Real keyword lines whose definitions the runtime executes directly.
#[test]
fn intrinsic_casting_keywords_lower_to_their_runtime_kinds() {
    let cases: &[(&str, &str, &str, Value)] = &[
        (
            "Workshop Warchief",
            "Creature — Rhino Warrior",
            "Blitz {4}{G}{G} (If you cast this spell for its blitz cost, it gains haste and \"When this creature dies, draw a card.\" Sacrifice it at the beginning of the next end step.)",
            json!({ "kind": "blitz", "cost": pay("{4}{G}{G}") }),
        ),
        (
            "Steel Seraph",
            "Artifact Creature — Angel",
            "Prototype {1}{W}{W} — 3/3 (You may cast this spell with different mana cost, color, and size. It keeps its abilities and types.)",
            json!({
                "kind": "prototype",
                "cost": pay("{1}{W}{W}"),
                "power": { "kind": "integer", "value": 3 },
                "toughness": { "kind": "integer", "value": 3 },
            }),
        ),
        (
            "Overlord of the Hauntwoods",
            "Enchantment Creature — Avatar Horror",
            "Impending 4—{1}{G}{G} (If you cast this spell for its impending cost, it enters with four time counters and isn't a creature until the last is removed. At the beginning of your end step, remove a time counter from it.)",
            json!({
                "kind": "impending",
                "timeCounters": { "kind": "integer", "value": 4 },
                "cost": pay("{1}{G}{G}"),
            }),
        ),
        (
            "Coastal Discovery",
            "Sorcery",
            "Awaken 2—{5}{U} (If you cast this spell for {5}{U}, also put two +1/+1 counters on target land you control and it becomes a 0/0 Elemental creature with haste. It's still a land.)",
            json!({
                "kind": "awaken",
                "count": { "kind": "integer", "value": 2 },
                "cost": pay("{5}{U}"),
            }),
        ),
        (
            "Burn Trail",
            "Sorcery",
            "Conspire (As you cast this spell, you may tap two untapped creatures you control that share a color with it. When you do, copy it and you may choose a new target for the copy.)",
            json!({ "kind": "conspire" }),
        ),
        (
            "Bitter Ordeal",
            "Sorcery",
            "Gravestorm (When you cast this spell, copy it for each permanent put into a graveyard from the battlefield this turn.)",
            json!({ "kind": "gravestorm" }),
        ),
        (
            "Collective Resistance",
            "Instant",
            "Escalate {G} (Pay this cost for each mode chosen beyond the first.)",
            json!({ "kind": "escalate", "cost": pay("{G}") }),
        ),
        (
            "Dream's Grip",
            "Instant",
            "Entwine {1} (Choose both if you pay the entwine cost.)",
            json!({ "kind": "entwine", "cost": pay("{1}") }),
        ),
        (
            "Ultramarines Honour Guard",
            "Creature — Astartes Warrior",
            "Teamwork 2 (As an additional cost to cast this spell, you may tap any number of creatures you control with total power 2 or more.)",
            json!({ "kind": "teamwork", "minimumPower": { "kind": "integer", "value": 2 } }),
        ),
    ];
    for (name, type_line, text, expected) in cases {
        assert_eq!(&ability(name, type_line, text), expected, "{name}");
    }
}

/// Spectacle, surge and web-slinging are alternative-cast permissions
/// lowered through the one shared permission (CR 118.9).
#[test]
fn alternative_cost_keywords_share_the_alternative_cast_permission() {
    let spectacle = ability(
        "Skewer the Critics",
        "Sorcery",
        "Spectacle {R} (You may cast this spell for its spectacle cost rather than its mana cost if an opponent lost life this turn.)",
    );
    assert_eq!(spectacle["kind"], "alternativeCost");
    assert_eq!(spectacle["mode"], "spectacle");
    assert_eq!(spectacle["costs"], json!([pay("{R}")]));
    assert_eq!(spectacle["condition"]["kind"], "compare");
    assert_eq!(
        spectacle["condition"]["left"]["kind"],
        "countEventsThisTurn"
    );
    assert_eq!(spectacle["condition"]["left"]["event"], "lifeLost");
    assert_eq!(
        spectacle["condition"]["left"]["player"]["kind"],
        "opponentsOf"
    );

    let surge = ability(
        "Reckless Bushwhacker",
        "Creature — Goblin Warrior Ally",
        "Surge {1}{R} (You may cast this spell for its surge cost if you or a teammate has cast another spell this turn.)",
    );
    assert_eq!(surge["mode"], "surge");
    assert_eq!(surge["condition"]["left"]["event"], "spellCast");
    assert_eq!(surge["condition"]["operator"], ">=");

    let web = ability(
        "Spider-Girl",
        "Legendary Creature — Spider Human Hero",
        "Web-slinging {W} (You may cast this spell for {W} if you also return a tapped creature you control to its owner's hand.)",
    );
    assert_eq!(web["mode"], "webSlinging");
    assert_eq!(web["costs"][0], pay("{W}"));
    assert_eq!(web["costs"][1]["kind"], "returnPermanentToOwnersHand");
    assert!(web.get("condition").is_none());
}

/// Escape reads the shared cost sequence after its em dash (CR 702.138a).
#[test]
fn escape_reads_the_shared_cost_sequence() {
    let escape = ability(
        "Kroxa, Titan of Death's Hunger",
        "Legendary Creature — Elder Giant",
        "Escape—{B}{B}{R}{R}, Exile five other cards from your graveyard. (You may cast this card from your graveyard for its escape cost.)",
    );
    assert_eq!(escape["kind"], "escape");
    assert_eq!(escape["costs"][0], pay("{B}{B}{R}{R}"));
    assert_eq!(escape["costs"][1]["kind"], "exileGraveyardCards");
    assert_eq!(escape["costs"][1]["excludeSource"], true);
    assert_eq!(
        escape["costs"][1]["quantity"],
        json!({ "kind": "exactly", "count": { "kind": "integer", "value": 5 } })
    );
    // Sibling on a sorcery face, without a reminder.
    let sibling = ability(
        "Underworld Probe",
        "Sorcery",
        "Escape—{2}{R}, Exile two other cards from your graveyard.",
    );
    assert_eq!(sibling["costs"][1]["quantity"]["count"]["value"], 2);
}

#[test]
fn casting_keyword_near_misses_fail_explicitly() {
    for (text, reason) in [
        (
            "Spectacle {R} (You may cast this spell for its spectacle cost rather than its mana cost if an opponent gained life this turn.)",
            "a reminder that is not the keyword's",
        ),
        (
            "Impending 4—{1}{G}{G} (If you cast this spell for its impending cost, it enters with five time counters and isn't a creature until the last is removed. At the beginning of your end step, remove a time counter from it.)",
            "a reminder count other than the printed one",
        ),
        (
            "Escape—{2}{R}, Exile a land you control, Exile five other cards from your graveyard.",
            "an escape payment the runtime does not pay",
        ),
        (
            "Blitz—{2}{B}{B}, Pay 2 life. (If you cast this spell for its blitz cost, it gains haste and \"When this creature dies, draw a card.\" Sacrifice it at the beginning of the next end step.)",
            "a nonmana blitz payment",
        ),
        (
            "Escalate—Discard a card. (Pay this cost for each mode chosen beyond the first.)",
            "a nonmana escalate payment",
        ),
        ("Prototype {1}{W}{W} — X/3", "a variable prototype size"),
        ("Surge {X}{R}", "a variable surge cost"),
    ] {
        assert!(
            parse("Near Miss", "Creature — Human", text).is_err(),
            "{reason}: {text}"
        );
    }
}
