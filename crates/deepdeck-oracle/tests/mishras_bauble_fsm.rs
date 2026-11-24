use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const MISHRA_BAUBLE_ORACLE: &str = "{T}, Sacrifice this artifact: Look at the top card of target player's library. Draw a card at the beginning of the next turn's upkeep.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Mishra's Bauble".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn targeted_top_card_inspection_and_delayed_draw_are_native_primitives() {
    let parsed = parse(MISHRA_BAUBLE_ORACLE);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "tap", "object": { "kind": "self" } },
                { "kind": "sacrificePermanent", "permanent": { "kind": "self" } },
            ],
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetPlayer",
                    "kind": "chooseTargets",
                    "candidates": { "kind": "players" },
                    "minimum": 1,
                    "maximum": 1,
                }],
            },
            "effects": [
                {
                    "kind": "lookAtTopCards",
                    "zone": {
                        "kind": "library",
                        "player": { "kind": "chosenTarget", "id": "targetPlayer" },
                    },
                    "count": { "kind": "integer", "value": 1 },
                    "bind": "lookedTopCards",
                },
                {
                    "kind": "installDelayedStepTrigger",
                    "step": "upkeep",
                    "controller": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "effects": [{
                        "kind": "drawCards",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                        "count": { "kind": "integer", "value": 1 },
                    }],
                },
            ],
        }))
    );
}

#[test]
fn controller_library_plural_count_and_end_step_reuse_the_same_children() {
    let parsed = parse(
        "{2}, {T}: Look at the top two cards of your library. Draw two cards at the beginning of the next turn's end step.",
    );
    let ability = &parsed.abilities[0];
    let rule = ability.rule.as_ref().expect("sibling primitive rule");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(rule.get("declaration").is_none());
    assert_eq!(rule["costs"][0]["manaCost"], "{2}");
    assert_eq!(rule["costs"][1]["kind"], "tap");
    assert_eq!(rule["effects"][0]["count"]["value"], 2);
    assert_eq!(rule["effects"][1]["step"], "endStep");
    assert_eq!(rule["effects"][1]["effects"][0]["count"]["value"], 2);
}

#[test]
fn target_opponent_possessive_reuses_the_target_player_child() {
    let parsed = parse(
        "{T}, Sacrifice this artifact: Look at the top card of target opponent's library. Draw a card at the beginning of the next turn's upkeep.",
    );
    let ability = &parsed.abilities[0];
    let rule = ability
        .rule
        .as_ref()
        .expect("opponent library primitive rule");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "isOpponentOf"
    );
    assert_eq!(rule["effects"][0]["zone"]["player"]["id"], "targetOpponent");
}

#[test]
fn nonpossessive_target_owner_is_a_committed_native_failure() {
    let parsed = parse(
        "{T}, Sacrifice this artifact: Look at the top card of target player library. Draw a card at the beginning of the next turn's upkeep.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("activated-top-card-delayed-draw")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("target-list.possessive-player.role")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("player"));
}

#[test]
fn unsupported_delayed_step_is_reported_by_the_temporal_child() {
    let parsed = parse(
        "{T}, Sacrifice this artifact: Look at the top card of target player's library. Draw a card at the beginning of the next turn's draw step.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("effect.delayed-step.step")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("draw"));
}

#[test]
fn a_trailing_sentence_composes_after_the_delayed_draw() {
    // The shared activated program reads the `next turn's` delayed step, so
    // a following ordinary sentence is one more immediate action rather than
    // an unconsumed suffix of the former family envelope.
    let parsed = parse(&format!("{MISHRA_BAUBLE_ORACLE} Draw a card."));
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "canonical", "{:#?}", ability.diagnostics);
    let effects = ability.rule.as_ref().unwrap()["effects"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(effects[1]["kind"], "installDelayedStepTrigger");
    assert_eq!(effects.last().unwrap()["kind"], "drawCards");
    // An unread trailing sentence still fails full consumption explicitly.
    let parsed = parse(&format!("{MISHRA_BAUBLE_ORACLE} Frobnicate a card."));
    assert_eq!(parsed.abilities[0].status, "unsupported");
    assert!(!parsed.abilities[0].diagnostics.is_empty());
}

#[test]
fn mishras_bauble_promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(MISHRA_BAUBLE_ORACLE);
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
