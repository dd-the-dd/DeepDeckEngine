use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a compact tap-led mana activation with independent activation costs,
/// fixed/choice/any-color production, and an optional effect-sequence suffix.
/// This is only a token-shape boundary; child automata own every semantic
/// transition after the claim commits.
pub(in crate::oracle::canonical) fn claims_promoted_activated_mana_family(
    input: &AbilityInput<'_>,
) -> bool {
    claims_activated_mana_tokens(input.tokens)
}

fn claims_activated_mana_tokens(tokens: &[crate::oracle::model::RecognizedEntity]) -> bool {
    let Some(separator) = tokens.iter().position(|token| token.raw == ":") else {
        return false;
    };
    let costs = &tokens[..separator];
    let supported_lead = costs
        .first()
        .is_some_and(|token| token.token_type == "tapSymbol" || token.token_type == "manaSymbol");
    let has_tap_self = costs.iter().any(|token| token.token_type == "tapSymbol");
    if !supported_lead || !has_tap_self {
        return false;
    }
    let effect = &tokens[separator + 1..];
    if effect.len() < 3 || !effect[0].raw.eq_ignore_ascii_case("add") {
        return false;
    }
    let Some(first_terminator) = effect.iter().position(|token| token.raw == ".") else {
        return false;
    };
    if effect
        .get(first_terminator + 1)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("spend"))
    {
        // Spending restrictions belong to the scoped-mana family. If that
        // family does not claim the wording, keep this compact recognizer from
        // stealing it before the scoped grammar is extended.
        return false;
    }
    let production = &effect[..first_terminator];
    // Commit at the stable family boundary. The child mana automaton remains
    // responsible for accepting a valid list or reporting the exact malformed
    // token; validating the whole list here would silently fall back to legacy.
    let fixed_mana_prefix = production.get(1).is_some_and(|token| {
        token.token_type == "manaSymbol" || (token.raw.starts_with('{') && token.raw.ends_with('}'))
    });
    let any_color_head = production.len() >= 6
        && production[1].token_type == "quantity"
        && production[2].raw.eq_ignore_ascii_case("mana")
        && production[3].raw.eq_ignore_ascii_case("of")
        && production[4].raw.eq_ignore_ascii_case("any")
        && production[5].raw.eq_ignore_ascii_case("color");
    fixed_mana_prefix || any_color_head
}

pub(in crate::oracle::canonical) fn claims_promoted_parenthesized_activated_mana_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if !tokens.first().is_some_and(|token| token.raw == "(") {
        return false;
    }
    let end = tokens
        .last()
        .is_some_and(|token| token.raw == ")")
        .then_some(tokens.len().saturating_sub(1))
        .unwrap_or(tokens.len());
    tokens.get(1..end).is_some_and(claims_activated_mana_tokens)
}

pub(in crate::oracle::canonical) fn parse_promoted_parenthesized_activated_mana(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let envelope = recognize_parenthetical_ability_envelope(input.tokens)?;
    let nested = AbilityInput {
        face_name: input.face_name,
        face_type_line: input.face_type_line,
        source: input.source,
        tokens: envelope.inner,
        name_links: input.name_links,
        name_resolution_failure: input.name_resolution_failure,
    };
    let mut draft = parse_promoted_activated_mana(&nested)?;
    draft
        .operations
        .push("Resolve a whole-ability parenthetical token envelope".to_string());
    Ok(draft)
}

pub(in crate::oracle::canonical) fn parse_promoted_activated_mana(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("activated-mana-ability", input.tokens);
    let ability = recognize_activated_mana_ability(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("activation.mana.validation");
    cursor.require(
        ability
            .costs
            .costs
            .iter()
            .any(|cost| matches!(cost, LocalActivationCostNode::TapSelf(_))),
        "a tap-self activation cost",
    )?;
    match lower_recognized_add_led_activation(&mut cursor, ability) {
        EffectProgramParseAttempt::Parsed(draft) => Ok(draft),
        EffectProgramParseAttempt::Unpromoted(failure)
        | EffectProgramParseAttempt::Failed(failure)
        | EffectProgramParseAttempt::FailedSupport(failure) => Err(failure),
        EffectProgramParseAttempt::NotApplicable => {
            unreachable!("lowering an already recognized activation has a structural context")
        }
    }
}
