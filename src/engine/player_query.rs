//! Checked player references/populations for shared query readers.
//! Zone holder, card owner, physical source controller and ability actor are
//! separate facts. A permanent target never becomes a player by coercion.
use super::*;

pub(super) fn reference_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController" | "controller" | "triggeringPlayer") => true,
        Some("controllerOf" | "controllerOfObject" | "ownerOf") => {
            player_reference_supported(reference)
        }
        Some("boundValue" | "chosenTarget") => {
            reference["id"].as_str().is_some_and(|id| !id.is_empty())
        }
        _ => false,
    }
}
pub(super) fn population_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("eachPlayer") => true,
        Some("opponentsOf" | "playersExcept") => reference_supported(&reference["player"]),
        _ => reference_supported(reference),
    }
}
pub(super) fn zone_holders_supported(zone: &Value) -> bool {
    match (zone.get("player"), zone.get("players")) {
        (Some(reference), None) => reference_supported(reference),
        (None, Some(reference)) => population_supported(reference),
        _ => false,
    }
}

#[derive(Clone, Copy)]
pub(super) struct PlayerQueryContext<'a> {
    ability_actor: &'a str,
    physical_source: Option<&'a CardInstance>,
    runtime: Option<(&'a StackObject, &'a BTreeMap<String, RuntimeBinding>)>,
}
impl<'a> PlayerQueryContext<'a> {
    pub(super) fn runtime(
        source: &'a StackObject,
        bindings: &'a BTreeMap<String, RuntimeBinding>,
    ) -> Self {
        Self {
            ability_actor: &source.controller,
            physical_source: Some(&source.card),
            runtime: Some((source, bindings)),
        }
    }
    pub(super) fn source(
        ability_actor: &'a str,
        physical_source: Option<&'a CardInstance>,
    ) -> Self {
        Self {
            ability_actor,
            physical_source,
            runtime: None,
        }
    }
    pub(super) fn resolve_ref(
        self,
        engine: &GameEngine,
        reference: &Value,
    ) -> Result<String, EngineError> {
        let resolved = match value_kind(reference) {
            Some("abilityController" | "controller") => self.ability_actor.to_string(),
            Some("controllerOf")
                if matches!(
                    value_kind(&reference["object"]),
                    Some("self" | "abilitySource")
                ) =>
            {
                self.ability_actor.to_string()
            }
            Some("controllerOfObject")
                if matches!(
                    value_kind(&reference["object"]),
                    Some("self" | "abilitySource")
                ) =>
            {
                if let Some((source, bindings)) = self.runtime {
                    engine
                        .resolved_object_controller(&reference["object"], source, bindings)
                        .ok_or_else(|| {
                            EngineError::new("query.player: unresolved physical source controller")
                        })?
                } else {
                    self.physical_source
                        .ok_or_else(|| {
                            EngineError::new("query.player: missing physical source controller")
                        })?
                        .controller
                        .clone()
                }
            }
            Some("ownerOf")
                if matches!(
                    value_kind(&reference["object"]),
                    Some("self" | "abilitySource")
                ) =>
            {
                self.physical_source
                    .ok_or_else(|| EngineError::new("query.player: missing physical source owner"))?
                    .owner
                    .clone()
            }
            Some("controllerOf" | "controllerOfObject" | "ownerOf") => {
                let (source,bindings)=self.runtime.ok_or_else(||EngineError::new("query.player: object relation requires an explicit runtime binding context"))?;
                if !player_reference_supported(reference) {
                    return Err(EngineError::new(
                        "query.player: unsupported object relation",
                    ));
                }
                // An explicit relationship operation is not coercion of a
                // direct Player binding. Keep the established object service.
                engine
                    .resolve_player(reference, source, bindings)
                    .ok_or_else(|| EngineError::new("query.player: unresolved object relation"))?
            }
            Some("triggeringPlayer") => {
                let (source, bindings) = self.runtime.ok_or_else(|| {
                    EngineError::new("query.player: missing observed player context")
                })?;
                engine
                    .resolve_player(reference, source, bindings)
                    .ok_or_else(|| EngineError::new("query.player: unresolved observed player"))?
            }
            Some("boundValue") => {
                let id = reference["id"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("query.player: missing binding identifier"))?;
                let (_, bindings) = self
                    .runtime
                    .ok_or_else(|| EngineError::new("query.player: unresolved binding"))?;
                match bindings.get(id) {
                    Some(RuntimeBinding::Player(id))
                    | Some(RuntimeBinding::Target(TargetRef::Player { player_id: id })) => {
                        id.clone()
                    }
                    Some(_) => {
                        return Err(EngineError::new("query.player: binding is not a player"));
                    }
                    None => return Err(EngineError::new("query.player: unresolved binding")),
                }
            }
            Some("chosenTarget") => {
                let id = reference["id"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("query.player: missing target identifier"))?;
                let (source, _) = self
                    .runtime
                    .ok_or_else(|| EngineError::new("query.player: unresolved target binding"))?;
                let target = source
                    .targets
                    .get(id)
                    .ok_or_else(|| EngineError::new("query.player: unresolved target binding"))?;
                let TargetRef::Player { player_id } = target else {
                    return Err(EngineError::new(
                        "query.player: target binding is not a player",
                    ));
                };
                // Existing target declaration/longest-binding service remains
                // authoritative. This role intentionally requires a player-only
                // producer; mixed target domains are not silently narrowed.
                let has_producer = source
                    .ability_rule
                    .as_ref()
                    .map_or_else(
                        || {
                            source
                                .card
                                .definition
                                .printed_rules()
                                .iter()
                                .filter(|r| value_kind(r) == Some("spellAbility"))
                                .collect::<Vec<_>>()
                        },
                        |rule| vec![rule],
                    )
                    .iter()
                    .any(|rule| {
                        rule["declaration"]["decisions"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|decision| {
                                value_kind(decision) == Some("chooseTargets")
                                    && value_kind(&decision["candidates"]) == Some("players")
                                    && decision["id"].as_str().is_some_and(|declaration_id| {
                                        target_legality::target_belongs_to_declaration(
                                            source,
                                            declaration_id,
                                            id,
                                        )
                                    })
                            })
                    });
                if !has_producer {
                    return Err(EngineError::new(
                        "query.player: missing declared player producer",
                    ));
                }
                match engine.target_was_legal_at_resolution_start(source, id) {
                    Some(true) => {}
                    Some(false) => {
                        return Err(EngineError::new(
                            "query.player: target is unavailable at resolution start",
                        ));
                    }
                    None => {
                        return Err(EngineError::new(
                            "query.player: missing target resolution context",
                        ));
                    }
                }
                player_id.clone()
            }
            // In particular, random choice, a bare graveyard target and a
            // fabricated player string are not player-reference producers.
            _ => {
                return Err(EngineError::new(
                    "query.player: unsupported checked player reference",
                ));
            }
        };
        engine.player_index(&resolved)?;
        Ok(resolved)
    }
    pub(super) fn resolve_population(
        self,
        engine: &GameEngine,
        reference: &Value,
    ) -> Result<Vec<String>, EngineError> {
        match value_kind(reference) {
            Some("eachPlayer") => Ok(engine
                .state
                .players
                .iter()
                .filter(|p| !p.has_lost)
                .map(|p| p.id.clone())
                .collect()),
            Some("opponentsOf" | "playersExcept") => {
                let except = self.resolve_ref(engine, &reference["player"])?;
                Ok(engine
                    .state
                    .players
                    .iter()
                    .filter(|p| !p.has_lost && p.id != except)
                    .map(|p| p.id.clone())
                    .collect())
            }
            _ => self.resolve_ref(engine, reference).map(|id| vec![id]),
        }
    }
    pub(super) fn zone_holders(
        self,
        engine: &GameEngine,
        zone: &Value,
    ) -> Result<Vec<String>, EngineError> {
        match (zone.get("player"), zone.get("players")) {
            (Some(_), Some(_)) => Err(EngineError::new(
                "query.zone: player and players are mutually exclusive",
            )),
            (Some(reference), None) => self.resolve_ref(engine, reference).map(|id| vec![id]),
            (None, Some(reference)) => self.resolve_population(engine, reference),
            (None, None) => Err(EngineError::new("query.zone: missing holder reference")),
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "player_query_tests.rs"]
mod tests;
