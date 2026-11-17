//! Read-only, bounded counterfactual projection. This is not action discovery.
//! Unsupported is never represented by a successful empty type set.
use super::mana_production::ManaProductionMode;
use super::mana_projection_contract::{
    ManaType, ManaTypeProjection, NoManaReason, certify_pure_mana_program,
};
use super::mana_projection_fixed_point::{
    ManaFixedPointFailure, ManaTypeTable, solve_mana_type_fixed_point_with_error,
};
use super::*;

#[path = "mana_projection_value.rs"]
mod value;
use value::context_free_mana_value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ManaProjectionContextError {
    pub state: &'static str,
    pub source_instance_id: String,
    pub rule_index: Option<usize>,
    pub detail: String,
}

impl ManaProjectionContextError {
    fn new(state: &'static str, source: &str, detail: impl Into<String>) -> Self {
        Self {
            state,
            source_instance_id: source.to_string(),
            rule_index: None,
            detail: detail.into(),
        }
    }
    pub(super) fn into_engine_error(self) -> EngineError {
        EngineError::new(format!(
            "unsupported mana evaluation: state={} source={} rule={:?} detail={}",
            self.state, self.source_instance_id, self.rule_index, self.detail,
        ))
    }
}

/// Executable output choices retain quantities and mixed-color combinations.
/// NoMana remains distinct until activation/payment policy consumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ExecutableManaOutputs {
    NoMana(NoManaReason),
    Choices(Vec<Vec<String>>),
}

enum ValueProjection {
    NoMana(NoManaReason),
    /// Witnesses preserve exact possible TYPES; they are not an exhaustive
    /// executable choice list (notably for chooseColors). Never use for actions.
    TypeWitnesses(Vec<Vec<String>>),
}

/// Borrowing the engine prevents reusing this context across a mutable engine
/// operation. It owns only derived type facts for that immutable state.
pub(super) struct ManaProjectionContext<'a> {
    engine: &'a GameEngine,
    lands: BTreeMap<String, &'a CardInstance>,
    projected: ManaTypeTable,
    lazy_projection:
        std::cell::OnceCell<Result<Box<ManaProjectionContext<'a>>, ManaProjectionContextError>>,
}

impl<'a> ManaProjectionContext<'a> {
    pub(super) fn for_value_interpretation(engine: &'a GameEngine) -> Self {
        // Executable interpretation constructs the land graph only when a
        // positive query is actually evaluated. A closed zero quantity never
        // asks an unrelated producer to define a type.
        Self {
            engine,
            lands: BTreeMap::new(),
            projected: ManaTypeTable::new(),
            lazy_projection: std::cell::OnceCell::new(),
        }
    }

    /// One immutable state frame shares its lazy fixed point across all checked
    /// production/payment queries. Errors are cached honestly as errors.
    fn projected_context(&self) -> Result<&ManaProjectionContext<'a>, ManaProjectionContextError> {
        match self
            .lazy_projection
            .get_or_init(|| Self::new(self.engine).map(Box::new))
        {
            Ok(context) => Ok(context.as_ref()),
            Err(error) => Err(error.clone()),
        }
    }

    pub(super) fn new(engine: &'a GameEngine) -> Result<Self, ManaProjectionContextError> {
        engine
            .validate_granted_mana_projection_static_context()
            .map_err(|error| {
                ManaProjectionContextError::new(
                    "mana.projection.granted-static-context",
                    "",
                    error.message,
                )
            })?;
        // Validate the admitted-grant world before the land graph can omit an
        // unproved characteristic transformation. Native-only worlds keep the
        // existing projection path and its own capability checks unchanged.
        if engine.has_active_granted_mana_programs() {
            engine
                .certify_granted_mana_characteristics()
                .map_err(|error| {
                    ManaProjectionContextError::new(
                        "mana.projection.granted-characteristics",
                        "",
                        error.message,
                    )
                })?;
        }
        let mut lands = BTreeMap::new();
        for permanent in engine.state.players.iter().flat_map(|p| &p.battlefield) {
            if permanent.flags.get("phasedOut").copied().unwrap_or(false)
                || !engine.permanent_has_card_type(permanent, "Land")
            {
                continue;
            }
            if !is_land(&permanent.definition) {
                return Err(ManaProjectionContextError::new(
                    "mana.projection.effective-land-domain",
                    &permanent.instance_id,
                    "the shared production planner does not yet cover newly added Land types",
                ));
            }
            if lands
                .insert(permanent.instance_id.clone(), permanent)
                .is_some()
            {
                return Err(ManaProjectionContextError::new(
                    "mana.projection.duplicate-source",
                    &permanent.instance_id,
                    "duplicate battlefield identity",
                ));
            }
        }
        let mut context = Self {
            engine,
            lands,
            projected: ManaTypeTable::new(),
            lazy_projection: std::cell::OnceCell::new(),
        };
        for land in context.lands.values() {
            context.certify_source(land)?;
        }
        let ids = context.lands.keys().cloned().collect::<Vec<_>>();
        let projected = solve_mana_type_fixed_point_with_error(&ids, |id, table| {
            let source = context.lands.get(id).ok_or_else(|| {
                ManaProjectionContextError::new(
                    "mana.projection.missing-source",
                    id,
                    "fixed-point source is absent",
                )
            })?;
            context.project_source(source, table)
        })
        .map_err(|failure| match failure {
            ManaFixedPointFailure::Producer(error) => error,
            other => {
                ManaProjectionContextError::new("mana.projection.graph", "", format!("{other:?}"))
            }
        })?;
        context.projected = projected;
        Ok(context)
    }

    pub(super) fn types_for_source(
        &self,
        id: &str,
    ) -> Result<&ManaTypeProjection, ManaProjectionContextError> {
        self.projected.get(id).ok_or_else(|| {
            ManaProjectionContextError::new(
                "mana.projection.missing-source",
                id,
                "source is not a current effective unphased land",
            )
        })
    }

    pub(super) fn types_for_controller(
        &self,
        controller_id: &str,
    ) -> Result<ManaTypeProjection, ManaProjectionContextError> {
        self.require_player(controller_id)?;
        self.query_types(&self.projected, controller_id, false)
    }

    fn require_player(&self, id: &str) -> Result<usize, ManaProjectionContextError> {
        self.engine.player_index(id).map_err(|e| {
            ManaProjectionContextError::new("mana.projection.controller", id, e.message)
        })
    }

    fn certify_source(&self, source: &CardInstance) -> Result<(), ManaProjectionContextError> {
        self.require_player(&source.controller)?;
        self.engine
            .admitted_mana_programs(source)
            .map_err(|error| {
                ManaProjectionContextError::new(
                    "mana.projection.granted-program",
                    &source.instance_id,
                    error.message,
                )
            })?;
        if !self.engine.mana_abilities_replaced_by_land_subtype(source)
            && !self.engine.permanent_loses_all_abilities(source)
        {
            for (index, rule) in source.definition.rules.iter().enumerate() {
                if !self.engine.rule_active_for_source(source, rule) {
                    continue;
                }
                if !rule_is_executable(rule)
                    || rule_contains_kind(rule, "firebending")
                    || rule_contains_kind(rule, "addFixedManaSymbols")
                {
                    return Err(ManaProjectionContextError::new(
                        "mana.projection.opaque-producer",
                        &source.instance_id,
                        "source has a producer whose primitive program is not available",
                    ));
                }
                if matches!(value_kind(rule), Some("staticAbility" | "keywordAbility")) {
                    continue;
                }
                if rule_contains_kind(rule, "addMana") {
                    certify_pure_mana_program(&source.instance_id, index, rule).map_err(
                        |failure| ManaProjectionContextError {
                            state: "mana.projection.program",
                            source_instance_id: failure.source_instance_id,
                            rule_index: Some(failure.rule_index),
                            detail: format!("{:?}", failure.state),
                        },
                    )?;
                }
            }
        }
        // The historical named grant helper bypasses effective availability and
        // hardcodes C. Reject that scope until the same typed grant program is used.
        for granting in self
            .engine
            .state
            .players
            .iter()
            .flat_map(|p| &p.battlefield)
        {
            for rule in &granting.definition.rules {
                for modifier in rule["modifiers"].as_array().into_iter().flatten() {
                    if value_kind(modifier) == Some("dampingSphereMana") {
                        return Err(ManaProjectionContextError::new(
                            "mana.projection.cardinality-dependent-types",
                            &source.instance_id,
                            "type-changing replacement needs an ordered quantity-aware projection",
                        ));
                    }
                    if matches!(
                        value_kind(modifier),
                        Some(
                            "setLandSubtype"
                                | "nonbasicLandsBecomeMountains"
                                | "addAllBasicLandTypes"
                                | "addSubtype"
                                | "setSubtypes"
                                | "loseAllAbilities"
                                | "grantManaAbility"
                                | "grantAbility"
                                | "grantActivatedAbility"
                                | "grantTriggeredAbility"
                                | "grantActivatedAbilitiesFromLinkedExile"
                                | "grantActivatedAbilitiesFromZone"
                                | "multiplyManaProduction"
                                | "addCardType"
                                | "removeCardTypeWhile"
                        )
                    ) && (requires_activation_binding(&rule["condition"])
                        || requires_activation_binding(&modifier["condition"]))
                    {
                        return Err(ManaProjectionContextError::new(
                            "mana.projection.static-condition-binding",
                            &source.instance_id,
                            format!(
                                "unclosed condition of granting source {}",
                                granting.instance_id
                            ),
                        ));
                    }
                    if value_kind(modifier) == Some("grantAbility")
                        && GameEngine::static_modifier_matches_permanent_name(
                            granting, modifier, source,
                        )
                    {
                        return Err(ManaProjectionContextError::new(
                            "mana.projection.named-grant",
                            &source.instance_id,
                            format!(
                                "granting source {} needs a typed grant program",
                                granting.instance_id
                            ),
                        ));
                    }
                }
            }
        }
        for (granting, modifier) in self.engine.battlefield_static_modifiers() {
            if !GameEngine::static_selector_matches(&granting, &modifier["objects"], source) {
                continue;
            }
            if value_kind(&modifier) == Some("setSubtypes")
                || (value_kind(&modifier) == Some("addSubtype")
                    && matches!(
                        modifier["subtype"].as_str(),
                        Some("Plains" | "Island" | "Swamp" | "Mountain")
                    ))
            {
                return Err(ManaProjectionContextError::new(
                    "mana.projection.subtype-environment",
                    &source.instance_id,
                    "the shared intrinsic-mana helper does not yet cover this subtype transformation",
                ));
            }
            if matches!(
                value_kind(&modifier),
                Some("grantActivatedAbilitiesFromLinkedExile" | "grantActivatedAbilitiesFromZone")
            ) {
                return Err(ManaProjectionContextError::new(
                    "mana.projection.granted-program",
                    &source.instance_id,
                    "unprojected granted ability envelope",
                ));
            }
            if value_kind(&modifier) == Some("grantManaAbility") {
                if self.engine.permanent_loses_all_abilities(source) {
                    return Err(ManaProjectionContextError::new(
                        "mana.projection.ability-layer-order",
                        &source.instance_id,
                        "simultaneous ability loss and grant need ordered layer proof",
                    ));
                }
                if granting.instance_id != source.instance_id
                    && !context_free_mana_value(&modifier["mana"])
                {
                    return Err(ManaProjectionContextError::new(
                        "mana.projection.grant-source-context",
                        &source.instance_id,
                        format!(
                            "granting source {} differs from the ability holder",
                            granting.instance_id
                        ),
                    ));
                }
            }
            if value_kind(&modifier) == Some("grantTriggeredAbility")
                && rule_contains_kind(&modifier["ability"], "addMana")
            {
                return Err(ManaProjectionContextError::new(
                    "mana.projection.granted-program",
                    &source.instance_id,
                    "unprojected granted triggered producer envelope",
                ));
            }
            if value_kind(&modifier) == Some("grantKeyword")
                && modifier["keyword"].as_str() == Some("firebending")
            {
                return Err(ManaProjectionContextError::new(
                    "mana.projection.opaque-producer",
                    &source.instance_id,
                    "granted keyword contains an implicit producer",
                ));
            }
        }
        if self.engine.permanent_loses_all_abilities(source)
            && self.engine.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("emrakulExigentLandMana")
                    && modifier["landInstanceId"].as_str() == Some(source.instance_id.as_str())
            })
        {
            return Err(ManaProjectionContextError::new(
                "mana.projection.ability-layer-order",
                &source.instance_id,
                "temporary grant with ability loss needs ordered layer proof",
            ));
        }
        Ok(())
    }

    fn project_source(
        &self,
        source: &CardInstance,
        table: &ManaTypeTable,
    ) -> Result<ManaTypeProjection, ManaProjectionContextError> {
        if self.engine.permanent_loses_all_abilities(source) {
            return Ok(ManaTypeProjection::NoMana(NoManaReason::NoManaAbility));
        }
        let mut empty_reasons = Vec::new();
        let programs = self
            .engine
            .admitted_mana_programs(source)
            .map_err(|error| {
                ManaProjectionContextError::new(
                    "mana.projection.granted-program",
                    &source.instance_id,
                    error.message,
                )
            })?;
        let choices = self.engine.try_plan_admitted_mana_production(
            source,
            &programs,
            None,
            true,
            true,
            &BTreeSet::new(),
            ManaProductionMode::Counterfactual,
            &mut |value, controller, _, origin| {
                let origin = origin.ok_or_else(|| {
                    ManaProjectionContextError::new(
                        "mana.projection.source-context",
                        &source.instance_id,
                        "missing output source",
                    )
                })?;
                match self.value_projection(value, controller, origin, table)? {
                    ValueProjection::TypeWitnesses(options) => Ok(options),
                    ValueProjection::NoMana(reason) => {
                        empty_reasons.push(reason);
                        Ok(Vec::new())
                    }
                }
            },
        )?;
        let mut types = BTreeSet::new();
        for symbol in choices.into_iter().flat_map(|choice| choice.symbols) {
            let kind = ManaType::from_symbol(&symbol).ok_or_else(|| {
                ManaProjectionContextError::new(
                    "mana.projection.output-symbol",
                    &source.instance_id,
                    symbol,
                )
            })?;
            types.insert(kind);
        }
        let reason = if empty_reasons.contains(&NoManaReason::UndefinedType) {
            NoManaReason::UndefinedType
        } else if empty_reasons.contains(&NoManaReason::ZeroAmount) {
            NoManaReason::ZeroAmount
        } else {
            NoManaReason::NoManaAbility
        };
        Ok(ManaTypeProjection::from_types(types, reason))
    }

    fn query_types(
        &self,
        table: &ManaTypeTable,
        controller: &str,
        opponents: bool,
    ) -> Result<ManaTypeProjection, ManaProjectionContextError> {
        let mut result = BTreeSet::new();
        for (id, land) in &self.lands {
            if (land.controller == controller) == opponents {
                continue;
            }
            let projection = table.get(id).ok_or_else(|| {
                ManaProjectionContextError::new(
                    "mana.projection.missing-source",
                    id,
                    "query source absent from projection table",
                )
            })?;
            if let ManaTypeProjection::Types(types) = projection {
                result.extend(
                    types
                        .iter()
                        .copied()
                        .filter(|t| !opponents || *t != ManaType::Colorless),
                );
            }
        }
        Ok(ManaTypeProjection::from_types(
            result,
            NoManaReason::UndefinedType,
        ))
    }
}

fn requires_activation_binding(value: &Value) -> bool {
    [
        "decisionResult",
        "boundValue",
        "countDecisionCardsMatching",
        "triggeringSpellManaValue",
        "triggeringSpellManaSpent",
    ]
    .iter()
    .any(|kind| rule_contains_kind(value, kind))
}

#[cfg(test)]
#[path = "mana_projection_context_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mana_projection_cache_tests.rs"]
mod cache_tests;
