mod repository;
pub use repository::{Verdicts, VerdictGuard};
mod value;
pub use value::Verdict;

mod cancellation;
pub use cancellation::Cancellation;
