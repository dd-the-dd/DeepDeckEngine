use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_counter_prohibition_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let self_spell = tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("this"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("spell"));
    let controlled_spell_set = tokens.iter().any(|token| {
        token.raw.eq_ignore_ascii_case("spell") || token.raw.eq_ignore_ascii_case("spells")
    }) && tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("you") && window[1].raw.eq_ignore_ascii_case("control")
    });
    // `SPELLS [you cast] can't be countered.`: one sentence whose spell
    // subject ends at the passive prohibition (CR 701.6).
    let spell_sentence = tokens.len() >= 5
        && tokens[tokens.len() - 1].raw == "."
        && tokens[tokens.len() - 2]
            .raw
            .eq_ignore_ascii_case("countered")
        && tokens[tokens.len() - 3].raw.eq_ignore_ascii_case("be")
        && (tokens[tokens.len() - 4].raw.eq_ignore_ascii_case("can't")
            || tokens[tokens.len() - 4].raw.eq_ignore_ascii_case("cannot"))
        && tokens[..tokens.len() - 4].iter().any(|token| {
            token.raw.eq_ignore_ascii_case("spell") || token.raw.eq_ignore_ascii_case("spells")
        })
        && tokens[..tokens.len() - 4]
            .iter()
            .all(|token| !matches!(token.raw.as_str(), "," | "." | ":" | "(" | "\""));
    (self_spell || controlled_spell_set || spell_sentence)
        && tokens.windows(3).any(|window| {
            (window[0].raw.eq_ignore_ascii_case("can't")
                || window[0].raw.eq_ignore_ascii_case("cannot"))
                && window[1].raw.eq_ignore_ascii_case("be")
                && window[2].raw.eq_ignore_ascii_case("countered")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_counter_prohibition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("counter-prohibition", input.tokens);
    let prohibition = recognize_counter_prohibition(&mut cursor)?;
    let (active_while, modifier) = match prohibition.subject {
        CounterProhibitionSubjectNode::SourceSelf(LocalObjectReference::SourceSelf) => (
            json!({
                "kind": "inZone",
                "object": self_ref(),
                "zone": { "kind": "stack" },
            }),
            json!({
                "kind": "cantBeCountered",
                "object": self_ref(),
            }),
        ),
        CounterProhibitionSubjectNode::ObjectSet(subject) => {
            cursor.enter("counter-prohibition.subject.validation");
            // `Creature spells can't be countered`: every controller.
            let controller = match subject.controller {
                Some(ObjectControllerScope::AbilityController) => controller(),
                None => json!({ "kind": "eachPlayer" }),
                Some(_) => {
                    return Err(cursor.fail(
                        "a spell set controlled by the ability controller or by any player",
                    ));
                }
            };
            cursor.require(
                subject.predicate.is_in_domain(ObjectKind::Spell),
                "a selector in the spell object domain",
            )?;
            let mut objects = json!({
                "kind": "spells",
                "controller": controller,
            });
            if let Some(filter) = lower_spell_object_predicate(&subject.predicate) {
                objects["where"] = filter;
            }
            (
                active_while_battlefield(),
                json!({
                    "kind": "cantBeCountered",
                    "objects": objects,
                }),
            )
        }
        // The caster of a spell controls it as it is cast (CR 601.2a); a
        // spell's controller is the player the prohibition protects.
        CounterProhibitionSubjectNode::CastSpells(spells) => {
            cursor.enter("counter-prohibition.subject.caster");
            let controller = match spells.caster {
                None => json!({ "kind": "eachPlayer" }),
                Some(SpellCasterScope::AbilityController) => controller(),
                Some(SpellCasterScope::OpponentOfAbilityController) => {
                    json!({ "kind": "opponentsOf", "player": controller() })
                }
                Some(SpellCasterScope::NotAbilityController | SpellCasterScope::EventPlayer) => {
                    return Err(cursor.fail(
                        "spells cast by every player, the ability controller or its opponents",
                    ));
                }
            };
            cursor.require(
                domain_filter_is_representable(&spells.predicate),
                "a spell filter preserving Boolean domain identities",
            )?;
            let mut objects = json!({ "kind": "spells", "controller": controller });
            if let Some(filter) = lower_spell_object_predicate(&spells.predicate) {
                objects["where"] = filter;
            }
            (
                active_while_battlefield(),
                json!({ "kind": "cantBeCountered", "objects": objects }),
            )
        }
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while,
            "modifiers": [modifier],
        }),
        &[
            "Recognize a source spell or controlled spell set from the shared token stream",
            "Recognize the passive counter prohibition",
            "Preserve the existing atomic cantBeCountered modifier",
            &trace,
        ],
    ))
}
