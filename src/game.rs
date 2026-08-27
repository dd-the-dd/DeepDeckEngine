use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardState {
    #[serde(default)]
    pub tapped: bool,
    #[serde(default)]
    pub summoning_sick: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub mana_cost: String,
    #[serde(default)]
    pub mana_value: Option<f64>,
    #[serde(default)]
    pub oracle_text: String,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub type_line: String,
    #[serde(default)]
    pub source_zone: Option<String>,
    #[serde(default)]
    pub state: CardState,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battlefield {
    #[serde(default)]
    pub creatures: Vec<Card>,
    #[serde(default)]
    pub lands: Vec<Card>,
    #[serde(default)]
    pub non_creature_permanents: Vec<Card>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardZone {
    #[serde(default)]
    pub cards: Vec<Card>,
    #[serde(default)]
    pub recoverable: Vec<Card>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Zones {
    #[serde(default)]
    pub battlefield: Battlefield,
    #[serde(default)]
    pub exile: CardZone,
    #[serde(default)]
    pub graveyard: CardZone,
    #[serde(default)]
    pub hand: Vec<Card>,
    #[serde(default)]
    pub land_plays_available: i32,
    #[serde(default)]
    pub mana_pool: BTreeMap<String, i32>,
    #[serde(default)]
    pub playable_hand: Vec<Card>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub zones: Zones,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRequest {
    pub phase: String,
    pub player: Player,
    #[serde(default)]
    pub target_players: Vec<Player>,
    #[serde(default = "default_include_advance_step")]
    pub include_advance_step: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionPacket {
    pub options: Vec<DecisionOption>,
    pub phase: String,
    pub player_key: String,
    pub player_name: String,
    pub role: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    pub mana_cost: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mana_value: Option<f64>,
    pub oracle_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    pub type_line: String,
    pub source_zone: String,
    pub state: CardState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionOption {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<CardSnapshot>,
    pub conditions: Vec<NamedRecord>,
    pub costs: Vec<ActionCost>,
    pub kind: String,
    pub label: String,
    pub option: ActionOption,
    pub player_key: String,
    pub source_zone: String,
    pub targets: ActionTargets,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOption {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub source_zone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payment_options: Vec<PaymentOption>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana_produced: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_types: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_target: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration_branch: Option<DeclarationBranch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<NamedRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub costs: Vec<ActionCost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<ActionTargets>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclarationBranch {
    pub id: String,
    pub label: String,
    pub requires_target: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_types: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedRecord {
    pub name: String,
    pub params: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionCost {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payment_options: Vec<PaymentOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentOption {
    pub sources: Vec<CardSnapshot>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionTargets {
    pub candidates: TargetCandidates,
    pub required: bool,
    pub target_types: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TargetCandidates {
    pub cards: Vec<TargetCardCandidate>,
    pub players: Vec<TargetPlayerCandidate>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetCardCandidate {
    pub card: CardSnapshot,
    pub player_key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TargetPlayerCandidate {
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug)]
struct ManaCost {
    colored: BTreeMap<String, i32>,
    generic: i32,
}

fn default_include_advance_step() -> bool {
    true
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn has_type(card: &Card, card_type: &str) -> bool {
    Regex::new(&format!(r"(?i)\b{}\b", regex::escape(card_type)))
        .expect("type regex compiles")
        .is_match(&card.type_line)
}

fn is_land(card: &Card) -> bool {
    has_type(card, "land")
}

fn is_creature(card: &Card) -> bool {
    has_type(card, "creature")
}

fn is_instant(card: &Card) -> bool {
    has_type(card, "instant")
}

fn is_sorcery(card: &Card) -> bool {
    has_type(card, "sorcery")
}

fn is_permanent(card: &Card) -> bool {
    !is_instant(card) && !is_sorcery(card)
}

fn phase_allows_sorcery(phase: &str) -> bool {
    matches!(phase, "main" | "secondMain")
}

fn card_snapshot(card: &Card, source_zone: &str) -> CardSnapshot {
    CardSnapshot {
        id: card.id.clone(),
        name: card.name.clone(),
        image_url: card.image_url.clone(),
        mana_cost: card.mana_cost.clone(),
        mana_value: card.mana_value,
        oracle_text: card.oracle_text.clone(),
        power: card.power.clone(),
        toughness: card.toughness.clone(),
        type_line: card.type_line.clone(),
        source_zone: source_zone.to_string(),
        state: card.state.clone(),
    }
}

fn battlefield_cards(player: &Player) -> Vec<(&Card, &'static str)> {
    player
        .zones
        .battlefield
        .creatures
        .iter()
        .map(|card| (card, "battlefield"))
        .chain(
            player
                .zones
                .battlefield
                .lands
                .iter()
                .map(|card| (card, "battlefield")),
        )
        .chain(
            player
                .zones
                .battlefield
                .non_creature_permanents
                .iter()
                .map(|card| (card, "battlefield")),
        )
        .collect()
}

fn mana_symbols_from_basic_land(card: &Card) -> Vec<String> {
    [
        ("forest", "G"),
        ("island", "U"),
        ("mountain", "R"),
        ("plains", "W"),
        ("swamp", "B"),
        ("wastes", "C"),
    ]
    .iter()
    .filter(|(subtype, _)| has_type(card, subtype))
    .map(|(_, symbol)| symbol.to_string())
    .collect()
}

fn mana_ability_choices_from_oracle(card: &Card) -> Vec<Vec<String>> {
    let mut abilities = Vec::new();
    let add_re = Regex::new(r"(?i)\{T\}[^:\n]*:\s*add ([^.]+)\.").expect("mana add regex compiles");
    let symbol_re = Regex::new(r"\{([WUBRGC])\}").expect("mana symbol regex compiles");
    let any_color_re = Regex::new(r"(?i)one mana of any color").expect("any color regex compiles");
    let or_re = Regex::new(r"(?i)\bor\b").expect("or regex compiles");
    for captures in add_re.captures_iter(&card.oracle_text) {
        let clause = &captures[1];
        if any_color_re.is_match(clause) {
            abilities.extend(
                ["W", "U", "B", "R", "G"]
                    .iter()
                    .map(|symbol| vec![symbol.to_string()]),
            );
            continue;
        }

        let symbols = symbol_re
            .captures_iter(clause)
            .map(|symbol| symbol[1].to_uppercase())
            .collect::<Vec<_>>();
        if symbols.is_empty() {
            continue;
        }

        if or_re.is_match(clause) {
            abilities.extend(symbols.into_iter().map(|symbol| vec![symbol]));
        } else {
            abilities.push(symbols);
        }
    }

    abilities
}

fn mana_ability_choices(card: &Card) -> Vec<Vec<String>> {
    if is_land(card) {
        let mut choices = mana_symbols_from_basic_land(card);
        let oracle_choices = mana_ability_choices_from_oracle(card);
        if !oracle_choices.is_empty() {
            return oracle_choices;
        }
        choices.drain(..).map(|symbol| vec![symbol]).collect()
    } else {
        mana_ability_choices_from_oracle(card)
    }
}

fn non_mana_activated_labels(card: &Card) -> Vec<String> {
    card.oracle_text
        .lines()
        .map(str::trim)
        .filter(|line| line.contains(':') && !Regex::new(r"(?i)\badd\b").unwrap().is_match(line))
        .map(|line| line.trim_end_matches('.').to_string())
        .collect()
}

fn parse_mana_cost(value: &str) -> ManaCost {
    let mut colored = ["W", "U", "B", "R", "G", "C"]
        .iter()
        .map(|symbol| (symbol.to_string(), 0))
        .collect::<BTreeMap<_, _>>();
    let mut generic = 0;
    let symbol_re = Regex::new(r"\{([^}]+)\}").expect("mana cost regex compiles");
    for captures in symbol_re.captures_iter(value) {
        let symbol = captures[1].to_uppercase();
        if let Ok(amount) = symbol.parse::<i32>() {
            generic += amount;
        } else if let Some(count) = colored.get_mut(&symbol) {
            *count += 1;
        }
    }

    ManaCost { colored, generic }
}

fn mana_cost_has_requirement(cost: &ManaCost) -> bool {
    cost.generic > 0 || cost.colored.values().any(|amount| *amount > 0)
}

fn symbols_can_pay(symbols: &[String], cost: &ManaCost) -> bool {
    let mut pool = ["W", "U", "B", "R", "G", "C"]
        .iter()
        .map(|symbol| {
            (
                symbol.to_string(),
                symbols
                    .iter()
                    .filter(|value| value.as_str() == *symbol)
                    .count() as i32,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (symbol, required) in &cost.colored {
        let available = pool.get_mut(symbol).expect("pool has symbol");
        if *available < *required {
            return false;
        }
        *available -= *required;
    }

    pool.values().sum::<i32>() >= cost.generic
}

fn payment_options_for_card(card: &Card, player: &Player) -> Vec<PaymentOption> {
    let cost = parse_mana_cost(&card.mana_cost);
    if !mana_cost_has_requirement(&cost) {
        return Vec::new();
    }

    let sources = battlefield_cards(player)
        .into_iter()
        .filter(|(source, _)| {
            !(source.state.tapped || is_creature(source) && source.state.summoning_sick)
        })
        .filter_map(|(source, zone)| {
            let choices = mana_ability_choices(source);
            (!choices.is_empty()).then_some((source, zone, choices))
        })
        .collect::<Vec<_>>();
    let mut initial_symbols = Vec::new();
    for (symbol, amount) in &player.zones.mana_pool {
        for _ in 0..(*amount).max(0) {
            initial_symbols.push(symbol.to_uppercase());
        }
    }
    let mut selections = Vec::<Vec<usize>>::new();

    fn visit(
        index: usize,
        sources: &[(&Card, &str, Vec<Vec<String>>)],
        cost: &ManaCost,
        symbols: &mut Vec<String>,
        selected: &mut Vec<usize>,
        selections: &mut Vec<Vec<usize>>,
    ) {
        if symbols_can_pay(symbols, cost) {
            selections.push(selected.clone());
            return;
        }
        if index >= sources.len() || selections.len() >= 64 {
            return;
        }

        visit(index + 1, sources, cost, symbols, selected, selections);
        for choice in &sources[index].2 {
            let symbol_count = symbols.len();
            symbols.extend(choice.iter().cloned());
            selected.push(index);
            visit(index + 1, sources, cost, symbols, selected, selections);
            selected.pop();
            symbols.truncate(symbol_count);
        }
    }

    visit(
        0,
        &sources,
        &cost,
        &mut initial_symbols,
        &mut Vec::new(),
        &mut selections,
    );
    let Some(minimum_source_count) = selections.iter().map(Vec::len).min() else {
        return Vec::new();
    };
    let mut seen = std::collections::BTreeSet::new();
    selections
        .into_iter()
        .filter(|selection| selection.len() == minimum_source_count)
        .filter_map(|selection| {
            let mut source_ids = selection
                .iter()
                .map(|index| {
                    sources[*index]
                        .0
                        .id
                        .clone()
                        .unwrap_or_else(|| sources[*index].0.name.clone())
                })
                .collect::<Vec<_>>();
            source_ids.sort();
            seen.insert(source_ids).then(|| PaymentOption {
                sources: selection
                    .iter()
                    .map(|index| card_snapshot(sources[*index].0, sources[*index].1))
                    .collect(),
            })
        })
        .take(20)
        .collect()
}

fn can_pay(card: &Card, player: &Player) -> bool {
    let cost = parse_mana_cost(&card.mana_cost);
    !mana_cost_has_requirement(&cost) || !payment_options_for_card(card, player).is_empty()
}

fn cast_source_allowed(card: &Card, source_zone: &str) -> bool {
    match source_zone {
        "hand" => true,
        "graveyard" => Regex::new(r"(?i)\b(?:cast|play|return)\b[^.]*\bfrom your graveyard\b|\b(?:flashback|jump-start|escape|disturb|aftermath)\b")
            .expect("graveyard cast regex compiles")
            .is_match(&card.oracle_text),
        "exile" => Regex::new(r"(?i)\b(?:cast|play)\b[^.]*\bfrom exile\b|\b(?:adventure|plot|foretell|suspend)\b")
            .expect("exile cast regex compiles")
            .is_match(&card.oracle_text),
        _ => false,
    }
}

fn target_types_from_text(text: &str) -> Vec<String> {
    let text = text.to_lowercase();
    if text.contains("any target") {
        return ["player", "creature", "planeswalker", "battle"]
            .iter()
            .map(|value| value.to_string())
            .collect();
    }

    let mut types = Vec::new();
    for (phrase, card_type) in [
        ("target player", "player"),
        ("target opponent", "player"),
        ("target creature", "creature"),
        ("target planeswalker", "planeswalker"),
        ("target battle", "battle"),
        ("target artifact", "artifact"),
        ("target enchantment", "enchantment"),
        ("target permanent", "permanent"),
    ] {
        if text.contains(phrase) {
            types.push(card_type.to_string());
        }
    }
    types.sort();
    types.dedup();
    types
}

fn declaration_branches_from_oracle(card: &Card) -> Vec<DeclarationBranch> {
    let lines = card
        .oracle_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let choice_lines = if lines
        .first()
        .is_some_and(|line| line.to_lowercase().starts_with("choose "))
    {
        lines
            .iter()
            .skip(1)
            .filter_map(|line| line.strip_prefix('\u{2022}').map(str::trim))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let is_modal = !choice_lines.is_empty();
    let branch_texts = if !is_modal {
        vec![card.oracle_text.trim()]
    } else {
        choice_lines
    };

    branch_texts
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let target_types = target_types_from_text(text);
            DeclarationBranch {
                id: format!("branch-{}", index + 1),
                label: if is_modal {
                    text.to_string()
                } else {
                    format!("Declaration {}", index + 1)
                },
                requires_target: !target_types.is_empty(),
                target_types,
            }
        })
        .collect()
}

fn card_matches_target_types(card: &Card, target_types: &[String]) -> bool {
    target_types.iter().any(|target_type| {
        target_type == "permanent" && is_permanent(card) || has_type(card, target_type)
    })
}

fn action_targets(target_types: &[String], players: &[Player]) -> ActionTargets {
    let cards = players
        .iter()
        .flat_map(|player| {
            battlefield_cards(player)
                .into_iter()
                .filter(|(card, _)| card_matches_target_types(card, target_types))
                .map(|(card, source_zone)| TargetCardCandidate {
                    card: card_snapshot(card, source_zone),
                    player_key: player.key.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let target_players = if target_types
        .iter()
        .any(|target_type| target_type == "player")
    {
        players
            .iter()
            .map(|player| TargetPlayerCandidate {
                key: player.key.clone(),
                name: player.name.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    ActionTargets {
        candidates: TargetCandidates {
            cards,
            players: target_players,
        },
        required: !target_types.is_empty(),
        target_types: target_types.to_vec(),
    }
}

fn named_record(name: &str, params: Value) -> NamedRecord {
    NamedRecord {
        name: name.to_string(),
        params,
    }
}

fn action_costs(option: &ActionOption) -> Vec<ActionCost> {
    let mut costs = Vec::new();
    if option.mana_cost.is_some() || !option.payment_options.is_empty() {
        costs.push(ActionCost {
            kind: "mana".to_string(),
            mana_cost: option.mana_cost.clone(),
            payment_options: option.payment_options.clone(),
            amount: None,
            source: None,
        });
    }

    if matches!(option.kind.as_str(), "activate" | "attack" | "mana") {
        costs.push(ActionCost {
            kind: "tap".to_string(),
            mana_cost: None,
            payment_options: Vec::new(),
            amount: None,
            source: Some("self".to_string()),
        });
    }

    costs
}

fn action_conditions(
    option: &ActionOption,
    card: &Card,
    player: &Player,
    phase: &str,
) -> Vec<NamedRecord> {
    let mut conditions = Vec::new();
    match option.kind.as_str() {
        "playLand" => {
            conditions.push(named_record(
                "phaseAllowsLandPlay",
                json!({ "phase": phase }),
            ));
            conditions.push(named_record(
                "landPlayAvailable",
                json!({ "remaining": player.zones.land_plays_available }),
            ));
        }
        "cast" => {
            conditions.push(named_record(
                "zoneAllowsCast",
                json!({ "sourceZone": option.source_zone }),
            ));
            conditions.push(named_record(
                "phaseAllowsCast",
                json!({
                    "phase": phase,
                    "speed": if is_instant(card) { "instant" } else { "sorcery" }
                }),
            ));
            if option.requires_target {
                conditions.push(named_record(
                    "validTargetAvailable",
                    json!({ "targetTypes": option.target_types }),
                ));
            }
        }
        "mana" => {
            conditions.push(named_record(
                "permanentUntapped",
                json!({ "source": "self" }),
            ));
        }
        "activate" => {
            conditions.push(named_record("sourceOnBattlefield", json!({})));
            conditions.push(named_record("activatedAbilityAvailable", json!({})));
        }
        "attack" => {
            conditions.push(named_record("creatureCanAttack", json!({})));
        }
        _ => {}
    }

    conditions
}

fn decision_from_action(
    card: &Card,
    player: &Player,
    phase: &str,
    mut option: ActionOption,
    target_players: &[Player],
) -> DecisionOption {
    let targets = action_targets(&option.target_types, target_players);
    let conditions = action_conditions(&option, card, player, phase);
    let costs = action_costs(&option);
    let source_zone = option.source_zone.clone();
    option.conditions = conditions.clone();
    option.costs = costs.clone();
    option.targets = Some(targets.clone());

    DecisionOption {
        id: format!(
            "{}:{}:{}:{}",
            player.key,
            option.source_zone,
            card.id.as_deref().unwrap_or(&card.name),
            option.id
        ),
        card: Some(card_snapshot(card, &option.source_zone)),
        conditions,
        costs,
        kind: option.kind.clone(),
        label: option.label.clone(),
        option,
        player_key: player.key.clone(),
        source_zone,
        targets,
    }
}

fn cast_options_for_card(
    card: &Card,
    player: &Player,
    phase: &str,
    source_zone: &str,
    target_players: &[Player],
) -> Vec<DecisionOption> {
    if is_land(card) || !cast_source_allowed(card, source_zone) {
        return Vec::new();
    }
    if !is_instant(card) && !phase_allows_sorcery(phase) {
        return Vec::new();
    }
    if !can_pay(card, player) {
        return Vec::new();
    }

    let branches = declaration_branches_from_oracle(card);
    let show_branch_label = branches.len() > 1;
    let payment_options = payment_options_for_card(card, player);
    branches
        .into_iter()
        .filter_map(|branch| {
            let targets = action_targets(&branch.target_types, target_players);
            if branch.requires_target
                && targets.candidates.cards.is_empty()
                && targets.candidates.players.is_empty()
            {
                return None;
            }
            let label = if show_branch_label {
                format!("Cast {}: {}", card.name, branch.label)
            } else {
                format!("Cast {}", card.name)
            };
            let option = ActionOption {
                id: format!(
                    "cast:{}:{}",
                    card.id.as_deref().unwrap_or(&card.name),
                    branch.id
                ),
                kind: "cast".to_string(),
                label,
                source_zone: source_zone.to_string(),
                mana_cost: Some(card.mana_cost.clone()).filter(|value| !value.is_empty()),
                payment_options: payment_options.clone(),
                mana_produced: Vec::new(),
                target_types: branch.target_types.clone(),
                requires_target: branch.requires_target,
                declaration_branch: Some(branch),
                conditions: Vec::new(),
                costs: Vec::new(),
                targets: None,
            };

            Some(decision_from_action(
                card,
                player,
                phase,
                option,
                target_players,
            ))
        })
        .collect()
}

fn play_land_option(
    card: &Card,
    player: &Player,
    phase: &str,
    target_players: &[Player],
) -> Option<DecisionOption> {
    if !is_land(card) || !phase_allows_sorcery(phase) || player.zones.land_plays_available <= 0 {
        return None;
    }

    let option = ActionOption {
        id: format!("playLand:{}", card.id.as_deref().unwrap_or(&card.name)),
        kind: "playLand".to_string(),
        label: format!("Play land: {}", card.name),
        source_zone: "hand".to_string(),
        mana_cost: None,
        payment_options: Vec::new(),
        mana_produced: Vec::new(),
        target_types: Vec::new(),
        requires_target: false,
        declaration_branch: None,
        conditions: Vec::new(),
        costs: Vec::new(),
        targets: None,
    };

    Some(decision_from_action(
        card,
        player,
        phase,
        option,
        target_players,
    ))
}

fn battlefield_options(
    card: &Card,
    player: &Player,
    phase: &str,
    target_players: &[Player],
) -> Vec<DecisionOption> {
    if card.state.tapped {
        return Vec::new();
    }

    let mut options = mana_ability_choices(card)
        .into_iter()
        .map(|symbols| {
            let label = format!(
                "Add {}",
                symbols
                    .iter()
                    .map(|symbol| format!("{{{}}}", symbol))
                    .collect::<Vec<_>>()
                    .join("")
            );
            let option = ActionOption {
                id: format!(
                    "mana:{}:{}",
                    card.id.as_deref().unwrap_or(&card.name),
                    label
                ),
                kind: "mana".to_string(),
                label,
                source_zone: "battlefield".to_string(),
                mana_cost: None,
                payment_options: Vec::new(),
                mana_produced: symbols,
                target_types: Vec::new(),
                requires_target: false,
                declaration_branch: None,
                conditions: Vec::new(),
                costs: Vec::new(),
                targets: None,
            };
            decision_from_action(card, player, phase, option, target_players)
        })
        .collect::<Vec<_>>();

    options.extend(non_mana_activated_labels(card).into_iter().map(|label| {
        let option = ActionOption {
            id: format!(
                "activate:{}:{}",
                card.id.as_deref().unwrap_or(&card.name),
                label
            ),
            kind: "activate".to_string(),
            label,
            source_zone: "battlefield".to_string(),
            mana_cost: None,
            payment_options: Vec::new(),
            mana_produced: Vec::new(),
            target_types: Vec::new(),
            requires_target: false,
            declaration_branch: None,
            conditions: Vec::new(),
            costs: Vec::new(),
            targets: None,
        };
        decision_from_action(card, player, phase, option, target_players)
    }));

    options
}

fn cards_to_cast_from_zone<'a>(
    zone: &'a CardZone,
    source_zone: &'static str,
) -> Vec<(&'a Card, &'static str)> {
    zone.cards
        .iter()
        .chain(zone.recoverable.iter())
        .map(|card| (card, source_zone))
        .collect()
}

pub fn build_player_decision_options(request: DecisionRequest) -> DecisionPacket {
    let mut target_players = vec![request.player.clone()];
    target_players.extend(request.target_players.clone());
    let hand_cards = if request.player.zones.playable_hand.is_empty() {
        request.player.zones.hand.iter().collect::<Vec<_>>()
    } else {
        request
            .player
            .zones
            .playable_hand
            .iter()
            .collect::<Vec<_>>()
    };

    let mut options = Vec::new();
    for card in hand_cards {
        if let Some(option) =
            play_land_option(card, &request.player, &request.phase, &target_players)
        {
            options.push(option);
            continue;
        }
        options.extend(cast_options_for_card(
            card,
            &request.player,
            &request.phase,
            "hand",
            &target_players,
        ));
    }

    for (card, source_zone) in cards_to_cast_from_zone(&request.player.zones.graveyard, "graveyard")
        .into_iter()
        .chain(cards_to_cast_from_zone(
            &request.player.zones.exile,
            "exile",
        ))
    {
        options.extend(cast_options_for_card(
            card,
            &request.player,
            &request.phase,
            source_zone,
            &target_players,
        ));
    }

    for (card, _) in battlefield_cards(&request.player) {
        options.extend(battlefield_options(
            card,
            &request.player,
            &request.phase,
            &target_players,
        ));
    }

    if request.include_advance_step {
        options.push(DecisionOption {
            id: format!("{}:game:advanceStep:{}", request.player.key, request.phase),
            card: None,
            conditions: Vec::new(),
            costs: Vec::new(),
            kind: "advanceStep".to_string(),
            label: "Continue to next step".to_string(),
            option: ActionOption {
                id: format!("{}:game:advanceStep:{}", request.player.key, request.phase),
                kind: "advanceStep".to_string(),
                label: "Continue to next step".to_string(),
                source_zone: "game".to_string(),
                mana_cost: None,
                payment_options: Vec::new(),
                mana_produced: Vec::new(),
                target_types: Vec::new(),
                requires_target: false,
                declaration_branch: None,
                conditions: Vec::new(),
                costs: Vec::new(),
                targets: None,
            },
            player_key: request.player.key.clone(),
            source_zone: "game".to_string(),
            targets: ActionTargets {
                candidates: TargetCandidates {
                    cards: Vec::new(),
                    players: Vec::new(),
                },
                required: false,
                target_types: Vec::new(),
            },
        });
    }

    DecisionPacket {
        options,
        phase: request.phase,
        player_key: request.player.key,
        player_name: request.player.name,
        role: request.player.role,
    }
}
