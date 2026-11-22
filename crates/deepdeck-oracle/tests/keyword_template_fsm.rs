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

#[test]
fn exploit_names_its_action_and_its_trigger_reads_the_keyword_event() {
    // Fell Stinger: exploit (CR 702.110a) and "exploits a creature" (702.110b).
    let parsed = parse(
        "Fell Stinger",
        "Creature \u{2014} Zombie Scorpion",
        "Deathtouch\nExploit (When this creature enters, you may sacrifice a creature.)\nWhen this creature exploits a creature, target player draws two cards and loses 2 life.",
    );
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let expansion = parsed.abilities[1].rule.as_ref().unwrap();
    assert_eq!(expansion["keyword"], "exploit");
    assert_eq!(
        expansion["namedAction"],
        json!({ "lemma": "exploit", "performedBy": "sacrifice" })
    );
    let playable = keyword_expansion_rules(expansion);
    assert_eq!(
        playable[0]["keywordProvenance"]["namedAction"]["lemma"],
        "exploit"
    );
    let trigger = parsed.abilities[2].rule.as_ref().unwrap();
    assert_eq!(
        trigger["event"],
        json!({
            "kind": "eventPattern",
            "lemma": "exploit",
            "subject": { "object": { "kind": "self" } },
            "counterpart": { "where": { "kind": "cardTypeContains", "value": "Creature" } },
        })
    );
    // The named form of the source (Sidisi, Undead Vizier) is the same event.
    let named = parse(
        "Sidisi, Undead Vizier",
        "Legendary Creature \u{2014} Zombie Naga",
        "When Sidisi exploits a creature, you gain 2 life.",
    );
    assert_eq!(named.status, "canonical", "{:#?}", named.diagnostics);
    assert_eq!(
        named.abilities[0].rule.as_ref().unwrap()["event"],
        trigger["event"]
    );
    // Another creature exploiting (Skull Skaab) has no matcher: explicit.
    let other = parse(
        "Skull Skaab",
        "Creature \u{2014} Zombie",
        "Whenever a creature you control exploits a nontoken creature, create a 2/2 black Zombie creature token.",
    );
    assert_eq!(other.status, "unsupported");
}

#[test]
fn bloodthirst_compiles_through_the_shared_conditional_entry_replacement() {
    // Gorehorn Minotaurs (bloodthirst 2), CR 702.54a.
    const TYPE_LINE: &str = "Creature \u{2014} Minotaur Warrior";
    let rules = expanded(
        "Gorehorn Minotaurs",
        TYPE_LINE,
        "Bloodthirst 2 (If an opponent was dealt damage this turn, this creature enters with two +1/+1 counters on it.)",
    );
    let condition = json!({
        "kind": "compare",
        "left": {
            "kind": "countOpponentsDealtDamageThisTurn",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
        "operator": ">=",
        "right": { "kind": "integer", "value": 1 },
    });
    assert_eq!(rules[0]["kind"], "replacementEffect");
    assert_eq!(rules[0]["condition"], condition);
    // The same condition written after the modification is the same rule
    // (a printed sibling wording); the condition slot is shared.
    let trailing = printed_rules(
        TYPE_LINE,
        "This creature enters with two +1/+1 counters on it if an opponent was dealt damage this turn.",
    );
    assert_eq!(rules, trailing);
    // Near miss: one condition before and another after is refused.
    let parsed = parse(
        "Near Miss",
        TYPE_LINE,
        "If you control a Swamp, this creature enters with a +1/+1 counter on it if an opponent was dealt damage this turn.",
    );
    assert_eq!(parsed.status, "unsupported");
    assert_eq!(
        parsed.abilities[0].diagnostics[0].state.as_deref(),
        Some("replacement.entry-counters.condition")
    );
}

#[test]
fn counter_removal_is_the_counter_action_in_the_removal_direction() {
    // Dark Depths (printed): the source's own counter, a literal count.
    let parsed = parse(
        "Dark Depths",
        "Legendary Snow Land",
        "{3}: Remove an ice counter from Dark Depths.",
    );
    let rule = single_rule(&parsed);
    assert_eq!(
        rule["effects"],
        json!([{
            "kind": "removeCounters",
            "permanent": { "kind": "self" },
            "counter": "ice",
            "count": 1,
        }])
    );
    // Vanishing (CR 702.63a): entry counters, the guarded removal whose `it`
    // is the guard's source, and the last-counter event.
    let rules = expanded(
        "Keldon Marauders",
        "Creature \u{2014} Human Warrior",
        "Vanishing 2 (This creature enters with two time counters on it. At the beginning of your upkeep, remove a time counter from it. When the last is removed, sacrifice it.)",
    );
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[1]["effects"][0]["kind"], "removeCounters");
    assert_eq!(
        rules[2]["event"],
        json!({ "kind": "counterRemoved", "object": { "kind": "self" }, "counter": "time", "last": true })
    );
    // Near miss: removing from another object has no runtime; explicit.
    let parsed = parse(
        "Near Miss",
        "Artifact",
        "{3}: Remove a +1/+1 counter from target creature.",
    );
    assert_eq!(parsed.status, "unsupported");
}

#[test]
fn evolve_guard_reads_the_event_object_through_the_trigger_frame() {
    // Evolve (CR 702.100a): `that creature` in the intervening `if` is the
    // event's entering creature, as in the effects; `this creature` is the
    // source; `and/or` is the shared disjunction connector.
    let rules = expanded(
        "Cloudfin Raptor",
        "Creature \u{2014} Bird Mutant",
        "Evolve (Whenever a creature you control enters, if that creature has greater power or toughness than this creature, put a +1/+1 counter on this creature.)",
    );
    let entering = json!({ "kind": "observedEventObject", "event": "permanentEntered" });
    let compare = |property: &str| {
        json!({
            "kind": "compare",
            "operator": ">",
            "left": { "kind": property, "object": entering },
            "right": { "kind": property, "object": { "kind": "self" } },
        })
    };
    assert_eq!(
        rules[0]["condition"],
        json!({ "kind": "or", "operands": [compare("powerOf"), compare("toughnessOf")] })
    );
    // Sibling: a printed guard composes another property, another strict
    // operator and the source's other characteristic, with no new code.
    let parsed = parse(
        "Sibling",
        "Creature \u{2014} Elf",
        "Whenever another creature you control enters, if that creature's toughness is less than this creature's power, you gain 2 life.",
    );
    assert_eq!(
        single_rule(&parsed)["condition"],
        json!({
            "kind": "compare",
            "operator": "<",
            "left": { "kind": "toughnessOf", "object": entering },
            "right": { "kind": "powerOf", "object": { "kind": "self" } },
        })
    );
    // Near miss: an unknown comparison word stays an explicit failure.
    let parsed = parse(
        "Near Miss",
        "Creature \u{2014} Elf",
        "Whenever a creature you control enters, if that creature's power is bigger than this creature's power, draw a card.",
    );
    assert_eq!(parsed.status, "unsupported");
    assert!(
        parsed.abilities[0]
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.state.as_deref()
                == Some("condition.sentence.relation.operator"))
    );
}

#[test]
fn mentor_compares_the_candidate_with_a_source_characteristic() {
    // Mentor (CR 702.134a): `with power less than this creature's power` is
    // the selector's numeric comparison with a strict operator and a source
    // operand.
    let rules = expanded(
        "Goblin Banneret",
        "Creature \u{2014} Goblin Soldier",
        "Mentor (Whenever this creature attacks, put a +1/+1 counter on target attacking creature with lesser power.)",
    );
    let filter = &rules[0]["declaration"]["decisions"][0]["candidates"]["where"];
    assert_eq!(
        filter["operands"][1],
        json!({
            "kind": "compare",
            "operator": "<",
            "left": { "kind": "powerOf", "object": { "kind": "candidate" } },
            "right": { "kind": "powerOf", "object": { "kind": "self" } },
        })
    );
    // Sibling: another property, the other strict operator and another
    // source characteristic in an activated ability, with no new code.
    let parsed = parse(
        "Sibling",
        "Creature \u{2014} Elf",
        "{T}: Destroy target creature with toughness greater than this creature's toughness.",
    );
    let rule = single_rule(&parsed);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["operands"][1],
        json!({
            "kind": "compare",
            "operator": ">",
            "left": { "kind": "toughnessOf", "object": { "kind": "candidate" } },
            "right": { "kind": "toughnessOf", "object": { "kind": "self" } },
        })
    );
    // Near miss: `less than` with no operand is not a comparison.
    let parsed = parse(
        "Near Miss",
        "Creature \u{2014} Elf",
        "{T}: Destroy target creature with power less than.",
    );
    assert_eq!(parsed.status, "unsupported");
}

#[test]
fn designations_come_from_one_registry_for_predicates_conditions_and_actions() {
    // Renown (CR 702.112a): `if it isn't renowned` is the object predicate
    // and `it becomes renowned` the designating action of the registry.
    let rules = expanded(
        "Topan Freeblade",
        "Creature \u{2014} Human Soldier",
        "Renown 1 (When this creature deals combat damage to a player, if it isn't renowned, put a +1/+1 counter on it and it becomes renowned.)",
    );
    let renowned = json!({ "kind": "hasDesignation", "designation": "renowned" });
    assert_eq!(
        rules[0]["condition"],
        json!({ "kind": "objectMatchesFilter", "object": { "kind": "self" }, "where": { "kind": "not", "operand": renowned } })
    );
    assert_eq!(
        rules[0]["effects"][1],
        json!({ "kind": "setPermanentFlag", "object": { "kind": "self" }, "flag": "renowned", "value": true })
    );
    // Siblings: the same registry in a target selector and in a static
    // source-state condition (monstrous), with no new code.
    let parsed = parse("Sibling", "Sorcery", "Destroy target renowned creature.");
    assert_eq!(
        single_rule(&parsed)["declaration"]["decisions"][0]["candidates"]["where"]["operands"][0],
        renowned
    );
    let parsed = parse(
        "Sibling",
        "Creature \u{2014} Hydra",
        "As long as this creature is monstrous, this creature has trample.",
    );
    assert_eq!(
        single_rule(&parsed)["condition"],
        json!({ "kind": "sourceFlagSet", "flag": "monstrous" })
    );
    // Near miss: a word outside the registry is not a designation.
    let parsed = parse(
        "Near Miss",
        "Creature \u{2014} Human",
        "When this creature dies, it becomes famous.",
    );
    assert_eq!(parsed.status, "unsupported");
}

#[test]
fn unleash_is_an_optional_entry_replacement_and_a_counter_condition() {
    // Unleash (CR 702.98a): `You may have ... enter with` is the shared
    // entry-counter replacement applied by its controller's choice, and
    // `as long as it has a +1/+1 counter on it` the shared counter qualifier.
    let rules = expanded(
        "Gore-House Chainwalker",
        "Creature \u{2014} Human Warrior",
        "Unleash (You may have this creature enter with a +1/+1 counter on it. It can't block as long as it has a +1/+1 counter on it.)",
    );
    assert_eq!(
        rules[0]["decisions"],
        json!([{ "id": "optionalEntryCounters", "kind": "chooseWhether" }])
    );
    assert_eq!(
        rules[0]["condition"],
        json!({ "kind": "decisionWas", "decisionId": "optionalEntryCounters", "value": true })
    );
    assert_eq!(
        rules[1]["condition"],
        json!({ "kind": "objectMatchesFilter", "object": { "kind": "self" }, "where": { "kind": "hasCounter", "counter": "+1/+1" } })
    );
    // Sibling: another counter kind and count, and a counter-possession
    // condition granting a keyword, with no new code.
    let parsed = parse(
        "Sibling",
        "Creature \u{2014} Elf",
        "You may have this creature enter with two shield counters on it.",
    );
    assert_eq!(
        single_rule(&parsed)["replacement"][0]["count"],
        json!({ "kind": "integer", "value": 2 })
    );
    let parsed = parse(
        "Sibling",
        "Creature \u{2014} Elf",
        "This creature has flying as long as it has two or more +1/+1 counters on it.",
    );
    assert_eq!(
        single_rule(&parsed)["condition"]["where"],
        json!({ "kind": "hasCounter", "counter": "+1/+1", "minimum": 2 })
    );
    // Near miss: the finite verb after `you may have` is not the causative.
    let parsed = parse(
        "Near Miss",
        "Creature \u{2014} Elf",
        "You may have this creature enters with a +1/+1 counter on it.",
    );
    assert_eq!(parsed.status, "unsupported");
}
