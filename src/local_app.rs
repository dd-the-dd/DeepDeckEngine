use rand::random;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::card_catalog::named_card_printing;
use crate::engine::CardDefinition;
use crate::model::{PlayableCardInput, compile_playable_card, compile_related_token_definition};
use crate::oracle::{OracleCardFace, OracleCardParseRequest};

const LOCAL_APP_PATH_ENV: &str = "MTG_LOCAL_APP_PATH";

#[derive(Debug)]
pub struct LocalAppResponse {
    pub status: u16,
    pub body: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalSession {
    id: String,
    name: String,
    #[serde(default)]
    creator: String,
    #[serde(default)]
    is_meta_deck: bool,
    #[serde(default)]
    state: Value,
    updated_at: String,
}

fn platform_api_url() -> String {
    std::env::var("DDL_PLATFORM_API_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8790/v1".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn platform_deck_request(
    method: &str,
    suffix: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let url = format!("{}/local/deck-sessions{}", platform_api_url(), suffix);
    let mut request = match method {
        "GET" => ureq::get(&url),
        "POST" => ureq::post(&url),
        "PUT" => ureq::put(&url),
        _ => {
            return Err(format!(
                "unsupported database deck request method: {method}"
            ));
        }
    };
    if let Ok(token) = std::env::var("DDL_DECK_READER_TOKEN")
        && !token.is_empty()
    {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    let response = match body {
        Some(value) => request.send_json(value.clone()),
        None => request.call(),
    }
    .map_err(|error| format!("database deck request failed for {url}: {error}"))?;
    let envelope: Value = response
        .into_json()
        .map_err(|error| format!("database deck response was invalid for {url}: {error}"))?;
    envelope
        .get("data")
        .cloned()
        .ok_or_else(|| format!("database deck response had no data for {url}"))
}

fn database_sessions() -> Result<Vec<LocalSession>, String> {
    serde_json::from_value(platform_deck_request("GET", "", None)?)
        .map_err(|error| format!("database deck list was invalid: {error}"))
}

fn database_session(session_id: &str) -> Result<LocalSession, String> {
    serde_json::from_value(platform_deck_request(
        "GET",
        &format!("/{}", session_id),
        None,
    )?)
    .map_err(|error| format!("database deck was invalid: {error}"))
}

#[derive(Clone, Debug)]
pub struct CompiledLocalDeck {
    pub cards: Vec<CardDefinition>,
    pub name: String,
    pub presentation_catalog: BTreeMap<String, Value>,
    pub session_id: String,
}

fn value_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn selected_card(card: &Value) -> &Value {
    card.get("selectedOption").unwrap_or(card)
}

fn selected_card_has_game_definition(selected: &Value) -> bool {
    !value_string(selected, "typeLine").trim().is_empty()
        || selected
            .get("faces")
            .and_then(Value::as_array)
            .is_some_and(|faces| {
                faces
                    .iter()
                    .any(|face| !value_string(face, "typeLine").trim().is_empty())
            })
}

fn merge_selected_card_with_fallback(card: &Value, fallback: &Value) -> Value {
    let mut completed = fallback.clone();
    if let (Some(target), Some(source)) =
        (completed.as_object_mut(), selected_card(card).as_object())
    {
        target.extend(source.clone());
        for key in [
            "typeLine",
            "manaCost",
            "oracleText",
            "power",
            "toughness",
            "faces",
        ] {
            let selected_value_is_empty = target.get(key).is_none_or(|value| match value {
                Value::Null => true,
                Value::String(value) => value.trim().is_empty(),
                Value::Array(values) => values.is_empty(),
                _ => false,
            });
            if selected_value_is_empty && let Some(fallback_value) = fallback.get(key) {
                target.insert(key.to_string(), fallback_value.clone());
            }
        }
    }
    let mut resolved = card.clone();
    if let Some(object) = resolved.as_object_mut() {
        object.insert("selectedOption".to_string(), completed);
    }
    resolved
}

fn resolve_card_catalog_fields(card: &Value) -> Result<Value, String> {
    if selected_card_has_game_definition(selected_card(card)) {
        return Ok(card.clone());
    }
    let name = value_string(card, "name");
    let Some(printing) = named_card_printing(&name)? else {
        return Ok(card.clone());
    };
    let fallback = serde_json::to_value(printing)
        .map_err(|error| format!("could not serialize catalog fallback for {name}: {error}"))?;
    Ok(merge_selected_card_with_fallback(card, &fallback))
}

fn card_flag(card: &Value, selected: &Value, key: &str) -> bool {
    card.get(key).and_then(Value::as_bool).unwrap_or(false)
        || selected.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn normalized_presentation_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn normalized_token_name(value: &str) -> String {
    let normalized = normalized_presentation_name(value);
    normalized
        .strip_suffix(" token")
        .unwrap_or(&normalized)
        .to_string()
}

fn normalized_token_type_line(value: &str) -> String {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|part| !part.is_empty() && !part.eq_ignore_ascii_case("token"))
        .map(|part| part.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

fn token_presentation_keys(token: &Value) -> Vec<String> {
    let name = normalized_token_name(&value_string(token, "name"));
    if name.is_empty() {
        return Vec::new();
    }
    let type_line = normalized_token_type_line(&value_string(token, "typeLine"));
    let power = value_string(token, "power");
    let toughness = value_string(token, "toughness");
    let stats =
        (!power.is_empty() || !toughness.is_empty()).then(|| format!("{power}/{toughness}"));
    let mut keys = Vec::new();
    if !type_line.is_empty()
        && let Some(stats) = &stats
    {
        keys.push(format!("token-art:{name}:{type_line}:{stats}"));
    }
    if let Some(stats) = &stats {
        keys.push(format!("token-art:{name}:{stats}"));
    }
    if !type_line.is_empty() {
        keys.push(format!("token-art:{name}:{type_line}"));
    }
    keys.push(format!("token-art:{name}"));
    keys
}

fn register_related_token_presentation(
    catalog: &mut BTreeMap<String, Value>,
    token: &Value,
    player_index: usize,
) {
    let mut metadata = token.clone();
    let Some(object) = metadata.as_object_mut() else {
        return;
    };
    let image_url = token
        .get("imageUrl")
        .or_else(|| token.get("imageUri"))
        .or_else(|| token.get("urlFront"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if image_url.is_empty() {
        return;
    }
    object.insert("imageUrl".to_string(), Value::String(image_url.to_string()));
    object.insert("isToken".to_string(), Value::Bool(true));
    object.insert("isGamePiece".to_string(), Value::Bool(true));
    for identity in ["scryfallId", "printingId", "id"] {
        if let Some(identity) = token
            .get(identity)
            .and_then(Value::as_str)
            .filter(|identity| !identity.trim().is_empty())
        {
            catalog
                .entry(identity.to_string())
                .or_insert_with(|| metadata.clone());
        }
    }
    let player_id = format!("player-{}", player_index + 1);
    for key in token_presentation_keys(&metadata) {
        catalog
            .entry(format!("{player_id}:{key}"))
            .or_insert_with(|| metadata.clone());
        catalog.entry(key).or_insert_with(|| metadata.clone());
    }
}

fn mana_colors(text: &str, colors: &mut BTreeSet<char>) {
    let mut in_symbol = false;
    for character in text.chars() {
        match character {
            '{' => in_symbol = true,
            '}' => in_symbol = false,
            'W' | 'U' | 'B' | 'R' | 'G' if in_symbol => {
                colors.insert(character);
            }
            _ => {}
        }
    }
}

fn collect_color_identity(value: &Value) -> Vec<String> {
    let selected = selected_card(value);
    let faces = selected
        .get("faces")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![selected.clone()]);
    let mut colors = BTreeSet::new();
    for face in faces {
        mana_colors(&value_string(&face, "manaCost"), &mut colors);
        mana_colors(&value_string(&face, "oracleText"), &mut colors);
        let type_line = value_string(&face, "typeLine").to_lowercase();
        if type_line.contains("land") {
            for (land_type, color) in [
                ("plains", 'W'),
                ("island", 'U'),
                ("swamp", 'B'),
                ("mountain", 'R'),
                ("forest", 'G'),
            ] {
                if type_line.contains(land_type) {
                    colors.insert(color);
                }
            }
        }
    }
    ['W', 'U', 'B', 'R', 'G']
        .into_iter()
        .filter(|color| colors.contains(color))
        .map(|color| color.to_string())
        .collect()
}

fn oracle_request(card: &Value) -> OracleCardParseRequest {
    let selected = selected_card(card);
    let faces = selected
        .get("faces")
        .cloned()
        .and_then(|faces| serde_json::from_value::<Vec<OracleCardFace>>(faces).ok())
        .unwrap_or_default();
    OracleCardParseRequest {
        card_name: value_string(card, "name"),
        type_line: value_string(selected, "typeLine"),
        mana_cost: selected
            .get("manaCost")
            .and_then(Value::as_str)
            .map(str::to_string),
        oracle_text: selected
            .get("oracleText")
            .and_then(Value::as_str)
            .map(str::to_string),
        layout: selected
            .get("layout")
            .and_then(Value::as_str)
            .map(str::to_string),
        faces,
    }
}

fn presentation_metadata(card: &Value, definition: &CardDefinition) -> Value {
    let selected = selected_card(card);
    let mut metadata = card.clone();
    if let (Some(target), Some(source)) = (metadata.as_object_mut(), selected.as_object()) {
        target.extend(source.clone());
        target.insert(
            "canonicalRules".to_string(),
            Value::Array(definition.rules.clone()),
        );
        let image_url = selected
            .get("imageUrl")
            .or_else(|| selected.get("imageUri"))
            .or_else(|| selected.get("urlFront"))
            .or_else(|| card.get("imageUrl"))
            .or_else(|| card.get("imageUri"))
            .or_else(|| card.get("urlFront"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        target.insert("imageUrl".to_string(), Value::String(image_url.to_string()));
    }
    metadata
}

fn compile_session_deck(
    session: &LocalSession,
    player_index: usize,
) -> Result<CompiledLocalDeck, String> {
    let stored_cards = session
        .state
        .get("cards")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("local deck {} has no cards", session.name))?;
    let mut cards = Vec::new();
    let mut presentation_catalog = BTreeMap::new();
    for (card_index, stored_card) in stored_cards.iter().enumerate() {
        let resolved_card = resolve_card_catalog_fields(stored_card)?;
        let card = &resolved_card;
        let selected = selected_card(card);
        let section = value_string(card, "section").to_lowercase();
        if card_flag(card, selected, "isConsidering")
            || matches!(section.as_str(), "considering" | "considered")
        {
            continue;
        }
        let type_line = value_string(selected, "typeLine");
        let is_game_piece = card_flag(card, selected, "isGamePiece")
            || type_line.starts_with("Token")
            || type_line.starts_with("Emblem")
            || type_line.starts_with("Dungeon");
        let layout = value_string(selected, "layout");
        let faces = selected.get("faces").and_then(Value::as_array);
        let face_id = matches!(layout.as_str(), "flip" | "prepare" | "transform")
            .then(|| {
                faces
                    .and_then(|values| values.first())
                    .and_then(|face| face.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten();
        let definition_id = selected
            .get("id")
            .or_else(|| selected.get("oracleId"))
            .or_else(|| card.get("id"))
            .or_else(|| card.get("oracleId"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("local-deck:{}:definition-{}", session.id, card_index + 1));
        let mut definition = compile_playable_card(PlayableCardInput {
            id: definition_id.clone(),
            face_id,
            is_token: card_flag(card, selected, "isToken"),
            is_game_piece,
            is_sideboard: card_flag(card, selected, "isSideboard"),
            power: selected
                .get("power")
                .and_then(Value::as_str)
                .map(str::to_string),
            toughness: selected
                .get("toughness")
                .and_then(Value::as_str)
                .map(str::to_string),
            oracle: oracle_request(card),
        })?
        .card;
        let related_tokens = selected
            .get("relatedTokens")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let related_token_definitions = related_tokens
            .iter()
            .filter_map(|token| compile_related_token_definition(token).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        if !related_token_definitions.is_empty() {
            definition.rules.push(json!({
                "kind": "rulesMarker",
                "text": "Source-linked token definitions",
                "relatedTokenDefinitions": related_token_definitions,
            }));
        }
        definition.is_commander = card_flag(card, selected, "isCommander")
            || matches!(section.as_str(), "commander" | "commanders");
        definition.rules.push(json!({
            "kind": "rulesMarker",
            "text": "Color identity",
            "colorIdentity": collect_color_identity(card),
        }));
        if definition.is_commander {
            definition.rules.push(json!({
                "kind": "rulesMarker",
                "source": { "kind": "self" },
                "text": "Commander",
            }));
        }
        let metadata = presentation_metadata(card, &definition);
        presentation_catalog.insert(definition_id, metadata.clone());
        for name in [
            value_string(card, "name"),
            value_string(selected, "name"),
            value_string(selected, "flavorName"),
            definition.name.clone(),
        ] {
            let normalized = normalized_presentation_name(&name);
            if !normalized.is_empty() {
                presentation_catalog
                    .entry(format!("card-art:{normalized}"))
                    .or_insert_with(|| metadata.clone());
            }
        }
        if let Some(faces) = faces {
            for face in faces {
                if let Some(face_id) = face.get("id").and_then(Value::as_str) {
                    presentation_catalog.insert(face_id.to_string(), metadata.clone());
                }
                for name in [value_string(face, "name"), value_string(face, "flavorName")] {
                    let normalized = normalized_presentation_name(&name);
                    if !normalized.is_empty() {
                        presentation_catalog
                            .entry(format!("card-art:{normalized}"))
                            .or_insert_with(|| metadata.clone());
                    }
                }
            }
        }
        for token in &related_tokens {
            register_related_token_presentation(&mut presentation_catalog, token, player_index);
        }
        if is_game_piece {
            continue;
        }
        let quantity = card.get("quantity").and_then(Value::as_u64).unwrap_or(1) as usize;
        cards.extend(std::iter::repeat_n(definition, quantity));
    }
    Ok(CompiledLocalDeck {
        cards,
        name: session.name.clone(),
        presentation_catalog,
        session_id: session.id.clone(),
    })
}

fn validation_session_deck(session: &LocalSession) -> Result<CompiledLocalDeck, String> {
    let stored_cards = session
        .state
        .get("cards")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("local deck {} has no cards", session.name))?;
    let mut cards = Vec::new();
    for (card_index, stored_card) in stored_cards.iter().enumerate() {
        let resolved_card = resolve_card_catalog_fields(stored_card)?;
        let card = &resolved_card;
        let selected = selected_card(card);
        let section = value_string(card, "section").to_lowercase();
        if card_flag(card, selected, "isConsidering")
            || matches!(section.as_str(), "considering" | "considered")
        {
            continue;
        }
        let type_line = value_string(selected, "typeLine");
        let is_game_piece = card_flag(card, selected, "isGamePiece")
            || type_line.starts_with("Token")
            || type_line.starts_with("Emblem")
            || type_line.starts_with("Dungeon");
        if is_game_piece {
            continue;
        }
        let selected_name = value_string(selected, "name");
        let definition = CardDefinition {
            id: selected
                .get("id")
                .or_else(|| selected.get("oracleId"))
                .or_else(|| card.get("id"))
                .or_else(|| card.get("oracleId"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!("local-deck:{}:validation-{}", session.id, card_index + 1)
                }),
            name: if selected_name.trim().is_empty() {
                value_string(card, "name")
            } else {
                selected_name
            },
            type_line,
            is_commander: card_flag(card, selected, "isCommander")
                || matches!(section.as_str(), "commander" | "commanders"),
            is_token: card_flag(card, selected, "isToken"),
            is_game_piece,
            is_sideboard: card_flag(card, selected, "isSideboard"),
            mana_cost: value_string(selected, "manaCost"),
            power: None,
            toughness: None,
            rules: vec![json!({
                "kind": "rulesMarker",
                "text": "Color identity",
                "colorIdentity": collect_color_identity(card),
            })],
        };
        let quantity = card.get("quantity").and_then(Value::as_u64).unwrap_or(1) as usize;
        cards.extend(std::iter::repeat_n(definition, quantity));
    }
    Ok(CompiledLocalDeck {
        cards,
        name: session.name.clone(),
        presentation_catalog: BTreeMap::new(),
        session_id: session.id.clone(),
    })
}

pub fn local_deck_for_validation(session_id: &str) -> Result<CompiledLocalDeck, String> {
    let session = database_session(session_id)?;
    validation_session_deck(&session)
}

pub fn compiled_local_deck(
    session_id: &str,
    player_index: usize,
) -> Result<CompiledLocalDeck, String> {
    static CACHE: OnceLock<Mutex<BTreeMap<String, CompiledLocalDeck>>> = OnceLock::new();
    let session = database_session(session_id)?;
    // Definitions belong to the deck, not to a lobby seat. Randomizing player order
    // must not make Rust parse the same deck again under another cache key.
    let cache_key = format!("{}:{}", session.id, session.updated_at);
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(deck) = cache
        .lock()
        .map_err(|_| "compiled local deck cache lock failed")?
        .get(&cache_key)
        .cloned()
    {
        return Ok(deck);
    }
    let deck = compile_session_deck(&session, player_index)?;
    cache
        .lock()
        .map_err(|_| "compiled local deck cache lock failed")?
        .insert(cache_key, deck.clone());
    Ok(deck)
}

fn find_project_root_from(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|candidate| {
        candidate
            .join(".local-app")
            .is_dir()
            .then(|| candidate.to_path_buf())
            .or_else(|| {
                candidate
                    .join("package.json")
                    .is_file()
                    .then(|| candidate.to_path_buf())
            })
    })
}

fn storage_directory() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(LOCAL_APP_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }
    if let Ok(current) = std::env::current_dir()
        && let Some(root) = find_project_root_from(&current)
    {
        return Ok(root.join(".local-app"));
    }
    find_project_root_from(Path::new(env!("CARGO_MANIFEST_DIR")))
        .map(|root| root.join(".local-app"))
        .ok_or_else(|| "could not locate the local app directory".to_string())
}

fn backup_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("store");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("json");
    path.with_file_name(format!("{stem}.backup.{extension}"))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("local store has no directory: {}", path.display()))?;
    fs::create_dir_all(directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    let pending = directory.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("store"),
        random::<u64>(),
    ));
    let contents = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not serialize local store: {error}"))?;
    fs::write(&pending, contents)
        .map_err(|error| format!("could not write {}: {error}", pending.display()))?;
    if path.is_file() {
        fs::copy(path, backup_path(path))
            .map_err(|error| format!("could not back up {}: {error}", path.display()))?;
        fs::remove_file(path)
            .map_err(|error| format!("could not replace {}: {error}", path.display()))?;
    }
    fs::rename(&pending, path)
        .map_err(|error| format!("could not publish {}: {error}", path.display()))
}

pub fn local_deck_creators() -> Result<BTreeMap<String, String>, String> {
    Ok(database_sessions()?
        .iter()
        .map(|session| (session.id.clone(), session.creator.clone()))
        .collect())
}

pub fn local_meta_deck_session_ids() -> Result<Vec<String>, String> {
    Ok(database_sessions()?
        .iter()
        .filter(|session| session.is_meta_deck)
        .map(|session| session.id.clone())
        .collect())
}

pub fn local_deck_session_exists(session_id: &str) -> Result<bool, String> {
    Ok(database_sessions()?
        .iter()
        .any(|session| session.id == session_id))
}

fn parser_feedback_store() -> Result<&'static Mutex<Value>, String> {
    static STORE: OnceLock<Result<Mutex<Value>, String>> = OnceLock::new();
    STORE
        .get_or_init(|| {
            let path = storage_directory()?.join("parser-feedback.json");
            let value = match fs::read(&path) {
                Ok(contents) => serde_json::from_slice(&contents)
                    .map_err(|error| format!("invalid parser feedback: {error}"))?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    json!({ "comments": {} })
                }
                Err(error) => return Err(format!("could not read parser feedback: {error}")),
            };
            Ok(Mutex::new(value))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn parse_body(body: &str) -> Result<Value, LocalAppResponse> {
    serde_json::from_str(body).map_err(|error| LocalAppResponse {
        status: 400,
        body: json!({ "error": format!("Invalid local app request: {error}") }),
    })
}

fn route_sessions(method: &str, path: &str, body: &str) -> Result<LocalAppResponse, String> {
    let suffix = path
        .strip_prefix("/api/local-sessions")
        .unwrap_or_default()
        .trim_start_matches('/');
    if method == "GET" && suffix.is_empty() {
        let sessions = platform_deck_request("GET", "", None)?;
        return Ok(LocalAppResponse {
            status: 200,
            body: json!({ "sessions": sessions }),
        });
    }
    let request = if matches!(method, "POST" | "PUT") {
        Some(parse_body(body).map_err(|response| response.body.to_string())?)
    } else {
        None
    };
    let api_suffix = if suffix.is_empty() {
        String::new()
    } else {
        format!("/{suffix}")
    };
    Ok(LocalAppResponse {
        status: 200,
        body: platform_deck_request(method, &api_suffix, request.as_ref())?,
    })
}

fn route_platform_deck_sessions(
    method: &str,
    path: &str,
    body: &str,
) -> Result<LocalAppResponse, String> {
    let suffix = path
        .strip_prefix("/api/v1/local/deck-sessions")
        .unwrap_or_default()
        .trim_start_matches('/');
    let request = if matches!(method, "POST" | "PUT") {
        Some(parse_body(body).map_err(|response| response.body.to_string())?)
    } else {
        None
    };
    let api_suffix = if suffix.is_empty() {
        String::new()
    } else {
        format!("/{suffix}")
    };
    let data = platform_deck_request(method, &api_suffix, request.as_ref())?;
    Ok(LocalAppResponse {
        status: 200,
        body: json!({ "data": data }),
    })
}

fn route_parser_feedback(method: &str, body: &str) -> Result<LocalAppResponse, String> {
    let store = parser_feedback_store()?;
    let mut store = store
        .lock()
        .map_err(|_| "parser feedback store lock failed")?;
    if method == "GET" {
        return Ok(LocalAppResponse {
            status: 200,
            body: store.clone(),
        });
    }
    if method == "PUT" {
        let request = match parse_body(body) {
            Ok(request) => request,
            Err(response) => return Ok(response),
        };
        *store = json!({ "comments": request.get("comments").cloned().unwrap_or(json!({})) });
        write_json(&storage_directory()?.join("parser-feedback.json"), &*store)?;
        return Ok(LocalAppResponse {
            status: 200,
            body: store.clone(),
        });
    }
    Ok(LocalAppResponse {
        status: 405,
        body: json!({ "error": "Method not allowed" }),
    })
}

pub fn route_local_app(
    method: &str,
    path: &str,
    body: &str,
) -> Option<Result<LocalAppResponse, String>> {
    if path.starts_with("/api/v1/local/deck-sessions") {
        return Some(route_platform_deck_sessions(method, path, body));
    }
    if path.starts_with("/api/local-sessions") {
        return Some(route_sessions(method, path, body));
    }
    if path == "/api/parser-feedback" {
        return Some(route_parser_feedback(method, body));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{LocalSession, compile_session_deck, merge_selected_card_with_fallback};
    use serde_json::json;

    #[test]
    fn image_only_printings_inherit_missing_game_fields_without_losing_selected_art() {
        let card = json!({
            "name": "test creature",
            "selectedOption": {
                "setCode": "custom-art",
                "collectorNumber": "1",
                "urlFront": "custom.jpg",
                "typeLine": "",
                "manaCost": null
            }
        });
        let fallback = json!({
            "setCode": "rules-set",
            "collectorNumber": "42",
            "urlFront": "rules.jpg",
            "typeLine": "Creature - Demon",
            "manaCost": "{4}{B}{B}{B}{B}",
            "oracleText": "Flying, lifelink"
        });

        let completed = merge_selected_card_with_fallback(&card, &fallback);

        assert_eq!(completed["selectedOption"]["setCode"], "custom-art");
        assert_eq!(completed["selectedOption"]["urlFront"], "custom.jpg");
        assert_eq!(completed["selectedOption"]["typeLine"], "Creature - Demon");
        assert_eq!(completed["selectedOption"]["manaCost"], "{4}{B}{B}{B}{B}");
    }

    #[test]
    fn incomplete_saved_printing_compiles_with_the_catalog_game_definition() {
        let session = LocalSession {
            id: "incomplete-printing-session".to_string(),
            name: "Incomplete printing".to_string(),
            creator: "dd-the-dd".to_string(),
            is_meta_deck: false,
            updated_at: "1".to_string(),
            state: json!({
                "cards": [{
                    "name": "griselbrand",
                    "quantity": 1,
                    "selectedOption": {
                        "setCode": "custom-art",
                        "collectorNumber": "1",
                        "flavorName": "Alternate Demon Name",
                        "urlFront": "custom-griselbrand.jpg"
                    }
                }]
            }),
        };

        let deck = compile_session_deck(&session, 0).expect("incomplete printing compiles");
        let definition = &deck.cards[0];

        assert_eq!(definition.mana_cost, "{4}{B}{B}{B}{B}");
        assert_eq!(definition.type_line, "Legendary Creature — Demon");
        assert!(
            definition
                .rules
                .iter()
                .any(|rule| rule["kind"] == "activatedAbility")
        );
        assert_eq!(
            deck.presentation_catalog[&definition.id]["imageUrl"],
            "custom-griselbrand.jpg"
        );
        assert_eq!(
            deck.presentation_catalog["card-art:alternate demon name"]["imageUrl"],
            "custom-griselbrand.jpg"
        );
    }

    #[test]
    fn deck_compilation_keeps_the_selected_printings_related_token_oracle() {
        let session = LocalSession {
            id: "token-source-session".to_string(),
            name: "Token source".to_string(),
            creator: "dd-the-dd".to_string(),
            is_meta_deck: false,
            updated_at: "1".to_string(),
            state: json!({
                "cards": [{
                    "name": "Blood maker",
                    "quantity": 1,
                    "selectedOption": {
                        "id": "blood-maker",
                        "typeLine": "Sorcery",
                        "manaCost": "{1}{B}",
                        "oracleText": "Create a Blood token.",
                        "relatedTokens": [{
                            "scryfallId": "blood-token-printing",
                            "name": "blood",
                            "typeLine": "Token Artifact - Blood",
                            "oracleText": "{1}, {T}, Discard a card, Sacrifice this token: Draw a card.",
                            "manaCost": "",
                            "imageUri": "blood-token.jpg"
                        }]
                    }
                }]
            }),
        };

        let deck = compile_session_deck(&session, 0).expect("session deck compiles");
        let marker = deck.cards[0]
            .rules
            .iter()
            .find(|rule| rule["text"] == "Source-linked token definitions")
            .expect("source keeps its related token definitions");
        let blood = &marker["relatedTokenDefinitions"][0];
        assert_eq!(blood["name"], "blood");
        assert_eq!(blood["id"], "blood-token-printing");
        assert_eq!(blood["rules"][0]["kind"], "activatedAbility");
        assert_eq!(blood["rules"][0]["effects"][0]["kind"], "drawCards");
        assert_eq!(
            deck.presentation_catalog["player-1:token-art:blood:artifact blood"]["imageUrl"],
            "blood-token.jpg"
        );
        assert_eq!(
            deck.presentation_catalog["blood-token-printing"]["imageUrl"],
            "blood-token.jpg"
        );
    }

    #[test]
    fn deck_compilation_excludes_considering_cards_from_the_game() {
        let session = LocalSession {
            id: "considering-session".to_string(),
            name: "Considering exclusion".to_string(),
            creator: "dd-the-dd".to_string(),
            is_meta_deck: false,
            updated_at: "1".to_string(),
            state: json!({
                "cards": [
                    {"name": "griselbrand", "quantity": 1},
                    {"name": "griselbrand", "quantity": 4, "isConsidering": true}
                ]
            }),
        };

        let deck = compile_session_deck(&session, 0).expect("session deck compiles");
        assert_eq!(deck.cards.len(), 1);
    }
}
