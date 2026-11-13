use serde_json::Value;

use std::sync::OnceLock;

use crate::card_catalog::{named_token_names, ordinary_card_names};

use super::audit::{build_stages, iterations_for_rule, semantic_kind, unsupported_iteration};
use super::canonical::{
    CanonicalParseOutcome, compose_card_rules, compose_card_rules_native, parse_canonical_rule,
    parse_canonical_rule_native,
};
use super::contextual_binding::validate_native_contextual_bindings;
use super::model::{
    OracleCardParseRequest, OracleCardParseResult, ParsedOracleAbility, ParserDiagnostic,
    ParserTransitionDiagnostic, RecognizerProvenance,
};
use super::syntax::{
    AbilityInput, ExactNameTokenIndex, NameResolutionContext, NamedCardIndex,
    apply_class_level_requirement, class_level_header, class_level_header_from_tokens,
    classify_ability, classify_promoted_ability_from_tokens, documents, is_class_type_line,
    recognize_oracle_entities, resolve_counter_name_spans, resolve_named_token_spans,
    resolve_unknown_names, split_abilities,
};

const PARSER_SCHEMA_VERSION: &str = "oracle-parser/v1";

struct CatalogNamedCardIndex;

struct CatalogNamedTokenIndex;

impl NamedCardIndex for CatalogNamedTokenIndex {
    fn exact_name_ends(
        &self,
        tokens: &[super::model::RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize> {
        static INDEX: OnceLock<ExactNameTokenIndex> = OnceLock::new();
        INDEX
            .get_or_init(|| {
                ExactNameTokenIndex::from_names(named_token_names().unwrap_or_default())
            })
            .exact_name_ends(tokens, start, maximum_end)
    }
}

impl NamedCardIndex for CatalogNamedCardIndex {
    fn exact_name_ends(
        &self,
        tokens: &[super::model::RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize> {
        static INDEX: OnceLock<ExactNameTokenIndex> = OnceLock::new();
        INDEX
            .get_or_init(|| {
                let names = ordinary_card_names().unwrap_or_default();
                ExactNameTokenIndex::from_names(names)
            })
            .exact_name_ends(tokens, start, maximum_end)
    }
}

pub fn parse_oracle_card(request: OracleCardParseRequest) -> OracleCardParseResult {
    parse_oracle_card_with_policy(request, OracleParsePolicy::Full)
}

/// Parses only abilities claimed by the shared token-driven automata.
///
/// Unlike [`parse_oracle_card`], this audit-oriented entry point never invokes
/// the raw-text ability classifier or the legacy canonical parser. An ability
/// that no native family claims remains explicitly unsupported.
pub fn parse_oracle_card_native(request: OracleCardParseRequest) -> OracleCardParseResult {
    parse_oracle_card_with_policy(request, OracleParsePolicy::NativeOnly)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OracleParsePolicy {
    Full,
    NativeOnly,
}

fn type_line_has_room(type_line: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("Room"))
}

/// Adds the face-local Room door identity after either canonical parser path.
///
/// Ability line numbers restart on each face, so they cannot identify the left
/// or right door. The request face order is the card-level source of truth and
/// applies equally to Full and NativeOnly parsing.
fn apply_room_door_index(
    request: &OracleCardParseRequest,
    source_face_id: Option<&str>,
    rule: &mut Value,
) {
    if rule["kind"].as_str() == Some("rulesMarker")
        || request.faces.is_empty()
        || request
            .faces
            .iter()
            .any(|face| !type_line_has_room(&face.type_line))
    {
        return;
    }

    let Some(source_face_id) = source_face_id else {
        return;
    };
    let Some(door_index) = request
        .faces
        .iter()
        .position(|face| face.id == source_face_id)
    else {
        return;
    };

    rule["roomDoorIndex"] = Value::from(door_index as u64);
}

fn native_failure_kind(
    input: &AbilityInput<'_>,
    promoted_kind: Option<&'static str>,
) -> &'static str {
    if let Some(kind) = promoted_kind {
        return kind;
    }
    if input
        .tokens
        .first()
        .is_some_and(|token| token.token_type == "keywordWord")
    {
        return "keywordAbility";
    }
    if input
        .face_type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("instant") || part.eq_ignore_ascii_case("sorcery"))
    {
        return "spellAbility";
    }
    "staticAbility"
}

fn parse_oracle_card_with_policy(
    request: OracleCardParseRequest,
    policy: OracleParsePolicy,
) -> OracleCardParseResult {
    let context = request.clone();
    let name_context = NameResolutionContext::new(
        &request.card_name,
        request.faces.iter().map(|face| face.name.as_str()),
    );
    let named_cards = CatalogNamedCardIndex;
    let named_tokens = CatalogNamedTokenIndex;
    let mut abilities = Vec::new();
    for document in documents(&request) {
        let is_class = is_class_type_line(document.face_type_line);
        let mut minimum_class_level = 1_i64;
        for source in split_abilities(&document) {
            let ability_index = abilities.len();
            let mut entities = recognize_oracle_entities(&source.text);
            let announced_class_level = is_class
                .then(|| match policy {
                    OracleParsePolicy::Full => class_level_header(&source.text),
                    OracleParsePolicy::NativeOnly => class_level_header_from_tokens(&entities),
                })
                .flatten();
            resolve_named_token_spans(&mut entities, &named_tokens);
            resolve_counter_name_spans(&mut entities);
            let mut name_resolution_failure = None;
            let name_links =
                match resolve_unknown_names(&source.text, &entities, &name_context, &named_cards) {
                    Ok(resolution) => resolution.resolved,
                    Err(failure) => {
                        let resolved = failure.resolved.clone();
                        name_resolution_failure = Some(failure);
                        resolved
                    }
                };
            let input = AbilityInput {
                face_name: document.face_name,
                face_type_line: document.face_type_line,
                source: &source,
                tokens: &entities,
                name_links: &name_links,
                name_resolution_failure: name_resolution_failure.as_ref(),
            };
            let promoted_ability_kind = classify_promoted_ability_from_tokens(&input);
            let ability_kind = match policy {
                OracleParsePolicy::Full => {
                    promoted_ability_kind.unwrap_or_else(|| classify_ability(&input))
                }
                OracleParsePolicy::NativeOnly => {
                    promoted_ability_kind.unwrap_or("unclassifiedAbility")
                }
            };
            let parse_outcome = match policy {
                OracleParsePolicy::Full => parse_canonical_rule(&input, ability_kind),
                OracleParsePolicy::NativeOnly => {
                    parse_canonical_rule_native(&input, promoted_ability_kind)
                }
            };
            let (parse_outcome, parsed_diagnostics) = match parse_outcome {
                CanonicalParseOutcome::ParsedLegacyAfterNativeRejection { draft, failure } => {
                    let trace_start = failure.trace.len().saturating_sub(32);
                    let message = format!(
                        "Native candidate remained unpromoted; legacy semantics were preserved. {failure}"
                    );
                    let diagnostic = ParserDiagnostic {
                        code: "native_candidate_unpromoted".to_string(),
                        message,
                        severity: "info".to_string(),
                        automaton: Some(failure.automaton.to_string()),
                        state: Some(failure.state.to_string()),
                        token_index: Some(failure.token_index),
                        span_start: Some(failure.span_start),
                        span_end: Some(failure.span_end),
                        expected: Some(failure.expected),
                        found: failure.actual,
                        trace: failure.trace[trace_start..]
                            .iter()
                            .map(|transition| ParserTransitionDiagnostic {
                                state: transition.state.to_string(),
                                token_index: transition.token_index,
                            })
                            .collect(),
                    };
                    (
                        CanonicalParseOutcome::Parsed {
                            draft,
                            recognizer: RecognizerProvenance::Legacy,
                        },
                        vec![diagnostic],
                    )
                }
                outcome => (outcome, Vec::new()),
            };
            match parse_outcome {
                CanonicalParseOutcome::Parsed {
                    mut draft,
                    recognizer,
                } => {
                    if announced_class_level.is_none() {
                        apply_class_level_requirement(&mut draft.rule, minimum_class_level);
                    }
                    apply_room_door_index(&request, source.face_id.as_deref(), &mut draft.rule);
                    let final_ability_kind = semantic_kind(&draft.rule)
                        .unwrap_or(ability_kind)
                        .to_string();
                    let iteration_kind = match policy {
                        OracleParsePolicy::Full => ability_kind,
                        OracleParsePolicy::NativeOnly => &final_ability_kind,
                    };
                    let iterations =
                        iterations_for_rule(&input, ability_index, iteration_kind, &draft);
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: final_ability_kind,
                        status: "canonical".to_string(),
                        recognizer,
                        rule: Some(draft.rule),
                        entities,
                        iterations,
                        diagnostics: parsed_diagnostics,
                    });
                }
                CanonicalParseOutcome::Unsupported => {
                    let iterations = unsupported_iteration(&input, ability_index, ability_kind);
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: ability_kind.to_string(),
                        status: "unsupported".to_string(),
                        recognizer: RecognizerProvenance::Unclaimed,
                        rule: None,
                        entities,
                        iterations,
                        diagnostics: vec![ParserDiagnostic {
                            code: "unsupported_oracle_ability".to_string(),
                            message: match policy {
                                OracleParsePolicy::Full =>
                                    "No semantics-preserving simplification rule matched this ability."
                                        .to_string(),
                                OracleParsePolicy::NativeOnly =>
                                    "No token-driven native automaton claimed this ability."
                                        .to_string(),
                            },
                            severity: "unsupported".to_string(),
                            automaton: None,
                            state: None,
                            token_index: None,
                            span_start: None,
                            span_end: None,
                            expected: None,
                            found: None,
                            trace: Vec::new(),
                        }],
                    });
                }
                CanonicalParseOutcome::Failed {
                    failure,
                    recognizer,
                } => {
                    let failure_kind = match policy {
                        OracleParsePolicy::Full => ability_kind,
                        OracleParsePolicy::NativeOnly => {
                            native_failure_kind(&input, promoted_ability_kind)
                        }
                    };
                    let iterations = unsupported_iteration(&input, ability_index, failure_kind);
                    let trace_start = failure.trace.len().saturating_sub(32);
                    abilities.push(ParsedOracleAbility {
                        source,
                        ability_type: failure_kind.to_string(),
                        status: "unsupported".to_string(),
                        recognizer,
                        rule: None,
                        entities,
                        iterations,
                        diagnostics: vec![ParserDiagnostic {
                            code: "oracle_machine_failure".to_string(),
                            message: failure.to_string(),
                            severity: "unsupported".to_string(),
                            automaton: Some(failure.automaton.to_string()),
                            state: Some(failure.state.to_string()),
                            token_index: Some(failure.token_index),
                            span_start: Some(failure.span_start),
                            span_end: Some(failure.span_end),
                            expected: Some(failure.expected),
                            found: failure.actual,
                            trace: failure.trace[trace_start..]
                                .iter()
                                .map(|transition| ParserTransitionDiagnostic {
                                    state: transition.state.to_string(),
                                    token_index: transition.token_index,
                                })
                                .collect(),
                        }],
                    });
                }
                CanonicalParseOutcome::ParsedLegacyAfterNativeRejection { .. } => {
                    unreachable!("legacy-after-native-rejection is normalized above")
                }
            }
            if let Some(level) = announced_class_level {
                minimum_class_level = level;
            }
        }
    }
    match policy {
        OracleParsePolicy::Full => compose_card_rules(&mut abilities),
        OracleParsePolicy::NativeOnly => compose_card_rules_native(&mut abilities),
    }
    // Binding validity is a property of native IR in either mode. Legacy
    // rules remain outside this validation boundary.
    validate_native_contextual_bindings(&request, &mut abilities);
    let status = if abilities
        .iter()
        .all(|ability| ability.status == "canonical")
    {
        "canonical"
    } else {
        "unsupported"
    }
    .to_string();
    let diagnostics = abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.clone())
        .collect();
    let stages = build_stages(&abilities);

    OracleCardParseResult {
        schema_version: PARSER_SCHEMA_VERSION.to_string(),
        status,
        context,
        abilities,
        stages,
        diagnostics,
    }
}

#[cfg(all(
    test,
    not(feature = "parser-standalone"),
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
mod tests {
    use super::*;

    fn parsed_rule_for_text<'a>(result: &'a OracleCardParseResult, text: &str) -> &'a Value {
        result
            .abilities
            .iter()
            .find(|ability| ability.source.text == text)
            .and_then(|ability| ability.rule.as_ref())
            .expect("expected Oracle line to have a canonical rule")
    }

    #[test]
    fn unmask_compiles_to_alternative_cost_and_shared_hand_operations() {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: "Unmask".to_string(),
            type_line: "Sorcery".to_string(),
            mana_cost: Some("{3}{B}".to_string()),
            oracle_text: Some(
                "You may exile a black card from your hand rather than pay this spell's mana cost.\n\
                 Target player reveals their hand. You choose a nonland card from it. That player discards that card."
                    .to_string(),
            ),
            layout: None,
            faces: Vec::new(),
        });

        assert_eq!(result.status, "canonical");
        assert_eq!(result.abilities.len(), 2);
        assert_eq!(
            result.abilities[0].rule.as_ref().unwrap()["ability"]["kind"],
            "alternativeCost"
        );
        let spell = result.abilities[1].rule.as_ref().unwrap();
        assert_eq!(
            spell["effects"]
                .as_array()
                .unwrap()
                .iter()
                .map(|effect| effect["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["revealHand", "chooseCards", "discardCards"]
        );
        assert!(
            result
                .abilities
                .iter()
                .filter_map(|ability| ability.rule.as_ref())
                .all(crate::engine::rule_is_executable)
        );
    }

    #[test]
    fn putrid_imp_threshold_compiles_with_its_oracle_ability_label() {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: "Putrid Imp".to_string(),
            type_line: "Creature — Zombie Imp".to_string(),
            mana_cost: Some("{B}".to_string()),
            oracle_text: Some(
                "Discard a card: This creature gains flying until end of turn.\n\
                 Threshold — As long as there are seven or more cards in your graveyard, this creature gets +1/+1 and can't block."
                    .to_string(),
            ),
            layout: None,
            faces: Vec::new(),
        });

        assert_eq!(result.status, "canonical");
        assert_eq!(
            result
                .abilities
                .iter()
                .filter(|ability| ability.rule.is_some())
                .count(),
            2
        );
        let threshold = result.abilities[1]
            .rule
            .as_ref()
            .expect("Putrid Imp threshold rule");
        assert_eq!(threshold["modifiers"][0]["kind"], "modifyPowerToughness");
        assert_eq!(threshold["modifiers"][0]["power"]["value"], 1);
        assert_eq!(threshold["modifiers"][1]["keyword"], "cantBlock");
        assert!(crate::engine::rule_is_executable(threshold));
    }

    #[test]
    fn acererak_compiles_its_incomplete_tomb_return_and_venture_trigger() {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: "Acererak the Archlich".to_string(),
            type_line: "Legendary Creature — Zombie Wizard".to_string(),
            mana_cost: Some("{2}{B}".to_string()),
            oracle_text: Some(
                "When Acererak enters, if you haven't completed Tomb of Annihilation, return Acererak to its owner's hand and venture into the dungeon.\n\
                 Whenever Acererak attacks, for each opponent, you create a 2/2 black Zombie creature token unless that player sacrifices a creature of their choice."
                    .to_string(),
            ),
            layout: None,
            faces: Vec::new(),
        });

        assert_eq!(result.status, "canonical");
        let rules = result
            .abilities
            .iter()
            .filter_map(|ability| ability.rule.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["event"]["kind"], "enterBattlefield");
        assert_eq!(rules[0]["effects"][0]["kind"], "conditionalEffect");
        assert_eq!(
            rules[0]["effects"][0]["condition"]["operand"]["name"],
            "Tomb of Annihilation"
        );
        assert_eq!(
            rules[0]["effects"][0]["then"][0]["kind"],
            "returnToOwnersHand"
        );
        assert_eq!(rules[0]["effects"][0]["then"][1]["kind"], "ventureDungeon");
        assert!(
            rules
                .iter()
                .all(|rule| crate::engine::rule_is_executable(rule))
        );
    }

    #[test]
    fn chrome_mox_entry_wordings_are_triggered_abilities() {
        for oracle_text in [
            "Imprint — When Chrome Mox enters, you may exile a nonartifact, nonland card from your hand.",
            "Imprint — When Chrome Mox enters the battlefield, you may exile a nonartifact, nonland card from your hand.",
            "Imprint — When Chrome Mox comes into play, you may remove a nonartifact, nonland card in your hand from the game.",
        ] {
            let result = parse_oracle_card(OracleCardParseRequest {
                card_name: "Chrome Mox".to_string(),
                type_line: "Artifact".to_string(),
                mana_cost: Some("{0}".to_string()),
                oracle_text: Some(oracle_text.to_string()),
                layout: None,
                faces: Vec::new(),
            });

            assert_eq!(result.status, "canonical", "{oracle_text}");
            assert_eq!(result.abilities.len(), 1, "{oracle_text}");
            let ability = &result.abilities[0];
            assert_eq!(ability.ability_type, "triggeredAbility", "{oracle_text}");
            let rule = ability.rule.as_ref().expect("Chrome Mox rule");
            assert_eq!(rule["kind"], "triggeredAbility", "{oracle_text}");
            assert_eq!(rule["event"]["kind"], "enterBattlefield", "{oracle_text}");
            assert_eq!(
                rule["effects"][0]["kind"], "exileTargetCardWithSource",
                "{oracle_text}"
            );
            assert!(crate::engine::rule_is_executable(rule), "{oracle_text}");
        }
    }

    #[test]
    fn class_sections_apply_the_announced_minimum_level_to_following_abilities() {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: "Scavenger's Talent".to_string(),
            type_line: "Enchantment - Class".to_string(),
            mana_cost: Some("{B}".to_string()),
            oracle_text: Some(
                "(Gain the next level as a sorcery to add its ability.)\n\
                 Whenever one or more creatures you control die, create a Food token. This ability triggers only once each turn.\n\
                 {1}{B}: Level 2\n\
                 Whenever you sacrifice a permanent, target player mills two cards.\n\
                 {2}{B}: Level 3\n\
                 At the beginning of your end step, you may sacrifice three other nonland permanents. If you do, return a creature card from your graveyard to the battlefield with a finality counter on it."
                    .to_string(),
            ),
            layout: Some("class".to_string()),
            faces: Vec::new(),
        });

        let level_one = parsed_rule_for_text(
            &result,
            "Whenever one or more creatures you control die, create a Food token. This ability triggers only once each turn.",
        );
        assert!(level_one.get("minimumClassLevel").is_none());

        let level_two_activation = parsed_rule_for_text(&result, "{1}{B}: Level 2");
        assert!(level_two_activation.get("minimumClassLevel").is_none());
        assert_eq!(
            level_two_activation["activationCondition"]["value"]["value"],
            1
        );

        let level_two = parsed_rule_for_text(
            &result,
            "Whenever you sacrifice a permanent, target player mills two cards.",
        );
        assert_eq!(level_two["minimumClassLevel"], 2);

        let level_three = parsed_rule_for_text(
            &result,
            "At the beginning of your end step, you may sacrifice three other nonland permanents. If you do, return a creature card from your graveyard to the battlefield with a finality counter on it.",
        );
        assert_eq!(level_three["minimumClassLevel"], 3);
    }

    #[test]
    fn class_level_annotation_is_not_inferred_for_non_class_cards() {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: "Not a Class".to_string(),
            type_line: "Enchantment".to_string(),
            mana_cost: Some("{B}".to_string()),
            oracle_text: Some(
                "{1}{B}: Level 2\nWhenever you sacrifice a permanent, target player mills two cards."
                    .to_string(),
            ),
            layout: None,
            faces: Vec::new(),
        });

        let trigger = parsed_rule_for_text(
            &result,
            "Whenever you sacrifice a permanent, target player mills two cards.",
        );
        assert!(trigger.get("minimumClassLevel").is_none());
    }

    #[test]
    fn no_lands_first_unsupported_batch_parses_to_executable_rules() {
        let cards = [
            (
                "Aftermath Analyst",
                "Creature - Elf Detective",
                Some("{1}{G}"),
                "When this creature enters, mill three cards. (Put the top three cards of your library into your graveyard.)\n{3}{G}, Sacrifice this creature: Return all land cards from your graveyard to the battlefield tapped.",
            ),
            (
                "Ashaya, Soul of the Wild",
                "Legendary Creature - Elemental",
                Some("{3}{G}{G}"),
                "Ashaya's power and toughness are each equal to the number of lands you control.\nNontoken creatures you control are Forest lands in addition to their other types. (They're still affected by summoning sickness.)",
            ),
            (
                "Braids, Arisen Nightmare",
                "Legendary Creature - Nightmare",
                Some("{1}{B}{B}"),
                "At the beginning of your end step, you may sacrifice an artifact, creature, enchantment, land, or planeswalker. If you do, each opponent may sacrifice a permanent of their choice that shares a card type with it. For each opponent who doesn't, that player loses 2 life and you draw a card.",
            ),
            (
                "Constant Mists",
                "Instant",
                Some("{1}{G}"),
                "Buyback\u{2014}Sacrifice a land. (You may sacrifice a land in addition to any other costs as you cast this spell. If you do, put this card into your hand as it resolves.)\nPrevent all combat damage that would be dealt this turn.",
            ),
            (
                "Deflecting Swat",
                "Instant",
                Some("{2}{R}"),
                "If you control a commander, you may cast this spell without paying its mana cost.\nYou may choose new targets for target spell or ability.",
            ),
            (
                "Dosan the Falling Leaf",
                "Legendary Creature - Human Monk",
                Some("{1}{G}{G}"),
                "Players can cast spells only during their own turns.",
            ),
            (
                "Exploration Broodship",
                "Artifact - Spacecraft",
                Some("{4}{G}"),
                "Station (Tap another creature you control: Put charge counters equal to its power on this Spacecraft. Station only as a sorcery. It's an artifact creature at 8+.)\n3+ | You may play an additional land on each of your turns.\n8+ | Flying\nOnce during each of your turns, you may cast a permanent spell from your graveyard by sacrificing a land in addition to paying its other costs.",
            ),
            (
                "Field of the Dead",
                "Land",
                None,
                "This land enters tapped.\n{T}: Add {C}.\nWhenever this land or another land you control enters, if you control seven or more lands with different names, create a 2/2 black Zombie creature token.",
            ),
            (
                "Glacial Chasm",
                "Land",
                None,
                "Cumulative upkeep\u{2014}Pay 2 life. (At the beginning of your upkeep, put an age counter on this permanent, then sacrifice it unless you pay its upkeep cost for each age counter on it.)\nWhen this land enters, sacrifice a land.\nCreatures you control can't attack.\nPrevent all damage that would be dealt to you.",
            ),
            (
                "Green Sun's Zenith",
                "Sorcery",
                Some("{X}{G}"),
                "Search your library for a green creature card with mana value X or less, put it onto the battlefield, then shuffle. Shuffle Green Sun's Zenith into its owner's library.",
            ),
        ];

        for (card_name, type_line, mana_cost, oracle_text) in cards {
            let result = parse_oracle_card(OracleCardParseRequest {
                card_name: card_name.to_string(),
                type_line: type_line.to_string(),
                mana_cost: mana_cost.map(str::to_string),
                oracle_text: Some(oracle_text.to_string()),
                layout: None,
                faces: Vec::new(),
            });
            assert_eq!(result.status, "canonical", "{card_name} should parse");
            assert!(
                result.abilities.iter().all(|ability| {
                    ability.status == "canonical"
                        && ability
                            .rule
                            .as_ref()
                            .is_some_and(crate::engine::rule_is_executable)
                }),
                "every {card_name} ability should compile for the engine"
            );
        }
    }

    #[test]
    fn no_lands_linked_mana_and_draw_choice_compile_to_executable_rules() {
        let cards = [
            (
                "Squandered Resources",
                "Enchantment",
                Some("{B}{G}"),
                "Sacrifice a land: Add one mana of any type the sacrificed land could produce.",
            ),
            (
                "Sylvan Library",
                "Enchantment",
                Some("{1}{G}"),
                "At the beginning of your draw step, you may draw two additional cards. If you do, choose two cards in your hand drawn this turn. For each of those cards, pay 4 life or put the card on top of your library.",
            ),
        ];

        for (card_name, type_line, mana_cost, oracle_text) in cards {
            let result = parse_oracle_card(OracleCardParseRequest {
                card_name: card_name.to_string(),
                type_line: type_line.to_string(),
                mana_cost: mana_cost.map(str::to_string),
                oracle_text: Some(oracle_text.to_string()),
                layout: None,
                faces: Vec::new(),
            });
            assert_eq!(result.status, "canonical", "{card_name} should parse");
            assert!(
                result.abilities.iter().all(|ability| {
                    ability.status == "canonical"
                        && ability
                            .rule
                            .as_ref()
                            .is_some_and(crate::engine::rule_is_executable)
                }),
                "every {card_name} ability should compile for the engine"
            );
        }

        let unsupported = parse_oracle_card(OracleCardParseRequest {
            card_name: "Broken Linked Mana".to_string(),
            type_line: "Enchantment".to_string(),
            mana_cost: None,
            oracle_text: Some(
                "{T}: Add one mana of any type the sacrificed land could produce.".to_string(),
            ),
            layout: None,
            faces: Vec::new(),
        });
        assert_eq!(unsupported.status, "unsupported");
    }
}
