//! Public (CloudEvents-formatted) trace entry.

use serde::{Deserialize, Serialize};
use traverse_contracts::ViolationRecord;

/// Outcome of a capability execution recorded in the public trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceOutcome {
    /// The capability completed successfully.
    Success,
    /// The capability failed.
    Failure,
}

/// A CloudEvents-formatted public trace entry.
///
/// Always logged and safe to share. Contains no raw inputs or outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTraceEntry {
    /// UUID v4 string identifying this trace.
    pub id: String,
    /// `CloudEvents` source: `traverse-runtime/<capability_id>`.
    pub source: String,
    /// `CloudEvents` type: `dev.traverse.execution.completed`.
    pub event_type: String,
    /// `CloudEvents` data content type: `application/json`.
    pub datacontenttype: String,
    /// RFC 3339 timestamp of when the trace was recorded.
    pub time: String,
    /// Identifier of the capability that was executed.
    pub capability_id: String,
    /// Placement target used during execution.
    pub placement_target: String,
    /// Whether the execution succeeded or failed.
    pub outcome: TraceOutcome,
    /// Wall-clock duration of the execution in milliseconds.
    pub duration_ms: u64,
    /// Aggregate contractual enforcement violations (if any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<ViolationRecord>,
}

impl PublicTraceEntry {
    /// Creates a new [`PublicTraceEntry`].
    #[must_use]
    pub fn new(
        id: String,
        capability_id: String,
        placement_target: String,
        outcome: TraceOutcome,
        duration_ms: u64,
        time: String,
    ) -> Self {
        let source = format!("traverse-runtime/{capability_id}");
        Self {
            id,
            source,
            event_type: "dev.traverse.execution.completed".to_string(),
            datacontenttype: "application/json".to_string(),
            time,
            capability_id,
            placement_target,
            outcome,
            duration_ms,
            violations: Vec::new(),
        }
    }
}
