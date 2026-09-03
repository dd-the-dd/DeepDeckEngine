use crate::engine::{CardDefinition, GameMode, GameSetup, PlayerDeck};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const COMMANDER_DECK_SIZE: usize = 100;
pub const COMMANDER_STARTING_LIFE: i32 = 40;
pub const DUEL_COMMANDER_STARTING_LIFE: i32 = 25;
pub const LEGACY_MINIMUM_DECK_SIZE: usize = 60;
pub const LEGACY_MAXIMUM_SIDEBOARD_SIZE: usize = 15;
pub const LEGACY_STARTING_LIFE: i32 = 20;
pub const TRAINING_DECK_SIZE: usize = 20;
pub const TRAINING_OPENING_HAND_SIZE: usize = 5;
pub const TRAINING_STARTING_LIFE: i32 = 5;
pub const TRAINING2_DECK_SIZE: usize = 40;
pub const TRAINING2_OPENING_HAND_SIZE: usize = 6;
pub const TRAINING2_STARTING_LIFE: i32 = 10;
pub const TRAINING2_FREE_MULLIGANS: usize = 1;
pub const TRAINING2_MAX_MULLIGANS: usize = 3;

impl GameMode {
    pub fn infer(setup: &GameSetup) -> Self {
        if setup
            .players
            .iter()
            .any(|player| player.cards.iter().any(|card| card.is_commander))
        {
            Self::Commander
        } else {
            Self::Free
        }
    }

    pub const fn default_starting_life(self) -> Option<i32> {
        match self {
            Self::Free => None,
            Self::Legacy => Some(LEGACY_STARTING_LIFE),
            Self::Commander => Some(COMMANDER_STARTING_LIFE),
            Self::DuelCommander => Some(DUEL_COMMANDER_STARTING_LIFE),
            Self::Training => Some(TRAINING_STARTING_LIFE),
            Self::Training2 => Some(TRAINING2_STARTING_LIFE),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRules {
    #[serde(default)]
    pub format: GameMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_life: Option<i32>,
    #[serde(default = "default_validate_decks")]
    pub validate_decks: bool,
}

impl Default for GameRules {
    fn default() -> Self {
        Self {
            format: GameMode::Free,
            starting_life: None,
            validate_decks: true,
        }
    }
}

impl GameRules {
    pub const fn commander() -> Self {
        Self {
            format: GameMode::Commander,
            starting_life: Some(COMMANDER_STARTING_LIFE),
            validate_decks: true,
        }
    }

    pub const fn training() -> Self {
        Self {
            format: GameMode::Training,
            starting_life: Some(TRAINING_STARTING_LIFE),
            validate_decks: true,
        }
    }

    pub const fn duel_commander() -> Self {
        Self {
            format: GameMode::DuelCommander,
            starting_life: Some(DUEL_COMMANDER_STARTING_LIFE),
            validate_decks: true,
        }
    }

    pub const fn legacy() -> Self {
        Self {
            format: GameMode::Legacy,
            starting_life: Some(LEGACY_STARTING_LIFE),
            validate_decks: true,
        }
    }

    pub const fn training2() -> Self {
        Self {
            format: GameMode::Training2,
            starting_life: Some(TRAINING2_STARTING_LIFE),
            validate_decks: true,
        }
    }

    pub const fn for_format(format: GameMode) -> Self {
        match format {
            GameMode::Free => Self {
                format,
                starting_life: None,
                validate_decks: true,
            },
            GameMode::Legacy => Self::legacy(),
            GameMode::Commander => Self::commander(),
            GameMode::DuelCommander => Self::duel_commander(),
            GameMode::Training => Self::training(),
            GameMode::Training2 => Self::training2(),
        }
    }

    pub fn inferred(setup: &GameSetup) -> Self {
        Self::for_format(GameMode::infer(setup))
    }

    pub fn apply(&self, setup: &mut GameSetup) -> Result<(), GameRulesError> {
        if let Some(starting_life) = self.starting_life {
            if starting_life <= 0 {
                return Err(GameRulesError::single(
                    "game",
                    "starting-life",
                    "starting life must be positive",
                ));
            }
            for player in &mut setup.players {
                player.starting_life = starting_life;
            }
        }
        self.validate(setup)
    }

    pub fn validate(&self, setup: &GameSetup) -> Result<(), GameRulesError> {
        if !self.validate_decks {
            return Ok(());
        }

        if self.format == GameMode::Training {
            let mut violations = setup
                .players
                .iter()
                .filter(|player| {
                    player
                        .cards
                        .iter()
                        .filter(|card| is_deck_card(card))
                        .count()
                        < TRAINING_DECK_SIZE
                })
                .map(|player| {
                    DeckViolation::new(
                        player,
                        "training-card-count",
                        format!("Training requires at least {TRAINING_DECK_SIZE} eligible cards"),
                    )
                })
                .collect::<Vec<_>>();
            if setup.opening_hand_size != TRAINING_OPENING_HAND_SIZE {
                violations.push(DeckViolation {
                    player_id: "game".to_string(),
                    deck_name: "game".to_string(),
                    code: "training-opening-hand".to_string(),
                    message: format!(
                        "Training requires an opening hand size of {TRAINING_OPENING_HAND_SIZE}"
                    ),
                });
            }
            return if violations.is_empty() {
                Ok(())
            } else {
                Err(GameRulesError { violations })
            };
        }

        if self.format == GameMode::Training2 {
            let mut violations = setup
                .players
                .iter()
                .filter(|player| {
                    player
                        .cards
                        .iter()
                        .filter(|card| is_deck_card(card) && !card.is_commander)
                        .count()
                        < TRAINING2_DECK_SIZE
                })
                .map(|player| {
                    DeckViolation::new(
                        player,
                        "training2-card-count",
                        format!(
                            "Training 2 requires at least {TRAINING2_DECK_SIZE} eligible non-commander cards"
                        ),
                    )
                })
                .collect::<Vec<_>>();
            violations.extend(
                setup
                    .players
                    .iter()
                    .filter(|player| {
                        player
                            .cards
                            .iter()
                            .filter(|card| is_deck_card(card) && card.is_commander)
                            .count()
                            != 1
                    })
                    .map(|player| {
                        DeckViolation::new(
                            player,
                            "training2-commander-count",
                            "Training 2 requires exactly one commander",
                        )
                    }),
            );
            if setup.opening_hand_size != TRAINING2_OPENING_HAND_SIZE {
                violations.push(DeckViolation {
                    player_id: "game".to_string(),
                    deck_name: "game".to_string(),
                    code: "training2-opening-hand".to_string(),
                    message: format!(
                        "Training 2 requires an opening hand size of {TRAINING2_OPENING_HAND_SIZE}"
                    ),
                });
            }
            return if violations.is_empty() {
                Ok(())
            } else {
                Err(GameRulesError { violations })
            };
        }

        if self.format == GameMode::Legacy {
            let mut violations = setup
                .players
                .iter()
                .flat_map(validate_legacy_deck)
                .collect::<Vec<_>>();
            if setup.opening_hand_size != 7 {
                violations.push(DeckViolation {
                    player_id: "game".to_string(),
                    deck_name: "game".to_string(),
                    code: "legacy-opening-hand".to_string(),
                    message: "Legacy requires an opening hand size of seven".to_string(),
                });
            }
            return if violations.is_empty() {
                Ok(())
            } else {
                Err(GameRulesError { violations })
            };
        }

        if !matches!(self.format, GameMode::Commander | GameMode::DuelCommander) {
            return Ok(());
        }

        let mut violations = setup
            .players
            .iter()
            .flat_map(validate_commander_deck)
            .collect::<Vec<_>>();
        if setup.opening_hand_size != 7 {
            violations.push(DeckViolation {
                player_id: "game".to_string(),
                deck_name: "game".to_string(),
                code: "commander-opening-hand".to_string(),
                message: "Commander requires an opening hand size of seven".to_string(),
            });
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(GameRulesError { violations })
        }
    }
}

fn validate_legacy_deck(player: &PlayerDeck) -> Vec<DeckViolation> {
    let main_deck = player
        .cards
        .iter()
        .filter(|card| is_deck_card(card))
        .collect::<Vec<_>>();
    let sideboard = player
        .cards
        .iter()
        .filter(|card| card.is_sideboard && !is_auxiliary_game_piece(card))
        .collect::<Vec<_>>();
    let mut violations = Vec::new();

    if main_deck.len() < LEGACY_MINIMUM_DECK_SIZE {
        violations.push(DeckViolation::new(
            player,
            "legacy-card-count",
            format!(
                "Legacy requires at least {LEGACY_MINIMUM_DECK_SIZE} cards in the main deck; found {}",
                main_deck.len()
            ),
        ));
    }
    if sideboard.len() > LEGACY_MAXIMUM_SIDEBOARD_SIZE {
        violations.push(DeckViolation::new(
            player,
            "legacy-sideboard-count",
            format!(
                "Legacy permits at most {LEGACY_MAXIMUM_SIDEBOARD_SIZE} sideboard cards; found {}",
                sideboard.len()
            ),
        ));
    }
    if let Some(commander) = player.cards.iter().find(|card| card.is_commander) {
        violations.push(DeckViolation::new(
            player,
            "legacy-commander",
            format!(
                "{} is marked as a commander in a non-Commander format",
                commander.name
            ),
        ));
    }

    let mut copies = BTreeMap::<String, (&CardDefinition, usize)>::new();
    for card in main_deck.into_iter().chain(sideboard) {
        let entry = copies
            .entry(card.name.trim().to_lowercase())
            .or_insert((card, 0));
        entry.1 += 1;
    }
    for (_, (card, count)) in copies {
        if count > 4 && !is_basic_land(card) {
            violations.push(DeckViolation::new(
                player,
                "legacy-copy-limit",
                format!("{} appears {count} times across the main deck and sideboard; the limit is four", card.name),
            ));
        }
    }

    violations
}

fn default_validate_decks() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckViolation {
    pub player_id: String,
    pub deck_name: String,
    pub code: String,
    pub message: String,
}

impl DeckViolation {
    fn new(player: &PlayerDeck, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            player_id: player.id.clone(),
            deck_name: player.name.clone(),
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameRulesError {
    pub violations: Vec<DeckViolation>,
}

impl GameRulesError {
    fn single(scope: &str, code: &str, message: &str) -> Self {
        Self {
            violations: vec![DeckViolation {
                player_id: scope.to_string(),
                deck_name: scope.to_string(),
                code: code.to_string(),
                message: message.to_string(),
            }],
        }
    }
}

impl fmt::Display for GameRulesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let summary = self
            .violations
            .iter()
            .map(|violation| {
                format!(
                    "{} [{}]: {}",
                    violation.deck_name, violation.code, violation.message
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        write!(formatter, "illegal game setup: {summary}")
    }
}

impl std::error::Error for GameRulesError {}

fn validate_commander_deck(player: &PlayerDeck) -> Vec<DeckViolation> {
    let deck_cards = player
        .cards
        .iter()
        .filter(|card| is_deck_card(card))
        .collect::<Vec<_>>();
    let mut violations = Vec::new();

    if deck_cards.len() != COMMANDER_DECK_SIZE {
        violations.push(DeckViolation::new(
            player,
            "commander-card-count",
            format!(
                "Commander requires exactly {COMMANDER_DECK_SIZE} cards including the commander; found {}",
                deck_cards.len()
            ),
        ));
    }

    let commanders = deck_cards
        .iter()
        .copied()
        .filter(|card| card.is_commander)
        .collect::<Vec<_>>();
    if commanders.len() != 1 {
        violations.push(DeckViolation::new(
            player,
            "commander-count",
            format!(
                "the current Commander rules require exactly one commander; found {}",
                commanders.len()
            ),
        ));
    }

    let mut counts_by_name = BTreeMap::<String, usize>::new();
    let mut display_names = BTreeMap::<String, String>::new();
    for card in &deck_cards {
        let key = card.name.trim().to_lowercase();
        *counts_by_name.entry(key.clone()).or_default() += 1;
        display_names
            .entry(key)
            .or_insert_with(|| card.name.clone());
    }
    for (name, count) in counts_by_name {
        if count <= 1 {
            continue;
        }
        let card = deck_cards
            .iter()
            .find(|card| card.name.trim().to_lowercase() == name)
            .expect("counted card exists");
        if is_basic_land(card) {
            continue;
        }
        let card_name = display_names
            .get(&name)
            .map(String::as_str)
            .unwrap_or(&name);
        violations.push(DeckViolation::new(
            player,
            "commander-singleton",
            format!("{card_name} appears {count} times; non-basic cards must be unique"),
        ));
    }

    if let [commander] = commanders.as_slice() {
        let commander_identity = card_color_identity(commander);
        for card in deck_cards {
            let identity = card_color_identity(card);
            let outside_identity = identity
                .difference(&commander_identity)
                .copied()
                .collect::<BTreeSet<_>>();
            if outside_identity.is_empty() {
                continue;
            }
            violations.push(DeckViolation::new(
                player,
                "commander-color-identity",
                format!(
                    "{} has color identity {} outside commander {}'s identity {}",
                    card.name,
                    display_colors(&outside_identity),
                    commander.name,
                    display_colors(&commander_identity)
                ),
            ));
        }
    }

    violations
}

fn is_deck_card(card: &CardDefinition) -> bool {
    !is_auxiliary_game_piece(card) && !card.is_sideboard
}

fn is_auxiliary_game_piece(card: &CardDefinition) -> bool {
    let type_line = card.type_line.trim().to_ascii_lowercase();
    card.is_token
        || card.is_game_piece
        || type_line.starts_with("token ")
        || type_line.starts_with("emblem")
        || type_line.starts_with("dungeon")
}

fn is_basic_land(card: &CardDefinition) -> bool {
    let normalized = card.type_line.to_ascii_lowercase();
    normalized.contains("basic land") || normalized.contains("basic snow land")
}

fn card_color_identity(card: &CardDefinition) -> BTreeSet<char> {
    let mut colors = BTreeSet::new();
    collect_mana_colors(&card.mana_cost, &mut colors);
    collect_value_colors(&Value::Array(card.rules.clone()), &mut colors);
    for color in card.rules.iter().filter_map(|rule| {
        (rule.get("kind").and_then(Value::as_str) == Some("rulesMarker")
            && rule.get("text").and_then(Value::as_str) == Some("Color identity"))
        .then(|| rule.get("colorIdentity").and_then(Value::as_array))
        .flatten()
    }) {
        for value in color.iter().filter_map(Value::as_str) {
            if let Some(color) = value.chars().next()
                && matches!(color, 'W' | 'U' | 'B' | 'R' | 'G')
            {
                colors.insert(color);
            }
        }
    }
    collect_land_type_colors(&card.type_line, &mut colors);
    colors
}

fn collect_value_colors(value: &Value, colors: &mut BTreeSet<char>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_value_colors(value, colors);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_value_colors(value, colors);
            }
        }
        Value::String(text) => collect_mana_colors(text, colors),
        _ => {}
    }
}

fn collect_mana_colors(text: &str, colors: &mut BTreeSet<char>) {
    let mut inside_symbol = false;
    for character in text.chars() {
        match character {
            '{' => inside_symbol = true,
            '}' => inside_symbol = false,
            color @ ('W' | 'U' | 'B' | 'R' | 'G') if inside_symbol => {
                colors.insert(color);
            }
            _ => {}
        }
    }
}

fn collect_land_type_colors(type_line: &str, colors: &mut BTreeSet<char>) {
    let normalized = type_line.to_ascii_lowercase();
    let words = normalized
        .split(|character: char| !character.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .collect::<BTreeSet<_>>();
    if !words.contains("land") {
        return;
    }
    for (land_type, color) in [
        ("plains", 'W'),
        ("island", 'U'),
        ("swamp", 'B'),
        ("mountain", 'R'),
        ("forest", 'G'),
    ] {
        if words.contains(land_type) {
            colors.insert(color);
        }
    }
}

fn display_colors(colors: &BTreeSet<char>) -> String {
    let ordered = ['W', 'U', 'B', 'R', 'G']
        .into_iter()
        .filter(|color| colors.contains(color))
        .collect::<String>();
    if ordered.is_empty() {
        "colorless".to_string()
    } else {
        ordered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: usize, name: impl Into<String>, mana_cost: &str) -> CardDefinition {
        CardDefinition {
            id: format!("card-{id}"),
            name: name.into(),
            type_line: "Artifact".to_string(),
            is_commander: false,
            is_token: false,
            is_game_piece: false,
            is_sideboard: false,
            mana_cost: mana_cost.to_string(),
            power: None,
            toughness: None,
            rules: Vec::new(),
        }
    }

    fn legal_deck() -> PlayerDeck {
        let mut commander = card(0, "White Commander", "{2}{W}");
        commander.is_commander = true;
        commander.type_line = "Legendary Creature — Human".to_string();
        let mut cards = vec![commander];
        cards.extend((1..100).map(|index| card(index, format!("Relic {index}"), "{2}")));
        PlayerDeck {
            id: "player-1".to_string(),
            name: "Legal Commander".to_string(),
            starting_life: 20,
            cards,
        }
    }

    fn legal_legacy_deck() -> PlayerDeck {
        let cards = (0..60)
            .map(|index| card(index, format!("Legacy card {index}"), "{1}"))
            .chain((60..75).map(|index| {
                let mut sideboard_card = card(index, format!("Sideboard card {index}"), "{1}");
                sideboard_card.is_sideboard = true;
                sideboard_card
            }))
            .collect();
        PlayerDeck {
            id: "legacy-player".to_string(),
            name: "Legal Legacy".to_string(),
            starting_life: 40,
            cards,
        }
    }

    fn setup(deck: PlayerDeck) -> GameSetup {
        GameSetup {
            players: vec![deck],
            opening_hand_size: 7,
            starting_player: 0,
        }
    }

    #[test]
    fn commander_rules_set_default_starting_life() {
        let mut setup = setup(legal_deck());
        GameRules::commander().apply(&mut setup).unwrap();
        assert_eq!(setup.players[0].starting_life, 40);
    }

    #[test]
    fn duel_commander_uses_commander_deck_rules_and_twenty_five_life() {
        let mut setup = setup(legal_deck());

        GameRules::duel_commander().apply(&mut setup).unwrap();

        assert_eq!(setup.players[0].starting_life, DUEL_COMMANDER_STARTING_LIFE);
        assert_eq!(
            GameRules::for_format(GameMode::DuelCommander),
            GameRules::duel_commander()
        );
    }

    #[test]
    fn legacy_rules_set_twenty_life_and_accept_sixty_plus_fifteen() {
        let mut setup = setup(legal_legacy_deck());

        GameRules::legacy().apply(&mut setup).unwrap();

        assert_eq!(setup.players[0].starting_life, LEGACY_STARTING_LIFE);
        assert_eq!(
            setup.players[0]
                .cards
                .iter()
                .filter(|card| card.is_sideboard)
                .count(),
            15
        );
    }

    #[test]
    fn legacy_rejects_short_main_decks_oversized_sideboards_and_commanders() {
        let mut deck = legal_legacy_deck();
        deck.cards[0].is_sideboard = true;
        deck.cards[1].is_commander = true;

        let error = GameRules::legacy().validate(&setup(deck)).unwrap_err();
        let codes = error
            .violations
            .iter()
            .map(|violation| violation.code.as_str())
            .collect::<BTreeSet<_>>();

        assert!(codes.contains("legacy-card-count"));
        assert!(codes.contains("legacy-sideboard-count"));
        assert!(codes.contains("legacy-commander"));
    }

    #[test]
    fn legacy_does_not_count_auxiliary_game_pieces_even_without_flags() {
        let mut deck = legal_legacy_deck();
        let mut main_token = card(75, "Zombie".to_string(), "");
        main_token.type_line = "Token Creature — Zombie".to_string();
        let mut sideboard_token = card(76, "Marit Lage".to_string(), "");
        sideboard_token.type_line = "Token Legendary Creature — Avatar".to_string();
        sideboard_token.is_sideboard = true;
        deck.cards.extend([main_token, sideboard_token]);

        GameRules::legacy().validate(&setup(deck)).unwrap();
    }

    #[test]
    fn legacy_copy_limit_combines_main_deck_and_sideboard_but_exempts_basics() {
        let mut deck = legal_legacy_deck();
        for index in [0, 1, 2, 60, 61] {
            deck.cards[index].name = "Repeated spell".to_string();
        }
        for index in 3..10 {
            deck.cards[index].name = "Island".to_string();
            deck.cards[index].type_line = "Basic Land — Island".to_string();
        }

        let error = GameRules::legacy().validate(&setup(deck)).unwrap_err();

        assert_eq!(
            error
                .violations
                .iter()
                .filter(|violation| violation.code == "legacy-copy-limit")
                .count(),
            1
        );
        assert!(error.violations.iter().any(|violation| {
            violation.code == "legacy-copy-limit" && violation.message.contains("Repeated spell")
        }));
    }

    #[test]
    fn training_rules_require_twenty_cards_and_five_card_hands() {
        let mut valid_setup = setup(legal_deck());
        valid_setup.opening_hand_size = TRAINING_OPENING_HAND_SIZE;
        GameRules::training().apply(&mut valid_setup).unwrap();
        assert_eq!(valid_setup.players[0].starting_life, TRAINING_STARTING_LIFE);

        let mut short_setup = valid_setup.clone();
        short_setup.players[0]
            .cards
            .truncate(TRAINING_DECK_SIZE - 1);
        let error = GameRules::training().validate(&short_setup).unwrap_err();
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.code == "training-card-count")
        );

        valid_setup.opening_hand_size = 7;
        let error = GameRules::training().validate(&valid_setup).unwrap_err();
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.code == "training-opening-hand")
        );
    }

    #[test]
    fn training2_rules_require_commander_forty_cards_and_six_card_hands() {
        let mut valid_setup = setup(legal_deck());
        valid_setup.opening_hand_size = TRAINING2_OPENING_HAND_SIZE;
        GameRules::training2().apply(&mut valid_setup).unwrap();
        assert_eq!(
            valid_setup.players[0].starting_life,
            TRAINING2_STARTING_LIFE
        );

        let mut short_setup = valid_setup.clone();
        short_setup.players[0].cards.truncate(TRAINING2_DECK_SIZE);
        let error = GameRules::training2().validate(&short_setup).unwrap_err();
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.code == "training2-card-count")
        );

        let mut commanderless_setup = valid_setup.clone();
        commanderless_setup.players[0].cards[0].is_commander = false;
        let error = GameRules::training2()
            .validate(&commanderless_setup)
            .unwrap_err();
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.code == "training2-commander-count")
        );

        valid_setup.opening_hand_size = 7;
        let error = GameRules::training2().validate(&valid_setup).unwrap_err();
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.code == "training2-opening-hand")
        );
    }

    #[test]
    fn commander_rejects_duplicate_non_basic_cards() {
        let mut deck = legal_deck();
        deck.cards[99] = deck.cards[1].clone();
        let error = GameRules::commander().validate(&setup(deck)).unwrap_err();
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.code == "commander-singleton")
        );
    }

    #[test]
    fn commander_allows_repeated_basic_lands() {
        let mut deck = legal_deck();
        for (index, card) in deck.cards.iter_mut().enumerate().skip(1) {
            card.id = format!("plains-{index}");
            card.name = "Plains".to_string();
            card.type_line = "Basic Land — Plains".to_string();
            card.mana_cost.clear();
        }
        GameRules::commander().validate(&setup(deck)).unwrap();
    }

    #[test]
    fn commander_rejects_cards_outside_color_identity() {
        let mut deck = legal_deck();
        deck.cards[1].mana_cost = "{U}".to_string();
        let error = GameRules::commander().validate(&setup(deck)).unwrap_err();
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.code == "commander-color-identity")
        );
    }

    #[test]
    fn commander_identity_marker_includes_every_printed_face() {
        let mut deck = legal_deck();
        deck.cards[0].rules.push(serde_json::json!({
            "kind": "rulesMarker",
            "text": "Color identity",
            "colorIdentity": ["W", "U", "B", "R", "G"],
        }));
        deck.cards[1].mana_cost = "{B}".to_string();

        GameRules::commander().validate(&setup(deck)).unwrap();
    }
}
