use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const CATALOG_SCHEMA_VERSION: &str = "card-catalog/v1";
const CATALOG_PATH_ENV: &str = "MTG_CARD_CATALOG_PATH";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPrinting {
    pub set_code: String,
    pub collector_number: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_digital: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_promo: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_token: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_game_piece: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_value: Option<f64>,
    /// Color letters of a token game piece whose colors are printed on the
    /// token itself (it has no mana cost), in Scryfall notation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_tokens: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_game_pieces: Option<Vec<Value>>,
    pub url_front: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_back: Option<String>,
}

impl CatalogPrinting {
    /// Full color names of a token printed with colors, in catalog order.
    pub fn color_names(&self) -> Vec<&'static str> {
        self.colors
            .iter()
            .flatten()
            .filter_map(|letter| match letter.as_str() {
                "W" => Some("white"),
                "U" => Some("blue"),
                "B" => Some("black"),
                "R" => Some("red"),
                "G" => Some("green"),
                _ => None,
            })
            .collect()
    }

    /// The color-indicator marker that gives a catalog token its printed
    /// colors; the engine reads the same marker for every colored object.
    pub fn color_indicator_rule(&self) -> Option<Value> {
        let symbols = self.colors.iter().flatten().cloned().collect::<Vec<_>>();
        (!symbols.is_empty()).then(|| {
            serde_json::json!({
                "kind": "rulesMarker",
                "text": format!("Color indicator: {}.", symbols.join(", ")),
                "colorIndicator": symbols,
            })
        })
    }
}

#[derive(Debug, Deserialize)]
struct StoredCardCatalog {
    cards: HashMap<String, Vec<CatalogPrinting>>,
    sets: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardLookupRequest {
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub include_game_pieces: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardLookupResponse {
    pub schema_version: &'static str,
    pub cards: BTreeMap<String, Vec<CatalogPrinting>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub game_pieces: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSetCatalogResponse {
    pub schema_version: &'static str,
    pub sets: BTreeMap<String, String>,
}

fn find_project_root_from(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|candidate| {
        candidate
            .join("data/cards-minimized.json")
            .is_file()
            .then(|| candidate.to_path_buf())
    })
}

fn catalog_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(CATALOG_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }
    if let Ok(current) = std::env::current_dir()
        && let Some(root) = find_project_root_from(&current)
    {
        return Ok(root.join("data/cards-minimized.json"));
    }
    find_project_root_from(Path::new(env!("CARGO_MANIFEST_DIR")))
        .map(|root| root.join("data/cards-minimized.json"))
        .or_else(|| {
            #[cfg(test)]
            {
                let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("fixtures/card-catalog/named-tokens.json");
                return fixture.is_file().then_some(fixture);
            }
            #[cfg(not(test))]
            None
        })
        .ok_or_else(|| "could not locate data/cards-minimized.json".to_string())
}

fn read_catalog(path: &Path) -> Result<StoredCardCatalog, String> {
    let contents = fs::read(path)
        .map_err(|error| format!("could not read card catalog {}: {error}", path.display()))?;
    serde_json::from_slice(&contents)
        .map_err(|error| format!("invalid card catalog {}: {error}", path.display()))
}

fn catalog() -> Result<&'static StoredCardCatalog, String> {
    static CATALOG: OnceLock<Result<StoredCardCatalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| catalog_path().and_then(|path| read_catalog(&path)))
        .as_ref()
        .map_err(Clone::clone)
}

fn builtin_game_pieces() -> &'static StoredCardCatalog {
    static CATALOG: OnceLock<StoredCardCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../data/builtin-game-pieces.json"))
            .expect("built-in game-piece catalog must be valid")
    })
}

/// Tokens whose definition is part of a keyword action (CR 701), such as the
/// Jace planeswalker token of `empower Jace`. They are consulted after the
/// external and built-in catalogs and never feed the parser-free runtime
/// artifact, which only compiles the built-in game pieces.
fn keyword_action_tokens() -> &'static StoredCardCatalog {
    static CATALOG: OnceLock<StoredCardCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../data/keyword-action-tokens.json"))
            .expect("keyword-action token catalog must be valid")
    })
}

fn select_named_token_printing<'a>(
    catalog: &'a StoredCardCatalog,
    name: &str,
) -> Option<&'a CatalogPrinting> {
    let normalized = name.trim().to_lowercase();
    catalog.cards.get(&normalized)?.iter().find(|printing| {
        printing.is_token.unwrap_or(false)
            && printing.is_game_piece.unwrap_or(false)
            && printing
                .oracle_text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
    })
}

/// Returns the catalog definition of a named token game piece.
///
/// Token rules deliberately come from the same Oracle catalog as ordinary
/// cards. The engine must not maintain a second, handwritten list of Clue,
/// Treasure, Blood, Food, and other predefined token abilities.
pub fn named_token_printing(name: &str) -> Result<Option<&'static CatalogPrinting>, String> {
    match catalog() {
        Ok(catalog) => Ok(select_named_token_printing(catalog, name)
            .or_else(|| select_named_token_printing(builtin_game_pieces(), name))
            .or_else(|| select_named_token_printing(keyword_action_tokens(), name))),
        Err(error) if catalog_path().is_ok() => Err(error),
        Err(_) => Ok(select_named_token_printing(builtin_game_pieces(), name)
            .or_else(|| select_named_token_printing(keyword_action_tokens(), name))),
    }
}

/// Returns a deterministic ordinary-card printing for effects such as conjure.
pub fn named_card_printing(name: &str) -> Result<Option<&'static CatalogPrinting>, String> {
    let normalized = name.trim().to_lowercase();
    Ok(catalog()?.cards.get(&normalized).and_then(|printings| {
        printings.iter().find(|printing| {
            !printing.is_token.unwrap_or(false)
                && !printing.is_game_piece.unwrap_or(false)
                && printing.type_line.is_some()
        })
    }))
}

/// Returns the exact normalized names backed by at least one ordinary-card
/// printing. Consumers may build read-only indexes from this catalog-owned
/// name set; the catalog remains the single source of truth.
// This source file is also compiled into the standalone runtime crate, where
// Oracle name resolution is intentionally absent.
#[allow(dead_code)]
pub(crate) fn ordinary_card_names() -> Result<Vec<&'static str>, String> {
    Ok(catalog()?
        .cards
        .iter()
        .filter(|(_, printings)| {
            printings.iter().any(|printing| {
                !printing.is_token.unwrap_or(false)
                    && !printing.is_game_piece.unwrap_or(false)
                    && printing.type_line.is_some()
            })
        })
        .map(|(name, _)| name.as_str())
        .collect())
}

/// Returns every exact normalized name backed by an Oracle-defined token game
/// piece. The external catalog and the built-in fallback are one logical
/// catalog; callers may build indexes but must not add token-name lists.
#[allow(dead_code)]
pub(crate) fn named_token_names() -> Result<Vec<&'static str>, String> {
    fn collect(catalog: &'static StoredCardCatalog, names: &mut Vec<&'static str>) {
        names.extend(catalog.cards.iter().filter_map(|(name, printings)| {
            printings
                .iter()
                .any(|printing| {
                    printing.is_token.unwrap_or(false)
                        && printing.is_game_piece.unwrap_or(false)
                        && printing
                            .oracle_text
                            .as_deref()
                            .is_some_and(|text| !text.trim().is_empty())
                })
                .then_some(name.as_str())
        }));
    }

    let mut names = Vec::new();
    if let Ok(external) = catalog() {
        collect(external, &mut names);
    }
    collect(builtin_game_pieces(), &mut names);
    names.sort_unstable();
    names.dedup();
    Ok(names)
}

#[allow(dead_code)]
pub(crate) fn builtin_named_token_names() -> Vec<&'static str> {
    let mut names = builtin_game_pieces()
        .cards
        .iter()
        .filter_map(|(name, _)| {
            select_named_token_printing(builtin_game_pieces(), name).map(|_| name.as_str())
        })
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

#[allow(dead_code)]
pub(crate) fn builtin_named_token_printing(name: &str) -> Option<&'static CatalogPrinting> {
    select_named_token_printing(builtin_game_pieces(), name)
}

fn game_piece_value(name: &str, printing: &CatalogPrinting) -> Value {
    let mut value = serde_json::to_value(printing).expect("catalog printing serializes");
    if let Some(object) = value.as_object_mut() {
        object.insert("name".to_string(), Value::String(name.to_string()));
        object.insert(
            "imageUrl".to_string(),
            Value::String(printing.url_front.clone()),
        );
        object.insert(
            "urlBack".to_string(),
            Value::String(printing.url_front.clone()),
        );
    }
    value
}

fn lookup_catalog(
    catalog: &StoredCardCatalog,
    request: CardLookupRequest,
) -> Result<CardLookupResponse, String> {
    if request.names.len() > 1_000 {
        return Err("card lookup accepts at most 1000 names".to_string());
    }
    let mut cards = BTreeMap::new();
    for requested_name in request.names {
        let normalized = requested_name.trim().to_lowercase();
        if cards.contains_key(&requested_name) {
            continue;
        }
        if let Some(printings) = catalog.cards.get(&normalized) {
            cards.insert(requested_name, printings.clone());
        }
    }
    let game_pieces = request
        .include_game_pieces
        .then(|| {
            catalog
                .cards
                .iter()
                .flat_map(|(name, printings)| {
                    printings.iter().filter_map(|printing| {
                        printing
                            .is_game_piece
                            .unwrap_or(false)
                            .then(|| game_piece_value(name, printing))
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(CardLookupResponse {
        schema_version: CATALOG_SCHEMA_VERSION,
        cards,
        game_pieces,
    })
}

pub fn card_sets() -> Result<CardSetCatalogResponse, String> {
    let catalog = catalog()?;
    Ok(CardSetCatalogResponse {
        schema_version: CATALOG_SCHEMA_VERSION,
        sets: catalog.sets.clone(),
    })
}

pub fn lookup_cards(request: CardLookupRequest) -> Result<CardLookupResponse, String> {
    lookup_catalog(catalog()?, request)
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
mod tests {
    use super::{CardLookupRequest, lookup_catalog, read_catalog, select_named_token_printing};
    use std::fs;

    #[test]
    fn lookup_returns_only_requested_cards_and_optional_game_pieces() {
        let directory =
            std::env::temp_dir().join(format!("mtg-card-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("catalog fixture directory");
        let path = directory.join("cards.json");
        fs::write(
            &path,
            r#"{
                "cards": {
                    "island": [{"setCode":"lea","collectorNumber":"287","urlFront":"island.jpg"}],
                    "goblin": [{"setCode":"tst","collectorNumber":"1","isGamePiece":true,"urlFront":"goblin.jpg"}]
                },
                "sets": {"lea":"Limited Edition Alpha"}
            }"#,
        )
        .expect("catalog fixture");
        let catalog = read_catalog(&path).expect("catalog parses");

        let response = lookup_catalog(
            &catalog,
            CardLookupRequest {
                names: vec!["Island".to_string()],
                include_game_pieces: true,
            },
        )
        .expect("lookup succeeds");

        assert_eq!(response.cards.len(), 1);
        assert!(response.cards.contains_key("Island"));
        assert_eq!(response.game_pieces.len(), 1);
        assert_eq!(response.game_pieces[0]["name"], "goblin");
        fs::remove_dir_all(directory).expect("catalog fixture cleanup");
    }

    #[test]
    fn named_token_lookup_requires_an_oracle_backed_token_game_piece() {
        let directory =
            std::env::temp_dir().join(format!("mtg-token-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("catalog fixture directory");
        let path = directory.join("cards.json");
        fs::write(
            &path,
            r#"{
                "cards": {
                    "blood": [{
                        "setCode":"tvow",
                        "collectorNumber":"17",
                        "isToken":true,
                        "isGamePiece":true,
                        "typeLine":"Token Artifact - Blood",
                        "oracleText":"{1}, {T}, Discard a card, Sacrifice this token: Draw a card.",
                        "urlFront":"blood.jpg"
                    }],
                    "blank": [{
                        "setCode":"tst",
                        "collectorNumber":"1",
                        "isToken":true,
                        "isGamePiece":true,
                        "urlFront":"blank.jpg"
                    }],
                    "not a token": [{
                        "setCode":"tst",
                        "collectorNumber":"2",
                        "oracleText":"{T}: Add {C}.",
                        "urlFront":"card.jpg"
                    }]
                },
                "sets": {}
            }"#,
        )
        .expect("catalog fixture");
        let catalog = read_catalog(&path).expect("catalog parses");

        let blood = select_named_token_printing(&catalog, "Blood").expect("Blood token");
        assert_eq!(
            blood.oracle_text.as_deref(),
            Some("{1}, {T}, Discard a card, Sacrifice this token: Draw a card.")
        );
        assert!(select_named_token_printing(&catalog, "blank").is_none());
        assert!(select_named_token_printing(&catalog, "not a token").is_none());
        fs::remove_dir_all(directory).expect("catalog fixture cleanup");
    }
}
