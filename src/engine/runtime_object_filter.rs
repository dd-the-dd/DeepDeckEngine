//! Runtime filter context. Logical/numeric grammar is shared; a battlefield
//! permanent keeps its effective characteristics rather than becoming a
//! definition-only candidate during collection selection.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum ObjectFilterView<'a> {
    Definition(object_predicate::DefinitionView<'a>),
    Permanent(&'a CardInstance),
}
impl ObjectFilterView<'_> {
    pub(super) fn definition(&self) -> &CardDefinition {
        match self {
            Self::Definition(view) => view.definition(),
            Self::Permanent(card) => &card.definition,
        }
    }
}
#[derive(Clone, Copy)]
enum FilterPreparationContext<'a> {
    Runtime(&'a StackObject),
    StaticSourceColors,
    Casting(&'a cost_query::CastingQueryContext<'a>),
}

impl GameEngine {
    /// Resolve candidate-independent numeric operands once, before enumerating
    /// or mutating a collection. This walks the predicate grammar, not effects.
    /// Boolean short-circuiting or an empty battlefield cannot hide a missing
    /// producer, invalid arithmetic, or overflow in a declared numeric operand.
    pub(super) fn prepare_runtime_object_filter(
        &self,
        filter: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Value, EngineError> {
        self.prepare_object_filter_in_view(
            filter,
            None,
            false,
            bindings,
            decisions,
            FilterPreparationContext::Runtime(source),
        )
    }

    pub(super) fn prepare_permanent_runtime_object_filter(
        &self,
        filter: &Value,
        permanent: Option<&CardInstance>,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<Value, EngineError> {
        self.prepare_object_filter_in_view(
            filter,
            permanent,
            true,
            bindings,
            decisions,
            FilterPreparationContext::Runtime(source),
        )
    }

    /// Share the exact color/Boolean preparation with static source selectors.
    /// Their historical numeric consumer is unchanged; this role cannot claim
    /// that it prepared or checked a numeric producer. New native multiplicity
    /// grammar explicitly refuses numerical comparisons without that contract.
    pub(super) fn prepare_static_permanent_object_filter(
        &self,
        filter: &Value,
        permanent: &CardInstance,
    ) -> Result<Value, EngineError> {
        self.prepare_object_filter_in_view(
            filter,
            Some(permanent),
            true,
            &BTreeMap::new(),
            &BTreeMap::new(),
            FilterPreparationContext::StaticSourceColors,
        )
    }

    pub(super) fn prepare_casting_query_filter(
        &self,
        filter: &Value,
        permanent: Option<&CardInstance>,
        permanent_domain: bool,
        context: &cost_query::CastingQueryContext<'_>,
    ) -> Result<Value, EngineError> {
        self.prepare_object_filter_in_view(
            filter,
            permanent,
            permanent_domain,
            &BTreeMap::new(),
            context.decisions,
            FilterPreparationContext::Casting(context),
        )
    }

    fn prepare_object_filter_in_view(
        &self,
        filter: &Value,
        permanent: Option<&CardInstance>,
        permanent_domain: bool,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        context: FilterPreparationContext<'_>,
    ) -> Result<Value, EngineError> {
        fn prepare(
            engine: &GameEngine,
            filter: &Value,
            bindings: &BTreeMap<String, RuntimeBinding>,
            decisions: &BTreeMap<String, Value>,
            depth: usize,
            permanent: Option<&CardInstance>,
            permanent_domain: bool,
            context: FilterPreparationContext<'_>,
        ) -> Result<Value, EngineError> {
            if depth >= 64 {
                return Err(EngineError::new(
                    "object query nesting exceeds supported depth",
                ));
            }
            object_predicate::try_map_predicate_leaves(filter, depth, &mut |filter| {
                let mut prepared = filter.clone();
                match value_kind(filter) {
                    Some("compare") => {
                        let right = match context {
                            FilterPreparationContext::Runtime(source) => engine
                                .checked_runtime_numeric_expression(
                                    &filter["right"],
                                    source,
                                    bindings,
                                    decisions,
                                    "objectQuery.compare.right",
                                )?,
                            FilterPreparationContext::Casting(context) => engine
                                .checked_cost_query_numeric(
                                    &filter["right"],
                                    context,
                                    "objectQuery.compare.right",
                                )?,
                            FilterPreparationContext::StaticSourceColors => return Ok(prepared),
                        };
                        prepared["right"] = json!({"kind":"integer","value":right});
                        if permanent_domain
                            && matches!(
                                value_kind(&filter["left"]),
                                Some("powerOf" | "toughnessOf")
                            )
                        {
                            if value_kind(&filter["left"]["object"]) != Some("candidate") {
                                return Err(EngineError::new(
                                    "unsupported permanent query characteristic reference",
                                ));
                            }
                            // The current global P/T service is infallible. Only a
                            // closed vanilla world is proved here; no printed-stat
                            // substitution is used in the presence of modifiers.
                            if !engine.state.rule_modifiers.is_empty()
                                || engine
                                    .state
                                    .players
                                    .iter()
                                    .flat_map(|p| &p.battlefield)
                                    .any(|c| {
                                        c.definition
                                            .rules
                                            .iter()
                                            .any(|rule| rule.get("modifiers").is_some())
                                            || !c.counters.is_empty()
                                            || c.power_modifier != 0
                                            || c.toughness_modifier != 0
                                    })
                            {
                                return Err(EngineError::new(
                                    "unsupported checked permanent query P/T modifiers",
                                ));
                            }
                            if let Some(card) = permanent {
                                // CR208.3: a noncreature permanent has no P/T,
                                // including a Vehicle with printed characteristics.
                                // It cannot match this characteristic predicate.
                                if !engine.permanent_is_creature(card) {
                                    return Ok(json!({"kind":"not","operand":null}));
                                }
                                let stat = if value_kind(&filter["left"]) == Some("powerOf") {
                                    &card.definition.power
                                } else {
                                    &card.definition.toughness
                                };
                                if stat
                                    .as_deref()
                                    .and_then(|s| s.parse::<i32>().ok())
                                    .is_none()
                                {
                                    return Err(EngineError::new(
                                        "unsupported checked permanent query noninteger P/T",
                                    ));
                                }
                                let left = if value_kind(&filter["left"]) == Some("powerOf") {
                                    engine.current_power(card)
                                } else {
                                    engine.current_toughness(card)
                                };
                                prepared = if object_predicate::compare_integers(
                                    left,
                                    filter["operator"].as_str(),
                                    right,
                                ) {
                                    Value::Null
                                } else {
                                    json!({"kind":"not","operand":null})
                                };
                            }
                        }
                    }
                    Some("colorContains" | "colorDoesNotContain") if permanent_domain => {
                        let requested = filter["value"]
                            .as_str()
                            .ok_or_else(|| EngineError::new("invalid query color"))?;
                        let symbol = mana::color_name_to_symbol(requested)
                            .ok_or_else(|| EngineError::new("unsupported query color"))?;
                        if let Some(card) = permanent {
                            let colors =
                                engine
                                    .checked_permanent_five_colors(card)
                                    .map_err(|error| {
                                        EngineError::new(format!(
                                            "{}: {}",
                                            error.state, error.detail
                                        ))
                                    })?;
                            let matches = colors.symbols().iter().any(|color| color == symbol)
                                != (value_kind(filter) == Some("colorDoesNotContain"));
                            prepared = if matches {
                                Value::Null
                            } else {
                                json!({"kind":"not","operand":null})
                            };
                        }
                    }
                    _ => {}
                }
                Ok(prepared)
            })
        }
        prepare(
            self,
            filter,
            bindings,
            decisions,
            0,
            permanent,
            permanent_domain,
            context,
        )
    }

    pub(super) fn runtime_object_filter_matches(
        &self,
        view: ObjectFilterView<'_>,
        filter: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        match object_predicate::try_evaluate_predicate(filter, &mut |leaf| {
            Ok::<_, std::convert::Infallible>(self.runtime_object_filter_atom_matches(
                view,
                leaf,
                stack_object,
                bindings,
                decisions,
            ))
        }) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }
    pub(super) fn runtime_object_filter_atom_matches(
        &self,
        view: ObjectFilterView<'_>,
        filter: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        let card = view.definition();
        match value_kind(filter) {
            Some("compare")
                if value_kind(&filter["left"]) == Some("manaValueOf")
                    && value_kind(&filter["left"]["object"]) == Some("candidate") =>
            {
                let Some(right) = self.runtime_numeric_expression(
                    &filter["right"],
                    stack_object,
                    bindings,
                    decisions,
                ) else {
                    return false;
                };
                let left = mana_value(card);
                match filter["operator"].as_str() {
                    Some(">=") => left >= right,
                    Some(">") => left > right,
                    Some("<=") => left <= right,
                    Some("<") => left < right,
                    Some("==") => left == right,
                    _ => false,
                }
            }
            _ => match view {
                ObjectFilterView::Definition(_) => {
                    matches_card_filter_with_decisions(card, filter, decisions)
                }
                ObjectFilterView::Permanent(permanent) => match value_kind(filter) {
                    // These predicates have an established decision/numeric
                    // producer; preserve that shared evaluator as the authority.
                    Some(
                        "manaValueParity" | "chosenCreatureType" | "chosenCardType" | "compare",
                    ) => matches_card_filter_with_decisions(card, filter, decisions),
                    _ => self.permanent_matches_effective_filter(permanent, filter),
                },
            },
        }
    }
    pub(super) fn runtime_collection_object_matches(
        &self,
        instance_id: &str,
        filter: &Value,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        let view = if let Some((player, index)) = self.permanent_position(instance_id) {
            ObjectFilterView::Permanent(&self.state.players[player].battlefield[index])
        } else if let Some(card) = self.card_outside_battlefield(instance_id) {
            ObjectFilterView::Definition(object_predicate::DefinitionView::new(&card.definition))
        } else {
            return false;
        };
        self.runtime_object_filter_matches(view, filter, source, bindings, decisions)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "runtime_object_filter_preparation_tests.rs"]
mod preparation_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "runtime_object_filter_static_role_tests.rs"]
mod static_role_tests;
