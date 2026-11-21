//! Pure casting-query reads. This context never fabricates a stack object and
//! never performs a choice, a payment or a zone transition.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct CastingQueryContext<'a> {
    pub spell: &'a CardDefinition,
    pub caster: &'a str,
    pub casting_source: Option<&'a CardInstance>,
    pub source: Option<&'a CardInstance>,
    pub actor: &'a str,
    pub decisions: &'a BTreeMap<String, Value>,
    depth: usize,
}
impl<'a> CastingQueryContext<'a> {
    pub(super) fn spell(
        spell: &'a CardDefinition,
        caster: &'a str,
        decisions: &'a BTreeMap<String, Value>,
        casting_source: Option<&'a CardInstance>,
    ) -> Self {
        Self {
            spell,
            caster,
            casting_source,
            source: casting_source,
            actor: caster,
            decisions,
            depth: 0,
        }
    }
    pub(super) fn modifier<'b>(&'b self, source: &'b CardInstance) -> CastingQueryContext<'b> {
        CastingQueryContext {
            spell: self.spell,
            caster: self.caster,
            casting_source: self.casting_source,
            source: Some(source),
            actor: &source.controller,
            decisions: self.decisions,
            depth: self.depth,
        }
    }
    fn nested(self) -> Result<Self, EngineError> {
        if self.depth >= 64 {
            return Err(EngineError::new(
                "cost query nesting exceeds supported depth",
            ));
        }
        Ok(Self {
            depth: self.depth + 1,
            ..self
        })
    }
    pub(super) fn player_context(self) -> player_query::PlayerQueryContext<'a> {
        player_query::PlayerQueryContext::source(self.actor, self.source)
    }
}

impl GameEngine {
    pub(super) fn checked_cost_query_numeric(
        &self,
        expression: &Value,
        context: &CastingQueryContext<'_>,
        field: &'static str,
    ) -> Result<i32, EngineError> {
        self.player_index(context.actor)?;
        self.player_index(context.caster)?;
        let nested = context.nested()?;
        let mut failure = None;
        let result = numeric_evaluation::evaluate_checked_numeric_tree(
            expression,
            &mut |leaf| {
                let read = (|| -> Result<i32, EngineError> {
                    if let Some(value) = integer_value(leaf) {
                        return Ok(value);
                    }
                    match value_kind(leaf) {
                    Some("decisionResult")=>leaf["decisionId"].as_str().and_then(|id|context.decisions.get(id)).and_then(integer_value)
                        .ok_or_else(||EngineError::new(format!("unsupported cost query numeric expression for {field}: unresolved numeric binding"))),
                    Some("countObjects")=> {
                        let values=self.resolve_collection_in_context(&leaf["objects"],runtime_collections::CollectionReadContext::Casting(&nested),&BTreeMap::new(),context.decisions)?;
                        match values {runtime_collections::RuntimeCollectionResolution::Complete(values)=>i32::try_from(values.len()).map_err(|_|EngineError::new("cost query count exceeds i32")),_=>Err(EngineError::new("cost query requires complete collection information"))}
                    }
                    Some("countPermanents")=> {
                        if leaf.get("ownership").is_some() {return Err(EngineError::new("unsupported checked cost count ownership profile"));}
                        let mut objects=json!({"kind":"eachPermanent","where":leaf["where"]});
                        if leaf["allPlayers"].as_bool()==Some(true){objects["players"]=json!({"kind":"eachPlayer"});}
                        else {objects["player"]=leaf.get("player").cloned().unwrap_or_else(||json!({"kind":"abilityController"}));}
                        if let Some(exclude)=leaf.get("excludeSource"){objects["excludeSource"]=exclude.clone();}
                        self.checked_cost_query_numeric(&json!({"kind":"countObjects","objects":objects}),&nested,field)
                    }
                    Some("countCards")=> {
                        let mut zone=leaf["zone"].clone();
                        if leaf["allPlayers"].as_bool()==Some(true){zone.as_object_mut().ok_or_else(||EngineError::new("invalid cost count zone"))?.remove("player");zone["players"]=json!({"kind":"eachPlayer"});}
                        self.checked_cost_query_numeric(&json!({"kind":"countObjects","objects":{"kind":"cardsInZone","zone":zone,"where":leaf["where"]}}),&nested,field)
                    }
                    // Existing noncollection leaves retain their established
                    // Option resolver, whose failure is an explicit error.
                    _ if numeric_expression_supported(leaf)=>self.numeric_expression_with_source(leaf,context.actor,context.source)
                        .ok_or_else(||EngineError::new(format!("unsupported cost query numeric expression for {field}: unresolved ordinary leaf"))),
                    _=>Err(EngineError::new(format!("unsupported cost query numeric expression for {field}"))),
                }
                })();
                match read {
                    Ok(value) => Some(value),
                    Err(error) => {
                        if failure.is_none() {
                            failure = Some(error);
                        }
                        None
                    }
                }
            },
            &mut |condition| {
                condition_supported(condition).then(|| {
                    self.condition_value_with_source(
                        condition,
                        context.actor,
                        context.decisions,
                        context.source,
                    )
                })
            },
        );
        if let Some(error) = failure {
            return Err(error);
        }
        result.ok_or_else(|| {
            EngineError::new(format!(
                "unsupported cost query numeric expression for {field}: arithmetic or condition"
            ))
        })
    }

    pub(super) fn prepare_cost_query_filter(
        &self,
        filter: &Value,
        context: &CastingQueryContext<'_>,
        permanent: Option<&CardInstance>,
        permanent_domain: bool,
    ) -> Result<Value, EngineError> {
        let validated = object_predicate::try_map_predicate_leaves(filter, 0, &mut |leaf| {
            let mut prepared = leaf.clone();
            if matches!(
                value_kind(leaf),
                Some("chosenCreatureType" | "chosenCardType")
            ) {
                let id = leaf["decisionId"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("cost query: missing chosen type producer"))?;
                let source_definition = context
                    .source
                    .map_or(context.spell, |source| &source.definition);
                let value = stored_decision_value(source_definition, id).ok_or_else(|| {
                    EngineError::new("cost query: unresolved chosen type producer")
                })?;
                prepared = json!({"kind":if value_kind(leaf)==Some("chosenCreatureType"){"subtypeContains"}else{"cardTypeContains"},"value":value});
            }
            let applicable = if permanent_domain {
                permanent_filter_supported(&prepared)
            } else {
                object_predicate::definition_atom_supported(&prepared)
            };
            if !applicable {
                return Err(EngineError::new(
                    "unsupported checked cost query predicate for this object domain",
                ));
            }
            // Candidate color count (e.g. "colorless spells", CR 105.2c) is a
            // definition/effective characteristic read like mana value.
            if value_kind(&prepared) == Some("compare")
                && value_kind(&prepared["left"]) != Some("manaValueOf")
                && !(value_kind(&prepared["left"]) == Some("colorCountOf")
                    && value_kind(&prepared["left"]["object"]) == Some("candidate"))
            {
                return Err(EngineError::new(
                    "unsupported checked cost query numeric property",
                ));
            }
            Ok(prepared)
        })?;
        self.prepare_casting_query_filter(&validated, permanent, permanent_domain, context)
    }

    pub(super) fn cost_query_filter_matches(
        &self,
        view: runtime_object_filter::ObjectFilterView<'_>,
        filter: &Value,
        context: &CastingQueryContext<'_>,
    ) -> Result<bool, EngineError> {
        object_predicate::try_evaluate_predicate(filter, &mut |leaf| {
            if value_kind(leaf) == Some("compare")
                && value_kind(&leaf["left"]) == Some("manaValueOf")
            {
                let right = integer_value(&leaf["right"]).ok_or_else(|| {
                    EngineError::new("cost query: comparison RHS was not prepared")
                })?;
                return Ok(object_predicate::compare_integers(
                    mana_value(view.definition()),
                    leaf["operator"].as_str(),
                    right,
                ));
            }
            Ok(match view {
                runtime_object_filter::ObjectFilterView::Definition(definition) => {
                    object_predicate::matches_object_atom(definition, leaf)
                }
                runtime_object_filter::ObjectFilterView::Permanent(permanent) => {
                    self.permanent_matches_effective_filter(permanent, leaf)
                }
            })
        })
    }

    /// The spell has already logically left its source zone when its total
    /// cost is fixed. Only the exact admitted incarnation is excluded.
    pub(super) fn casting_query_excluded_instance(
        &self,
        context: &CastingQueryContext<'_>,
        filter: &Value,
        permanent_domain: bool,
    ) -> Result<Option<String>, EngineError> {
        if let Some(source) = context.casting_source {
            let captured = object_identity::object_identity_of(source)?;
            let actual = self
                .prepare_card_movement_source(&source.instance_id, None)?
                .ok_or_else(|| {
                    EngineError::new(
                        "cost query: casting source is not present in the current world",
                    )
                })?;
            if actual.identity != captured {
                return Err(EngineError::new(
                    "cost query: casting source incarnation is not current",
                ));
            }
            return Ok(Some(source.instance_id.clone()));
        }
        // A definition-only discovery is sufficient ONLY when the proven
        // predicate excludes this spell. Unsupported properties must not be
        // used as a false witness to waive the identity requirement.
        let supports_definition = object_predicate::try_map_predicate_leaves(
            filter,
            0,
            &mut |leaf| {
                if !object_predicate::definition_atom_supported(leaf) {
                    return Err(EngineError::new(
                        "cost query: casting source identity is required for state-dependent filtering",
                    ));
                }
                Ok(leaf.clone())
            },
        )?;
        let matches = self.cost_query_filter_matches(
            runtime_object_filter::ObjectFilterView::Definition(
                object_predicate::DefinitionView::new(context.spell),
            ),
            &supports_definition,
            context,
        )?;
        if matches {
            return Err(EngineError::new(
                "cost query: casting source identity is required",
            ));
        }
        let _ = permanent_domain;
        Ok(None)
    }

    pub(super) fn checked_global_casting_cost_reduction(
        &self,
        player_id: &str,
        card: &CardDefinition,
        source_zone: &str,
        decisions: &BTreeMap<String, Value>,
        casting_source: Option<&CardInstance>,
    ) -> Result<i32, EngineError> {
        let base = CastingQueryContext::spell(card, player_id, decisions, casting_source);
        let mut modifiers = self.battlefield_static_modifiers();
        modifiers.extend(self.eminence_static_modifiers());
        let mut total = 0_i32;
        for (source, modifier) in modifiers {
            if value_kind(&modifier) != Some("reduceCastingCost") {
                continue;
            }
            let context = base.modifier(&source);
            let players = context
                .player_context()
                .resolve_population(self, &modifier["player"])?;
            if !players.iter().any(|id| id == player_id) {
                continue;
            }
            if modifier["duringOtherPlayersTurns"].as_bool() == Some(true)
                && self.state.players[self.state.active_player].id == context.actor
            {
                continue;
            }
            if modifier["sourceZone"]
                .as_str()
                .is_some_and(|z| z != source_zone)
                || modifier["sourceZoneNot"]
                    .as_str()
                    .is_some_and(|z| z == source_zone)
            {
                continue;
            }
            if modifier["excludeSource"].as_bool() == Some(true) {
                match casting_source {
                    Some(casting) if casting.instance_id == source.instance_id => continue,
                    None if source.definition.id == card.id => {
                        return Err(EngineError::new(
                            "cost query: exact casting source is required for source exclusion",
                        ));
                    }
                    _ => {}
                }
            }
            if modifier.get("condition").is_some_and(|condition| {
                !self.condition_value_with_source(
                    condition,
                    context.actor,
                    &BTreeMap::new(),
                    Some(&source),
                )
            }) {
                continue;
            }
            if modifier["firstEachTurn"].as_bool() == Some(true) && context.actor != player_id {
                return Err(EngineError::new(
                    "unsupported cross-player first-spell reduction scope",
                ));
            }
            if modifier["firstEachTurn"].as_bool() == Some(true)
                && self.state.events.iter().any(|event| {
                    event.turn_number == self.state.turn_number
                        && event.kind == "spellCast"
                        && event.player_id.as_deref() == Some(player_id)
                        && spell_cast_event_matches_filter(event, &modifier["where"])
                })
            {
                continue;
            }
            let filter =
                self.prepare_cost_query_filter(&modifier["where"], &context, None, false)?;
            if !self.cost_query_filter_matches(
                runtime_object_filter::ObjectFilterView::Definition(
                    object_predicate::DefinitionView::new(card),
                ),
                &filter,
                &context,
            )? {
                continue;
            }
            let amount = self.checked_cost_query_numeric(
                &modifier["amount"],
                &context,
                "casting reduction amount",
            )?;
            total = total
                .checked_add(amount)
                .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))?;
        }
        for modifier in &self.state.rule_modifiers {
            if value_kind(modifier) != Some("temporaryCastingCostReduction")
                || modifier["playerId"].as_str() != Some(player_id)
                || !modifier["expiresAfterTurn"]
                    .as_u64()
                    .is_some_and(|turn| turn >= u64::from(self.state.turn_number))
            {
                continue;
            }
            let filter = self.prepare_cost_query_filter(&modifier["where"], &base, None, false)?;
            if self.cost_query_filter_matches(
                runtime_object_filter::ObjectFilterView::Definition(
                    object_predicate::DefinitionView::new(card),
                ),
                &filter,
                &base,
            )? {
                let amount = self.checked_cost_query_numeric(
                    &modifier["amount"],
                    &base,
                    "temporary casting reduction amount",
                )?;
                total = total
                    .checked_add(amount)
                    .ok_or_else(|| EngineError::new("casting reduction total exceeds i32"))?;
            }
        }
        Ok(total.max(0))
    }

    #[cfg(test)]
    pub(super) fn global_casting_cost_reduction(
        &self,
        player_id: &str,
        card: &CardDefinition,
        source_zone: &str,
    ) -> i32 {
        self.checked_global_casting_cost_reduction(
            player_id,
            card,
            source_zone,
            &BTreeMap::new(),
            None,
        )
        .expect("historical cost fixture must have a supported checked query")
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "cost_query_tests.rs"]
mod tests;
