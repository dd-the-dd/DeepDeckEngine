use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;

use super::{AbilityInput, NameLinkKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivatedEnvelopeState {
    Costs,
    EffectHead,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ActivatedEnvelopeFrame {
    parenthesis_depth: usize,
    quoted: bool,
    saw_cost_token: bool,
}

/// Recognizes a root `COSTS: EFFECT_HEAD ...` envelope from the shared token
/// stream. Nested reminder text and quoted granted abilities are ignored. Cost
/// and effect semantics remain owned by their committed local automata.
pub(crate) fn has_root_activation_effect_head(
    input: &AbilityInput<'_>,
    expected_effect_head: &[&str],
) -> bool {
    if expected_effect_head.is_empty() {
        return false;
    }
    let Some(effect_start) = root_activation_effect_start(input) else {
        return false;
    };
    input.tokens[effect_start..]
        .iter()
        .zip(expected_effect_head)
        .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected))
        && input.tokens.len().saturating_sub(effect_start) >= expected_effect_head.len()
}

/// Returns the first effect token for a root `COSTS: EFFECT` envelope. The
/// classifier deliberately does not validate cost or effect semantics; their
/// domain automata remain authoritative. Colons in quoted granted abilities
/// and nested reminder text are excluded transactionally.
pub(crate) fn root_activation_effect_start(input: &AbilityInput<'_>) -> Option<usize> {
    let whole_ability_parenthesized = input.tokens.first().is_some_and(|token| token.raw == "(")
        && input.tokens.last().is_some_and(|token| token.raw == ")");
    let root_depth = usize::from(whole_ability_parenthesized);
    let mut state = ActivatedEnvelopeState::Costs;
    let mut frame = ActivatedEnvelopeFrame::default();

    for (index, token) in input.tokens.iter().enumerate() {
        match token.raw.as_str() {
            "\"" => {
                frame.quoted = !frame.quoted;
                continue;
            }
            "(" if !frame.quoted => {
                frame.parenthesis_depth += 1;
                continue;
            }
            ")" if !frame.quoted => {
                frame.parenthesis_depth = frame.parenthesis_depth.saturating_sub(1);
                continue;
            }
            _ => {}
        }
        if frame.quoted || frame.parenthesis_depth != root_depth {
            continue;
        }

        state = match state {
            ActivatedEnvelopeState::Costs => {
                if token.raw == ":" {
                    if !frame.saw_cost_token {
                        return None;
                    }
                    ActivatedEnvelopeState::EffectHead
                } else if matches!(token.raw.as_str(), "." | ";") {
                    return None;
                } else {
                    frame.saw_cost_token = true;
                    ActivatedEnvelopeState::Costs
                }
            }
            ActivatedEnvelopeState::EffectHead => {
                return (!matches!(token.raw.as_str(), "." | ";" | ":")).then_some(index);
            }
        };
    }

    None
}

/// Recognizes the root envelope `COSTS: Add ...` from the shared token stream.
fn is_promoted_mana_activation_envelope(input: &AbilityInput<'_>) -> bool {
    has_root_activation_effect_head(input, &["add"])
}

/// Recognizes the root envelope `COSTS: Search your library for ...` without
/// validating either child grammar. This keeps malformed members committed to
/// the native activated-library-search machine and its structured failures.
fn is_promoted_library_search_activation_envelope(input: &AbilityInput<'_>) -> bool {
    has_root_activation_effect_head(input, &["search", "your", "library", "for"])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ControlledCountEntryEnvelopeState {
    Source,
    EnteringState,
    Exception,
    Player,
    Control,
    Threshold,
    ComparatorConnector,
}

/// Recognizes the ordered root envelope
/// `SOURCE enters tapped unless you control QUANTITY or ...` without
/// reconstructing or scanning the Oracle source string. The committed
/// controlled-count automaton owns comparison direction, `other`, selector,
/// punctuation, and full consumption, including structured near-misses.
fn is_promoted_controlled_count_entry_envelope(input: &AbilityInput<'_>) -> bool {
    let Some(first) = input.tokens.first() else {
        return false;
    };
    let starts_with_this = first.raw.eq_ignore_ascii_case("this");
    let current_name_end = input.name_links.iter().find_map(|link| {
        (link.token_start == 0
            && matches!(
                link.kind,
                NameLinkKind::CurrentCard | NameLinkKind::CurrentFace
            ))
        .then_some(link.token_end)
    });
    if !starts_with_this && current_name_end.is_none() {
        return false;
    }

    let mut state = ControlledCountEntryEnvelopeState::Source;
    let mut saw_source_token = starts_with_this;
    for (index, token) in input.tokens.iter().enumerate() {
        state = match state {
            ControlledCountEntryEnvelopeState::Source => {
                let source_name_is_complete = current_name_end.is_none_or(|end| index >= end);
                if token.raw.eq_ignore_ascii_case("enters")
                    && saw_source_token
                    && source_name_is_complete
                {
                    ControlledCountEntryEnvelopeState::EnteringState
                } else if matches!(token.raw.as_str(), "." | ":" | ";") {
                    return false;
                } else {
                    saw_source_token |= current_name_end.is_some_and(|end| index + 1 >= end);
                    ControlledCountEntryEnvelopeState::Source
                }
            }
            ControlledCountEntryEnvelopeState::EnteringState => {
                if token.raw.eq_ignore_ascii_case("tapped") {
                    ControlledCountEntryEnvelopeState::Exception
                } else {
                    return false;
                }
            }
            ControlledCountEntryEnvelopeState::Exception => {
                if token.raw.eq_ignore_ascii_case("unless") {
                    ControlledCountEntryEnvelopeState::Player
                } else {
                    return false;
                }
            }
            ControlledCountEntryEnvelopeState::Player => {
                if token.raw.eq_ignore_ascii_case("you") {
                    ControlledCountEntryEnvelopeState::Control
                } else {
                    return false;
                }
            }
            ControlledCountEntryEnvelopeState::Control => {
                if token.raw.eq_ignore_ascii_case("control") {
                    ControlledCountEntryEnvelopeState::Threshold
                } else {
                    return false;
                }
            }
            ControlledCountEntryEnvelopeState::Threshold => {
                if token.token_type == "quantity" {
                    ControlledCountEntryEnvelopeState::ComparatorConnector
                } else {
                    return false;
                }
            }
            ControlledCountEntryEnvelopeState::ComparatorConnector => {
                return token.raw.eq_ignore_ascii_case("or");
            }
        };
    }
    false
}

/// Recognizes only the stable token envelope `SOURCE enters STATE` for the
/// promoted unconditional source-entry automaton. A current-card/current-face
/// name is consumed through its lexical link; the typed form retains the
/// existing `this TYPE` boundary. Terminators, reminders, permanent typing,
/// and trailing conditions remain committed child-automaton responsibilities.
fn is_promoted_source_entry_state_envelope(input: &AbilityInput<'_>) -> bool {
    let source_end = input
        .name_links
        .iter()
        .find_map(|link| {
            (link.token_start == 0
                && matches!(
                    link.kind,
                    NameLinkKind::CurrentCard | NameLinkKind::CurrentFace
                ))
            .then_some(link.token_end)
        })
        .or_else(|| {
            matches!(input.tokens, [source, source_type, ..]
                if source.raw.eq_ignore_ascii_case("this")
                    && source_type.token_type == "typeWord")
            .then_some(2)
        });
    let Some(source_end) = source_end else {
        return false;
    };
    let Some(event) = input.tokens.get(source_end) else {
        return false;
    };
    let Some(state) = input.tokens.get(source_end + 1) else {
        return false;
    };
    if event.token_type != "eventWord"
        || !event.raw.eq_ignore_ascii_case("enters")
        || state.token_type != "stateWord"
        || (!state.raw.eq_ignore_ascii_case("tapped")
            && !state.raw.eq_ignore_ascii_case("prepared"))
    {
        return false;
    }
    let state_end = source_end + 2;
    if state.raw.eq_ignore_ascii_case("tapped")
        && input.tokens.get(state_end).is_some_and(|introducer| {
            introducer.raw.eq_ignore_ascii_case("if")
                || introducer.raw.eq_ignore_ascii_case("unless")
        })
        // Only the shared turn proposition (`it's ...`); other conditional
        // entries keep their own parent families.
        && input.tokens.get(state_end + 1).is_some_and(|subject| {
            subject.raw.eq_ignore_ascii_case("it's") || subject.raw.eq_ignore_ascii_case("it’s")
        })
    {
        return true;
    }
    if !input
        .tokens
        .get(state_end)
        .is_some_and(|terminator| terminator.raw == ".")
    {
        return false;
    }
    let trailing = &input.tokens[state_end + 1..];
    trailing.is_empty()
        || (state.raw.eq_ignore_ascii_case("prepared")
            && is_canonical_prepare_reminder_tokens(trailing))
}

fn is_canonical_prepare_reminder_tokens(tokens: &[crate::oracle::model::RecognizedEntity]) -> bool {
    let expected = [
        "(",
        "while",
        "it's",
        "prepared",
        ",",
        "you",
        "may",
        "cast",
        "a",
        "copy",
        "of",
        "its",
        "spell",
        ".",
        "doing",
        "so",
        "unprepares",
        "it",
        ".",
        ")",
    ];
    tokens.len() == expected.len()
        && tokens
            .iter()
            .zip(expected)
            .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected))
}

/// Recognizes the token envelope `Destroy TARGET. It can't be ...` so the
/// promoted sequence reaches native dispatch without executing the legacy
/// regex/raw-text classifier. The semantic automaton owns the selector and
/// validates the final amendment verb.
fn is_promoted_destroy_regeneration_envelope(input: &AbilityInput<'_>) -> bool {
    let tokens = input.tokens;
    if !tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("destroy"))
        || !tokens.get(1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("target") || token.raw.eq_ignore_ascii_case("up")
        })
    {
        return false;
    }
    let Some(terminator) = tokens.iter().position(|token| token.raw == ".") else {
        return false;
    };
    let suffix = &tokens[terminator + 1..];
    suffix.len() >= 3
        && suffix[0].raw.eq_ignore_ascii_case("it")
        && (suffix[1].raw.eq_ignore_ascii_case("can't")
            || suffix[1].raw.eq_ignore_ascii_case("can’t"))
        && suffix[2].raw.eq_ignore_ascii_case("be")
}

/// Supplies only the imperative spell envelope for non-targeting collection
/// destruction. The collection selector and every sibling action remain owned
/// by the committed Oracle automata.
fn is_promoted_collection_destroy_envelope(input: &AbilityInput<'_>) -> bool {
    matches!(input.tokens, [first, second, ..]
        if first.raw.eq_ignore_ascii_case("destroy")
            && (second.raw.eq_ignore_ascii_case("all")
                || second.raw.eq_ignore_ascii_case("each")))
}

/// Recognizes the imperative counter envelope directly from the shared token
/// stream. Selector and effect semantics remain owned by the committed
/// counter-family automata.
fn is_promoted_counter_envelope(input: &AbilityInput<'_>) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && input
            .tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
}

fn is_promoted_attack_tax_envelope(input: &AbilityInput<'_>) -> bool {
    input.tokens.get(0..3).is_some_and(|prefix| {
        prefix[0].raw.eq_ignore_ascii_case("creatures")
            && (prefix[1].raw.eq_ignore_ascii_case("can't")
                || prefix[1].raw.eq_ignore_ascii_case("can’t"))
            && prefix[2].raw.eq_ignore_ascii_case("attack")
    }) && input.tokens.windows(7).any(|window| {
        window[0].raw.eq_ignore_ascii_case("for")
            && window[1].raw.eq_ignore_ascii_case("each")
            && window[2].raw.eq_ignore_ascii_case("creature")
            && window[3].raw.eq_ignore_ascii_case("they")
            && window[4].raw.eq_ignore_ascii_case("control")
            && (window[5].raw.eq_ignore_ascii_case("that's")
                || window[5].raw.eq_ignore_ascii_case("that’s"))
            && window[6].raw.eq_ignore_ascii_case("attacking")
    })
}

/// Recognizes the complete root envelope `SPELL_COLLECTION cost MANA more to
/// cast.` without interpreting the spell predicate or mana symbols. Those
/// concepts remain owned by their canonical child automata. Exact suffix
/// consumption keeps reductions, activation taxes, and conditional
/// continuations outside this promoted family.
pub(crate) fn is_promoted_spell_cost_increase_envelope(input: &AbilityInput<'_>) -> bool {
    let tokens = input.tokens;
    let Some(cost_index) = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("cost"))
    else {
        return false;
    };
    if cost_index == 0
        || !tokens[..cost_index].iter().any(|token| {
            token.raw.eq_ignore_ascii_case("spell") || token.raw.eq_ignore_ascii_case("spells")
        })
    {
        return false;
    }

    let mut suffix_index = cost_index + 1;
    while tokens
        .get(suffix_index)
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        suffix_index += 1;
    }
    suffix_index > cost_index + 1
        && tokens.len() == suffix_index + 4
        && tokens[suffix_index].raw.eq_ignore_ascii_case("more")
        && tokens[suffix_index + 1].raw.eq_ignore_ascii_case("to")
        && tokens[suffix_index + 2].raw.eq_ignore_ascii_case("cast")
        && tokens[suffix_index + 3].raw == "."
}

/// Recognizes a complete fixed spell-cost-reduction envelope without
/// interpreting the selector or mana value. The canonical children own those
/// concepts; this hint only keeps unrelated reductions and continuations out.
pub(crate) fn is_promoted_static_spell_reduction_envelope(input: &AbilityInput<'_>) -> bool {
    let tokens = input.tokens;
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return false;
    }
    let Some(cost_index) = tokens.iter().position(|token| {
        token.raw.eq_ignore_ascii_case("cost") || token.raw.eq_ignore_ascii_case("costs")
    }) else {
        return false;
    };
    if cost_index == 0
        || !tokens[..cost_index].iter().any(|token| {
            token.raw.eq_ignore_ascii_case("spell") || token.raw.eq_ignore_ascii_case("spells")
        })
        || !tokens[..cost_index].iter().any(|token| {
            token.raw.eq_ignore_ascii_case("cast") || token.raw.eq_ignore_ascii_case("casts")
        })
    {
        return false;
    }

    let mut suffix_index = cost_index + 1;
    while tokens
        .get(suffix_index)
        .is_some_and(|token| token.token_type == "manaSymbol")
    {
        suffix_index += 1;
    }
    suffix_index > cost_index + 1
        && tokens.len() == suffix_index + 4
        && tokens[suffix_index].raw.eq_ignore_ascii_case("less")
        && tokens[suffix_index + 1].raw.eq_ignore_ascii_case("to")
        && tokens[suffix_index + 2].raw.eq_ignore_ascii_case("cast")
        && tokens[suffix_index + 3].raw == "."
}

/// An instant or sorcery's text is its spell ability (CR 113.3a, 608.2): a
/// leading one-shot `When EVENT, EFFECTS.` sentence whose event is a bound
/// object (`When that creature dies this turn`) or the controller's next
/// spell (`When you cast your next ... spell this turn`) is a rule the
/// resolving spell creates (CR 603.7c), read by the spell's effect sequence.
/// Any other leading trigger word stays a triggered ability, whose frame owns
/// the nonpermanent-face refusal (CR 113.6).
fn is_spell_face_created_rule_sentence(input: &AbilityInput<'_>) -> bool {
    let type_line = input.face_type_line;
    let card_types = type_line.split('\u{2014}').next().unwrap_or(type_line);
    let spell_face = card_types
        .split(|character: char| !character.is_alphabetic())
        .any(|part| part.eq_ignore_ascii_case("instant") || part.eq_ignore_ascii_case("sorcery"));
    if !spell_face || type_line_has_permanent_card_type(type_line) {
        return false;
    }
    let tokens = input.tokens;
    let word = |index: usize, expected: &str| {
        tokens
            .get(index)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
    };
    if !word(0, "when") && !word(0, "whenever") {
        return false;
    }
    let next_spell = word(1, "you")
        && (word(2, "next") || (word(2, "cast") && word(3, "your") && word(4, "next")));
    let bound_object = word(1, "it") || word(1, "that");
    // `Whenever EVENT this turn,`: an event bounded by the created
    // trigger's own lifetime (CR 603.7, 611.2a).
    let lifetime = (1..tokens.len())
        .take_while(|index| tokens[*index].raw != ".")
        .any(|index| word(index, "this") && word(index + 1, "turn") && word(index + 2, ","));
    (word(0, "when") && (next_spell || bound_object)) || lifetime
}

/// Supplies a token-only dispatch hint for promoted envelopes. `None` leaves
/// unclaimed families to structural native dispatch and explicit failure.
pub(crate) fn classify_promoted_ability_from_tokens(
    input: &AbilityInput<'_>,
) -> Option<&'static str> {
    let ability_word_width = input
        .tokens
        .iter()
        .take_while(|token| token.token_type == "abilityWord")
        .count();
    if ability_word_width > 0
        && input
            .tokens
            .get(ability_word_width)
            .is_some_and(|separator| {
                separator.token_type == "separator" && separator.raw == "\u{2014}"
            })
        && input
            .tokens
            .get(ability_word_width + 1)
            .is_some_and(|trigger| {
                trigger.token_type == "triggerWord"
                    && ["when", "whenever", "at"]
                        .iter()
                        .any(|head| trigger.raw.eq_ignore_ascii_case(head))
            })
    {
        return Some("triggeredAbility");
    }
    if is_spell_face_created_rule_sentence(input) {
        return Some("spellAbility");
    }
    if input.tokens.first().is_some_and(|token| {
        token.token_type == "triggerWord"
            && ["when", "whenever", "at"]
                .iter()
                .any(|head| token.raw.eq_ignore_ascii_case(head))
    }) {
        return Some("triggeredAbility");
    }
    if root_activation_effect_start(input).is_some() {
        return Some("activatedAbility");
    }
    if is_promoted_mana_activation_envelope(input) {
        return Some("activatedAbility");
    }
    if is_promoted_library_search_activation_envelope(input) {
        return Some("activatedAbility");
    }
    if is_promoted_destroy_regeneration_envelope(input) {
        return Some("spellAbility");
    }
    if is_promoted_collection_destroy_envelope(input) {
        return Some("spellAbility");
    }
    if is_promoted_counter_envelope(input) {
        return Some("spellAbility");
    }
    if is_promoted_source_entry_state_envelope(input) {
        return Some("replacementEffect");
    }
    if is_promoted_controlled_count_entry_envelope(input) {
        return Some("replacementEffect");
    }
    if is_promoted_attack_tax_envelope(input) {
        return Some("staticAbility");
    }
    if is_promoted_spell_cost_increase_envelope(input) {
        return Some("staticAbility");
    }
    if is_promoted_static_spell_reduction_envelope(input) {
        return Some("staticAbility");
    }
    None
}
