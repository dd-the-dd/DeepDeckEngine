use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::regex_cache::Regex;

use super::{AbilityInput, NameLinkKind, strip_short_oracle_label};

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

/// Supplies a token-only dispatch hint for promoted envelopes. `None` leaves
/// unpromoted families behind the explicit legacy-classifier boundary.
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

pub(crate) fn activation_parts(text: &str) -> Option<(&str, &str)> {
    let text = strip_short_oracle_label(text);
    let activation_re = Regex::new(
        r"(?i)^\s*(?:\(\s*)?(?:(?:\{[^}]+\})+|Pay \d+ life)(?:\s*,\s*(?:(?:\{[^}]+\})+|Pay \d+ life))*\s*:",
    )
    .expect("activation prefix regex compiles");
    if let Some(matched) = activation_re.find(text) {
        return Some((&text[..matched.end() - 1], &text[matched.end()..]));
    }
    let mut parenthesis_depth = 0_u32;
    let mut quoted = false;
    let mut separator = None;
    for (index, character) in text.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => parenthesis_depth += 1,
            ')' if !quoted => parenthesis_depth = parenthesis_depth.saturating_sub(1),
            ':' if !quoted && parenthesis_depth == 0 => {
                separator = Some(index);
                break;
            }
            _ => {}
        }
    }
    let separator = separator?;
    let cost = &text[..separator];
    let effect = &text[separator + 1..];
    (!cost.trim().is_empty() && !effect.trim().is_empty()).then_some((cost, effect))
}

pub(crate) fn classify_ability(input: &AbilityInput<'_>) -> &'static str {
    let text = input.source.text.trim();
    let normalized = strip_short_oracle_label(text.trim_start_matches('(').trim_start());
    let lower = normalized.to_ascii_lowercase();
    let starts_with_current_name = input.name_links.iter().any(|link| {
        link.token_start == 0
            && matches!(
                link.kind,
                NameLinkKind::CurrentCard | NameLinkKind::CurrentFace
            )
    });

    if activation_parts(text).is_some() {
        return "activatedAbility";
    }
    if input
        .tokens
        .first()
        .is_some_and(|token| token.token_type == "keywordWord")
        && !starts_with_current_name
    {
        return "keywordAbility";
    }
    if lower.starts_with("when ") || lower.starts_with("whenever ") || lower.starts_with("at ") {
        return "triggeredAbility";
    }
    let standalone_enters_with = !lower.starts_with("graft ")
        && lower
            .split_once(" enters with ")
            .is_some_and(|(subject, _)| !subject.contains('.'));
    let standalone_enters_tapped = lower
        .strip_suffix(" enters tapped.")
        .is_some_and(|subject| !subject.contains('.'));
    if Regex::new(r"(?i)^as .+ enters,")
        .expect("as-enters classifier regex compiles")
        .is_match(normalized)
        || lower.contains(" enters tapped unless ")
        || standalone_enters_with
        || standalone_enters_tapped
        || lower.contains(" enters prepared")
    {
        return "replacementEffect";
    }
    let keyword_list_candidate = (lower.contains(",") || lower.contains(" and "))
        && (Regex::new(
            r"(?i)^(changeling|deathtouch|defender|devoid|double strike|first strike|flash|flying|haste|hexproof|indestructible|lifelink|menace|myriad|prowess|reach|trample|vigilance)\b|^firebending\s+(\d+|x\b)|^mobilize\s+\d+|^ward(\s+\{|—|â€”|Ã¢â‚¬â€)",
        )
        .expect("keyword list classifier regex compiles")
        .is_match(normalized)
            || Regex::new(r"(?i)^blitz\s+\{")
                .expect("blitz keyword list classifier regex compiles")
                .is_match(normalized));
    if keyword_list_candidate {
        return "keywordAbilityGroup";
    }
    let firebending_keyword_candidate = Regex::new(r"(?i)^firebending\s+(\d+|x\b)")
        .expect("firebending keyword classifier regex compiles")
        .is_match(normalized);
    if lower == "flying"
        || lower.starts_with("graft ")
        || lower.starts_with("kicker ")
        || lower.starts_with("blitz ")
        || lower.starts_with("ward")
        || firebending_keyword_candidate
        || lower.starts_with("offspring ")
        || lower.starts_with("paradigm ")
    {
        return "keywordAbility";
    }
    if lower.starts_with("this spell can't be countered")
        || lower.starts_with("activated abilities of sources")
        || lower.starts_with("lands with the chosen name")
    {
        return "staticAbility";
    }
    if input
        .face_type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("instant") || part.eq_ignore_ascii_case("sorcery"))
    {
        return "spellAbility";
    }
    "staticAbility"
}
