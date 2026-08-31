use crate::card_catalog::{named_card_printing, named_token_printing};
use crate::engine::{CardDefinition, rule_is_executable};
use crate::oracle::{
    OracleCardParseRequest, OracleCardParseResult, ParsedOracleAbility, parse_oracle_card,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayableCardInput {
    pub id: String,
    #[serde(default)]
    pub face_id: Option<String>,
    #[serde(default)]
    pub is_token: bool,
    #[serde(default)]
    pub is_game_piece: bool,
    #[serde(default)]
    pub is_sideboard: bool,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    pub oracle: OracleCardParseRequest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayableCardCompilation {
    pub card: CardDefinition,
    pub parser_status: String,
    pub engine_status: String,
    pub canonical_ability_count: usize,
    pub unsupported_ability_count: usize,
    pub executable_rule_count: usize,
    pub unexecutable_rule_count: usize,
    pub parser_result: OracleCardParseResult,
}

fn compile_token_oracle_definition(
    name: &str,
    type_line: String,
    mana_cost: Option<String>,
    oracle_text: Option<String>,
    layout: Option<String>,
    power: Option<String>,
    toughness: Option<String>,
) -> Result<CardDefinition, String> {
    Ok(compile_playable_card(PlayableCardInput {
        id: format!("token:{}", name.trim().to_lowercase()),
        face_id: None,
        is_token: true,
        is_game_piece: true,
        is_sideboard: false,
        power,
        toughness,
        oracle: OracleCardParseRequest {
            card_name: name.to_string(),
            type_line,
            mana_cost,
            oracle_text,
            layout,
            faces: Vec::new(),
        },
    })?
    .card)
}

pub fn compile_named_token_definition(name: &str) -> Result<Option<CardDefinition>, String> {
    let Some(printing) = named_token_printing(name)? else {
        return Ok(None);
    };
    let type_line = printing
        .type_line
        .clone()
        .unwrap_or_else(|| format!("Token - {name}"));
    compile_token_oracle_definition(
        name,
        type_line,
        printing.mana_cost.clone(),
        printing.oracle_text.clone(),
        printing.layout.clone(),
        printing.power.clone(),
        printing.toughness.clone(),
    )
    .map(Some)
}

pub fn compile_named_card_definition(name: &str) -> Result<Option<CardDefinition>, String> {
    let Some(printing) = named_card_printing(name)? else {
        return Ok(None);
    };
    let type_line = printing
        .type_line
        .clone()
        .ok_or_else(|| format!("catalog card {name} has no type line"))?;
    Ok(Some(
        compile_playable_card(PlayableCardInput {
            id: format!(
                "conjured:{}:{}",
                printing.set_code, printing.collector_number
            ),
            face_id: None,
            is_token: false,
            is_game_piece: true,
            is_sideboard: false,
            power: printing.power.clone(),
            toughness: printing.toughness.clone(),
            oracle: OracleCardParseRequest {
                card_name: name.to_string(),
                type_line,
                mana_cost: printing.mana_cost.clone(),
                oracle_text: printing.oracle_text.clone(),
                layout: printing.layout.clone(),
                faces: printing
                    .faces
                    .clone()
                    .and_then(|faces| serde_json::from_value(Value::Array(faces)).ok())
                    .unwrap_or_default(),
            },
        })?
        .card,
    ))
}

pub fn compile_related_token_definition(token: &Value) -> Result<Option<CardDefinition>, String> {
    let Some(name) = token
        .get("displayName")
        .or_else(|| token.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
    else {
        return Ok(None);
    };
    let Some(type_line) = token
        .get("typeLine")
        .and_then(Value::as_str)
        .filter(|type_line| !type_line.trim().is_empty())
    else {
        return Ok(None);
    };
    let mut definition = compile_token_oracle_definition(
        name,
        type_line.to_string(),
        token
            .get("manaCost")
            .and_then(Value::as_str)
            .map(str::to_string),
        token
            .get("oracleText")
            .and_then(Value::as_str)
            .map(str::to_string),
        Some("token".to_string()),
        token
            .get("power")
            .and_then(Value::as_str)
            .map(str::to_string),
        token
            .get("toughness")
            .and_then(Value::as_str)
            .map(str::to_string),
    )?;
    if let Some(printing_identity) = token
        .get("scryfallId")
        .or_else(|| token.get("printingId"))
        .or_else(|| token.get("id"))
        .and_then(Value::as_str)
        .filter(|identity| !identity.trim().is_empty())
    {
        definition.id = printing_identity.to_string();
    }
    Ok(Some(definition))
}

fn linked_prepare_face_id<'a>(
    parser_result: &'a OracleCardParseResult,
    selected_face_id: Option<&str>,
) -> Option<&'a str> {
    if parser_result.context.layout.as_deref() != Some("prepare") {
        return None;
    }
    let mut faces = parser_result.context.faces.iter();
    let permanent_face = faces.next()?;
    let spell_face = faces.next()?;
    (selected_face_id == Some(permanent_face.id.as_str())).then_some(spell_face.id.as_str())
}

fn has_intrinsic_basic_land_mana_ability(type_line: &str) -> bool {
    let words = || {
        type_line
            .split(|character: char| !character.is_ascii_alphabetic())
            .filter(|word| !word.is_empty())
    };
    words().any(|word| word.eq_ignore_ascii_case("land"))
        && words().any(|word| {
            ["plains", "island", "swamp", "mountain", "forest"]
                .iter()
                .any(|subtype| word.eq_ignore_ascii_case(subtype))
        })
}

fn is_intrinsic_basic_land_mana_reminder(type_line: &str, ability: &ParsedOracleAbility) -> bool {
    if !matches!(
        ability.ability_type.as_str(),
        "activatedAbility" | "manaAbility"
    ) || !has_intrinsic_basic_land_mana_ability(type_line)
    {
        return false;
    }
    let text = ability.source.text.trim();
    let normalized = text.to_ascii_lowercase();
    text.starts_with('(')
        && text.ends_with(')')
        && normalized.contains("{t}")
        && normalized.contains(": add ")
}

fn playable_rule_for_ability(ability: &ParsedOracleAbility, type_line: &str) -> Option<Value> {
    (!is_intrinsic_basic_land_mana_reminder(type_line, ability))
        .then(|| ability.rule.clone())
        .flatten()
}

pub fn playable_rules_for_face(
    parser_result: &OracleCardParseResult,
    selected_face_id: Option<&str>,
) -> Vec<Value> {
    if parser_result.context.faces.len() == 2
        && parser_result
            .context
            .faces
            .iter()
            .all(|face| face.type_line.contains("Room"))
    {
        return parser_result
            .context
            .faces
            .iter()
            .enumerate()
            .flat_map(|(door_index, face)| {
                parser_result
                    .abilities
                    .iter()
                    .filter(move |ability| {
                        ability.source.face_id.as_deref() == Some(face.id.as_str())
                    })
                    .filter_map(|ability| playable_rule_for_ability(ability, &face.type_line))
                    .map(move |mut rule| {
                        if rule["kind"].as_str() != Some("rulesMarker") {
                            rule["roomDoorIndex"] = Value::from(door_index as u64);
                        }
                        rule
                    })
            })
            .collect();
    }

    if parser_result.context.layout.as_deref() == Some("split")
        && parser_result.context.faces.len() >= 2
    {
        return vec![json!({
            "kind": "rulesMarker",
            "text": "Split card faces.",
            "splitFaces": parser_result.context.faces.iter().map(|face| json!({
                "id": face.id,
                "name": face.name,
                "typeLine": face.type_line,
                "manaCost": face.mana_cost.as_deref().unwrap_or_default(),
                "oracleText": face.oracle_text,
                "power": face.power,
                "toughness": face.toughness,
                "rules": parser_result.abilities.iter()
                    .filter(|ability| ability.source.face_id.as_deref() == Some(face.id.as_str()))
                    .filter_map(|ability| playable_rule_for_ability(ability, &face.type_line))
                    .collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })];
    }

    if matches!(
        parser_result.context.layout.as_deref(),
        Some("flip" | "transform")
    ) && selected_face_id
        == parser_result
            .context
            .faces
            .first()
            .map(|face| face.id.as_str())
    {
        let mut rules = parser_result
            .context
            .faces
            .iter()
            .enumerate()
            .flat_map(|(face_index, face)| {
                parser_result
                    .abilities
                    .iter()
                    .filter(move |ability| {
                        ability.source.face_id.as_deref() == Some(face.id.as_str())
                    })
                    .filter_map(|ability| playable_rule_for_ability(ability, &face.type_line))
                    .map(move |mut rule| {
                        rule["activeFaceIndex"] = Value::from(face_index as u64);
                        rule
                    })
            })
            .collect::<Vec<_>>();
        rules.push(json!({
            "kind": "rulesMarker",
            "text": "Transformable card faces.",
            "transformFaces": parser_result.context.faces.iter().map(|face| json!({
                "id": face.id,
                "name": face.name,
                "typeLine": face.type_line,
                "manaCost": face.mana_cost.as_deref().unwrap_or_default(),
                "oracleText": face.oracle_text,
                "power": face.power,
                "toughness": face.toughness,
            })).collect::<Vec<_>>(),
        }));
        return rules;
    }

    let selected_type_line = selected_face_id
        .and_then(|face_id| {
            parser_result
                .context
                .faces
                .iter()
                .find(|face| face.id == face_id)
                .map(|face| face.type_line.as_str())
        })
        .unwrap_or(parser_result.context.type_line.as_str());
    let mut rules = parser_result
        .abilities
        .iter()
        .filter(|ability| ability.source.face_id.as_deref() == selected_face_id)
        .filter_map(|ability| playable_rule_for_ability(ability, selected_type_line))
        .collect::<Vec<_>>();

    let Some(spell_face_id) = linked_prepare_face_id(parser_result, selected_face_id) else {
        return rules;
    };
    let Some(spell_face) = parser_result
        .context
        .faces
        .iter()
        .find(|face| face.id == spell_face_id)
    else {
        return rules;
    };
    let spell_rules = parser_result
        .abilities
        .iter()
        .filter(|ability| ability.source.face_id.as_deref() == Some(spell_face_id))
        .filter_map(|ability| playable_rule_for_ability(ability, &spell_face.type_line))
        .collect::<Vec<_>>();
    rules.push(json!({
        "kind": "prepareSpell",
        "spell": {
            "id": spell_face.id,
            "name": spell_face.name,
            "typeLine": spell_face.type_line,
            "manaCost": spell_face.mana_cost.as_deref().unwrap_or_default(),
            "rules": spell_rules,
        }
    }));
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_single_face(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
        parse_oracle_card(OracleCardParseRequest {
            card_name: "Test Land".to_string(),
            type_line: type_line.to_string(),
            mana_cost: None,
            oracle_text: Some(oracle_text.to_string()),
            layout: None,
            faces: Vec::new(),
        })
    }

    #[test]
    fn intrinsic_basic_land_mana_reminder_is_not_a_playable_rule() {
        let parser_result = parse_single_face("Basic Land — Island", "({T}: Add {U}.)");

        assert!(
            parser_result
                .abilities
                .iter()
                .any(|ability| ability.rule.is_some())
        );
        assert!(
            playable_rules_for_face(&parser_result, None).is_empty(),
            "{:#?}",
            parser_result.abilities
        );
    }

    #[test]
    fn typed_dual_land_mana_reminder_is_not_a_playable_rule() {
        let parser_result = parse_single_face("Land — Island Forest", "({T}: Add {U} or {G}.)");

        assert!(
            parser_result
                .abilities
                .iter()
                .any(|ability| ability.rule.is_some())
        );
        assert!(
            playable_rules_for_face(&parser_result, None).is_empty(),
            "{:#?}",
            parser_result.abilities
        );
    }

    #[test]
    fn explicit_mana_ability_on_untyped_land_remains_playable() {
        let parser_result = parse_single_face("Land", "{T}: Add {C}.");

        assert!(!playable_rules_for_face(&parser_result, None).is_empty());
    }
}

pub fn compile_playable_card(input: PlayableCardInput) -> Result<PlayableCardCompilation, String> {
    let selected_face = if input.oracle.faces.is_empty() {
        None
    } else if let Some(face_id) = input.face_id.as_deref() {
        Some(
            input
                .oracle
                .faces
                .iter()
                .find(|face| face.id == face_id)
                .ok_or_else(|| format!("unknown playable face: {face_id}"))?,
        )
    } else {
        input.oracle.faces.first()
    };
    let selected_face_id = selected_face.map(|face| face.id.clone());
    let name = selected_face
        .map(|face| face.name.clone())
        .unwrap_or_else(|| input.oracle.card_name.clone());
    let type_line = selected_face
        .map(|face| face.type_line.clone())
        .unwrap_or_else(|| input.oracle.type_line.clone());
    let mana_cost = selected_face
        .and_then(|face| face.mana_cost.clone())
        .or_else(|| input.oracle.mana_cost.clone())
        .unwrap_or_default();
    let power = input
        .power
        .clone()
        .or_else(|| selected_face.and_then(|face| face.power.clone()));
    let toughness = input
        .toughness
        .clone()
        .or_else(|| selected_face.and_then(|face| face.toughness.clone()));

    let parser_result = parse_oracle_card(input.oracle);
    let linked_face_id =
        linked_prepare_face_id(&parser_result, selected_face_id.as_deref()).map(ToOwned::to_owned);
    let selected_abilities = parser_result
        .abilities
        .iter()
        .filter(|ability| {
            parser_result.context.layout.as_deref() == Some("split")
                || ability.source.face_id.as_deref() == selected_face_id.as_deref()
                || ability.source.face_id.as_deref() == linked_face_id.as_deref()
        })
        .collect::<Vec<_>>();
    let canonical_ability_count = selected_abilities
        .iter()
        .filter(|ability| ability.status == "canonical")
        .count();
    let unsupported_ability_count = selected_abilities.len() - canonical_ability_count;
    let parser_status = if unsupported_ability_count == 0 {
        "canonical"
    } else {
        "unsupported"
    }
    .to_string();
    let candidate_rules = playable_rules_for_face(&parser_result, selected_face_id.as_deref());
    let executable_rule_count = candidate_rules
        .iter()
        .filter(|rule| rule_is_executable(rule))
        .count();
    let unexecutable_rule_count = candidate_rules.len() - executable_rule_count;
    let rules = candidate_rules
        .into_iter()
        .filter(rule_is_executable)
        .collect();
    let engine_status = if unsupported_ability_count == 0 && unexecutable_rule_count == 0 {
        "executable"
    } else {
        "incomplete"
    }
    .to_string();

    Ok(PlayableCardCompilation {
        card: CardDefinition {
            id: input.id,
            name,
            type_line,
            is_commander: false,
            is_token: input.is_token,
            is_game_piece: input.is_game_piece,
            is_sideboard: input.is_sideboard,
            mana_cost,
            power,
            toughness,
            rules,
        },
        parser_status,
        engine_status,
        canonical_ability_count,
        unsupported_ability_count,
        executable_rule_count,
        unexecutable_rule_count,
        parser_result,
    })
}
