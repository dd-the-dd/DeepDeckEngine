//! A sentence conditional (CR 608.2c) lowers its Boolean program with the
//! one condition lowering (`condition.rs`), at the condition's position in
//! the shared sequence lowering, so that referenced objects resolve to the
//! declaration or bound card result their symbolic binding names. This
//! module keeps the real-card proofs of that path.

#[cfg(test)]
#[path = "sentence_condition_tests.rs"]
mod tests;
