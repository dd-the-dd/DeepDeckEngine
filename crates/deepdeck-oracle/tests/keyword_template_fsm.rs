//! Template keywords (CR 702) compile their rules definition through the
//! ordinary ability parser: Oracle keyword line -> instantiated definition ->
//! the same IR the printed definition compiles to, with keyword provenance.
use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, keyword_expansion_rules,
    parse_oracle_card,
};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread")
        .join()
        .expect("parser")
}

/// The rules the printed definition text compiles to on the same face.
fn printed_rules(type_line: &str, definition: &str) -> Vec<Value> {
    let parsed = parse("Definition Probe", type_line, definition);
    assert_eq!(
        parsed.status, "canonical",
        "{definition}: {:#?}",
        parsed.diagnostics
    );
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical rule"))
        .collect()
}

fn single_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let [ability] = parsed.abilities.as_slice() else {
        panic!("one ability: {:#?}", parsed.abilities);
    };
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    ability.rule.as_ref().expect("canonical rule")
}

#[test]
fn accorder_paladin_battle_cry_compiles_its_cr_definition_with_provenance() {
    const TYPE_LINE: &str = "Creature \u{2014} Human Knight";
    let parsed = parse(
        "Accorder Paladin",
        TYPE_LINE,
        "Battle cry (Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn.)",
    );
    let rule = single_rule(&parsed);
    let definition = printed_rules(
        TYPE_LINE,
        "Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn.",
    );
    assert_eq!(rule["kind"], "keywordExpansion");
    assert_eq!(rule["keyword"], "battle-cry");
    assert_eq!(rule["parameters"], json!({}));
    assert_eq!(rule["definition"]["rulesSection"], "702.91a");
    assert_eq!(rule["definition"]["rulesClause"], "battle-cry.cr-702-91a");
    assert_eq!(rule["rules"], Value::Array(definition.clone()));

    // Flattened, the playable rule is the definition's rule plus provenance.
    let playable = keyword_expansion_rules(rule);
    let [flattened] = playable.as_slice() else {
        panic!("one playable rule: {playable:#?}");
    };
    let mut expected = definition[0].clone();
    expected["keywordProvenance"] = json!({
        "keyword": "battle-cry",
        "parameters": {},
        "rulesSection": "702.91a",
        "templateVersion": 1,
        "part": 0,
    });
    assert_eq!(flattened, &expected);
}

#[test]
fn a_bare_keyword_line_and_an_unrelated_rule_flatten_independently() {
    // Goblin Wardriver prints no reminder on some printings' reference text.
    let parsed = parse(
        "Goblin Wardriver",
        "Creature \u{2014} Goblin Warrior",
        "Battle cry\nFlying",
    );
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let expansion = parsed.abilities[0].rule.as_ref().unwrap();
    let flying = parsed.abilities[1].rule.as_ref().unwrap();
    assert_eq!(expansion["kind"], "keywordExpansion");
    assert_eq!(keyword_expansion_rules(flying), vec![flying.clone()]);
}

#[test]
fn a_foreign_reminder_fails_explicitly_instead_of_being_ignored() {
    let parsed = parse(
        "Near Miss",
        "Creature \u{2014} Human Knight",
        "Battle cry (Whenever this creature attacks, it gets +1/+0 until end of turn.)",
    );
    assert_eq!(parsed.status, "unsupported");
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("keyword.template.reminder.body")
    );
    assert_eq!(diagnostic.automaton.as_deref(), Some("keyword-template"));
}

/// The IR the native bushido program emitted before its migration
/// (`parameterized_keyword_native.rs`, removed): the template must keep it.
fn former_native_bushido(count: i64) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "oneOf",
            "events": [
                { "kind": "declaredBlocker", "object": { "kind": "self" } },
                { "kind": "permanentBecameBlocked", "object": { "kind": "self" } },
            ],
        },
        "effects": [{
            "kind": "modifyPowerToughness",
            "object": { "kind": "self" },
            "power": { "kind": "integer", "value": count },
            "toughness": { "kind": "integer", "value": count },
            "duration": { "kind": "untilEndOfCurrentTurn" },
        }],
    })
}

fn expanded(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse(name, type_line, text);
    let rule = single_rule(&parsed);
    assert_eq!(rule["kind"], "keywordExpansion", "{text}");
    rule["rules"].as_array().expect("expanded rules").clone()
}

#[test]
fn migrated_bushido_and_living_weapon_keep_their_executable_ir() {
    // Konda's Hatamoto (bushido 1) and Sensei Golden-Tail-style bushido 2.
    for count in [1, 2] {
        let rules = expanded(
            "Bushido Bearer",
            "Creature \u{2014} Human Samurai",
            &format!(
                "Bushido {count} (Whenever this creature blocks or becomes blocked, it gets +{count}/+{count} until end of turn.)"
            ),
        );
        assert_eq!(rules, vec![former_native_bushido(count)]);
    }
    // Batterskull.
    let rules = expanded(
        "Batterskull",
        "Artifact \u{2014} Equipment",
        "Living weapon (When this Equipment enters, create a 0/0 black Phyrexian Germ creature token, then attach this to it.)",
    );
    assert_eq!(
        rules,
        vec![json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
            "effects": [
                {
                    "kind": "createTokens",
                    "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "quantity": { "kind": "integer", "value": 1 },
                    "token": {
                        "types": ["Creature"],
                        "subtypes": ["Phyrexian", "Germ"],
                        "colors": ["Black"],
                        "power": 0,
                        "toughness": 0,
                        "abilities": [],
                    },
                    // The attach reads the created token: the creation binds
                    // it (the former native program left it unbound).
                    "bind": "createdTokens",
                },
                {
                    "kind": "attachPermanent",
                    "attachment": { "kind": "self" },
                    "to": { "kind": "boundObjects", "binding": "createdTokens" },
                },
            ],
        })]
    );
}

#[test]
fn afterlife_and_soulshift_compile_the_cr_zone_change_wording() {
    // CR 702.135a / 702.46a say "put into a graveyard from the battlefield":
    // the departure pattern, not the former creature-only `permanentDied`.
    const SPIRIT: &str = "Creature \u{2014} Spirit";
    for (text, definition) in [
        (
            "Afterlife 2 (When this creature dies, create two 1/1 white and black Spirit creature tokens with flying.)",
            "When this permanent is put into a graveyard from the battlefield, create two 1/1 white and black Spirit creature tokens with flying.",
        ),
        // Number agreement for one token; Indebted Spirit's reminder wording.
        (
            "Afterlife 1 (When this permanent is put into a graveyard from the battlefield, create a 1/1 white and black Spirit creature token with flying.)",
            "When this permanent is put into a graveyard from the battlefield, create a 1/1 white and black Spirit creature token with flying.",
        ),
        (
            "Soulshift 3 (When this creature dies, you may return target Spirit card with mana value 3 or less from your graveyard to your hand.)",
            "When this permanent is put into a graveyard from the battlefield, you may return target Spirit card with mana value 3 or less from your graveyard to your hand.",
        ),
    ] {
        let rules = expanded("Spirit Bearer", SPIRIT, text);
        assert_eq!(rules, printed_rules(SPIRIT, definition), "{text}");
        assert_eq!(
            rules[0]["event"],
            json!({
                "kind": "permanentLeftBattlefield",
                "object": { "kind": "self" },
                "destination": "graveyard",
            }),
            "{text}"
        );
    }
}

#[test]
fn migrated_keywords_keep_their_explicit_failures() {
    for (text, type_line, state) in [
        // A reminder quantity that is not the printed parameter.
        (
            "Afterlife 2 (When this creature dies, create a 1/1 white and black Spirit creature token with flying.)",
            "Creature \u{2014} Spirit",
            "keyword.template.reminder.body",
        ),
        (
            "Soulshift 4 (When this creature dies, you may return target Spirit card with mana value 3 or less from your graveyard to your hand.)",
            "Creature \u{2014} Spirit",
            "keyword.template.reminder.body",
        ),
        (
            "Bushido 2 (Whenever this creature blocks or becomes blocked, it gets +1/+1 until end of turn.)",
            "Creature \u{2014} Samurai",
            "keyword.template.reminder.body",
        ),
        // The definition names its source "this Equipment".
        ("Living weapon", "Artifact", "keyword.template.face-subtype"),
        // A variable count is claimed and refused.
        (
            "Bushido X",
            "Creature \u{2014} Samurai",
            "numeric.atom.start",
        ),
    ] {
        let parsed = parse("Near Miss", type_line, text);
        assert_eq!(parsed.status, "unsupported", "{text}");
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("keyword-template"),
            "{text}"
        );
        assert!(
            diagnostic.state.as_deref() == Some(state)
                || diagnostic
                    .trace
                    .iter()
                    .any(|transition| transition.state == state),
            "{text}: {diagnostic:?}"
        );
    }
}

#[test]
fn batch_keywords_compile_their_cr_definitions_on_real_cards() {
    for (name, type_line, line, definition) in [
        (
            "Arcbound Worker",
            "Artifact Creature \u{2014} Construct",
            "Modular 1 (This creature enters with a +1/+1 counter on it. When it dies, you may put its +1/+1 counters on target artifact creature.)",
            "This permanent enters with a +1/+1 counter on it.\nWhen this permanent is put into a graveyard from the battlefield, you may put a +1/+1 counter on target artifact creature for each +1/+1 counter on this permanent.",
        ),
        (
            "Arcbound Bruiser",
            "Artifact Creature \u{2014} Construct",
            "Modular 3 (This creature enters with three +1/+1 counters on it. When it dies, you may put its +1/+1 counters on target artifact creature.)",
            "This permanent enters with three +1/+1 counters on it.\nWhen this permanent is put into a graveyard from the battlefield, you may put a +1/+1 counter on target artifact creature for each +1/+1 counter on this permanent.",
        ),
        (
            "Peema Outrider",
            "Creature \u{2014} Elf Knight",
            "Fabricate 1 (When this creature enters, put a +1/+1 counter on it or create a 1/1 colorless Servo artifact creature token.)",
            "When this permanent enters, you may put a +1/+1 counter on it. If you don't, create a 1/1 colorless Servo artifact creature token.",
        ),
        (
            "Glint-Sleeve Artisan",
            "Creature \u{2014} Dwarf Artificer",
            "Fabricate 2 (When this creature enters, put two +1/+1 counters on it or create two 1/1 colorless Servo artifact creature tokens.)",
            "When this permanent enters, you may put two +1/+1 counters on it. If you don't, create two 1/1 colorless Servo artifact creature tokens.",
        ),
        (
            "Mirran Bardiche",
            "Artifact \u{2014} Equipment",
            "For Mirrodin! (When this Equipment enters, create a 2/2 red Rebel creature token, then attach this to it.)",
            "When this Equipment enters, create a 2/2 red Rebel creature token, then attach this Equipment to it.",
        ),
        (
            "Monk's Fist",
            "Artifact \u{2014} Equipment",
            "Job select (When this Equipment enters, create a 1/1 colorless Hero creature token, then attach this to it.)",
            "When this Equipment enters, create a 1/1 colorless Hero creature token, then attach this Equipment to it.",
        ),
        (
            "Ainok Bond-Kin",
            "Creature \u{2014} Dog Soldier",
            "Outlast {1}{W} ({1}{W}, {T}: Put a +1/+1 counter on this creature. Outlast only as a sorcery.)",
            "{1}{W}, {T}: Put a +1/+1 counter on this creature. Activate only as a sorcery.",
        ),
        (
            "Hunting Triad",
            "Tribal Sorcery \u{2014} Elf",
            "Reinforce 3\u{2014}{3}{G} ({3}{G}, Discard this card: Put three +1/+1 counters on target creature.)",
            "{3}{G}, Discard this card: Put three +1/+1 counters on target creature.",
        ),
        (
            "Frenzy Probe",
            "Creature \u{2014} Human Warrior",
            "Frenzy 2 (Whenever this creature attacks and isn't blocked, it gets +2/+0 until end of turn.)",
            "Whenever this creature attacks and isn't blocked, it gets +2/+0 until end of turn.",
        ),
    ] {
        let rules = expanded(name, type_line, line);
        assert_eq!(rules, printed_rules(type_line, definition), "{name}");
    }
}

#[test]
fn two_template_keywords_on_one_face_expand_independently() {
    // An unused combination: fabricate and frenzy on one artifact creature.
    let parsed = parse(
        "Combination Probe",
        "Artifact Creature \u{2014} Construct",
        "Fabricate 2\nFrenzy 1",
    );
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let playable = parsed
        .abilities
        .iter()
        .flat_map(|ability| keyword_expansion_rules(ability.rule.as_ref().unwrap()))
        .collect::<Vec<_>>();
    let keywords = playable
        .iter()
        .map(|rule| {
            (
                rule["keywordProvenance"]["keyword"]
                    .as_str()
                    .unwrap()
                    .to_string(),
                rule["keywordProvenance"]["part"].as_u64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        keywords,
        [("fabricate".to_string(), 0), ("frenzy".to_string(), 0)]
    );
}

#[test]
fn unsupported_batch_shapes_stay_explicit() {
    // Swell of Courage: a variable reinforce count is claimed and refused.
    let parsed = parse(
        "Swell of Courage",
        "Instant",
        "Reinforce X\u{2014}{X}{W}{W} ({X}{W}{W}, Discard this card: Put X +1/+1 counters on target creature.)",
    );
    assert_eq!(parsed.status, "unsupported");
    assert_eq!(
        parsed.abilities[0].diagnostics[0].automaton.as_deref(),
        Some("keyword-template")
    );
    // Arcbound Wanderer: a modular line without a count is not claimed by
    // the template; the structural keyword frame reports it.
    let parsed = parse(
        "Arcbound Wanderer",
        "Artifact Creature \u{2014} Construct",
        "Modular\u{2014}Sunburst (This creature enters with a +1/+1 counter on it for each color of mana spent to cast it. When it dies, you may put its +1/+1 counters on target artifact creature.)",
    );
    assert_eq!(parsed.status, "unsupported");
    assert_ne!(
        parsed.abilities[0].diagnostics[0].automaton.as_deref(),
        Some("keyword-template")
    );
}
