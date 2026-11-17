//! Shared convoke/improvise selection. Dependency errors are never no-payment.
use super::*;

impl GameEngine {
    pub(super) fn convoke_payment_for_mana_costs(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
        requested_creature_count: usize,
    ) -> Option<(ManaPayment, Vec<String>)> {
        match self.try_plan_convoke_payment(
            player_index,
            mana_costs,
            excluded_sources,
            spending_card,
            spending_source_zone,
            requested_creature_count,
            &mut |source, spending| {
                Ok::<_, std::convert::Infallible>(
                    self.mana_choices_for_permanent(source, spending, false, false),
                )
            },
            &mut |index, costs, excluded, spending, zone| {
                Ok::<_, std::convert::Infallible>(self.payment_for_mana_costs_with_context(
                    index, costs, excluded, spending, None, zone,
                ))
            },
        ) {
            Ok(payment) => payment,
            Err(never) => match never {},
        }
    }

    pub(super) fn try_plan_convoke_payment<C, P, E>(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
        requested_creature_count: usize,
        resolve_choices: &mut C,
        resolve_payment: &mut P,
    ) -> Result<Option<(ManaPayment, Vec<String>)>, E>
    where
        C: FnMut(&CardInstance, Option<&CardDefinition>) -> Result<Vec<ManaChoice>, E>,
        P: FnMut(
            usize,
            &[String],
            &BTreeSet<String>,
            Option<&CardDefinition>,
            Option<&str>,
        ) -> Result<Option<ManaPayment>, E>,
    {
        if requested_creature_count == 0 {
            return resolve_payment(
                player_index,
                mana_costs,
                excluded_sources,
                spending_card,
                spending_source_zone,
            )
            .map(|payment| payment.map(|payment| (payment, Vec::new())));
        }
        let mut requirement = ManaRequirement::default();
        for mana_cost in mana_costs {
            let Some(parsed) = parse_mana_requirement(mana_cost) else {
                return Ok(None);
            };
            requirement.add(&parsed);
        }
        if requested_creature_count > requirement.symbol_count() {
            return Ok(None);
        }
        let controller_id = self.state.players[player_index].id.as_str();
        let mut candidates = Vec::new();
        for permanent in self.state.players[player_index]
            .battlefield
            .iter()
            .filter(|permanent| {
                permanent.controller == controller_id
                    && !permanent.tapped
                    && !excluded_sources.contains(&permanent.instance_id)
                    && self.permanent_is_creature(permanent)
            })
        {
            candidates.push((
                permanent.instance_id.clone(),
                self.permanent_color_symbols(permanent),
                permanent.summoning_sick,
                resolve_choices(permanent, spending_card)?.is_empty(),
            ));
        }
        candidates.sort_by_key(|(instance_id, _, summoning_sick, has_no_mana_ability)| {
            (!*summoning_sick, !*has_no_mana_ability, instance_id.clone())
        });

        fn search<P, E>(
            player_index: usize,
            candidates: &[(String, BTreeSet<String>, bool, bool)],
            candidate_start: usize,
            remaining_creatures: usize,
            requirement: &ManaRequirement,
            excluded_sources: &BTreeSet<String>,
            spending_card: Option<&CardDefinition>,
            spending_source_zone: Option<&str>,
            selected: &mut Vec<String>,
            payment: &mut P,
        ) -> Result<Option<ManaPayment>, E>
        where
            P: FnMut(
                usize,
                &[String],
                &BTreeSet<String>,
                Option<&CardDefinition>,
                Option<&str>,
            ) -> Result<Option<ManaPayment>, E>,
        {
            if remaining_creatures == 0 {
                let mut reserved = excluded_sources.clone();
                reserved.extend(selected.iter().cloned());
                return payment(
                    player_index,
                    &[requirement.as_mana_cost()],
                    &reserved,
                    spending_card,
                    spending_source_zone,
                );
            }
            if candidates.len().saturating_sub(candidate_start) < remaining_creatures {
                return Ok(None);
            }
            for candidate_index in candidate_start..candidates.len() {
                let (instance_id, colors, _, _) = &candidates[candidate_index];
                for reduced in requirement.convoke_reductions(colors) {
                    selected.push(instance_id.clone());
                    if let Some(payment) = search(
                        player_index,
                        candidates,
                        candidate_index + 1,
                        remaining_creatures - 1,
                        &reduced,
                        excluded_sources,
                        spending_card,
                        spending_source_zone,
                        selected,
                        payment,
                    )? {
                        return Ok(Some(payment));
                    }
                    selected.pop();
                }
            }
            Ok(None)
        }
        let mut selected = Vec::new();
        Ok(search(
            player_index,
            &candidates,
            0,
            requested_creature_count,
            &requirement,
            excluded_sources,
            spending_card,
            spending_source_zone,
            &mut selected,
            resolve_payment,
        )?
        .map(|payment| (payment, selected)))
    }

    pub(super) fn improvise_reduction_for_mana_costs(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        requested_artifact_count: usize,
    ) -> Option<(Vec<String>, Vec<String>)> {
        match self.try_plan_improvise_reduction(
            player_index,
            mana_costs,
            excluded_sources,
            spending_card,
            requested_artifact_count,
            &mut |source, spending| {
                Ok::<_, std::convert::Infallible>(
                    self.mana_choices_for_permanent(source, spending, false, false),
                )
            },
        ) {
            Ok(reduction) => reduction,
            Err(never) => match never {},
        }
    }

    pub(super) fn try_plan_improvise_reduction<C, E>(
        &self,
        player_index: usize,
        mana_costs: &[String],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        requested_artifact_count: usize,
        resolve_choices: &mut C,
    ) -> Result<Option<(Vec<String>, Vec<String>)>, E>
    where
        C: FnMut(&CardInstance, Option<&CardDefinition>) -> Result<Vec<ManaChoice>, E>,
    {
        let mut requirement = ManaRequirement::default();
        for mana_cost in mana_costs {
            let Some(parsed) = parse_mana_requirement(mana_cost) else {
                return Ok(None);
            };
            requirement.add(&parsed);
        }
        if requested_artifact_count > requirement.generic.max(0) as usize {
            return Ok(None);
        }
        // Same output as the old zero-selection branch, without querying
        // unrelated artifacts whose mana is not used for this substitution.
        if requested_artifact_count == 0 {
            return Ok(Some((vec![requirement.as_mana_cost()], Vec::new())));
        }
        let controller_id = self.state.players[player_index].id.as_str();
        let mut candidates = Vec::new();
        for permanent in self.state.players[player_index]
            .battlefield
            .iter()
            .filter(|permanent| {
                permanent.controller == controller_id
                    && !permanent.tapped
                    && !excluded_sources.contains(&permanent.instance_id)
                    && has_card_type(&permanent.definition, "Artifact")
            })
        {
            candidates.push((
                permanent.instance_id.clone(),
                resolve_choices(permanent, spending_card)?.is_empty(),
            ));
        }
        candidates.sort_by_key(|(instance_id, has_no_mana_ability)| {
            (!*has_no_mana_ability, instance_id.clone())
        });
        if requested_artifact_count > candidates.len() {
            return Ok(None);
        }
        let selected = candidates
            .into_iter()
            .take(requested_artifact_count)
            .map(|(id, _)| id)
            .collect();
        requirement.generic -= requested_artifact_count as i32;
        Ok(Some((vec![requirement.as_mana_cost()], selected)))
    }
}
