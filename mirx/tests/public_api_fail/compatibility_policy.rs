use mirx::CompatibilityPolicy;
use mirx::document::CompatibilityPolicy as DocumentCompatibilityPolicy;

fn main() {
    let _ = CompatibilityPolicy::Preserve;
    let _ = DocumentCompatibilityPolicy::NormalizeToCurrent;
}
