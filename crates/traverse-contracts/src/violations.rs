use serde::{Deserialize, Serialize};

/// Stable, machine-readable representation of a governed contract violation.
///
/// Governed by spec 040-contractual-enforcement-gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViolationRecord {
    pub violation_code: String,
    pub path: String,
    pub message: String,
}

impl ViolationRecord {
    #[must_use]
    pub fn new(
        violation_code: impl Into<String>,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            violation_code: violation_code.into(),
            path: path.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ViolationRecord;

    #[test]
    fn violation_record_new_round_trips_through_json() {
        let record = ViolationRecord::new("missing_required_field", "$.id", "id is required");
        let json = serde_json::to_string(&record);
        assert!(json.is_ok(), "serialize must succeed: {json:?}");
        let json = json.unwrap_or_default();

        let decoded = serde_json::from_str::<ViolationRecord>(&json);
        assert!(decoded.is_ok(), "deserialize must succeed: {decoded:?}");
        let decoded = decoded.unwrap_or_else(|_| record.clone());
        assert_eq!(decoded, record);
    }
}
