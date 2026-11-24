use super::super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectPredicateDomain {
    Card,
    Permanent,
    Spell,
}

fn lower_card_type(card_type_value: ObjectCardType) -> Value {
    card_type(card_type_value.ir_name())
}

/// Validate declaration availability and bounded numeric leaves without
/// constructing child IR or interpreting the predicate's Boolean meaning.
pub(in crate::oracle::canonical) fn validate_object_predicate_numeric_expressions(
    predicate: &ObjectPredicate,
    declarations: TargetlessDeclarationSupport,
) -> Result<(), TargetlessLoweringError> {
    match predicate {
        ObjectPredicate::NumericComparison { right, .. } => {
            validate_targetless_numeric_expression(right, declarations)
        }
        ObjectPredicate::Not(child) => {
            validate_object_predicate_numeric_expressions(child, declarations)
        }
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            for child in children {
                validate_object_predicate_numeric_expressions(child, declarations)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Recursively lowers one shared object-predicate AST. Domain identities are
/// represented exactly as the former context-specific lowerers represented
/// them so this extraction does not alter canonical IR.
fn lower_object_predicate(
    predicate: &ObjectPredicate,
    domain: ObjectPredicateDomain,
) -> Option<Value> {
    match predicate {
        ObjectPredicate::IsCommander => Some(json!({ "kind": "isCommander" })),
        ObjectPredicate::IsToken => Some(json!({ "kind": "isToken" })),
        ObjectPredicate::CardType(card_type_value) => Some(lower_card_type(*card_type_value)),
        ObjectPredicate::Subtype(value) => Some(subtype(value)),
        ObjectPredicate::CharacteristicBatch(batch) => {
            Some(or(characteristic_batch_members(*batch)
                .iter()
                .filter_map(|member| {
                    lower_object_predicate(&member.characteristic.predicate(), domain)
                })
                .collect()))
        }
        ObjectPredicate::Color(color) => Some(json!({
            "kind": "colorContains",
            "value": canonical_color_name(*color),
        })),
        // The shared color-count characteristic compared with a constant.
        ObjectPredicate::ColorQuantity(quantity) => {
            let (operator, count) = match quantity {
                ColorQuantity::Colorless => ("==", 0),
                ColorQuantity::Monocolored => ("==", 1),
                ColorQuantity::Multicolored => (">=", 2),
            };
            Some(compare(
                operator,
                json!({ "kind": "colorCountOf", "object": { "kind": "candidate" } }),
                integer(count),
            ))
        }
        ObjectPredicate::Supertype(CardSupertype::Basic) => {
            Some(json!({ "kind": "typeLineContains", "value": "Basic" }))
        }
        ObjectPredicate::Supertype(CardSupertype::Legendary) => {
            Some(json!({ "kind": "isLegendary" }))
        }
        // CR 205.4a: the printed type line carries the snow supertype.
        ObjectPredicate::Supertype(CardSupertype::Snow) => {
            Some(json!({ "kind": "typeLineContains", "value": "Snow" }))
        }
        ObjectPredicate::ExactManaCost(cost) => Some(json!({
            "kind": "manaCostEquals",
            "value": cost.as_oracle_cost(),
        })),
        ObjectPredicate::Status(status) => {
            let (kind, negated) = status.ir_atom();
            let atom = json!({ "kind": kind });
            Some(if negated { not(atom) } else { atom })
        }
        ObjectPredicate::OwnedByAbilityController { negated } => {
            let atom = json!({ "kind": "ownedBy", "player": controller() });
            Some(if *negated { not(atom) } else { atom })
        }
        ObjectPredicate::Designation(designation) => Some(json!({
            "kind": "hasDesignation",
            "designation": designation.flag(),
        })),
        ObjectPredicate::PermanentCard => Some(json!({ "kind": "isPermanentCard" })),
        ObjectPredicate::HasCounter(presence) => {
            // The established atom is "at least one"; bounds are written only
            // when they differ from it, so the existing IR is unchanged.
            let mut atom = json!({ "kind": "hasCounter" });
            if let Some(counter) = &presence.counter {
                atom["counter"] = json!(counter.as_str());
            }
            if presence.minimum != 1 {
                atom["minimum"] = json!(presence.minimum);
            }
            if let Some(maximum) = presence.maximum {
                atom["maximum"] = json!(maximum);
            }
            Some(atom)
        }
        ObjectPredicate::Relation(ObjectRelation::Named(name)) => Some(json!({
            "kind": "nameEquals",
            "value": name,
        })),
        ObjectPredicate::Relation(ObjectRelation::ChosenName) => Some(json!({
            "kind": "chosenCardName",
            "decisionId": crate::oracle::contextual_binding::RESOLUTION_CARD_NAME_DECISION,
        })),
        ObjectPredicate::Relation(ObjectRelation::DealtDamageToPlayerThisTurn {
            recipient,
            combat_only,
        }) => Some(json!({
            "kind": "dealtDamageToPlayerThisTurn",
            "recipient": recipient.ir_name(),
            "combatOnly": combat_only,
        })),
        ObjectPredicate::Relation(ObjectRelation::Related(related)) => {
            Some(lower_related_object(related))
        }
        // CR 115.1: a stack object one of whose targets is a listed player
        // or a permanent of the listed query. Players and controllers are
        // roles relative to the ability controller choosing the candidates.
        ObjectPredicate::Relation(ObjectRelation::Targets(targeted)) => {
            let mut atom = json!({ "kind": "targets" });
            if let Some(player) = targeted.players {
                atom["players"] = json!(player.ir_name());
            }
            if let Some(permanents) = &targeted.permanents {
                let mut query = json!({
                    "where": lower_permanent_object_predicate(&permanents.predicate),
                });
                if let Some(controller) = permanents.controller {
                    query["controller"] = json!(match controller {
                        ObjectControllerScope::AbilityController => "abilityController",
                        ObjectControllerScope::NotAbilityController => "notAbilityController",
                        ObjectControllerScope::OpponentOfAbilityController => {
                            "opponentOfAbilityController"
                        }
                        // Refused by the engine's target-relation validator.
                        ObjectControllerScope::EventPlayer => "eventPlayer",
                        // Refused by the relative clause before lowering.
                        ObjectControllerScope::Player(_) => "relatedPlayer",
                    });
                }
                atom["permanents"] = query;
            }
            Some(atom)
        }
        ObjectPredicate::ChosenCreatureType => Some(json!({
            "kind": "chosenCreatureType",
            "decisionId": crate::oracle::contextual_binding::RESOLUTION_CREATURE_TYPE_DECISION,
        })),
        ObjectPredicate::ChosenColor => Some(json!({
            "kind": "chosenColor",
            "decisionId": crate::oracle::contextual_binding::RESOLUTION_COLOR_DECISION,
        })),
        ObjectPredicate::HasKeyword(keyword) => Some(json!({
            "kind": "hasKeyword",
            "value": keyword.canonical_name(),
        })),
        ObjectPredicate::Qualifier(qualifier) => Some(lower_object_qualifier(qualifier, domain)),
        ObjectPredicate::NumericComparison {
            left,
            operator,
            right,
        } => lower_selector_numeric_comparison(*left, *operator, right),
        ObjectPredicate::ObjectKind(object_kind) => match (domain, object_kind) {
            (ObjectPredicateDomain::Card, ObjectKind::Card)
            | (ObjectPredicateDomain::Permanent, ObjectKind::Permanent)
            | (ObjectPredicateDomain::Spell, ObjectKind::Spell) => None,
            _ => unreachable!("the selector automaton validates its object domain before lowering"),
        },
        ObjectPredicate::Not(operand) => lower_object_predicate(operand, domain).map(not),
        ObjectPredicate::Intersection(operands) => {
            let lowered = operands
                .iter()
                .filter_map(|operand| lower_object_predicate(operand, domain))
                .collect::<Vec<_>>();
            match lowered.len() {
                0 => None,
                1 => lowered.into_iter().next(),
                _ => Some(and(lowered)),
            }
        }
        // A branch naming the whole domain (`permanents or artifacts`) makes
        // the union the whole domain; dropping it would narrow the query.
        ObjectPredicate::Union(operands) => operands
            .iter()
            .map(|operand| lower_object_predicate(operand, domain))
            .collect::<Option<Vec<_>>>()
            .map(or),
    }
}

fn numeric_property_ir_name(property: SelectorNumericProperty) -> &'static str {
    match property {
        SelectorNumericProperty::ManaValue => "manaValue",
        SelectorNumericProperty::Power => "power",
        SelectorNumericProperty::Toughness => "toughness",
    }
}

/// The qualifier atoms. A keyword ability is any of the rule kinds the
/// runtime records for it; a set constraint and a superlative are atoms of
/// their own that only their consumers' validators admit, so a consumer
/// that cannot evaluate them refuses the filter instead of reading it as a
/// candidate test.
fn lower_object_qualifier(qualifier: &ObjectQualifier, domain: ObjectPredicateDomain) -> Value {
    match qualifier {
        ObjectQualifier::KeywordAbility(keyword) => {
            let atoms = keyword
                .ir_kinds()
                .iter()
                .map(|kind| json!({ "kind": "hasKeyword", "value": kind }))
                .collect::<Vec<_>>();
            if atoms.len() == 1 {
                atoms.into_iter().next().expect("one keyword kind")
            } else {
                or(atoms)
            }
        }
        ObjectQualifier::ManaCostSymbol(ManaCostSymbolClass::VariableX) => {
            json!({ "kind": "manaCostContainsX" })
        }
        ObjectQualifier::DamagedBySourceThisTurn => {
            json!({ "kind": "wasDealtDamageBySourceThisTurn" })
        }
        ObjectQualifier::DistinctValues(property) => {
            json!({ "kind": "distinctValues", "property": property.ir_name() })
        }
        ObjectQualifier::TotalAtMost { property, maximum } => json!({
            "kind": "totalAtMost",
            "property": numeric_property_ir_name(*property),
            "value": lower_numeric_expression(maximum).unwrap_or(Value::Null),
        }),
        ObjectQualifier::Extremum {
            property,
            extreme,
            among,
            population,
        } => json!({
            "kind": "extremumAmong",
            "property": numeric_property_ir_name(*property),
            "extreme": extreme.ir_name(),
            "population": population.ir_name(),
            "where": lower_object_predicate(among, domain).unwrap_or(Value::Null),
        }),
    }
}

/// The number of battlefield permanents a player controls matching
/// `predicate` (`you control three or more creatures with different
/// powers`). A count over a distinctness constraint counts the distinct
/// values (the greatest subset whose members differ, CR 201.2 for names):
/// the runtime's distinct-power and distinct-name counts. A constraint the
/// runtime cannot count stays in the filter, which its validator refuses.
pub(in crate::oracle::canonical) fn lower_controlled_permanent_count(
    predicate: &ObjectPredicate,
    player: Value,
    exclude_source: bool,
) -> Value {
    if let Some((filter, constraints)) = predicate.split_set_constraints()
        && let [ObjectQualifier::DistinctValues(property)] = constraints.as_slice()
    {
        let kind = match (property, exclude_source) {
            (DistinctProperty::Power, _) => Some("countDistinctPowers"),
            // The runtime's name count has no source exclusion.
            (DistinctProperty::Name, false) => Some("countDistinctPermanentNames"),
            _ => None,
        };
        if let Some(kind) = kind {
            let mut count = json!({
                "kind": kind,
                "player": player,
                "where": lower_permanent_object_predicate(&filter),
            });
            if exclude_source {
                count["excludeSource"] = json!(true);
            }
            return count;
        }
    }
    json!({
        "kind": "countPermanents",
        "player": player,
        "where": lower_permanent_object_predicate(predicate),
        "excludeSource": exclude_source,
    })
}

/// Whether an IR atom is a set constraint of the shared query.
fn is_set_constraint_atom(atom: &Value) -> bool {
    matches!(
        atom["kind"].as_str(),
        Some("distinctValues" | "totalAtMost")
    )
}

/// Moves the set constraints a declared target slot's candidate filter
/// states (`any number of target creature cards with total mana value 8 or
/// less`, `up to three target cards with different names`) to the slot's
/// `selectionConstraint`, which the engine checks over the whole chosen
/// target set (CR 601.2c). Only members of the filter's top-level
/// conjunction move; a constraint nested elsewhere stays in the filter,
/// where the candidate validator refuses it. An existing selection
/// constraint is kept beside the new ones (`allOf`).
pub(in crate::oracle::canonical) fn lift_target_set_constraints(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.get("kind").and_then(Value::as_str) == Some("chooseTargets") {
                lift_slot_set_constraints(map);
            }
            map.values_mut().for_each(lift_target_set_constraints);
        }
        Value::Array(items) => items.iter_mut().for_each(lift_target_set_constraints),
        _ => {}
    }
}

/// The members of a conjunction, nested conjunctions flattened.
fn conjunction_members(filter: &Value, members: &mut Vec<Value>) {
    match filter["kind"].as_str() {
        Some("and") if filter["operands"].is_array() => filter["operands"]
            .as_array()
            .into_iter()
            .flatten()
            .for_each(|operand| conjunction_members(operand, members)),
        _ => members.push(filter.clone()),
    }
}

fn lift_slot_set_constraints(decision: &mut serde_json::Map<String, Value>) {
    let Some(filter) = decision
        .get_mut("candidates")
        .and_then(|candidates| candidates.get_mut("where"))
    else {
        return;
    };
    let mut members = Vec::new();
    conjunction_members(filter, &mut members);
    // A filter without a top-level set constraint keeps its exact IR.
    if !members.iter().any(is_set_constraint_atom) {
        return;
    }
    let (mut lifted, rest): (Vec<Value>, Vec<Value>) =
        members.into_iter().partition(is_set_constraint_atom);
    *filter = match rest.len() {
        0 => Value::Null,
        1 => rest.into_iter().next().expect("one remaining operand"),
        _ => json!({ "kind": "and", "operands": rest }),
    };
    if let Some(existing) = decision.remove("selectionConstraint") {
        lifted.insert(0, existing);
    }
    let constraint = if lifted.len() == 1 {
        lifted.pop().expect("one constraint")
    } else {
        json!({ "kind": "allOf", "constraints": lifted })
    };
    decision.insert("selectionConstraint".to_string(), constraint);
}

/// Splits a superlative that ranks the candidates of a player's own choice
/// (`sacrifices a creature with the greatest power among creatures they
/// control`): the population is the chooser's permanents named by the same
/// predicate as the candidates, so the ranking is the choice's `extremum`
/// over its candidates (ties stay candidates, CR 107.1). Returns `None`
/// without a superlative, `Some(None)` for a superlative whose population
/// differs from the candidates or sits below the top-level conjunction.
pub(in crate::oracle::canonical) fn split_candidate_extremum(
    predicate: &ObjectPredicate,
) -> Option<Option<(ObjectPredicate, Value)>> {
    let is_extremum =
        |qualifier: &ObjectQualifier| matches!(qualifier, ObjectQualifier::Extremum { .. });
    if !predicate.contains_qualifier(&is_extremum) {
        return None;
    }
    let ObjectPredicate::Intersection(members) = predicate else {
        return Some(None);
    };
    let mut rest = Vec::new();
    let mut extremum = None;
    for member in members {
        match member {
            ObjectPredicate::Qualifier(ObjectQualifier::Extremum {
                property,
                extreme,
                among,
                population: ExtremumPopulation::CandidateController,
            }) if extremum.is_none() => extremum = Some((*property, *extreme, among.as_ref())),
            other if other.contains_qualifier(&is_extremum) => return Some(None),
            other => rest.push(other.clone()),
        }
    }
    let Some((property, extreme, among)) = extremum else {
        return Some(None);
    };
    let rest = match rest.len() {
        0 => return Some(None),
        1 => rest.remove(0),
        _ => ObjectPredicate::Intersection(rest),
    };
    if rest != *among {
        return Some(None);
    }
    Some(Some((
        rest,
        json!({
            "property": numeric_property_ir_name(property),
            "direction": extreme.ir_name(),
        }),
    )))
}

/// Lifts the damage history relative to the ability source out of a
/// zone-change subject filter (`a creature dealt damage by this creature
/// this turn dies`): the zone-change matcher reads it on the departing
/// object as `damagedBySourceThisTurn` (CR 603.10a). `None` when the atom
/// is absent; `Some(None)` when it is present but not a member of the
/// top-level conjunction, which the matcher cannot represent.
pub(in crate::oracle::canonical) fn split_source_damage_history(
    predicate: &ObjectPredicate,
) -> Option<Option<ObjectPredicate>> {
    let history = |member: &ObjectPredicate| {
        matches!(
            member,
            ObjectPredicate::Qualifier(ObjectQualifier::DamagedBySourceThisTurn)
        )
    };
    if !predicate.contains_qualifier(&|qualifier| {
        matches!(qualifier, ObjectQualifier::DamagedBySourceThisTurn)
    }) {
        return None;
    }
    let ObjectPredicate::Intersection(members) = predicate else {
        return Some(None);
    };
    if members.iter().filter(|member| history(*member)).count() != 1 {
        return Some(None);
    }
    let mut rest = members
        .iter()
        .filter(|member| !history(*member))
        .cloned()
        .collect::<Vec<_>>();
    if rest.iter().any(|member| {
        member.contains_qualifier(&|qualifier| {
            matches!(qualifier, ObjectQualifier::DamagedBySourceThisTurn)
        })
    }) {
        return Some(None);
    }
    Some(Some(match rest.len() {
        0 => return Some(None),
        1 => rest.remove(0),
        _ => ObjectPredicate::Intersection(rest),
    }))
}

/// A permanent collection over `predicate`: its filter and, when the query
/// states one, its ownership relation. `None` when an ownership relation is
/// nested where a collection cannot carry it.
pub(in crate::oracle::canonical) fn lower_permanent_collection(
    predicate: &ObjectPredicate,
) -> Option<Value> {
    let (rest, ownership) = split_collection_ownership(predicate)?;
    let mut collection = json!({
        "kind": "permanents",
        "where": lower_permanent_object_predicate(&rest),
    });
    if let Some(ownership) = ownership {
        collection["ownership"] = json!(ownership);
    }
    Some(collection)
}

/// Splits a top-level ownership relation off a permanent collection's
/// predicate: the engine's collections carry it as `ownership`
/// (`owned` / `notOwned`). `None` when an ownership atom is nested where a
/// collection relation cannot express it.
pub(in crate::oracle::canonical) fn split_collection_ownership(
    predicate: &ObjectPredicate,
) -> Option<(ObjectPredicate, Option<&'static str>)> {
    fn contains_owner(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::OwnedByAbilityController { .. } => true,
            ObjectPredicate::Not(child) => contains_owner(child),
            ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
                children.iter().any(contains_owner)
            }
            _ => false,
        }
    }
    let relation = |negated: bool| if negated { "notOwned" } else { "owned" };
    match predicate {
        ObjectPredicate::OwnedByAbilityController { negated } => Some((
            ObjectPredicate::ObjectKind(ObjectKind::Permanent),
            Some(relation(*negated)),
        )),
        ObjectPredicate::Intersection(children) => {
            let mut ownership = None;
            let mut rest = Vec::new();
            for child in children {
                match child {
                    ObjectPredicate::OwnedByAbilityController { negated } => {
                        if ownership.is_some() {
                            return None;
                        }
                        ownership = Some(relation(*negated));
                    }
                    other if contains_owner(other) => return None,
                    other => rest.push(other.clone()),
                }
            }
            let rest = match rest.len() {
                0 => ObjectPredicate::ObjectKind(ObjectKind::Permanent),
                1 => rest.remove(0),
                _ => ObjectPredicate::Intersection(rest),
            };
            Some((rest, ownership))
        }
        other if contains_owner(other) => None,
        other => Some((other.clone(), None)),
    }
}

pub(in crate::oracle::canonical) fn lower_card_object_predicate(
    predicate: &ObjectPredicate,
) -> Value {
    lower_object_predicate(predicate, ObjectPredicateDomain::Card).unwrap_or(Value::Null)
}

pub(in crate::oracle::canonical) fn lower_permanent_object_predicate(
    predicate: &ObjectPredicate,
) -> Value {
    lower_object_predicate(predicate, ObjectPredicateDomain::Permanent).unwrap_or(Value::Null)
}

pub(in crate::oracle::canonical) fn lower_spell_object_predicate(
    predicate: &ObjectPredicate,
) -> Option<Value> {
    lower_object_predicate(predicate, ObjectPredicateDomain::Spell)
}

#[cfg(test)]
#[path = "object_predicate_token_tests.rs"]
mod token_tests;
