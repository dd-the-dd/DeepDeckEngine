//! Validation of the executable engine IR, grouped by operation family.
//!
//! Family validators return `None` for unknown operation kinds so the root
//! dispatcher can remain responsible for composing independently testable
//! validation modules.

mod damage;
mod tokens;

pub(super) use damage::damage_effect_supported;
pub(super) use tokens::{copy_exception_supported, token_copy_effect_supported};

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
mod damage_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
mod token_tests;
