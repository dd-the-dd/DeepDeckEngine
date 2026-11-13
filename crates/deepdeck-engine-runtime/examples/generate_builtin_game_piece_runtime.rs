fn main() -> Result<(), String> {
    let artifact = deepdeck_engine_runtime::game_piece_artifact::generate_artifact()?;
    let mut rendered = serde_json::to_string_pretty(&artifact)
        .map_err(|error| format!("could not serialize artifact: {error}"))?;
    rendered.push('\n');
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(&path, rendered)
            .map_err(|error| format!("could not write {path}: {error}"))?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}
