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

/// CR 115.1: a stack object one of whose targets is a listed player or a
/// permanent of the listed query. Players and controllers are roles
/// relative to the ability controller choosing the candidates. Shared by the
/// spell query and the stack-item selector (`spell or ability that targets
/// ...`).
pub(in crate::oracle::canonical) fn lower_targets_relation(targeted: &TargetedObjects) -> Value {
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
    atom
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
        // CR 601.2a: the zone a spell was cast from. Only a consumer that
        // splits it off (`split_cast_origin`) evaluates it; left in a filter
        // it stays an atom the runtime validators refuse.
        ObjectPredicate::Relation(ObjectRelation::CastFrom(origin)) => Some(json!({
            "kind": "castFromZone",
            "zone": origin.zone.ir_name(),
            "casterOwned": origin.caster_owned,
        })),
        // CR 115.1: a stack object one of whose targets is a listed player
        // or a permanent of the listed query. Players and controllers are
        // roles relative to the ability controller choosing the candidates.
        ObjectPredicate::Relation(ObjectRelation::Targets(targeted)) => {
            Some(lower_targets_relation(targeted))
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

/// One numeric property of the candidate under test, as a comparison
/// operand.
fn candidate_property(property: SelectorNumericProperty) -> Value {
    let kind = match property {
        SelectorNumericProperty::ManaValue => "manaValueOf",
        SelectorNumericProperty::Power => "powerOf",
        SelectorNumericProperty::Toughness => "toughnessOf",
    };
    json!({ "kind": kind, "object": { "kind": "candidate" } })
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
        // The engine's joint target-set constraint (CR 108.4, 601.2c).
        ObjectQualifier::DistinctControllers => json!({ "kind": "distinctPermanentControllers" }),
        ObjectQualifier::SharedValue(characteristic) => json!({
            "kind": "sharedValue",
            "characteristic": characteristic.ir_name(),
        }),
        ObjectQualifier::TotalAtMost { property, maximum } => json!({
            "kind": "totalAtMost",
            "property": numeric_property_ir_name(*property),
            "value": lower_numeric_expression(maximum).unwrap_or(Value::Null),
        }),
        // A relation of the permanent to the player who controlled it as it
        // entered (CR 110.2a); the engine resolves the role at preparation.
        ObjectQualifier::EnteredUnderControlThisTurn(player) => json!({
            "kind": "enteredUnderControlThisTurn",
            "player": player.ir_name(),
        }),
        // The attacked player of an attacking permanent (CR 508.1b); the
        // engine resolves the role at preparation, as for the entry
        // controller.
        ObjectQualifier::AttackingPlayer {
            player,
            planeswalkers,
        } => json!({
            "kind": "attackingPlayer",
            "player": player.ir_name(),
            "orPlaneswalkers": planeswalkers,
        }),
        // `toughness greater than its power`: both operands read the
        // candidate under test (CR 208.1).
        ObjectQualifier::CandidatePropertyComparison {
            left,
            operator,
            right,
        } => compare(
            operator.ir_operator(),
            candidate_property(*left),
            candidate_property(*right),
        ),
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
        Some("distinctValues" | "totalAtMost" | "distinctPermanentControllers" | "sharedValue")
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
            if map.get("kind").and_then(Value::as_str) == Some("chooseCards") {
                lift_choice_set_constraints(map);
            }
            map.values_mut().for_each(lift_target_set_constraints);
        }
        Value::Array(items) => items.iter_mut().for_each(lift_target_set_constraints),
        _ => {}
    }
}

/// Whether a candidate filter (`where`) still holds a set constraint after
/// the declared target slots and resolution choices lifted theirs: such a
/// filter names no chosen set (`Destroy all creatures that share a color`),
/// so no selection can be checked against it (CR 601.2c, 608.2c).
pub(in crate::oracle::canonical) fn holds_unlifted_set_constraint(value: &Value) -> bool {
    fn contains_set_constraint(value: &Value) -> bool {
        match value {
            Value::Object(map) => {
                is_set_constraint_atom(value) || map.values().any(contains_set_constraint)
            }
            Value::Array(items) => items.iter().any(contains_set_constraint),
            _ => false,
        }
    }
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            (key == "where" && contains_set_constraint(child))
                || holds_unlifted_set_constraint(child)
        }),
        Value::Array(items) => items.iter().any(holds_unlifted_set_constraint),
        _ => false,
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

/// `that share a land type` (CR 205.3i), lowered as a set-constraint atom.
fn is_shared_land_type_atom(atom: &Value) -> bool {
    atom["kind"].as_str() == Some("sharedValue")
        && atom["characteristic"].as_str() == Some("landType")
}

/// Moves the set constraints of a resolution card choice's candidate filter
/// (`up to three Aura cards with different names`, `up to two basic land
/// cards that share a land type`) to the choice itself: `setConstraints`
/// and `sameLandType`, which the engine checks over the whole selection
/// (CR 701.23a: the searcher finds a set as stated). Only members of the
/// filter's top-level conjunction move; another shared characteristic, or
/// a constraint nested elsewhere, stays in the filter, where the candidate
/// validator refuses it.
fn lift_choice_set_constraints(choice: &mut serde_json::Map<String, Value>) {
    if choice.get("candidates").is_none() {
        lift_bound_choice_set_constraints(choice);
    } else {
        lift_candidate_choice_set_constraints(choice);
    }
    strip_choice_bound_set_constraints(choice);
}

/// Splits the top-level set-constraint members off one domain filter. The
/// filter keeps its exact IR when it states none; otherwise it becomes the
/// remaining conjunction, or `Null` when nothing remains.
fn split_filter_set_constraints(filter: &mut Value, lifts: &dyn Fn(&Value) -> bool) -> Vec<Value> {
    let mut members = Vec::new();
    conjunction_members(filter, &mut members);
    if !members.iter().any(lifts) {
        return Vec::new();
    }
    let (lifted, rest): (Vec<Value>, Vec<Value>) = members.into_iter().partition(lifts);
    *filter = match rest.len() {
        0 => Value::Null,
        1 => rest.into_iter().next().expect("one remaining operand"),
        _ => json!({ "kind": "and", "operands": rest }),
    };
    lifted
}

/// Splits the set constraints off a card-collection domain (`cardsInZone`,
/// `filterObjects`, or a `union` whose every set states the same ones). A
/// domain left without a filter drops it (`cardsInZone`) or becomes its
/// unfiltered objects (`filterObjects`). A union whose sets disagree, or any
/// other domain, is left unchanged.
fn split_domain_set_constraints(domain: &mut Value, lifts: &dyn Fn(&Value) -> bool) -> Vec<Value> {
    let kind = domain["kind"].as_str().map(str::to_owned);
    match kind.as_deref() {
        Some("cardsInZone") => {
            let Some(filter) = domain.get_mut("where") else {
                return Vec::new();
            };
            let lifted = split_filter_set_constraints(filter, lifts);
            // Only a filter emptied by the lift is dropped: an unfiltered
            // domain keeps its exact IR.
            if !lifted.is_empty()
                && domain["where"].is_null()
                && let Some(map) = domain.as_object_mut()
            {
                map.remove("where");
            }
            lifted
        }
        Some("filterObjects") => {
            let lifted = split_filter_set_constraints(&mut domain["where"], lifts);
            if !lifted.is_empty() && domain["where"].is_null() {
                *domain = domain["objects"].take();
            }
            lifted
        }
        Some("union") => {
            let Some(sets) = domain["sets"].as_array() else {
                return Vec::new();
            };
            let mut trial = sets.clone();
            let mut shared: Option<Vec<Value>> = None;
            for set in &mut trial {
                let lifted = split_domain_set_constraints(set, lifts);
                match &shared {
                    Some(previous) if *previous != lifted => return Vec::new(),
                    Some(_) => {}
                    None => shared = Some(lifted),
                }
            }
            let shared = shared.unwrap_or_default();
            if !shared.is_empty() {
                domain["sets"] = Value::Array(trial);
            }
            shared
        }
        _ => Vec::new(),
    }
}

/// A resolution choice among a bound domain (`chooseCards` with `from`,
/// `up to two creature cards with total mana value 4 or less from your
/// graveyard`, `cards with different mana values`): the set constraints of
/// its own `where` and of its domain filter apply to the chosen set, never
/// to one candidate (CR 608.2d), so they move to `setConstraints`.
fn lift_bound_choice_set_constraints(choice: &mut serde_json::Map<String, Value>) {
    let lifts = |atom: &Value| is_set_constraint_atom(atom) || is_shared_land_type_atom(atom);
    let mut lifted = Vec::new();
    if let Some(filter) = choice.get_mut("where") {
        lifted.extend(split_filter_set_constraints(filter, &lifts));
        if filter.is_null() {
            choice.remove("where");
        }
    }
    if let Some(domain) = choice.get_mut("from") {
        for atom in split_domain_set_constraints(domain, &lifts) {
            if !lifted.contains(&atom) {
                lifted.push(atom);
            }
        }
    }
    record_choice_set_constraints(choice, lifted);
}

/// The size bounds of a choice count its candidates (`countObjects` of the
/// same domain): once the domain's set constraints are the choice's own,
/// they are not a candidate filter of that count either.
fn strip_choice_bound_set_constraints(choice: &mut serde_json::Map<String, Value>) {
    if choice.get("setConstraints").is_none() && choice.get("sameLandType").is_none() {
        return;
    }
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                if map.get("kind").and_then(Value::as_str) == Some("countObjects")
                    && let Some(domain) = map.get_mut("objects")
                {
                    let lifts = |atom: &Value| {
                        is_set_constraint_atom(atom) || is_shared_land_type_atom(atom)
                    };
                    split_domain_set_constraints(domain, &lifts);
                }
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    for bound in ["count", "minimum", "maximum"] {
        if let Some(value) = choice.get_mut(bound) {
            strip(value);
        }
    }
}

fn record_choice_set_constraints(choice: &mut serde_json::Map<String, Value>, lifted: Vec<Value>) {
    if lifted.is_empty() {
        return;
    }
    let mut constraints = match choice.remove("setConstraints") {
        Some(Value::Array(existing)) => existing,
        Some(other) => vec![other],
        None => Vec::new(),
    };
    for atom in lifted {
        if is_shared_land_type_atom(&atom) {
            choice.insert("sameLandType".to_string(), json!(true));
        } else if !constraints.contains(&atom) {
            constraints.push(atom);
        }
    }
    if !constraints.is_empty() {
        choice.insert("setConstraints".to_string(), Value::Array(constraints));
    }
}

fn lift_candidate_choice_set_constraints(choice: &mut serde_json::Map<String, Value>) {
    let Some(filter) = choice
        .get_mut("candidates")
        .and_then(|candidates| candidates.get_mut("where"))
    else {
        return;
    };
    let lifts = |atom: &Value| is_set_constraint_atom(atom) || is_shared_land_type_atom(atom);
    let mut members = Vec::new();
    conjunction_members(filter, &mut members);
    if !members.iter().any(lifts) {
        return;
    }
    let (lifted, rest): (Vec<Value>, Vec<Value>) = members.into_iter().partition(lifts);
    *filter = match rest.len() {
        0 => Value::Null,
        1 => rest.into_iter().next().expect("one remaining operand"),
        _ => json!({ "kind": "and", "operands": rest }),
    };
    let mut constraints = match choice.remove("setConstraints") {
        Some(Value::Array(existing)) => existing,
        Some(other) => vec![other],
        None => Vec::new(),
    };
    for atom in lifted {
        if is_shared_land_type_atom(&atom) {
            choice.insert("sameLandType".to_string(), json!(true));
        } else {
            constraints.push(atom);
        }
    }
    if !constraints.is_empty() {
        choice.insert("setConstraints".to_string(), Value::Array(constraints));
    }
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

/// The cast origin of a spell query, for a consumer that records the zone a
/// spell is cast from (CR 601.2a): `from your graveyard` is the origin,
/// `from anywhere other than your hand` its negation. The origin must be one
/// member of the top-level conjunction; nested under a union or a negation
/// of something else it has no single zone, and `None` is returned. The
/// remaining filter is the whole spell domain when only the origin was
/// stated.
pub(in crate::oracle::canonical) fn split_cast_origin(
    predicate: &ObjectPredicate,
) -> Option<(ObjectPredicate, Option<(CastOrigin, bool)>)> {
    fn contains_origin(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::Relation(ObjectRelation::CastFrom(_)) => true,
            ObjectPredicate::Not(child) => contains_origin(child),
            ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
                children.iter().any(contains_origin)
            }
            _ => false,
        }
    }
    fn origin_member(predicate: &ObjectPredicate) -> Option<(CastOrigin, bool)> {
        match predicate {
            ObjectPredicate::Relation(ObjectRelation::CastFrom(origin)) => Some((*origin, false)),
            ObjectPredicate::Not(child) => match child.as_ref() {
                ObjectPredicate::Relation(ObjectRelation::CastFrom(origin)) => {
                    Some((*origin, true))
                }
                _ => None,
            },
            _ => None,
        }
    }
    if !contains_origin(predicate) {
        return Some((predicate.clone(), None));
    }
    let members = match predicate {
        ObjectPredicate::Intersection(members) => members.as_slice(),
        single => std::slice::from_ref(single),
    };
    let mut origin = None;
    let mut rest = Vec::new();
    for member in members {
        match origin_member(member) {
            Some(read) if origin.is_none() => origin = Some(read),
            Some(_) => return None,
            None if contains_origin(member) => return None,
            None => rest.push(member.clone()),
        }
    }
    let rest = match rest.len() {
        0 => ObjectPredicate::ObjectKind(ObjectKind::Spell),
        1 => rest.remove(0),
        _ => ObjectPredicate::Intersection(rest),
    };
    Some((rest, origin))
}

#[cfg(test)]
#[path = "object_predicate_token_tests.rs"]
mod token_tests;
