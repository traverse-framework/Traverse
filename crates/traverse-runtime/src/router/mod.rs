//! Governed by spec 016-runtime-placement-router
//!
//! `PlacementRouter` is the single public entry point for all capability execution
//! in `traverse-runtime`.  It wires together:
//!
//! 1. Placement evaluation ([`PlacementConstraintEvaluator`])
//! 2. Executor selection ([`CapabilityExecutorRegistry`])
//! 3. Execution ([`CapabilityExecutor`])
//! 4. Trace recording ([`TraceStore`])
//! 5. Conditional event publishing ([`EventBroker`])

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use serde_json::Value;
use traverse_contracts::{CapabilityContract, ServiceType, ViolationRecord};

use crate::{
    events::types::{EventBroker, TraverseEvent},
    executor::{ArtifactType, CapabilityExecutor, ExecutorCapability},
    placement::{
        PlacementConstraintEvaluator, PlacementDecision, PlacementError, PlacementRequest,
        RuntimeSnapshot,
    },
    trace::{PrivateTraceEntry, PublicTraceEntry, TraceOutcome, TraceStore, new_trace_id_and_time},
};

use traverse_contracts::ExecutionTarget;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Maps [`ArtifactType`] to the appropriate [`CapabilityExecutor`] implementation.
pub type CapabilityExecutorRegistry = HashMap<ArtifactType, Box<dyn CapabilityExecutor>>;

/// Input to [`PlacementRouter::execute`].
pub struct RouterRequest {
    /// Unique capability identifier.
    pub capability_id: String,
    /// How the capability is packaged.
    pub artifact_type: ArtifactType,
    /// The validated contract for this capability (used for placement evaluation).
    pub contract: CapabilityContract,
    /// Optional caller hint for target placement.
    pub target_hint: Option<ExecutionTarget>,
    /// Current runtime load snapshot used by the placement evaluator.
    pub runtime_snapshot: RuntimeSnapshot,
    /// JSON input payload for the capability.
    pub input: Value,
    /// Resolved capability descriptor passed to the executor.
    pub executor_capability: ExecutorCapability,
    /// Events emitted by the capability (only published when `service_type == Subscribable`).
    pub emitted_events: Vec<TraverseEvent>,
}

/// Errors returned by [`PlacementRouter::execute`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterError {
    /// The placement constraint evaluator rejected the request.
    PlacementFailed(PlacementError),
    /// No executor is registered for the requested [`ArtifactType`].
    ExecutorNotFound(String),
    /// The selected executor returned an error.
    ExecutionFailed(String),
    /// Execution violated a governed contract (aggregate violations).
    ContractViolation(Vec<ViolationRecord>),
    /// The trace store lock was poisoned.
    TraceLockPoisoned,
}

impl std::fmt::Display for RouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlacementFailed(e) => write!(f, "placement failed: {e:?}"),
            Self::ExecutorNotFound(t) => write!(f, "no executor registered for artifact type: {t}"),
            Self::ExecutionFailed(msg) => write!(f, "execution failed: {msg}"),
            Self::ContractViolation(violations) => {
                write!(f, "contract violation: {} violation(s)", violations.len())
            }
            Self::TraceLockPoisoned => write!(f, "trace store lock is poisoned"),
        }
    }
}

impl std::error::Error for RouterError {}

/// Result of a successful [`PlacementRouter::execute`] call.
#[derive(Debug)]
pub struct RouterResponse {
    /// The JSON output produced by the executor.
    pub output: Value,
    /// The public trace entry written to the store.
    pub trace_id: String,
    /// The placement decision that was made.
    pub placement_decision: PlacementDecision,
}

// ---------------------------------------------------------------------------
// PlacementRouter
// ---------------------------------------------------------------------------

/// Single orchestrating entry point for all capability execution in Traverse.
///
/// Wires together placement evaluation → executor selection → execution →
/// trace recording → event publishing.
pub struct PlacementRouter {
    evaluator: PlacementConstraintEvaluator,
    executor_registry: CapabilityExecutorRegistry,
    trace_store: Arc<Mutex<TraceStore>>,
    event_broker: Arc<dyn EventBroker>,
}

impl PlacementRouter {
    /// Construct a new [`PlacementRouter`] from injected dependencies.
    #[must_use]
    pub fn new(
        evaluator: PlacementConstraintEvaluator,
        executor_registry: CapabilityExecutorRegistry,
        trace_store: Arc<Mutex<TraceStore>>,
        event_broker: Arc<dyn EventBroker>,
    ) -> Self {
        Self {
            evaluator,
            executor_registry,
            trace_store,
            event_broker,
        }
    }

    /// Execute a capability end-to-end.
    ///
    /// Steps:
    /// 1. Evaluate placement constraints — returns [`RouterError::PlacementFailed`] with no trace on failure.
    /// 2. Select executor by `artifact_type`.
    /// 3. Run the executor.
    /// 4. Write public + private trace entries to the store.
    /// 5. If `service_type == Subscribable`, publish emitted events.
    ///
    /// # Errors
    ///
    /// Returns [`RouterError`] when any step cannot complete.
    pub fn execute(&self, request: RouterRequest) -> Result<RouterResponse, RouterError> {
        // --- Step 1: Placement evaluation ---
        let placement_req = PlacementRequest {
            capability_id: request.capability_id.clone(),
            target_hint: request.target_hint,
            runtime_snapshot: request.runtime_snapshot,
        };

        let decision = self
            .evaluator
            .evaluate(&placement_req, &request.contract)
            .map_err(RouterError::PlacementFailed)?;

        let placement_target_str = format!("{:?}", decision.target);

        // --- Step 2: Executor selection ---
        let executor = self
            .executor_registry
            .get(&request.artifact_type)
            .ok_or_else(|| RouterError::ExecutorNotFound(format!("{:?}", request.artifact_type)))?;

        // --- Step 3: Execute capability ---
        let start = Instant::now();
        let exec_result = executor.execute(&request.executor_capability, &request.input);
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        let (output, outcome) = match exec_result {
            Ok(v) => (v, TraceOutcome::Success),
            Err(e) => return Err(RouterError::ExecutionFailed(format!("{e}"))),
        };

        // --- Step 3.5: Execution-time contractual enforcement gate ---
        let mut violations = Vec::new();
        if request.contract.service_type == ServiceType::Subscribable
            && !request.emitted_events.is_empty()
        {
            for event in &request.emitted_events {
                let declared =
                    request.contract.emits.iter().any(|decl| {
                        decl.event_id == event.event_type && decl.version == event.version
                    });
                if !declared {
                    violations.push(ViolationRecord::new(
                        "undeclared_event_emission",
                        &request.capability_id,
                        format!(
                            "capability emitted undeclared event {}@{}",
                            event.event_type, event.version
                        ),
                    ));
                }
            }
        }

        let outcome = if violations.is_empty() {
            outcome
        } else {
            TraceOutcome::Failure
        };

        // --- Step 4: Write trace ---
        let (trace_id, time) = new_trace_id_and_time();

        let mut public_entry = PublicTraceEntry::new(
            trace_id.clone(),
            request.capability_id.clone(),
            placement_target_str,
            outcome,
            duration_ms,
            time,
        );
        public_entry.violations.clone_from(&violations);

        let input_str = serde_json::to_string(&request.input).unwrap_or_default();
        let output_str = serde_json::to_string(&output).unwrap_or_default();
        let private_entry =
            PrivateTraceEntry::new(trace_id.clone(), &input_str, &output_str, duration_ms);

        {
            let mut store = self
                .trace_store
                .lock()
                .map_err(|_| RouterError::TraceLockPoisoned)?;
            store.insert(public_entry, Some(private_entry));
        }

        if !violations.is_empty() {
            return Err(RouterError::ContractViolation(violations));
        }

        // --- Step 5: Publish events for Subscribable capabilities ---
        if request.contract.service_type == ServiceType::Subscribable {
            for event in request.emitted_events {
                // Best-effort: publish errors are logged but do not fail the response.
                let _ = self.event_broker.publish(event);
            }
        }

        Ok(RouterResponse {
            output,
            trace_id,
            placement_decision: decision,
        })
    }
}
