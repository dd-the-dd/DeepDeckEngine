//! Parse a local Scryfall card array and write one JSON line per card with
//! each ability status, recognizer, IR and diagnostics. Used by
//! scripts/audit-latest-sets.py. Set DEEPDECK_ORACLE_NATIVE_ONLY=1 to
//! disable legacy recognizers.

use deepdeck_oracle::oracle::{
    OracleCardFace, OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufWriter, Write},
    time::Instant,
};

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

fn optional_text(value: &Value, key: &str) -> Option<String> {
    value[key].as_str().map(str::to_string)
}

fn run() {
    let args: Vec<_> = std::env::args().collect();
    let native_only = std::env::var_os("DEEPDECK_ORACLE_NATIVE_ONLY").is_some();
    let cards: Vec<Value> = serde_json::from_slice(&fs::read(&args[1]).unwrap()).unwrap();
    let mut output = BufWriter::new(fs::File::create(&args[2]).unwrap());
    let started = Instant::now();
    let mut ability_count = 0;
    for card in &cards {
        let faces = card["card_faces"]
            .as_array()
            .map(|faces| {
                faces
                    .iter()
                    .enumerate()
                    .map(|(index, face)| OracleCardFace {
                        id: optional_text(face, "oracle_id")
                            .unwrap_or_else(|| format!("{}:face-{index}", text(card, "id"))),
                        name: text(face, "name"),
                        type_line: text(face, "type_line"),
                        mana_cost: optional_text(face, "mana_cost"),
                        oracle_text: text(face, "oracle_text"),
                        power: optional_text(face, "power"),
                        toughness: optional_text(face, "toughness"),
                        loyalty: optional_text(face, "loyalty"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let request = OracleCardParseRequest {
            card_name: text(card, "name"),
            type_line: text(card, "type_line"),
            mana_cost: optional_text(card, "mana_cost"),
            oracle_text: optional_text(card, "oracle_text"),
            layout: optional_text(card, "layout"),
            faces,
        };
        let result = if native_only {
            parse_oracle_card_native(request)
        } else {
            parse_oracle_card(request)
        };
        let abilities: Vec<_> = result.abilities.iter().map(|ability| json!({
            "text": ability.source.text,
            "type": ability.ability_type,
            "status": ability.status,
            "recognizer": ability.recognizer,
            "rule": ability.rule,
            "diagnostics": ability.diagnostics,
            // First construction operation: identifies the recognizing family.
            "operations": ability.iterations.iter().map(|iteration| &iteration.operation).collect::<Vec<_>>(),
            "unknown": ability.entities.iter().filter(|entity| entity.token_type.eq_ignore_ascii_case("unknown")).map(|entity| &entity.raw).collect::<Vec<_>>(),
        })).collect();
        ability_count += abilities.len();
        serde_json::to_writer(
            &mut output,
            &json!({
                "oracle_id": card["oracle_id"], "name": card["name"], "set": card["set"],
                "collector_number": card["collector_number"], "status": result.status,
                "abilities": abilities,
            }),
        )
        .unwrap();
        writeln!(&mut output).unwrap();
    }
    output.flush().unwrap();
    eprintln!(
        "mode={}; {} cards; {} abilities; {:.3} seconds",
        if native_only { "nativeOnly" } else { "full" },
        cards.len(),
        ability_count,
        started.elapsed().as_secs_f64()
    );
}

fn main() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
