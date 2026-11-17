//! One immutable frame for public legal/declaration/payment discovery.
use super::mana_checked_discovery::CheckedManaActivationPlan;
use super::mana_projection_context::ManaProjectionContext;
use super::*;

pub(super) struct ManaDiscoverySession<'a> {
    engine: &'a GameEngine,
    context: ManaProjectionContext<'a>,
}

impl<'a> ManaDiscoverySession<'a> {
    pub(super) fn expand_splice_declarations(
        &self,
        assignments: Vec<CastingDeclaration>,
        card: &CardInstance,
        controller: &str,
    ) -> Result<Option<Vec<CastingDeclaration>>, EngineError> {
        self.engine
            .try_expand_splice_declarations(assignments, card, controller, self)
    }
    pub(super) fn new(engine: &'a GameEngine) -> Self {
        Self {
            engine,
            context: ManaProjectionContext::for_value_interpretation(engine),
        }
    }
    pub(super) fn declarations_for_card(
        &self,
        card: &CardDefinition,
        controller: &str,
    ) -> Result<Option<Vec<CastingDeclaration>>, EngineError> {
        self.declarations_for_card_source(card, controller, None)
    }
    pub(super) fn declarations_for_card_source(
        &self,
        card: &CardDefinition,
        controller: &str,
        casting_source: Option<&CardInstance>,
    ) -> Result<Option<Vec<CastingDeclaration>>, EngineError> {
        self.engine
            .try_declarations_for_card_source_with_budget(
                card,
                controller,
                casting_source,
                &mut |controller, spending| {
                    self.engine
                        .checked_mana_budget_in_context(controller, spending, &self.context)
                },
            )
            .map_err(declaration_result::DeclarationDependencyFailure::into_engine_error)
    }
    pub(super) fn declarations_for_activated_rule(
        &self,
        rule: &Value,
        controller: &str,
        source: &str,
    ) -> Result<Option<Vec<CastingDeclaration>>, EngineError> {
        self.engine
            .try_declarations_for_activated_rule_with_budget(
                rule,
                controller,
                source,
                &mut |controller, spending| {
                    self.engine
                        .checked_mana_budget_in_context(controller, spending, &self.context)
                },
            )
            .map_err(declaration_result::DeclarationDependencyFailure::into_engine_error)
    }
    pub(super) fn payment_for_mana_costs(
        &self,
        index: usize,
        costs: &[String],
        excluded: &BTreeSet<String>,
        spending: Option<&CardDefinition>,
    ) -> Result<Option<ManaPayment>, EngineError> {
        self.engine.checked_payment_in_context(
            index,
            costs,
            excluded,
            spending,
            None,
            None,
            &self.context,
        )
    }
    pub(super) fn payment_for_mana_requirement(
        &self,
        index: usize,
        requirement: ManaRequirement,
        excluded: &BTreeSet<String>,
        spending: Option<&CardDefinition>,
    ) -> Result<Option<ManaPayment>, EngineError> {
        self.engine.checked_payment_requirement_in_context(
            index,
            requirement,
            excluded,
            spending,
            None,
            None,
            &self.context,
        )
    }
    pub(super) fn payment_for_activation_mana_costs(
        &self,
        index: usize,
        costs: &[String],
        excluded: &BTreeSet<String>,
        source: &CardDefinition,
    ) -> Result<Option<ManaPayment>, EngineError> {
        self.engine.checked_payment_in_context(
            index,
            costs,
            excluded,
            None,
            Some(source),
            None,
            &self.context,
        )
    }
    pub(super) fn useful_mana_choices(
        &self,
        source: &CardInstance,
        spending: Option<&CardDefinition>,
    ) -> Result<Vec<ManaChoice>, EngineError> {
        self.engine
            .checked_mana_activation_plan_in_context(source, spending, false, false, &self.context)
            .map(CheckedManaActivationPlan::into_payment_sources)
    }
    pub(super) fn mana_choices_for_permanent(
        &self,
        source: &CardInstance,
        spending: Option<&CardDefinition>,
        unassigned: bool,
        nontrivial: bool,
    ) -> Result<Vec<ManaChoice>, EngineError> {
        self.engine
            .checked_mana_activation_plan_in_context(
                source,
                spending,
                unassigned,
                nontrivial,
                &self.context,
            )
            .map(CheckedManaActivationPlan::into_activation_choices)
    }
    pub(super) fn convoke_payment_for_mana_costs(
        &self,
        index: usize,
        costs: &[String],
        excluded: &BTreeSet<String>,
        spending: Option<&CardDefinition>,
        zone: Option<&str>,
        count: usize,
    ) -> Result<Option<(ManaPayment, Vec<String>)>, EngineError> {
        self.engine.try_plan_convoke_payment(
            index,
            costs,
            excluded,
            spending,
            zone,
            count,
            &mut |source, spending| self.useful_mana_choices(source, spending),
            &mut |index, costs, excluded, spending, zone| {
                self.engine.checked_payment_in_context(
                    index,
                    costs,
                    excluded,
                    spending,
                    None,
                    zone,
                    &self.context,
                )
            },
        )
    }
    pub(super) fn improvise_reduction_for_mana_costs(
        &self,
        index: usize,
        costs: &[String],
        excluded: &BTreeSet<String>,
        spending: Option<&CardDefinition>,
        count: usize,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, EngineError> {
        self.engine.try_plan_improvise_reduction(
            index,
            costs,
            excluded,
            spending,
            count,
            &mut |source, spending| self.useful_mana_choices(source, spending),
        )
    }
    pub(super) fn mana_activation_actions(
        &self,
        index: usize,
    ) -> Result<Vec<LegalAction>, EngineError> {
        let declared = self.engine.try_mana_activation_actions_with(
            index,
            &mut |source| self.mana_choices_for_permanent(source, None, true, true),
            &mut |rule, controller, source| {
                self.declarations_for_activated_rule(rule, controller, source)
            },
            &mut |index, costs, excluded, spending, ability_source| {
                self.engine.checked_payment_in_context(
                    index,
                    costs,
                    excluded,
                    spending,
                    ability_source,
                    None,
                    &self.context,
                )
            },
        )?;
        self.engine
            .refine_postcost_mana_actions(declared, &self.context)
    }
}
