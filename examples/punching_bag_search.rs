use mtg_engine::engine::{CardDefinition, PlayerDeck};
use mtg_engine::punching_bag::{
    PunchingBagBenchmarkConfig, PunchingBagSearchRoot, benchmark_punching_bag_tree,
};
use serde_json::json;

fn benchmark_learner_deck() -> PlayerDeck {
    let land = CardDefinition {
        id: "benchmark-wastes".to_string(),
        name: "Benchmark Wastes".to_string(),
        type_line: "Basic Land — Wastes".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: String::new(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    };
    let finisher = CardDefinition {
        id: "benchmark-finisher".to_string(),
        name: "Benchmark Finisher".to_string(),
        type_line: "Creature — Avatar".to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: Some("20".to_string()),
        toughness: Some("20".to_string()),
        rules: vec![
            json!({
                "kind": "keywordAbility",
                "source": { "kind": "self" },
                "ability": { "kind": "flying" },
            }),
            json!({
                "kind": "keywordAbility",
                "source": { "kind": "self" },
                "ability": { "kind": "haste" },
            }),
        ],
    };
    let mut cards = vec![land; 6];
    cards.push(finisher);
    PlayerDeck {
        id: "benchmark-learner".to_string(),
        name: "Untap Full-Tree Fixture".to_string(),
        starting_life: 20,
        cards,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let report = benchmark_punching_bag_tree(
        benchmark_learner_deck(),
        PunchingBagBenchmarkConfig {
            seed: 20_260_810,
            search_root: PunchingBagSearchRoot::WinningTurnUntap,
            maximum_random_games: 1_000,
            maximum_turns: 8,
            maximum_unique_nodes: 1_000_000,
            maximum_depth: 128,
            maximum_choices_per_node: 4_096,
        },
    )?;
    let root_players = report
        .root_state
        .players
        .iter()
        .map(|player| {
            json!({
                "id": player.id,
                "life": player.life,
                "hand": player.hand.len(),
                "library": player.library.len(),
                "battlefield": player.battlefield.len(),
                "graveyard": player.graveyard.len(),
            })
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schemaVersion": report.schema_version,
            "learnerDeck": report.learner_deck,
            "punchingBag": report.punching_bag,
            "discovery": report.discovery,
            "tree": report.tree,
            "root": {
                "turn": report.root_state.turn_number,
                "step": report.root_state.step,
                "activePlayer": report.root_state.players[report.root_state.active_player].id,
                "players": root_players,
            },
        }))?
    );
    Ok(())
}
