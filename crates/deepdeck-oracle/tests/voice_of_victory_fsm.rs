use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Mobilize 2 (Whenever this creature attacks, create two tapped and attacking 1/1 red Warrior creature tokens. Sacrifice them at the beginning of the next end step.)\nYour opponents can't cast spells during your turn.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Native Grammar Fixture".to_string(),
        type_line: "Creature - Human Bard".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn controller() -> serde_json::Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

#[test]
fn exact_card_is_native_and_lowers_mobilize_to_atomic_effects() {
    let parsed = parse(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical" && ability.recognizer == RecognizerProvenance::Native
    }));

    assert_eq!(
        parsed.abilities[0].rule,
        Some(json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": {
                "kind": "declaredAttacker",
                "object": { "kind": "self" },
            },
            "effects": [
                {
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": { "kind": "integer", "value": 2 },
                    "tapped": true,
                    "attacking": true,
                    "token": {
                        "types": ["Creature"],
                        "subtypes": ["Warrior"],
                        "colors": ["Red"],
                        "power": 1,
                        "toughness": 1,
                    },
                    "bind": "mobilizeTokens",
                },
                {
                    "kind": "installDelayedStepTrigger",
                    "controller": controller(),
                    "step": "endStep",
                    "captureBindings": ["mobilizeTokens"],
                    "effects": [{
                        "kind": "sacrificePermanents",
                        "player": controller(),
                        "objects": {
                            "kind": "boundObjects",
                            "binding": "mobilizeTokens",
                        },
                    }],
                },
            ],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule,
        Some(json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "prohibitCast",
                "players": {
                    "kind": "opponentsOf",
                    "player": controller(),
                },
                "duringTurnOf": controller(),
            }],
        }))
    );

    let all_ir = parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .map(ToString::to_string)
        .collect::<String>();
    for forbidden in [
        "keywordAbility",
        "sacrificeAtNextEndStep",
        "resolveTriggeredInstruction",
        "emitAbilitySignal",
    ] {
        assert!(!all_ir.contains(forbidden), "forbidden IR: {forbidden}");
    }
}

#[test]
fn quantities_and_cannot_spelling_reuse_the_same_machines() {
    // The cleanup pronoun agrees with the token batch (`it` for one token,
    // `them` for several), as printed on Mobilize cards.
    for (quantity, word, noun, pronoun) in [(1, "a", "token", "it"), (3, "three", "tokens", "them")]
    {
        let oracle = format!(
            "Mobilize {quantity} (Whenever this creature attacks, create {word} tapped and attacking 1/1 red Warrior creature {noun}. Sacrifice {pronoun} at the beginning of the next end step.)\nYour opponents cannot cast spells during your turn."
        );
        let parsed = parse(&oracle);
        assert_eq!(parsed.status, "canonical", "{oracle}");
        assert!(
            parsed
                .abilities
                .iter()
                .all(|ability| ability.recognizer == RecognizerProvenance::Native)
        );
        assert_eq!(
            parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["quantity"]["value"],
            quantity
        );
    }
}

#[test]
fn claimed_near_misses_report_the_responsible_native_state() {
    for (oracle, automaton, state) in [
        (
            "Mobilize 2 (Whenever this creature attacks, create three tapped and attacking 1/1 red Warrior creature tokens. Sacrifice them at the beginning of the next end step.)",
            "mobilize",
            "keyword.mobilize.reminder.quantity-match",
        ),
        (
            "Mobilize 2 (Whenever this creature attacks, create two tapped and blocking 1/1 red Warrior creature tokens. Sacrifice them at the beginning of the next end step.)",
            "mobilize",
            "effect.combat-token.attacking",
        ),
        // The cast prohibition is a player predicate of the shared static
        // frame (CR 101.2); it owns the turn window and the terminator.
        (
            "Your opponents can't cast spells during each turn.",
            "static-continuous-effect",
            "static.continuous.turn-condition",
        ),
        (
            "Your opponents can't cast spells during your turn twice.",
            "static-continuous-effect",
            "static.continuous.player-terminator",
        ),
    ] {
        let parsed = parse(oracle);
        assert_eq!(parsed.status, "unsupported", "{oracle}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
    }
}

#[test]
fn adjacent_keyword_and_player_restriction_families_are_not_stolen() {
    // Structural frames now claim every ability explicitly: the near-miss is
    // a committed ability-frame failure, never stolen by the adjacent family.
    for (oracle, automaton, state, found) in [
        // (Firebending is a keyword template row; Absorb has no expansion.)
        (
            "Absorb 2",
            "ability-frame",
            "ability-frame.keyword.expansion",
            "Absorb",
        ),
        // The player subject is read by the static frame, whose activation
        // prohibition requires the `of OBJECTS` sources it restricts.
        (
            "Your opponents can't activate abilities during your turn.",
            "static-continuous-effect",
            "static.player.lowering.activation-sources",
            "during",
        ),
    ] {
        let parsed = parse(oracle);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.status, "unsupported", "{oracle}");
        assert!(ability.rule.is_none(), "{oracle}");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native, "{oracle}");
        assert_eq!(
            ability.diagnostics[0].automaton.as_deref(),
            Some(automaton),
            "{oracle}"
        );
        assert_eq!(
            ability.diagnostics[0].state.as_deref(),
            Some(state),
            "{oracle}"
        );
        assert_eq!(
            ability.diagnostics[0].found.as_deref(),
            Some(found),
            "{oracle}"
        );
    }
}
