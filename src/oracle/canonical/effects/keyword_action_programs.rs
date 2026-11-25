//! Primitive programs for keyword actions (CR 701, 2026-11-06). Each action
//! keeps its identity in the parse tree and expands here into ordinary
//! runtime primitives: binds, draws, discards, choices, counters, tokens and
//! designations. No runtime branch interprets a keyword-action opcode.
use super::super::*;

const SINGULAR_OBJECT: &str =
    "one object for a keyword action performed by one object at a time (CR 701.44d, 701.50c)";

fn bound(id: &str) -> Value {
    json!({ "kind": "boundValue", "id": id })
}

fn plural_object(object: &Value) -> bool {
    matches!(
        object["kind"].as_str(),
        Some("chosenTargets" | "eachPermanent" | "chosenObjects")
    )
}

fn positive_literal(quantity: &Value) -> bool {
    quantity.as_i64().is_some_and(|value| value > 0)
        || (quantity["kind"] == "integer" && quantity["value"].as_i64().is_some_and(|v| v > 0))
}

/// `N > 0`, used where a zero quantity means that no event occurs at all
/// (CR 701.50e, 701.63b). A positive literal needs no runtime guard.
fn guard_positive(quantity: &Value, effects: Vec<Value>) -> Vec<Value> {
    if positive_literal(quantity) {
        return effects;
    }
    vec![json!({
        "kind": "conditional",
        "condition": {
            "kind": "compare", "left": quantity, "operator": ">", "right": integer(0),
        },
        "then": effects,
        "else": [],
    })]
}

fn plus_one_counters(object: Value, count: Value, placed_by: Option<Value>) -> Value {
    let mut effect = json!({
        "kind": "putCounters", "permanent": object, "counter": "+1/+1", "count": count,
    });
    if let Some(player) = placed_by {
        effect["placedBy"] = player;
    }
    effect
}

/// The ability's own source is already a stable object reference whose last
/// known information the engine captures; other references are bound once.
pub(in crate::oracle::canonical) fn source_reference(object: &Value) -> bool {
    matches!(object["kind"].as_str(), Some("self" | "abilitySource"))
}

/// Binds the acting object and its controller. CR 701.44c/701.50b: once it
/// has left the battlefield, last known information names both.
fn bind_subject(object: Value, scope: &str) -> (Vec<Value>, Value, Value) {
    let subject_id = format!("{scope}:subject");
    let player_id = format!("{scope}:controller");
    let player = bound(&player_id);
    let mut effects = Vec::new();
    let subject = if source_reference(&object) {
        object
    } else {
        effects.push(json!({ "kind": "bind", "id": subject_id, "value": object }));
        bound(&subject_id)
    };
    effects.push(json!({
        "kind": "bind", "id": player_id,
        "value": { "kind": "controllerOfObject", "object": subject },
    }));
    (effects, subject, player)
}

/// CR 701.50a/d: the controller draws N, discards N, then puts a +1/+1
/// counter on the conniving permanent for each nonland card discarded.
/// CR 701.50f: it connives even if some actions were impossible.
fn lower_connive_program(
    object: Value,
    quantity: Value,
    scope: &str,
) -> Result<Vec<Value>, String> {
    let (mut effects, subject, player) = bind_subject(object, scope);
    let discarded_id = format!("{scope}:discarded");
    let mut signal = lower_registered_ability_signal("connive", player.clone())
        .ok_or("an afterActionCompletes signal registered for Connive (CR 701.50f)")?;
    signal["object"] = subject.clone();
    effects.extend([
        json!({ "kind": "drawCards", "player": player, "count": quantity }),
        json!({
            "kind": "discardCards", "player": player, "count": quantity, "bind": discarded_id,
        }),
        json!({
            "kind": "conditional",
            "condition": { "kind": "objectOnBattlefield", "object": subject },
            "then": [plus_one_counters(
                subject.clone(),
                json!({
                    "kind": "countObjects",
                    "objects": {
                        "kind": "filterObjects",
                        "objects": { "kind": "boundObjects", "binding": discarded_id },
                        "where": { "kind": "not", "operand": card_type("Land") },
                    },
                }),
                Some(player.clone()),
            )],
            "else": [],
        }),
        signal,
    ]);
    Ok(guard_positive(&quantity, effects))
}

/// CR 701.63a: the controller creates an N/N white Spirit creature token
/// unless they put N +1/+1 counters on the permanent. A permanent no longer
/// on the battlefield cannot receive them. CR 701.63b: endure 0 does nothing.
fn lower_endure_program(object: Value, quantity: Value, scope: &str) -> Vec<Value> {
    let (mut effects, subject, player) = bind_subject(object, scope);
    let token = json!({
        "kind": "createTokens", "controller": player, "quantity": integer(1),
        "token": {
            // The subtype plus "Token" is its CR 111.4 name.
            "name": "Spirit Token", "colors": ["White"], "types": ["Creature"],
            "subtypes": ["Spirit"], "power": quantity, "toughness": quantity,
        },
    });
    effects.push(json!({
        "kind": "conditional",
        "condition": { "kind": "objectOnBattlefield", "object": subject },
        "then": [{
            "kind": "chooseOneEffect", "player": player,
            "options": [
                {
                    "label": "Put +1/+1 counters on the enduring permanent",
                    "effects": [plus_one_counters(subject.clone(), quantity.clone(), Some(player.clone()))],
                },
                { "label": "Create a white Spirit creature token", "effects": [token.clone()] },
            ],
        }],
        "else": [token],
    }));
    guard_positive(&quantity, effects)
}

/// CR 701.46a: if this permanent has no +1/+1 counters on it, put N +1/+1
/// counters on it.
fn lower_adapt_program(object: Value, quantity: Value) -> Vec<Value> {
    vec![json!({
        "kind": "conditional",
        "condition": {
            "kind": "compare",
            "left": { "kind": "countCounters", "object": object, "counter": "+1/+1" },
            "operator": "==",
            "right": integer(0),
        },
        "then": [plus_one_counters(object, quantity, None)],
        "else": [],
    })]
}

/// The permanent gets the designation (kept until it leaves, CR 701.37b).
fn set_designation(object: Value, designation: PermanentDesignation) -> Value {
    // Prepare: the engine's `setPrepared` installs the permission to cast a
    // copy of the prepare spell with the flag; a bare flag would not.
    if designation == PermanentDesignation::Prepared {
        return json!({ "kind": "setPrepared", "object": object, "value": true });
    }
    json!({
        "kind": "setPermanentFlag",
        "object": object,
        "flag": designation.flag(),
        "value": true,
    })
}

/// CR 701.37a-b: if this permanent isn't monstrous, put N +1/+1 counters on
/// it and it becomes monstrous (a designation kept until it leaves).
fn lower_monstrosity_program(object: Value, quantity: Value) -> Vec<Value> {
    vec![json!({
        "kind": "conditional",
        "condition": {
            "kind": "not",
            "operand": {
                "kind": "sourceFlagSet",
                "flag": PermanentDesignation::Monstrous.flag(),
            },
        },
        "then": [
            plus_one_counters(object.clone(), quantity, None),
            set_designation(object, PermanentDesignation::Monstrous),
        ],
        "else": [],
    })]
}

/// CR 701.35a: until the next turn of the controller of the spell or
/// ability, the permanent can't attack or block and its activated abilities
/// can't be activated.
fn lower_detain(object: Value) -> Vec<Value> {
    let duration = json!({ "kind": "untilNextTurn", "player": controller() });
    vec![
        json!({
            "kind": "grantKeyword", "object": object, "keyword": "cantAttack",
            "duration": duration,
        }),
        json!({
            "kind": "grantKeyword", "object": object, "keyword": "cantBlock",
            "duration": duration,
        }),
        json!({
            "kind": "prohibitPermanentActivations", "object": object, "duration": duration,
        }),
    ]
}

/// Designations apply to each object of a plural operand independently.
fn for_each_object(object: Value, scope: &str, apply: impl Fn(Value) -> Vec<Value>) -> Vec<Value> {
    if plural_object(&object) {
        let id = format!("{scope}:object");
        return vec![json!({
            "kind": "forEachObject", "objects": object, "bind": id,
            "effects": apply(bound(&id)),
        })];
    }
    if source_reference(&object) {
        return apply(object);
    }
    let id = format!("{scope}:object");
    let mut effects = vec![json!({ "kind": "bind", "id": id, "value": object })];
    effects.extend(apply(bound(&id)));
    effects
}

/// Lowers one permanent keyword action over one lowered object reference.
pub(in crate::oracle::canonical) fn lower_permanent_keyword_action(
    action: PermanentKeywordAction,
    object: Value,
    quantity: Option<Value>,
    scope: &str,
) -> Result<Vec<Value>, String> {
    if action.requires_singular_object() && plural_object(&object) {
        return Err(SINGULAR_OBJECT.to_string());
    }
    let quantity_or_one = quantity.clone().unwrap_or_else(|| integer(1));
    let required = || {
        quantity
            .clone()
            .ok_or_else(|| format!("a quantity for {}", action.name()))
    };
    Ok(match action {
        PermanentKeywordAction::Regenerate => vec![lower_regeneration_shield(object)],
        // CR 701.15a: the ability controller goads; the designation lasts
        // until that player's next turn (engine `goaded` modifier).
        PermanentKeywordAction::Goad => vec![json!({"kind": "goadPermanents", "objects": object})],
        // CR 701.60a-d: the engine designation grants menace and "can't
        // block" and is never applied twice.
        PermanentKeywordAction::Suspect => for_each_object(object, scope, |object| {
            vec![json!({ "kind": "suspectPermanent", "object": object })]
        }),
        PermanentKeywordAction::Detain => for_each_object(object, scope, lower_detain),
        PermanentKeywordAction::Explore => {
            lower_explore_program(object, scope).map_err(|failure| failure.expected)?
        }
        PermanentKeywordAction::Connive => lower_connive_program(object, quantity_or_one, scope)?,
        PermanentKeywordAction::Endure => lower_endure_program(object, required()?, scope),
        PermanentKeywordAction::Adapt => lower_adapt_program(object, required()?),
        PermanentKeywordAction::Monstrosity => {
            if object["kind"] != "self" {
                return Err("monstrosity of the ability's own source (CR 701.37a)".to_string());
            }
            lower_monstrosity_program(object, required()?)
        }
        PermanentKeywordAction::BecomeDesignated(designation) => {
            for_each_object(object, scope, |object| {
                vec![set_designation(object, designation)]
            })
        }
        // CR 702.26b: each permanent phases out (with what is attached to
        // it, CR 702.26d); it phases in during its controller's next untap
        // step.
        PermanentKeywordAction::PhaseOut => for_each_object(object, scope, |object| {
            vec![json!({ "kind": "phaseOutPermanent", "permanent": object })]
        }),
        // CR 506.4: each permanent stops being an attacking or blocking
        // creature; it deals and is dealt no combat damage.
        PermanentKeywordAction::RemoveFromCombat => for_each_object(object, scope, |object| {
            vec![json!({ "kind": "removeFromCombat", "permanent": object })]
        }),
        // CR 122.1, 608.2h: each recipient gets as many counters of each
        // kind as the source object has, or had as it last existed.
        PermanentKeywordAction::PutCountersOf(reference) => {
            let source = match reference {
                PermanentEffectReference::Source(LocalObjectReference::SourceSelf) => self_ref(),
                PermanentEffectReference::Observed(TypedObservedEventObject::EventPermanent) => {
                    json!({ "kind": "triggeringPermanent" })
                }
                _ => {
                    return Err(
                        "the ability's source or the event's permanent as the object whose counters are put"
                            .to_string(),
                    );
                }
            };
            for_each_object(object, scope, |object| {
                vec![json!({ "kind": "putSameCountersAs", "permanent": object, "source": source })]
            })
        }
        // CR 122.8: counters of any kinds leave each permanent; the ability
        // controller chooses which while the action resolves.
        PermanentKeywordAction::RemoveUnnamedCounters(removal) => {
            let maximum = match removal {
                UnnamedCounterRemoval::UpTo => Some(required()?),
                UnnamedCounterRemoval::Every | UnnamedCounterRemoval::AnyNumber => None,
            };
            for_each_object(object, scope, |object| {
                vec![match (removal, &maximum) {
                    (UnnamedCounterRemoval::Every, _) => {
                        json!({ "kind": "removeAllCounters", "permanent": object })
                    }
                    (UnnamedCounterRemoval::AnyNumber, _) => json!({
                        "kind": "removeAnyNumberOfCounters",
                        "permanent": object,
                        "player": controller(),
                    }),
                    (UnnamedCounterRemoval::UpTo, maximum) => json!({
                        "kind": "removeUpToCounters",
                        "permanent": object,
                        "player": controller(),
                        "maximum": maximum,
                    }),
                }]
            })
        }
    })
}

/// CR 701.36a-b: choose a creature token you control and create a token
/// that's a copy of it; with no creature token, no token is created.
fn lower_populate_program(scope: &str) -> Vec<Value> {
    let choice_id = format!("{scope}:populated");
    let object_id = format!("{choice_id}:object");
    let creature_token = and(vec![card_type("Creature"), json!({ "kind": "isToken" })]);
    vec![json!({
        "kind": "conditional",
        "condition": { "kind": "controlsPermanent", "where": creature_token },
        "then": [
            {
                "kind": "choosePermanents", "id": choice_id, "player": controller(),
                "candidates": {
                    "kind": "permanents", "controller": controller(), "where": creature_token,
                },
                "minimum": integer(1), "maximum": integer(1),
            },
            {
                "kind": "forEachObject", "objects": decision_result(&choice_id),
                "bind": object_id,
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": { "kind": "copyOf", "object": bound(&object_id), "except": [] },
                }],
            },
        ],
        "else": [],
    })]
}

/// CR 701.57a: exile from the top until a nonland card with mana value N or
/// less; it may be cast without paying its mana cost if the resulting spell's
/// mana value is N or less, otherwise it is put into its owner's hand; the
/// rest go to the bottom in a random order. The same primitives as cascade.
fn lower_discover_program(quantity: &Value, scope: &str) -> Result<Vec<Value>, String> {
    let threshold = quantity["value"]
        .as_i64()
        .filter(|_| quantity["kind"] == "integer")
        .ok_or("a literal discover quantity for the exile-until threshold")?;
    let exiled_id = format!("{scope}:discoverExiled");
    let hit_id = format!("{scope}:discovered");
    let at_most = json!({
        "kind": "compare",
        "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
        "operator": "<=",
        "right": integer(threshold),
    });
    let hit = json!({ "kind": "boundObjects", "binding": hit_id });
    let library = json!({ "kind": "library", "player": controller() });
    let exile = json!({ "kind": "exile", "player": controller() });
    Ok(vec![
        json!({
            "kind": "exileFromTopUntil", "zone": library, "bind": exiled_id,
            "stopCardBind": hit_id, "faceDown": false,
            "stopWhere": and(vec![not(card_type("Land")), at_most.clone()]),
            "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
        }),
        json!({
            "kind": "castOneCard", "player": controller(), "cards": hit,
            "sourceZone": "exile", "normalCostsApply": false, "optional": true,
            "spellWhere": at_most, "bind": format!("{scope}:discoverCast"),
        }),
        // A cast card has left exile; only an uncast discovered card moves.
        json!({
            "kind": "moveCards", "cards": hit, "from": exile,
            "to": { "kind": "hand", "player": controller() },
        }),
        json!({
            "kind": "moveCards",
            "cards": { "kind": "boundObjects", "binding": exiled_id },
            "from": exile,
            "to": { "kind": "library", "player": controller(), "position": "bottom" },
            "order": { "kind": "random" },
        }),
    ])
}

/// CR 701.39a: choose a creature you control with the least toughness (or
/// tied for it) among creatures you control and put N +1/+1 counters on it.
fn lower_bolster_program(quantity: Value, scope: &str) -> Vec<Value> {
    let choice_id = format!("{scope}:bolstered");
    let creature = card_type("Creature");
    vec![json!({
        "kind": "conditional",
        "condition": { "kind": "controlsPermanent", "where": creature },
        "then": [
            {
                "kind": "choosePermanents", "id": choice_id, "player": controller(),
                "candidates": {
                    "kind": "permanents", "controller": controller(), "where": creature,
                    "extremum": { "property": "toughness", "direction": "least" },
                },
                "minimum": integer(1), "maximum": integer(1),
            },
            plus_one_counters(decision_result(&choice_id), quantity, None),
        ],
        "else": [],
    })]
}

fn face_down_onto_battlefield(cards: Value, from: Value) -> Value {
    json!({
        "kind": "moveCards", "cards": cards, "from": from,
        "to": {
            "kind": "battlefield", "player": controller(), "tapped": false, "faceDown": true,
        },
    })
}

/// CR 701.40a/e: each card is turned face down and put onto the battlefield
/// as a 2/2 creature, one card at a time from the top of the library.
fn lower_manifest_program(quantity: &Value) -> Result<Vec<Value>, String> {
    let count = quantity["value"]
        .as_i64()
        .filter(|count| quantity["kind"] == "integer" && *count > 0)
        .ok_or("a positive literal count of cards manifested one at a time (CR 701.40e)")?;
    let library = json!({ "kind": "library", "player": controller() });
    Ok((0..count)
        .map(|_| {
            face_down_onto_battlefield(
                json!({ "kind": "libraryTopCards", "zone": library, "count": integer(1) }),
                library.clone(),
            )
        })
        .collect())
}

/// CR 701.62a: look at the top two cards, manifest one of them, then put the
/// cards looked at that were not manifested into the graveyard.
fn lower_manifest_dread_program(scope: &str) -> Vec<Value> {
    let looked_id = format!("{scope}:dreadLooked");
    let chosen_id = format!("{scope}:dreadManifested");
    let looked = json!({ "kind": "boundObjects", "binding": looked_id });
    let chosen = decision_result(&chosen_id);
    let library = json!({ "kind": "library", "player": controller() });
    vec![
        json!({
            "kind": "lookAtTopCards", "zone": library, "count": integer(2), "bind": looked_id,
        }),
        json!({
            "kind": "conditional",
            "condition": { "kind": "bindingNotEmpty", "binding": looked_id },
            "then": [
                {
                    "kind": "chooseCards", "id": chosen_id, "player": controller(),
                    "from": looked, "minimum": integer(1), "maximum": integer(1),
                },
                face_down_onto_battlefield(chosen.clone(), library.clone()),
                {
                    "kind": "moveCards",
                    "cards": { "kind": "setDifference", "left": looked, "right": chosen },
                    "from": library,
                    "to": { "kind": "graveyard", "player": controller() },
                },
            ],
            "else": [],
        }),
    ]
}

pub(in crate::oracle::canonical) fn lower_controller_keyword_action(
    action: ControllerKeywordAction,
    quantity: Option<Value>,
    scope: &str,
) -> Result<Vec<Value>, String> {
    match action {
        ControllerKeywordAction::Populate => Ok(lower_populate_program(scope)),
        ControllerKeywordAction::Discover => {
            lower_discover_program(quantity.as_ref().ok_or("a discover quantity")?, scope)
        }
        // CR 701.48a reaches outside the game; the engine tracks no
        // outside-game movement source yet, so learn stays an explicit gap.
        ControllerKeywordAction::Learn => {
            Err("a tracked outside-the-game movement source for learn (CR 701.48a)".to_string())
        }
        ControllerKeywordAction::Manifest => {
            lower_manifest_program(quantity.as_ref().ok_or("a manifested card count")?)
        }
        ControllerKeywordAction::ManifestDread => Ok(lower_manifest_dread_program(scope)),
        ControllerKeywordAction::Bolster => Ok(lower_bolster_program(
            quantity.ok_or("a bolster quantity")?,
            scope,
        )),
        // CR 705.1-705.3: the acting player flips the coins; the outcome is
        // the definite antecedent of `the flip` (`lower_random_result_quantity`).
        ControllerKeywordAction::FlipCoins => Ok(vec![json!({
            "kind": "flipCoins",
            "player": controller(),
            "count": quantity.ok_or("a number of coins flipped")?,
            "bind": COIN_FLIP_BINDING,
        })]),
        // CR 706.1-706.2: the acting player rolls the dice; `the result`
        // reads their total.
        ControllerKeywordAction::RollDice(DieSides(sides)) => {
            if sides < 2 {
                return Err("a die with at least two faces (CR 706.1)".to_string());
            }
            Ok(vec![json!({
                "kind": "rollDice",
                "player": controller(),
                "count": quantity.ok_or("a number of dice rolled")?,
                "sides": sides,
                "bind": DIE_ROLL_BINDING,
            })])
        }
        ControllerKeywordAction::Clash => Ok(lower_clash_program(scope)),
        ControllerKeywordAction::Recruit => Ok(lower_recruit_program(scope)),
    }
}

/// CR 701.70a: "Recruit" means "Draw a card, then discard a card. If you
/// discarded a nonland card this way, create a 1/1 white Human Soldier
/// creature token." The same primitives as connive (CR 701.50a).
fn lower_recruit_program(scope: &str) -> Vec<Value> {
    let discarded_id = format!("{scope}:recruitDiscarded");
    vec![
        json!({ "kind": "drawCards", "player": controller(), "count": integer(1) }),
        json!({
            "kind": "discardCards", "player": controller(), "count": integer(1),
            "bind": discarded_id,
        }),
        json!({
            "kind": "conditional",
            "condition": {
                "kind": "compare",
                "left": {
                    "kind": "countObjects",
                    "objects": {
                        "kind": "filterObjects",
                        "objects": { "kind": "boundObjects", "binding": discarded_id },
                        "where": { "kind": "not", "operand": card_type("Land") },
                    },
                },
                "operator": ">",
                "right": integer(0),
            },
            "then": [{
                "kind": "createTokens", "controller": controller(), "quantity": integer(1),
                "token": {
                    // The subtypes plus "Token" are its CR 111.4 name.
                    "name": "Human Soldier Token", "colors": ["White"],
                    "types": ["Creature"], "subtypes": ["Human", "Soldier"],
                    "power": 1, "toughness": 1,
                },
            }],
            "else": [],
        }),
    ]
}

/// Definite bindings of the latest random or contested outcome of a
/// program (CR 705, 706, 701.30). The runtime `flipCoins`, `rollDice` and
/// `clash` primitives (`engine/random_outcome.rs`) write them; a later
/// outcome of the same kind rebinds them, as `the flip` and `the result`
/// then name that later outcome. The suffixes are the same runtime contract.
pub(in crate::oracle::canonical) const COIN_FLIP_BINDING: &str = "randomOutcome:coinFlip";
pub(in crate::oracle::canonical) const DIE_ROLL_BINDING: &str = "randomOutcome:dieRoll";
pub(in crate::oracle::canonical) const CLASH_BINDING: &str = "randomOutcome:clash";

/// The runtime reader of one quantity of the latest outcome. It reads 0 when
/// no outcome of that kind happened while the program resolves (an optional
/// flip that was declined won and lost no flip, CR 705.2; no die showed a
/// face, and every face of a die is at least 1, CR 706.1).
pub(in crate::oracle::canonical) fn lower_random_result_quantity(
    quantity: RandomResultQuantity,
) -> Value {
    let id = match quantity {
        RandomResultQuantity::DieResult => DIE_ROLL_BINDING.to_string(),
        RandomResultQuantity::CoinFlipsWon => COIN_FLIP_BINDING.to_string(),
        RandomResultQuantity::CoinFlipsLost => format!("{COIN_FLIP_BINDING}:lost"),
        RandomResultQuantity::CoinsHeads => format!("{COIN_FLIP_BINDING}:heads"),
        RandomResultQuantity::CoinsTails => format!("{COIN_FLIP_BINDING}:tails"),
        RandomResultQuantity::ClashWon => CLASH_BINDING.to_string(),
    };
    json!({ "kind": "randomOutcomeValue", "binding": id })
}

/// CR 701.30b: `clash with an opponent` means "choose an opponent; you and
/// that opponent each clash". The opponent is the shared resolution-time
/// player choice; the clash itself (reveal, top-or-bottom, winner) is one
/// runtime primitive over both players.
fn lower_clash_program(scope: &str) -> Vec<Value> {
    let opponent_id = format!("{scope}:clashOpponent");
    vec![
        json!({
            "kind": "choosePlayers",
            "id": opponent_id,
            "player": controller(),
            "candidates": lower_player_set_reference(
                PlayerSetReferenceNode::OpponentsOfAbilityController,
            ),
            "minimum": integer(1),
            "maximum": integer(1),
            "bindAs": opponent_id,
        }),
        json!({
            "kind": "clash",
            "player": controller(),
            "opponent": bound(&opponent_id),
            "bind": CLASH_BINDING,
        }),
    ]
}

/// CR 701.16a: "Investigate" means "Create a Clue token."
pub(in crate::oracle::canonical) fn lower_investigate(
    effect: &InvestigateEffectNode,
) -> Option<Value> {
    if effect.player != LocalPlayerReference::Controller {
        return None;
    }
    let investigation = json!({
        "kind": "createTokens", "controller": controller(), "quantity": integer(1),
        "token": { "kind": "namedToken", "name": "Clue" },
    });
    if effect.times == NumericExpression::from(1_i64) {
        return Some(investigation);
    }
    // `investigate twice|N times`: N separate investigations, each its own
    // token creation event, the count read once before the first.
    Some(json!({
        "kind": "repeatEffects",
        "count": lower_numeric_expression(&effect.times)?,
        "effects": [investigation],
    }))
}

#[cfg(test)]
#[path = "keyword_action_programs_tests.rs"]
mod tests;
