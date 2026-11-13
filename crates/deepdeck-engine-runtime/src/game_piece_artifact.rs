use crate::engine::CardDefinition;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: &str = "deepdeck-game-piece-runtime/v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePieceRuntimeArtifact {
    pub schema_version: String,
    pub compiler: String,
    pub definitions: BTreeMap<String, CardDefinition>,
}

pub fn embedded_artifact() -> Result<GamePieceRuntimeArtifact, String> {
    let artifact: GamePieceRuntimeArtifact = serde_json::from_str(include_str!(
        "../generated/builtin-game-pieces-runtime.json"
    ))
    .map_err(|error| format!("invalid generated game-piece runtime artifact: {error}"))?;
    if artifact.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported game-piece runtime artifact {}; expected {SCHEMA_VERSION}",
            artifact.schema_version
        ));
    }
    if artifact.compiler != "named-token-native/v1" {
        return Err(format!(
            "unsupported game-piece compiler provenance {}; expected named-token-native/v1",
            artifact.compiler
        ));
    }
    Ok(artifact)
}

#[cfg(feature = "oracle-compilation")]
pub fn generate_artifact() -> Result<GamePieceRuntimeArtifact, String> {
    use crate::card_catalog::{builtin_named_token_names, builtin_named_token_printing};
    use crate::oracle::compile_named_token_native_rule;

    let mut definitions = BTreeMap::new();
    for name in builtin_named_token_names() {
        let printing = builtin_named_token_printing(name)
            .ok_or_else(|| format!("missing built-in named token {name}"))?;
        let oracle = printing
            .oracle_text
            .as_deref()
            .ok_or_else(|| format!("built-in named token {name} has no Oracle text"))?;
        let rule = compile_named_token_native_rule(oracle).ok_or_else(|| {
            format!("built-in named token {name} is not covered by the native token compiler")
        })?;
        if !crate::engine::rule_is_executable(&rule) {
            return Err(format!(
                "native rule for built-in named token {name} is not executable"
            ));
        }
        let definition = CardDefinition {
            id: format!("token:{name}"),
            name: name.to_string(),
            type_line: printing
                .type_line
                .clone()
                .unwrap_or_else(|| format!("Token - {name}")),
            is_commander: false,
            is_token: true,
            is_game_piece: true,
            is_sideboard: false,
            mana_cost: printing.mana_cost.clone().unwrap_or_default(),
            power: printing.power.clone(),
            toughness: printing.toughness.clone(),
            rules: vec![rule],
        };
        definitions.insert(name.to_string(), definition);
    }
    Ok(GamePieceRuntimeArtifact {
        schema_version: SCHEMA_VERSION.to_string(),
        compiler: "named-token-native/v1".to_string(),
        definitions,
    })
}
