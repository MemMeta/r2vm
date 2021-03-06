//! Entropy devices.
//!
//! This module provides a [`Entropy`] trait which bridges entropy source implementation and
//! I/O devices implementations.

pub use rand;

#[doc(no_inline)]
pub use rand::rngs::OsRng as Os;
#[doc(no_inline)]
pub use rand::rngs::StdRng as Seeded;
#[doc(no_inline)]
pub use rand::RngCore as Entropy;
