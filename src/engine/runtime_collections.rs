//! One fallible collection algebra, including target applicability.
use super::*;

pub(super) enum RuntimeCollectionResolution {
    Complete(Vec<String>),
    Partial(Vec<String>),
    Unavailable,
}

#[derive(Clone, Copy)]
pub(super) enum CollectionReadContext<'a> {
    Runtime(&'a StackObject),
    Casting(&'a cost_query::CastingQueryContext<'a>),
}
impl<'a> CollectionReadContext<'a> {
    fn runtime_source(self) -> Result<&'a StackObject, EngineError> {
        match self {
            Self::Runtime(source) => Ok(source),
            Self::Casting(_) => Err(EngineError::new(
                "unsupported casting collection binding or atom",
            )),
        }
    }
    fn players(
        self,
        bindings: &'a BTreeMap<String, RuntimeBinding>,
    ) -> player_query::PlayerQueryContext<'a> {
        match self {
            Self::Runtime(source) => player_query::PlayerQueryContext::runtime(source, bindings),
            Self::Casting(context) => context.player_context(),
        }
    }
}

impl GameEngine {
    pub(super) fn resolve_runtime_object_ids_checked(
        &self,
        expression: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Vec<String>, EngineError> {
        match self.resolve_runtime_collection_status(expression, source, bindings, decisions)? {
            RuntimeCollectionResolution::Complete(objects) => Ok(objects),
            _ => Err(EngineError::new(
                "complete collection information requires a target-aware consumer",
            )),
        }
    }

    /// An affected-set action can still apply independently identified union
    /// members. An unavailable dependency is not an empty collection.
    pub(super) fn resolve_runtime_object_ids_for_effect(
        &self,
        expression: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Option<Vec<String>>, EngineError> {
        Ok(
            match self.resolve_runtime_collection_status(expression, source, bindings, decisions)? {
                RuntimeCollectionResolution::Complete(objects)
                | RuntimeCollectionResolution::Partial(objects) => Some(objects),
                RuntimeCollectionResolution::Unavailable => None,
            },
        )
    }

    pub(super) fn resolve_runtime_collection_status(
        &self,
        expression: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<RuntimeCollectionResolution, EngineError> {
        self.resolve_collection_in_context(
            expression,
            CollectionReadContext::Runtime(source),
            bindings,
            decisions,
        )
    }

    pub(super) fn resolve_collection_in_context(
        &self,
        expression: &Value,
        context: CollectionReadContext<'_>,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<RuntimeCollectionResolution, EngineError> {
        self.resolve_runtime_collection_inner(expression, context, bindings, decisions, 0)
    }

    fn resolve_runtime_collection_inner(
        &self,
        expression: &Value,
        source: CollectionReadContext<'_>,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        depth: usize,
    ) -> Result<RuntimeCollectionResolution, EngineError> {
        use RuntimeCollectionResolution::{Complete, Partial, Unavailable};
        if depth >= 64 {
            return Err(EngineError::new(
                "runtime collection nesting exceeds supported depth",
            ));
        }
        let child = |value: &Value| {
            self.resolve_runtime_collection_inner(value, source, bindings, decisions, depth + 1)
        };
        if matches!(source, CollectionReadContext::Casting(_))
            && !matches!(
                value_kind(expression),
                Some("union" | "setDifference" | "filterObjects" | "cardsInZone" | "eachPermanent")
            )
        {
            return Err(EngineError::new(
                "unsupported casting collection binding or atom",
            ));
        }
        match value_kind(expression) {
            Some("chosenTargets") => {
                let source = source.runtime_source()?;
                let id = expression["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| EngineError::new("missing target collection identifier"))?;
                let members = source
                    .targets
                    .iter()
                    .filter(|(target_id, _)| {
                        target_legality::target_belongs_to_declaration(source, id, target_id)
                    })
                    .collect::<Vec<_>>();
                if members.is_empty() {
                    return if target_legality::declared_unselected_target_ids(source).contains(id) {
                        Ok(Complete(Vec::new()))
                    } else {
                        Err(EngineError::new(format!(
                            "unresolved target collection producer `{id}`"
                        )))
                    };
                }
                let mut objects = Vec::new();
                let mut partial = false;
                for (target_id, target) in members {
                    let legal_at_start = self
                        .target_was_legal_at_resolution_start(source, target_id)
                        .ok_or_else(|| {
                            EngineError::new(format!(
                                "missing resolution-start target legality for `{target_id}`"
                            ))
                        })?;
                    // Legality is checked once at resolution start. Later steps
                    // still require the same existing object, not a new target
                    // selected by re-running characteristic/controller filters.
                    if !legal_at_start
                        || !self.target_incarnation_is_current(source, target_id, target)
                        || !self.target_exists(target)
                    {
                        partial = true;
                        continue;
                    }
                    let object_id = match target {
                        TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                            instance_id
                        }
                        TargetRef::StackObject { stack_id } => stack_id,
                        TargetRef::Player { .. } => {
                            return Err(EngineError::new(
                                "a player target is not an object collection member",
                            ));
                        }
                    };
                    if !objects.contains(object_id) {
                        objects.push(object_id.clone());
                    }
                }
                Ok(if partial {
                    Partial(objects)
                } else {
                    Complete(objects)
                })
            }
            Some("boundObjects") => {
                let id = expression["binding"].as_str().ok_or_else(|| {
                    EngineError::new("missing runtime collection binding identifier")
                })?;
                match bindings.get(id) {
                    Some(RuntimeBinding::Objects(objects)) => Ok(Complete(objects.clone())),
                    Some(_) => Err(EngineError::new(format!(
                        "runtime collection binding `{id}` has the wrong type"
                    ))),
                    None => Err(EngineError::new(format!(
                        "unresolved runtime collection binding `{id}`"
                    ))),
                }
            }
            Some("decisionResult") => {
                let id = expression["decisionId"].as_str().ok_or_else(|| {
                    EngineError::new("missing runtime collection decision identifier")
                })?;
                let values = decisions.get(id).and_then(Value::as_array).ok_or_else(|| {
                    EngineError::new(format!("unresolved runtime collection decision `{id}`"))
                })?;
                values
                    .iter()
                    .map(|value| {
                        value.as_str().map(str::to_owned).ok_or_else(|| {
                            EngineError::new(format!(
                                "runtime collection decision `{id}` contains a non-object value"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(Complete)
            }
            Some("union") => {
                let sets = expression["sets"]
                    .as_array()
                    .ok_or_else(|| EngineError::new("invalid runtime collection union"))?;
                let mut objects = Vec::new();
                let mut partial = false;
                let mut identified = sets.is_empty();
                for set in sets {
                    let values = match child(set)? {
                        Complete(values) => {
                            identified = true;
                            values
                        }
                        Partial(values) => {
                            identified = true;
                            partial = true;
                            values
                        }
                        Unavailable => {
                            partial = true;
                            continue;
                        }
                    };
                    for id in values {
                        if !objects.contains(&id) {
                            objects.push(id)
                        }
                    }
                }
                Ok(if !identified {
                    Unavailable
                } else if partial {
                    Partial(objects)
                } else {
                    Complete(objects)
                })
            }
            Some("setDifference") => {
                let left = child(&expression["left"])?;
                let right = child(&expression["right"])?;
                let right = match right {
                    Complete(values) => values.into_iter().collect::<BTreeSet<_>>(),
                    _ => return Ok(Unavailable),
                };
                Ok(match left {
                    Complete(values) => Complete(
                        values
                            .into_iter()
                            .filter(|id| !right.contains(id))
                            .collect(),
                    ),
                    Partial(values) => Partial(
                        values
                            .into_iter()
                            .filter(|id| !right.contains(id))
                            .collect(),
                    ),
                    Unavailable => Unavailable,
                })
            }
            Some("filterObjects") => {
                let objects = child(&expression["objects"])?;
                // Unknown membership from an unavailable owner does not become
                // zero. Valid empty collections still prepare their predicate.
                if matches!(&objects, Unavailable) {
                    return Ok(Unavailable);
                }
                if let CollectionReadContext::Runtime(stack) = source {
                    if self.value_requires_initially_illegal_target_in_context(
                        &expression["where"],
                        stack,
                        bindings,
                        decisions,
                    )? {
                        return Ok(Unavailable);
                    }
                }
                let predicate = self.prepare_collection_query_filter(
                    &expression["where"],
                    source,
                    bindings,
                    decisions,
                    None,
                    false,
                )?;
                let (mut values, partial) = match objects {
                    Complete(v) => (v, false),
                    Partial(v) => (v, true),
                    Unavailable => return Ok(Unavailable),
                };
                let mut matched = Vec::new();
                for id in values {
                    if self.collection_query_object_matches(
                        &id, &predicate, source, bindings, decisions,
                    )? {
                        matched.push(id);
                    }
                }
                values = matched;
                Ok(if partial {
                    Partial(values)
                } else {
                    Complete(values)
                })
            }
            Some("cardsInZone" | "eachPermanent") => {
                if let CollectionReadContext::Runtime(stack) = source {
                    for reference in [
                        expression.get("player"),
                        expression.get("players"),
                        expression.get("zone").and_then(|zone| zone.get("player")),
                        expression.get("zone").and_then(|zone| zone.get("players")),
                        expression.get("where"),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if self.value_requires_initially_illegal_target_in_context(
                            reference, stack, bindings, decisions,
                        )? {
                            return Ok(Unavailable);
                        }
                    }
                }
                let excluded = if let Some(value) = expression.get("exclude") {
                    match child(value)? {
                        Complete(values) => values,
                        _ => return Ok(Unavailable),
                    }
                } else {
                    Vec::new()
                };
                self.resolve_shared_world_query_atom(
                    expression, source, bindings, decisions, &excluded,
                )
                .map(Complete)
            }
            _ => {
                let source = source.runtime_source()?;
                for reference in [
                    expression.get("player"),
                    expression.get("zone").and_then(|zone| zone.get("player")),
                ]
                .into_iter()
                .flatten()
                {
                    if self.value_requires_initially_illegal_target_in_context(
                        reference, source, bindings, decisions,
                    )? {
                        return Ok(Unavailable);
                    }
                    let player = self
                        .resolve_player(reference, source, bindings)
                        .ok_or_else(|| {
                            EngineError::new("unresolved runtime collection player reference")
                        })?;
                    self.player_index(&player)?;
                }
                let mut prepared = expression.clone();
                if let Some(predicate) = expression.get("where") {
                    if self.value_requires_initially_illegal_target_in_context(
                        predicate, source, bindings, decisions,
                    )? {
                        return Ok(Unavailable);
                    }
                    prepared["where"] =
                        self.prepare_runtime_object_filter(predicate, source, bindings, decisions)?;
                }
                if let Some(excluded) = expression.get("exclude") {
                    if !matches!(child(excluded)?, Complete(_)) {
                        return Ok(Unavailable);
                    }
                }
                self.resolve_runtime_object_ids_atom(&prepared, source, bindings, decisions)
                    .map(Complete)
                    .ok_or_else(|| {
                        EngineError::new(format!(
                            "unresolved or unsupported runtime collection: {}",
                            value_kind(expression).unwrap_or("missingKind")
                        ))
                    })
            }
        }
    }

    fn prepare_collection_query_filter(
        &self,
        filter: &Value,
        context: CollectionReadContext<'_>,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        permanent: Option<&CardInstance>,
        permanent_domain: bool,
    ) -> Result<Value, EngineError> {
        match context {
            CollectionReadContext::Runtime(source) => {
                if permanent_domain {
                    self.prepare_permanent_runtime_object_filter(
                        filter, permanent, source, bindings, decisions,
                    )
                } else {
                    self.prepare_runtime_object_filter(filter, source, bindings, decisions)
                }
            }
            CollectionReadContext::Casting(context) => {
                self.prepare_cost_query_filter(filter, context, permanent, permanent_domain)
            }
        }
    }
    fn collection_query_object_matches(
        &self,
        id: &str,
        filter: &Value,
        context: CollectionReadContext<'_>,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        let view = if let Some((p, c)) = self.permanent_position(id) {
            runtime_object_filter::ObjectFilterView::Permanent(
                &self.state.players[p].battlefield[c],
            )
        } else if let Some(card) = self.card_outside_battlefield(id) {
            runtime_object_filter::ObjectFilterView::Definition(
                object_predicate::DefinitionView::new(&card.definition),
            )
        } else {
            return match context {
                CollectionReadContext::Runtime(_) => Ok(false),
                CollectionReadContext::Casting(_) => {
                    Err(EngineError::new("cost query: collection object is absent"))
                }
            };
        };
        let filter = self.prepare_collection_query_filter(
            filter,
            context,
            bindings,
            decisions,
            match view {
                runtime_object_filter::ObjectFilterView::Permanent(card) => Some(card),
                _ => None,
            },
            matches!(view, runtime_object_filter::ObjectFilterView::Permanent(_)),
        )?;
        match context {
            CollectionReadContext::Runtime(source) => {
                Ok(self.runtime_object_filter_matches(view, &filter, source, bindings, decisions))
            }
            CollectionReadContext::Casting(context) => {
                self.cost_query_filter_matches(view, &filter, context)
            }
        }
    }

    /// Shared world enumeration. Holder populations are resolved completely
    /// before filtering, including in an empty world. Ownership is not holder.
    fn resolve_shared_world_query_atom(
        &self,
        expression: &Value,
        context: CollectionReadContext<'_>,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        excluded: &[String],
    ) -> Result<Vec<String>, EngineError> {
        let permanent_domain = value_kind(expression) == Some("eachPermanent");
        let players = context.players(bindings);
        let holders = if permanent_domain {
            match (expression.get("player"), expression.get("players")) {
                (Some(_), Some(_)) => {
                    return Err(EngineError::new(
                        "query.zone: player and players are mutually exclusive",
                    ));
                }
                (Some(value), None) | (None, Some(value)) => {
                    players.resolve_population(self, value)?
                }
                (None, None) => players.resolve_population(self, &json!({"kind":"eachPlayer"}))?,
            }
        } else {
            players.zone_holders(self, &expression["zone"])?
        };
        if !permanent_domain {
            object_predicate::try_map_predicate_leaves(&expression["where"], 0, &mut |leaf| {
                if !object_predicate::definition_atom_supported(leaf) {
                    return Err(EngineError::new(
                        "unsupported query predicate for a definition-only card domain",
                    ));
                }
                Ok(leaf.clone())
            })?;
        }
        let prepared = self.prepare_collection_query_filter(
            &expression["where"],
            context,
            bindings,
            decisions,
            None,
            permanent_domain,
        )?;
        let casting_exclusion = match context {
            CollectionReadContext::Runtime(_) => None,
            CollectionReadContext::Casting(context) => {
                self.casting_query_excluded_instance(context, &prepared, permanent_domain)?
            }
        };
        let (source_id, actor) = match context {
            CollectionReadContext::Runtime(source) => (
                Some(source.card.instance_id.as_str()),
                source.controller.as_str(),
            ),
            CollectionReadContext::Casting(context) => (
                context.source.map(|card| card.instance_id.as_str()),
                context.actor,
            ),
        };
        let mut candidates = Vec::new();
        let mut seen = BTreeSet::new();
        for holder in holders {
            let player_index = self.player_index(&holder)?;
            let player = &self.state.players[player_index];
            if permanent_domain
                && expression["excludeSourceController"].as_bool() == Some(true)
                && player.id == actor
            {
                continue;
            }
            let cards = if permanent_domain {
                &player.battlefield
            } else {
                match value_kind(&expression["zone"]) {
                    Some("graveyard") => &player.graveyard,
                    Some("hand") => &player.hand,
                    Some("exile") => &player.exile,
                    _ => return Err(EngineError::new("unsupported checked query card zone")),
                }
            };
            for card in cards {
                if permanent_domain && card.flags.get("phasedOut").copied().unwrap_or(false) {
                    continue;
                }
                if !permanent_domain && (card.definition.is_token || card.definition.is_game_piece)
                {
                    continue;
                }
                if !seen.insert(card.instance_id.clone()) {
                    return Err(EngineError::new(
                        "query: ambiguous physical object identity",
                    ));
                }
                if casting_exclusion.as_deref() == Some(card.instance_id.as_str())
                    || excluded.contains(&card.instance_id)
                    || expression["excludeSource"].as_bool() == Some(true)
                        && source_id == Some(card.instance_id.as_str())
                {
                    continue;
                }
                candidates.push(card.instance_id.clone());
            }
        }
        let mut result = Vec::new();
        for id in candidates {
            if self.collection_query_object_matches(&id, &prepared, context, bindings, decisions)? {
                result.push(id);
            }
        }
        Ok(result)
    }

    pub(super) fn checked_runtime_object_count(
        &self,
        expression: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<i32, EngineError> {
        let objects =
            self.resolve_runtime_object_ids_checked(expression, source, bindings, decisions)?;
        i32::try_from(objects.len())
            .map_err(|_| EngineError::new("runtime collection count exceeds i32"))
    }
}
