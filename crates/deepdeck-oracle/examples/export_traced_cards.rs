//! Data exporter for the standalone card-parser HTML inspector.
//! Enable `automata-trace` explicitly; normal parser builds have no collector.

#[cfg(not(feature = "automata-trace"))]
fn main() {
    eprintln!("Enable --features automata-trace to export actual cursor traces.");
    std::process::exit(2);
}

#[cfg(feature = "automata-trace")]
fn main() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(export)
        .expect("spawn traced parser exporter")
        .join()
        .expect("traced parser exporter completed");
}

#[cfg(feature = "automata-trace")]
fn export() {
    use deepdeck_oracle::oracle::{
        OracleCardFace, OracleCardParseRequest, parse_oracle_card_native_traced,
    };
    use serde_json::{Value, json};
    use std::io::Write;

    fn text(v: &Value, key: &str) -> String {
        v[key].as_str().unwrap_or_default().to_owned()
    }
    fn optional(v: &Value, key: &str) -> Option<String> {
        v[key].as_str().map(str::to_owned)
    }
    fn image(v: &Value) -> Option<&str> {
        v.pointer("/image_uris/normal")
            .or_else(|| v.pointer("/image_uris/large"))
            .and_then(Value::as_str)
    }

    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 3, "export_traced_cards INPUT.json OUTPUT.jsonl");
    let cards: Vec<Value> = serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
    let mut output = std::io::BufWriter::new(std::fs::File::create(&args[2]).unwrap());
    let start = std::time::Instant::now();
    let mut count = 0;
    for card in cards {
        let faces: Vec<OracleCardFace> = card["card_faces"]
            .as_array()
            .map(|faces| {
                faces
                    .iter()
                    .enumerate()
                    .map(|(index, face)| OracleCardFace {
                        id: optional(face, "oracle_id")
                            .unwrap_or_else(|| format!("{}:face-{index}", text(&card, "id"))),
                        name: text(face, "name"),
                        type_line: text(face, "type_line"),
                        mana_cost: optional(face, "mana_cost"),
                        oracle_text: text(face, "oracle_text"),
                        power: optional(face, "power"),
                        toughness: optional(face, "toughness"),
                        loyalty: optional(face, "loyalty"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let visual_faces: Vec<Value> = if let Some(physical_faces) = card["card_faces"].as_array() {
            physical_faces.iter().zip(&faces).map(|(face, parsed)| json!({
                "id": parsed.id, "name": parsed.name, "oracleText": parsed.oracle_text,
                "typeLine": parsed.type_line, "imageUrl": image(face).or_else(|| image(&card)),
            })).collect()
        } else {
            vec![json!({
                "id": Value::Null, "name": card["name"],
                "oracleText": card["oracle_text"].as_str().unwrap_or_default(),
                "typeLine": card["type_line"], "imageUrl": image(&card),
            })]
        };
        let request = OracleCardParseRequest {
            card_name: text(&card, "name"),
            type_line: text(&card, "type_line"),
            mana_cost: optional(&card, "mana_cost"),
            oracle_text: optional(&card, "oracle_text"),
            layout: optional(&card, "layout"),
            faces,
        };
        let parsed = serde_json::to_value(parse_oracle_card_native_traced(request))
            .expect("traced result serializes");
        serde_json::to_writer(
            &mut output,
            &json!({
                "id": card["oracle_id"], "name": card["name"], "set": card["set"],
                "collector": card["collector_number"], "layout": card["layout"],
                "faces": visual_faces, "sourceUrl": card["scryfall_uri"],
                "result": parsed["result"], "trace": parsed["trace"],
            }),
        )
        .unwrap();
        writeln!(&mut output).unwrap();
        count += 1;
    }
    output.flush().unwrap();
    eprintln!(
        "Traced export: {count} cards in {:.3}s; instrumentation enabled, not a speed benchmark",
        start.elapsed().as_secs_f64()
    );
}
