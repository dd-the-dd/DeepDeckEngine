//! Admitted native/granted mana programs. Holder owns self, costs and output;
//! granting object is authenticated availability provenance, never the actor.
use super::object_identity::{ObjectIdentity, object_identity_of};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum ManaProgramOrigin {
    Native {
        rule_index: usize,
    },
    Granted {
        holder: ObjectIdentity,
        grantor: ObjectIdentity,
        #[serde(rename = "ruleIndex")]
        rule_index: usize,
        #[serde(rename = "modifierIndex")]
        modifier_index: usize,
        program: Value,
    },
}

impl ManaProgramOrigin {
    pub(super) fn native_index(&self) -> Option<usize> {
        match self {
            Self::Native { rule_index } => Some(*rule_index),
            Self::Granted { .. } => None,
        }
    }
    pub(super) fn program<'a>(&'a self, holder: &'a CardInstance) -> Option<&'a Value> {
        match self {
            Self::Native { rule_index } => holder.definition.printed_rules().get(*rule_index),
            Self::Granted { program, .. } => Some(program),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct AdmittedManaProgram {
    pub origin: ManaProgramOrigin,
    pub rule: Value,
}

/// Same native flag keys; granted restrictions belong to the acquired ability
/// at its authentic site (CR602.5b/c), not all identical wording on the holder.
pub(super) fn mana_activation_limit_flag(
    limit: &Value,
    turn: u32,
    origin: Option<&ManaProgramOrigin>,
) -> Option<String> {
    let id = limit["id"].as_str()?;
    let scoped = match origin {
        Some(ManaProgramOrigin::Granted {
            holder,
            grantor,
            rule_index,
            modifier_index,
            ..
        }) => format!(
            "grantedManaLimit:{}",
            json!([holder, grantor, rule_index, modifier_index, id])
        ),
        _ => id.to_owned(),
    };
    Some(if value_kind(limit) == Some("oncePerTurn") {
        format!("{scoped}:turn:{turn}")
    } else {
        scoped
    })
}

impl ManaChoice {
    pub(super) fn native_rule_index(&self) -> Option<usize> {
        self.origin
            .as_ref()
            .and_then(ManaProgramOrigin::native_index)
    }
    pub(super) fn program<'a>(&'a self, holder: &'a CardInstance) -> Option<&'a Value> {
        self.origin
            .as_ref()
            .and_then(|origin| origin.program(holder))
    }
}

pub(super) fn native_mana_programs(holder: &CardInstance) -> Vec<AdmittedManaProgram> {
    holder
        .definition
        .printed_rules()
        .iter()
        .enumerate()
        .filter(|(_, rule)| mana_rule_supported(rule))
        .map(|(rule_index, rule)| AdmittedManaProgram {
            origin: ManaProgramOrigin::Native { rule_index },
            rule: rule.clone(),
        })
        .collect()
}

/// Grant metadata is absent for native/intrinsic declarations and receipts.
pub(super) fn insert_origin(
    origin: Option<&ManaProgramOrigin>,
    decisions: &mut BTreeMap<String, Value>,
) {
    match origin {
        Some(ManaProgramOrigin::Native { rule_index }) => {
            decisions.insert("abilityRuleIndex".into(), json!(rule_index));
        }
        Some(origin @ ManaProgramOrigin::Granted { .. }) => {
            decisions.insert("grantedManaProgram".into(), json!(origin));
        }
        None => {}
    }
}

pub(super) fn declared_origin(
    action: &LegalAction,
) -> Result<Option<ManaProgramOrigin>, EngineError> {
    if let Some(value) = action.decisions.get("grantedManaProgram") {
        if action.decisions.contains_key("abilityRuleIndex") {
            return Err(EngineError::new(
                "ambiguous native and granted mana program origin",
            ));
        }
        let origin: ManaProgramOrigin = serde_json::from_value(value.clone())
            .map_err(|_| EngineError::new("malformed granted mana program origin"))?;
        if !matches!(origin, ManaProgramOrigin::Granted { .. }) {
            return Err(EngineError::new(
                "granted mana metadata requires a granted origin",
            ));
        }
        return Ok(Some(origin));
    }
    match action.decisions.get("abilityRuleIndex") {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .map(|rule_index| Some(ManaProgramOrigin::Native { rule_index }))
            .ok_or_else(|| EngineError::new("invalid native mana program index")),
    }
}

pub(super) fn receipt_origin(receipt: &Value) -> Result<Option<ManaProgramOrigin>, EngineError> {
    if let Some(value) = receipt.get("grantedManaProgram") {
        if !receipt["ruleIndex"].is_null() {
            return Err(EngineError::new("ambiguous generated mana program origin"));
        }
        let origin: ManaProgramOrigin = serde_json::from_value(value.clone())
            .map_err(|_| EngineError::new("malformed generated granted mana origin"))?;
        if !matches!(origin, ManaProgramOrigin::Granted { .. }) {
            return Err(EngineError::new("generated granted origin is not granted"));
        }
        return Ok(Some(origin));
    }
    if receipt["ruleIndex"].is_null() {
        return Ok(None);
    }
    receipt["ruleIndex"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .map(|rule_index| Some(ManaProgramOrigin::Native { rule_index }))
        .ok_or_else(|| EngineError::new("invalid generated mana rule index"))
}

/// Decode one ordered receipt without consulting an earlier world. Availability
/// belongs to the normal executor at this activation's actual prefix state.
pub(super) fn ordered_receipt_declaration(
    receipt: &Value,
    payer: &str,
) -> Result<LegalAction, EngineError> {
    let declaration: LegalAction = serde_json::from_value(receipt["activationDeclaration"].clone())
        .map_err(|error| EngineError::new(format!("invalid ordered mana declaration: {error}")))?;
    let origin = receipt_origin(receipt)?;
    let declared = declared_origin(&declaration)?;
    if declaration.kind != ActionKind::ActivateAbility
        || declaration.card_instance_id.as_deref() != receipt["sourceId"].as_str()
        || declaration
            .decisions
            .get("manaAbility")
            .and_then(Value::as_bool)
            != Some(true)
        || declaration.decisions.get("manaProduced") != receipt.get("mana")
        || declaration.player_id != payer
        || origin != declared
        || receipt["lifePaid"]
            .as_i64()
            .is_none_or(|n| n < 0 || i32::try_from(n).is_err())
        || !receipt["sacrificesSource"].is_boolean()
        || !receipt["mana"].as_array().is_some_and(|symbols| {
            !symbols.is_empty()
                && symbols.iter().all(|symbol| {
                    matches!(symbol.as_str(), Some("W" | "U" | "B" | "R" | "G" | "C"))
                })
        })
    {
        return Err(EngineError::new(
            "ordered mana receipt does not match its declaration",
        ));
    }
    Ok(declaration)
}

fn membership_filter_supported(filter: &Value) -> bool {
    match value_kind(filter) {
        None => filter.is_null(),
        Some("cardTypeContains" | "subtypeContains") => {
            filter.as_object().is_some_and(|m| m.len() == 2) && filter["value"].is_string()
        }
        // Token status is not a characteristic (CR 111.1, 205): no
        // characteristic-changing effect can alter this membership read.
        Some("isToken") => filter.as_object().is_some_and(|m| m.len() == 1),
        Some("and" | "or") => {
            filter.as_object().is_some_and(|m| m.len() == 2)
                && filter["operands"]
                    .as_array()
                    .is_some_and(|v| !v.is_empty() && v.iter().all(membership_filter_supported))
        }
        Some("not") => {
            filter.as_object().is_some_and(|m| m.len() == 2)
                && membership_filter_supported(&filter["operand"])
        }
        _ => false,
    }
}

fn source_controller(reference: &Value) -> bool {
    reference.as_object().is_some_and(|v| v.len() == 2)
        && value_kind(reference) == Some("controllerOf")
        && reference["object"]
            .as_object()
            .is_some_and(|v| v.len() == 1)
        && value_kind(&reference["object"]) == Some("self")
}

fn controller_relation(reference: &Value) -> bool {
    source_controller(reference)
        || match value_kind(reference) {
            Some("opponentsOf") => {
                reference.as_object().is_some_and(|v| v.len() == 2)
                    && source_controller(&reference["player"])
            }
            Some("playersExcept") => {
                reference.as_object().is_some_and(|v| v.len() == 2)
                    && source_controller(&reference["player"])
            }
            _ => false,
        }
}

fn selector_shape_supported(selector: &Value) -> bool {
    value_kind(selector) == Some("permanents")
        && selector.as_object().is_some_and(|m| {
            m.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "kind" | "where" | "controller" | "ownership" | "excludeSource"
                )
            })
        })
        && selector.get("controller").is_none_or(controller_relation)
        && selector
            .get("ownership")
            .is_none_or(|v| matches!(v.as_str(), Some("owned" | "notOwned")))
        && selector.get("excludeSource").is_none_or(Value::is_boolean)
        && membership_filter_supported(&selector["where"])
}

fn filter_reads(filter: &Value, out: &mut BTreeSet<String>) -> bool {
    match value_kind(filter) {
        None => filter.is_null(),
        Some("cardTypeContains" | "subtypeContains") => {
            let Some(name) = filter["value"].as_str() else {
                return false;
            };
            out.insert(format!(
                "{}:{}",
                if value_kind(filter) == Some("cardTypeContains") {
                    "type"
                } else {
                    "subtype"
                },
                name.to_ascii_lowercase()
            ));
            true
        }
        // Token status reads no characteristic that a modifier can write.
        Some("isToken") => filter.as_object().is_some_and(|m| m.len() == 1),
        Some("and" | "or") => filter["operands"]
            .as_array()
            .is_some_and(|v| v.iter().all(|f| filter_reads(f, out))),
        Some("not") => filter_reads(&filter["operand"], out),
        _ => false,
    }
}

fn trait_write(modifier: &Value) -> Option<String> {
    let (prefix, key) = match value_kind(modifier) {
        Some("addCardType" | "temporaryCardType" | "removeCardTypeWhile") => ("type", "cardType"),
        Some("addSubtype" | "temporarySubtype") => ("subtype", "subtype"),
        Some("setSubtypes" | "setLandSubtype") => return Some("subtype:*".into()),
        _ => return None,
    };
    modifier[key]
        .as_str()
        .map(|name| format!("{prefix}:{}", name.to_ascii_lowercase()))
}

impl GameEngine {
    /// The existing effective service evaluates continuous producer selectors
    /// with printed facts. Prove those reads stable before reusing it; direct
    /// temporary changes and independent continuous additions remain supported.
    pub(super) fn has_active_granted_mana_programs(&self) -> bool {
        // Conservative scheduling/preflight gate, never a semantic absence
        // result. A rejected context must enter the fallible checked planner.
        match self.checked_granted_mana_static_modifier_sites() {
            Ok(sites) => sites.iter().any(|site| {
                value_kind(&site.modifier) == Some("grantActivatedAbility")
                    && value_kind(&site.modifier["ability"]) == Some("manaAbility")
            }),
            Err(_) => true,
        }
    }

    pub(super) fn certify_granted_mana_characteristics(&self) -> Result<(), EngineError> {
        self.validate_granted_mana_static_context()?;
        let statics = self.battlefield_static_modifiers();
        let mut writes = BTreeSet::new();
        for modifier in self
            .state
            .rule_modifiers
            .iter()
            .chain(statics.iter().map(|(_, m)| m))
        {
            if let Some(write) = trait_write(modifier) {
                writes.insert(write);
            }
            if value_kind(modifier) == Some("counterLinkedCharacteristics") {
                if ["addTypes", "addCardTypes"].iter().any(|key| {
                    modifier[*key]
                        .as_array()
                        .is_some_and(|types| !types.is_empty())
                }) {
                    return Err(EngineError::new(
                        "unsupported granted mana counter-linked card-type transformation",
                    ));
                }
                for (field, prefix) in [("addSubtypes", "subtype")] {
                    if let Some(values) = modifier[field].as_array() {
                        for name in values.iter().filter_map(Value::as_str) {
                            writes.insert(format!("{prefix}:{}", name.to_ascii_lowercase()));
                        }
                    }
                }
            }
        }
        let mut replacements = 0;
        for (_, modifier) in &statics {
            match value_kind(modifier) {
                Some("nonbasicLandsBecomeMountains" | "addAllBasicLandTypes") => {
                    return Err(EngineError::new(
                        "unsupported granted mana characteristic transformation service",
                    ));
                }
                Some(
                    "addCardType"
                    | "removeCardTypeWhile"
                    | "addSubtype"
                    | "setSubtypes"
                    | "setLandSubtype",
                ) => {}
                _ => continue,
            }
            if modifier.get("condition").is_some() {
                return Err(EngineError::new(
                    "unsupported granted mana characteristic producer condition",
                ));
            }
            if matches!(value_kind(modifier), Some("setSubtypes" | "setLandSubtype")) {
                replacements += 1;
            }
            let selector = &modifier["objects"];
            let mut reads = BTreeSet::new();
            match value_kind(selector) {
                Some("self") if selector.as_object().is_some_and(|m| m.len() == 1) => {}
                Some("attachedPermanent")
                    if selector.as_object().is_some_and(|m| m.len() == 2)
                        && selector["attachment"]
                            .as_object()
                            .is_some_and(|m| m.len() == 1)
                        && value_kind(&selector["attachment"]) == Some("self") => {}
                Some("permanents")
                    if selector_shape_supported(selector)
                        && filter_reads(&selector["where"], &mut reads) => {}
                _ => {
                    return Err(EngineError::new(
                        "unsupported granted mana characteristic producer selector",
                    ));
                }
            }
            if reads.iter().any(|read| {
                writes.contains(read)
                    || (read.starts_with("subtype:") && writes.contains("subtype:*"))
            }) {
                return Err(EngineError::new(
                    "unsupported granted mana effective characteristic selector dependency",
                ));
            }
        }
        if replacements > 0
            && (writes
                .iter()
                .any(|w| w.starts_with("subtype:") && w != "subtype:*")
                || self
                    .state
                    .players
                    .iter()
                    .flat_map(|p| &p.battlefield)
                    .any(|p| {
                        !p.flags.get("phasedOut").copied().unwrap_or(false)
                            && (self.permanent_has_keyword(p, "changeling")
                                || super::subtypes::definition_has_all_subtypes(
                                    &p.definition,
                                    "creature",
                                ))
                    }))
        {
            return Err(EngineError::new(
                "unsupported granted mana subtype replacement/addition ordering",
            ));
        }
        if replacements > 1 {
            return Err(EngineError::new(
                "unsupported granted mana competing subtype replacements",
            ));
        }
        Ok(())
    }

    fn granted_mana_selector_matches(
        &self,
        grantor: &CardInstance,
        selector: &Value,
        holder: &CardInstance,
    ) -> Result<bool, EngineError> {
        if !selector_shape_supported(selector) {
            return Err(EngineError::new(
                "unsupported granted mana selector/domain context",
            ));
        }
        if holder.flags.get("phasedOut").copied().unwrap_or(false) {
            return Ok(false);
        }
        if !Self::static_selector_relations_match(grantor, selector, holder) {
            return Ok(false);
        }
        if selector["excludeSource"].as_bool() == Some(true)
            && holder.instance_id == grantor.instance_id
        {
            return Ok(false);
        }
        self.certify_granted_mana_characteristics()?;
        Ok(self.permanent_matches_effective_filter(holder, &selector["where"]))
    }

    pub(super) fn admitted_mana_programs(
        &self,
        holder: &CardInstance,
    ) -> Result<Vec<AdmittedManaProgram>, EngineError> {
        let mut programs = native_mana_programs(holder);
        if holder.flags.get("phasedOut").copied().unwrap_or(false) {
            return Ok(programs);
        }
        // Printed off-battlefield programs retain their old role. A permanent
        // selector cannot grant a program to a hand/graveyard or stale object.
        let Some((p, c)) = self.permanent_position(&holder.instance_id) else {
            return Ok(programs);
        };
        if Self::tracked_zone_identity(&self.state.players[p].battlefield[c])
            != Self::tracked_zone_identity(holder)
        {
            return Ok(programs);
        }
        for site in self.checked_granted_mana_static_modifier_sites()? {
            let super::static_modifier_site::BattlefieldStaticModifierSite {
                source: grantor,
                modifier,
                rule_index,
                modifier_index,
                ..
            } = site;
            if value_kind(&modifier) != Some("grantActivatedAbility") {
                continue;
            }
            let program = &modifier["ability"];
            // The typed ordinary-activated envelope is not a mana ability;
            // its normal (possibly targeted) execution remains with that path.
            if activated_rule_supported(program) {
                continue;
            }
            if !modifier.as_object().is_some_and(|m| {
                m.keys()
                    .all(|k| matches!(k.as_str(), "kind" | "objects" | "ability"))
            }) {
                return Err(EngineError::new(
                    "unsupported granted mana modifier context",
                ));
            }
            if !self.granted_mana_selector_matches(&grantor, &modifier["objects"], holder)? {
                continue;
            }
            if !mana_rule_supported(program) {
                return Err(EngineError::new(
                    "unsupported granted mana producer envelope",
                ));
            }
            if value_kind(&program["source"]) != Some("self") {
                return Err(EngineError::new(
                    "granted mana program must bind self to its holder",
                ));
            }
            let actor = &program["effects"][0]["player"];
            if !source_controller(actor)
                && !(value_kind(actor) == Some("abilityController")
                    && actor.as_object().is_some_and(|m| m.len() == 1))
            {
                return Err(EngineError::new(
                    "unsupported granted mana recipient context",
                ));
            }
            if self.permanent_loses_all_abilities(holder)
                || self.mana_abilities_replaced_by_land_subtype(holder)
            {
                return Err(EngineError::new(
                    "unsupported granted mana ability loss/grant layer order",
                ));
            }
            // This certificate index is only its native API's diagnostic slot;
            // no native index is fabricated or serialized for a granted origin.
            super::mana_projection_contract::certify_pure_mana_program(
                &holder.instance_id,
                0,
                program,
            )
            .map_err(|error| {
                EngineError::new(format!(
                    "unsupported granted mana pure program from {}: {:?}",
                    grantor.instance_id, error.state
                ))
            })?;
            let origin = ManaProgramOrigin::Granted {
                holder: object_identity_of(holder)?,
                grantor: object_identity_of(&grantor)?,
                rule_index,
                modifier_index,
                program: program.clone(),
            };
            if !programs.iter().any(|existing| existing.origin == origin) {
                programs.push(AdmittedManaProgram {
                    origin,
                    rule: program.clone(),
                });
            }
        }
        Ok(programs)
    }

    pub(super) fn resolve_mana_program(
        &self,
        holder: &CardInstance,
        origin: Option<&ManaProgramOrigin>,
    ) -> Result<Option<Value>, EngineError> {
        let Some(origin) = origin else {
            return Ok(None);
        };
        self.admitted_mana_programs(holder)?
            .into_iter()
            .find(|candidate| &candidate.origin == origin)
            .map(|candidate| Some(candidate.rule))
            .ok_or_else(|| EngineError::new("declared mana program origin is stale or unavailable"))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "granted_mana_program_tests.rs"]
mod tests;
