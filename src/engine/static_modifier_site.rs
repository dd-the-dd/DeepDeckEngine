//! Authentic static modifier sites, before Boolean condition filtering.
//! The old tuple adapter and checked mana reader share this sole collector.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct BattlefieldStaticModifierSite {
    pub source: CardInstance,
    pub rule_index: usize,
    pub modifier_index: usize,
    pub modifier: Value,
    pub outer_rule_supported: bool,
    pub outer_condition: Option<Value>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrantedManaStaticRole {
    Activation,
    Projection,
}

fn ordinary_granted_producer(modifier: &Value) -> bool {
    value_kind(modifier) == Some("grantActivatedAbility")
        && activated_rule_supported(&modifier["ability"])
        && rule_contains_kind(&modifier["ability"], "addMana")
}

fn potential_mana_grant(modifier: &Value, role: GrantedManaStaticRole) -> bool {
    value_kind(modifier) == Some("grantActivatedAbility")
        && (!activated_rule_supported(&modifier["ability"])
            || (role == GrantedManaStaticRole::Projection && ordinary_granted_producer(modifier)))
}
fn characteristic_producer(modifier: &Value) -> bool {
    matches!(
        value_kind(modifier),
        Some(
            "addCardType"
                | "setCardTypes"
                | "modifySubtypeSet"
                | "removeCardTypeWhile"
                | "addSubtype"
                | "setSubtypes"
                | "setLandSubtype"
                | "nonbasicLandsBecomeMountains"
                | "addAllBasicLandTypes"
        )
    )
}

impl GameEngine {
    pub(super) fn battlefield_static_modifier_sites(&self) -> Vec<BattlefieldStaticModifierSite> {
        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|source| !source.flags.get("phasedOut").copied().unwrap_or(false))
            .flat_map(|source| {
                source
                    .definition
                    .printed_rules()
                    .iter()
                    .enumerate()
                    .filter(|(_, rule)| {
                        self.rule_active_for_source(source, rule)
                            && rule["minimumClassLevel"].as_i64().is_none_or(|minimum| {
                                i64::from(source.counters.get("classLevel").copied().unwrap_or(1))
                                    >= minimum
                            })
                    })
                    .flat_map(|(rule_index, rule)| {
                        rule["modifiers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .enumerate()
                            .map(
                                move |(modifier_index, modifier)| BattlefieldStaticModifierSite {
                                    source: source.clone(),
                                    rule_index,
                                    modifier_index,
                                    modifier: modifier.clone(),
                                    outer_rule_supported: battlefield_static_rule_supported(rule),
                                    outer_condition: rule.get("condition").cloned(),
                                },
                            )
                    })
                    .collect::<Vec<_>>()
            })
            .chain(self.card_zone_static_modifier_sites())
            .chain(self.effect_created_static_modifier_sites())
            .collect()
    }

    /// CR 604.3: static abilities of cards in a graveyard, hand or exile
    /// that state that zone as their zone of function. They share the
    /// battlefield modifier vocabulary and activity checks.
    fn card_zone_static_modifier_sites(&self) -> Vec<BattlefieldStaticModifierSite> {
        let zones = self.state.players.iter().flat_map(|player| {
            [
                ("graveyard", &player.graveyard),
                ("hand", &player.hand),
                ("exile", &player.exile),
            ]
        });
        let mut sites = Vec::new();
        for (zone, cards) in zones {
            for source in cards {
                for (rule_index, rule) in source.definition.printed_rules().iter().enumerate() {
                    if !zone_of_function::static_zone_of_function(rule).is_some_and(|function| {
                        function.is_card_zone_outside_battlefield() && function.functions_in(zone)
                    }) || !Self::rule_metadata_active_for_source(source, rule)
                    {
                        continue;
                    }
                    for (modifier_index, modifier) in rule["modifiers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        sites.push(BattlefieldStaticModifierSite {
                            source: source.clone(),
                            rule_index,
                            modifier_index,
                            modifier: modifier.clone(),
                            outer_rule_supported: zone_of_function::card_zone_static_rule_supported(
                                rule,
                            ),
                            outer_condition: rule.get("condition").cloned(),
                        });
                    }
                }
            }
        }
        sites
    }

    pub(super) fn static_modifier_site_is_active(
        &self,
        site: &BattlefieldStaticModifierSite,
    ) -> bool {
        site.outer_rule_supported
            && site.outer_condition.as_ref().is_none_or(|condition| {
                self.condition_value_with_source(
                    condition,
                    &site.source.controller,
                    &BTreeMap::new(),
                    Some(&site.source),
                )
            })
    }

    fn validate_granted_mana_sites(
        &self,
        sites: &[BattlefieldStaticModifierSite],
        role: GrantedManaStaticRole,
    ) -> Result<(), EngineError> {
        // Inspect raw relevant sites BEFORE the historical condition reader can
        // erase a grant or a characteristic producer using printed facts.
        if !sites
            .iter()
            .any(|site| potential_mana_grant(&site.modifier, role))
        {
            return Ok(());
        }
        for site in sites.iter().filter(|site| {
            potential_mana_grant(&site.modifier, role) || characteristic_producer(&site.modifier)
        }) {
            if !site.outer_rule_supported {
                return Err(EngineError::new(
                    "unsupported granted mana static producer envelope",
                ));
            }
            if site.outer_condition.as_ref().is_some_and(|condition|
                !super::static_condition_capability::condition_is_closed_without_characteristic_reads(condition,0)) {
                return Err(EngineError::new("unsupported granted mana outer static-rule condition context"));
            }
            // CR106.7 concerns any ability that could resolve to produce mana,
            // not only mana abilities. Direct discovery must keep ordinary
            // targeted abilities separate, but projection cannot erase them.
            // Only a proved closed outer condition may establish real absence.
            if role == GrantedManaStaticRole::Projection
                && ordinary_granted_producer(&site.modifier)
                && self.static_modifier_site_is_active(site)
            {
                return Err(EngineError::new(
                    "unsupported granted mana counterfactual ordinary activated producer",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn validate_granted_mana_static_context(&self) -> Result<(), EngineError> {
        self.validate_granted_mana_sites(
            &self.battlefield_static_modifier_sites(),
            GrantedManaStaticRole::Activation,
        )
    }

    pub(super) fn validate_granted_mana_projection_static_context(
        &self,
    ) -> Result<(), EngineError> {
        self.validate_granted_mana_sites(
            &self.battlefield_static_modifier_sites(),
            GrantedManaStaticRole::Projection,
        )
    }

    pub(super) fn checked_granted_mana_static_modifier_sites(
        &self,
    ) -> Result<Vec<BattlefieldStaticModifierSite>, EngineError> {
        let sites = self.battlefield_static_modifier_sites();
        self.validate_granted_mana_sites(&sites, GrantedManaStaticRole::Activation)?;
        Ok(sites
            .into_iter()
            .filter(|site| self.static_modifier_site_is_active(site))
            .collect())
    }
}
