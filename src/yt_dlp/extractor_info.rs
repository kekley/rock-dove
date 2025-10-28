use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractorInfo {
    extractor: String,
    extractor_key: String,
}
