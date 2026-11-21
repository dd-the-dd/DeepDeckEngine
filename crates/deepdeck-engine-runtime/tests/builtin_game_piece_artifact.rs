use deepdeck_engine_runtime::game_piece_artifact::{embedded_artifact, generate_artifact};

#[test]
fn generated_builtin_game_piece_runtime_artifact_is_current() {
    let embedded = embedded_artifact().expect("embedded artifact");
    let generated = generate_artifact().expect("regenerated artifact");
    assert_eq!(
        embedded, generated,
        "generated/builtin-game-pieces-runtime.json is stale; regenerate it with the runtime example"
    );
}
