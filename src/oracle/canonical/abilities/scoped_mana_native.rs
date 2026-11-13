use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the tap-to-add-mana family once its effect declares an explicit
/// `in ...` color domain. This is deliberately narrower than unrestricted
/// `of any color`, while malformed domain references remain committed native
/// failures instead of falling through to the legacy mana regex.
pub(in crate::oracle::canonical) fn claims_promoted_tap_scoped_mana_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 10
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("{T}"))
        && tokens.get(1).is_some_and(|token| token.raw == ":")
        && tokens
            .get(2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("add"))
        && tokens
            .get(3)
            .is_some_and(|token| token.token_type == "quantity")
        && tokens
            .get(4)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("mana"))
        && tokens
            .get(5)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("of"))
        && tokens
            .get(6)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("any"))
        && tokens
            .get(7)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("color"))
        && tokens
            .get(8)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("in"))
}

pub(in crate::oracle::canonical) fn parse_promoted_tap_scoped_mana(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("simple-tap-scoped-mana-ability", input.tokens);
    let ability = recognize_simple_tap_scoped_mana_ability(&mut cursor)?;
    let TapSelfCostNode = ability.cost;
    let mana_kind = match ability.effect.domain {
        ManaChoiceDomain::CommanderColorIdentity => "chooseCommanderColor",
        ManaChoiceDomain::LandColorsControlledBy(_) => {
            unreachable!("the commander-domain automaton cannot emit a land-color domain")
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
            "kind": "manaAbility",
            "source": self_ref(),
            "costs": [{
                "kind": "tap",
                "object": self_ref(),
            }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": self_ref(),
                },
                "mana": {
                    "kind": mana_kind,
                    "amount": ability.effect.amount,
                },
            }],
        }),
        &[
            "Recognize one tap-cost activation envelope from the shared token stream",
            "Resolve a positive literal amount through the shared numeric automaton",
            "Retain the color domain as typed scoped-mana syntax",
            "Lower to tap and addMana primitives",
            &trace,
        ],
    ))
}
