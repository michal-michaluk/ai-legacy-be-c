//! The notification payload — a config `key => value` change fact.

use serde::Serialize;

/// One observed change of a config `key => value`, carrying enough context for a
/// subscriber (another microservice) to react. Pure data — no framework type.
#[derive(Clone, Debug, Serialize)]
pub struct ConfigChange {
    /// Publishing service instance (`SERVICE_NAME`).
    pub service: String,
    /// `upsert` (put/patch) or `delete`.
    pub op: &'static str,
    pub tenant: String,
    pub key: String,
    /// The new value; `None` for a delete.
    pub value: Option<serde_json::Value>,
    /// Optimistic-lock version after the change (0 for delete).
    pub version: u64,
}
