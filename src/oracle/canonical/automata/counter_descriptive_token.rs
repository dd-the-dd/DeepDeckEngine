use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CounterEffectNode, CreateTokensEffectNode, GrantedKeyword, LocalEffectNode, TargetBinding,
    recognize_counter_effect, recognize_create_creature_tokens_effect, recognize_keyword_list,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterDescriptiveTokenSequenceNode {
    pub counter: CounterEffectNode,
    pub token_creation: CreateTokensEffectNode,
    pub keywords: Vec<GrantedKeyword>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterDescriptiveTokenState {
    Counter,
    SentenceBoundary,
    TokenCreation,
    KeywordRelation,
    Keywords,
    TokenTerminator,
    Complete,
}

/// Composes an atomic counter effect with a descriptive creature-token
/// creation linked to the counter target's controller. Target selection,
/// player anaphora, numeric characteristics, colors, subtypes, and keywords
/// remain independent typed child machines.
pub(in crate::oracle::canonical) fn recognize_counter_descriptive_token_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterDescriptiveTokenSequenceNode> {
    let mut state = CounterDescriptiveTokenState::Counter;
    let mut counter = None;
    let mut token_creation = None;
    let mut keywords = None;

    loop {
        state = match state {
            CounterDescriptiveTokenState::Counter => {
                cursor.enter("sequence.counter-descriptive-token.counter");
                counter = Some(match recognize_counter_effect(cursor)? {
                    LocalEffectNode::CounterSpell(counter) => counter,
                    _ => return Err(cursor.fail("an atomic counter effect")),
                });
                CounterDescriptiveTokenState::SentenceBoundary
            }
            CounterDescriptiveTokenState::SentenceBoundary => {
                cursor.enter("sequence.counter-descriptive-token.sentence-boundary");
                cursor.consume(".")?;
                CounterDescriptiveTokenState::TokenCreation
            }
            CounterDescriptiveTokenState::TokenCreation => {
                cursor.enter("sequence.counter-descriptive-token.creation");
                token_creation = Some(recognize_create_creature_tokens_effect(
                    cursor,
                    Some(TargetBinding {
                        effect_index: 0,
                        slot_index: 0,
                    }),
                )?);
                CounterDescriptiveTokenState::KeywordRelation
            }
            CounterDescriptiveTokenState::KeywordRelation => {
                cursor.enter("sequence.counter-descriptive-token.keyword-relation");
                cursor.consume("with")?;
                CounterDescriptiveTokenState::Keywords
            }
            CounterDescriptiveTokenState::Keywords => {
                cursor.enter("sequence.counter-descriptive-token.keywords");
                keywords = Some(recognize_keyword_list(cursor)?);
                CounterDescriptiveTokenState::TokenTerminator
            }
            CounterDescriptiveTokenState::TokenTerminator => {
                cursor.enter("sequence.counter-descriptive-token.creation-terminator");
                cursor.consume(".")?;
                CounterDescriptiveTokenState::Complete
            }
            CounterDescriptiveTokenState::Complete => {
                cursor.enter("sequence.counter-descriptive-token.complete");
                cursor.finish()?;
                return Ok(CounterDescriptiveTokenSequenceNode {
                    counter: cursor.require_value(counter, "a populated counter effect")?,
                    token_creation: cursor
                        .require_value(token_creation, "a populated descriptive token creation")?,
                    keywords: cursor.require_value(keywords, "one or more token keywords")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "counter_descriptive_token_tests.rs"]
mod tests;
