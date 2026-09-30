//! Evidence metrics only; never linked into the instrument plugin.
pub mod metrics;
pub mod diagnostics;

#[cfg(all(feature = "engine-heavy", feature = "engine-fast"))]
compile_error!("Select exactly one engine mode; both backends cannot share one comparison.");
#[cfg(not(any(feature = "engine-heavy", feature = "engine-fast")))]
compile_error!("Select engine-heavy or engine-fast for both reference and candidate.");
