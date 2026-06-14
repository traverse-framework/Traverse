use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use traverse_contracts::{
    CapabilityContract, ConnectorRequirement, ErrorSeverity, ExecutionTarget,
    governed_content_digest, parse_contract,
};

use crate::{
    ArtifactDigests, BinaryFormat, BinaryReference, CapabilityArtifactRecord,
    CapabilityRegistration, CapabilityRegistry, ComposabilityMetadata, CompositionKind,
    CompositionPattern, EventRegistry, ImplementationKind, LookupScope, RegistryProvenance,
    RegistryScope, SourceKind, SourceReference, WorkflowDefinition, WorkflowRegistration,
    WorkflowRegistry,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationBundleManifest {
    pub app_id: String,
    pub version: String,
    pub schema_version: String,
    pub workspace_defaults: Value,
    pub components: Vec<ApplicationComponent>,
    pub workflows: Vec<ApplicationWorkflowRef>,
    pub model_dependencies: Vec<ApplicationModelDependency>,
    pub config_schema: Value,
    pub default_config: Value,
    pub placement_policy: Value,
    pub public_surfaces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRegistryRecord {
    pub scope: RegistryScope,
    pub workspace_id: String,
    pub app_id: String,
    pub version: String,
    pub manifest_path: String,
    pub manifest_digest: String,
    pub bundle_digest: String,
    pub registered_at: String,
    pub readiness_status: ApplicationReadinessStatus,
    pub components: Vec<ApplicationRegisteredComponent>,
    pub workflows: Vec<ApplicationRegisteredWorkflow>,
    pub inspection_link: String,
    pub execution_links: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationReadinessStatus {
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplicationRegisteredComponent {
    pub component_id: String,
    pub component_version: String,
    pub capability_id: String,
    pub capability_version: String,
    pub wasm_digest: String,
    pub artifact_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplicationRegisteredWorkflow {
    pub workflow_id: String,
    pub workflow_version: String,
    pub workflow_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationRegistrationStatus {
    Created,
    AlreadyRegistered,
}

impl ApplicationRegistrationStatus {
    #[must_use]
    pub fn http_status(self) -> u16 {
        match self {
            Self::Created => 201,
            Self::AlreadyRegistered => 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRegistrationOutcome {
    pub status: ApplicationRegistrationStatus,
    pub record: ApplicationRegistryRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRegistrationRequest {
    pub scope: RegistryScope,
    pub workspace_id: String,
    pub manifest_path: PathBuf,
    pub registered_at: String,
    pub validator_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationRegistrationErrorCode {
    ManifestValidationFailed,
    MissingRequiredEvent,
    WorkflowReadFailed,
    WorkflowParseFailed,
    WorkflowReferenceMismatch,
    CapabilityRegistrationFailed,
    WorkflowRegistrationFailed,
    ImmutableApplicationVersionConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRegistrationError {
    pub code: ApplicationRegistrationErrorCode,
    pub path: String,
    pub message: String,
    pub severity: ErrorSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationRegistrationFailure {
    pub errors: Vec<ApplicationRegistrationError>,
}

#[derive(Debug, Clone, Default)]
pub struct ApplicationRegistry {
    records: BTreeMap<(RegistryScope, String, String), ApplicationRegistryRecord>,
}

impl ApplicationRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a complete application bundle atomically.
    ///
    /// The method validates the app manifest, component manifests, component
    /// contracts, referenced WASM digests, required event references, and
    /// workflow references before any caller-visible registry state is
    /// replaced. Failed registration attempts leave the application,
    /// capability, and workflow registries unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ApplicationRegistrationFailure`] when manifest validation,
    /// dependency validation, capability registration, workflow registration,
    /// or immutable application id/version checks fail.
    pub fn register_bundle(
        &mut self,
        capabilities: &mut CapabilityRegistry,
        events: &EventRegistry,
        workflows: &mut WorkflowRegistry,
        request: &ApplicationRegistrationRequest,
    ) -> Result<ApplicationRegistrationOutcome, ApplicationRegistrationFailure> {
        let manifest = load_application_bundle_manifest(&request.manifest_path)
            .map_err(map_manifest_failure)?;
        let workflow_artifacts = load_application_workflows(&request.manifest_path, &manifest)?;
        validate_component_event_references(events, request.scope, &manifest.components)?;

        let mut staged_apps = self.clone();
        let mut staged_capabilities = capabilities.clone();
        let mut staged_workflows = workflows.clone();
        let mut registered_components = Vec::new();
        let mut registered_workflows = Vec::new();

        for component in &manifest.components {
            let registration =
                build_application_capability_registration(&manifest, component, request);
            let outcome = staged_capabilities
                .register(registration)
                .map_err(|failure| map_capability_registration_failure(component, failure))?;
            registered_components.push(ApplicationRegisteredComponent {
                component_id: component.manifest.component_id.clone(),
                component_version: component.manifest.version.clone(),
                capability_id: outcome.record.id,
                capability_version: outcome.record.version,
                wasm_digest: component.verified_wasm_digest.clone(),
                artifact_ref: outcome.artifact.artifact_ref,
            });
        }

        for workflow in workflow_artifacts {
            let outcome = staged_workflows
                .register(
                    &staged_capabilities,
                    WorkflowRegistration {
                        scope: request.scope,
                        definition: workflow.definition,
                        workflow_path: workflow.path.display().to_string(),
                        registered_at: request.registered_at.clone(),
                        validator_version: request.validator_version.clone(),
                    },
                )
                .map_err(map_workflow_registration_failure)?;
            registered_workflows.push(ApplicationRegisteredWorkflow {
                workflow_id: outcome.record.id,
                workflow_version: outcome.record.version,
                workflow_digest: outcome.record.workflow_digest,
            });
        }

        let record = build_application_record(
            request,
            &manifest,
            registered_components,
            registered_workflows,
        );
        let key = (
            request.scope,
            manifest.app_id.clone(),
            manifest.version.clone(),
        );
        let status = staged_apps.reconcile_or_insert(key, record.clone())?;

        *self = staged_apps;
        *capabilities = staged_capabilities;
        *workflows = staged_workflows;

        Ok(ApplicationRegistrationOutcome { status, record })
    }

    #[must_use]
    pub fn find_exact(
        &self,
        scope: RegistryScope,
        app_id: &str,
        version: &str,
    ) -> Option<&ApplicationRegistryRecord> {
        self.records
            .get(&(scope, app_id.to_string(), version.to_string()))
    }

    fn reconcile_or_insert(
        &mut self,
        key: (RegistryScope, String, String),
        record: ApplicationRegistryRecord,
    ) -> Result<ApplicationRegistrationStatus, ApplicationRegistrationFailure> {
        if let Some(existing) = self.records.get(&key) {
            if existing.bundle_digest == record.bundle_digest
                && existing.components == record.components
                && existing.workflows == record.workflows
            {
                return Ok(ApplicationRegistrationStatus::AlreadyRegistered);
            }
            return Err(single_registration_error(
                ApplicationRegistrationErrorCode::ImmutableApplicationVersionConflict,
                "$.version",
                "registered application versions are immutable within a scope",
            ));
        }

        self.records.insert(key, record);
        Ok(ApplicationRegistrationStatus::Created)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationComponent {
    pub reference: ApplicationComponentRef,
    pub manifest_path: PathBuf,
    pub manifest: WasmComponentManifest,
    pub contract_path: PathBuf,
    pub contract: CapabilityContract,
    pub wasm_binary_path: PathBuf,
    pub verified_wasm_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoadedApplicationWorkflow {
    path: PathBuf,
    definition: WorkflowDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ApplicationComponentRef {
    pub component_id: String,
    pub version: String,
    pub digest: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ApplicationWorkflowRef {
    pub workflow_id: String,
    pub workflow_version: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ApplicationModelDependency {
    pub dependency_id: String,
    pub requirement_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmComponentManifest {
    pub component_id: String,
    pub version: String,
    pub schema_version: String,
    pub capability_id: String,
    pub capability_version: String,
    pub contract_path: String,
    pub wasm_binary_path: String,
    pub wasm_digest: String,
    pub runtime_constraints: Value,
    pub permitted_targets: Vec<ExecutionTarget>,
    pub dependencies: Vec<WasmComponentDependency>,
    pub connector_requirements: Vec<ConnectorRequirement>,
    pub validation_evidence: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WasmComponentDependency {
    pub component_id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub version_range: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationManifestErrorCode {
    ManifestParentMissing,
    ManifestReadFailed,
    ManifestParseFailed,
    DuplicateComponentReference,
    AppComponentManifestMissing,
    ComponentManifestReadFailed,
    ComponentManifestParseFailed,
    ComponentReferenceMismatch,
    ComponentContractMissing,
    ComponentContractParseFailed,
    ComponentContractMismatch,
    ComponentWasmMissing,
    InvalidDigestMetadata,
    ComponentDigestMismatch,
    ComponentDependencyMustBeConcrete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationManifestError {
    pub code: ApplicationManifestErrorCode,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationManifestFailure {
    pub errors: Vec<ApplicationManifestError>,
}

#[derive(Debug, Deserialize)]
struct ApplicationManifestSerde {
    app_id: String,
    version: String,
    schema_version: String,
    workspace_defaults: Value,
    components: Vec<ApplicationComponentRef>,
    workflows: Vec<ApplicationWorkflowRef>,
    model_dependencies: Vec<ApplicationModelDependency>,
    config_schema: Value,
    default_config: Value,
    placement_policy: Value,
    public_surfaces: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct WasmComponentManifestSerde {
    component_id: String,
    version: String,
    schema_version: String,
    capability_id: String,
    capability_version: String,
    contract_path: String,
    wasm_binary_path: String,
    wasm_digest: String,
    runtime_constraints: Value,
    permitted_targets: Vec<ExecutionTarget>,
    dependencies: Vec<WasmComponentDependency>,
    connector_requirements: Vec<ConnectorRequirement>,
    validation_evidence: Vec<Value>,
}

/// Loads and validates a Traverse application manifest plus its referenced
/// concrete WASM component manifests.
///
/// # Errors
///
/// Returns [`ApplicationManifestFailure`] when the app manifest, component
/// manifests, contracts, concrete component dependencies, or WASM digests are
/// invalid for spec `044-application-bundle-manifest`.
pub fn load_application_bundle_manifest(
    manifest_path: &Path,
) -> Result<ApplicationBundleManifest, ApplicationManifestFailure> {
    let manifest_dir = manifest_path.parent().ok_or_else(|| {
        single_error(
            ApplicationManifestErrorCode::ManifestParentMissing,
            manifest_path.display().to_string(),
            format!(
                "application manifest {} has no parent directory",
                manifest_path.display()
            ),
        )
    })?;

    let manifest_contents = fs::read_to_string(manifest_path).map_err(|error| {
        single_error(
            ApplicationManifestErrorCode::ManifestReadFailed,
            manifest_path.display().to_string(),
            format!(
                "failed to read application manifest {}: {error}",
                manifest_path.display()
            ),
        )
    })?;

    let manifest: ApplicationManifestSerde =
        serde_json::from_str(&manifest_contents).map_err(|error| {
            single_error(
                ApplicationManifestErrorCode::ManifestParseFailed,
                manifest_path.display().to_string(),
                format!(
                    "failed to parse application manifest {}: {error}",
                    manifest_path.display()
                ),
            )
        })?;

    ensure_unique_component_refs(&manifest.components)?;

    let components = manifest
        .components
        .iter()
        .map(|component| load_component(manifest_dir, component))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ApplicationBundleManifest {
        app_id: manifest.app_id,
        version: manifest.version,
        schema_version: manifest.schema_version,
        workspace_defaults: manifest.workspace_defaults,
        components,
        workflows: manifest.workflows,
        model_dependencies: manifest.model_dependencies,
        config_schema: manifest.config_schema,
        default_config: manifest.default_config,
        placement_policy: manifest.placement_policy,
        public_surfaces: manifest.public_surfaces,
    })
}

fn load_application_workflows(
    manifest_path: &Path,
    manifest: &ApplicationBundleManifest,
) -> Result<Vec<LoadedApplicationWorkflow>, ApplicationRegistrationFailure> {
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new(""));

    manifest
        .workflows
        .iter()
        .map(|workflow| load_application_workflow(manifest_dir, workflow))
        .collect()
}

fn load_application_workflow(
    manifest_dir: &Path,
    workflow: &ApplicationWorkflowRef,
) -> Result<LoadedApplicationWorkflow, ApplicationRegistrationFailure> {
    let path = manifest_dir.join(&workflow.path);
    let contents = fs::read_to_string(&path).map_err(|error| {
        single_registration_error(
            ApplicationRegistrationErrorCode::WorkflowReadFailed,
            path.display().to_string(),
            &format!(
                "failed to read workflow {}@{} at {}: {error}",
                workflow.workflow_id,
                workflow.workflow_version,
                path.display()
            ),
        )
    })?;
    let definition = serde_json::from_str::<WorkflowDefinition>(&contents).map_err(|error| {
        single_registration_error(
            ApplicationRegistrationErrorCode::WorkflowParseFailed,
            path.display().to_string(),
            &format!(
                "failed to parse workflow {}@{} at {}: {error}",
                workflow.workflow_id,
                workflow.workflow_version,
                path.display()
            ),
        )
    })?;
    if definition.id != workflow.workflow_id || definition.version != workflow.workflow_version {
        return Err(single_registration_error(
            ApplicationRegistrationErrorCode::WorkflowReferenceMismatch,
            path.display().to_string(),
            &format!(
                "workflow reference mismatch: app declared {}@{}, workflow file contains {}@{}",
                workflow.workflow_id, workflow.workflow_version, definition.id, definition.version
            ),
        ));
    }

    Ok(LoadedApplicationWorkflow { path, definition })
}

fn validate_component_event_references(
    events: &EventRegistry,
    scope: RegistryScope,
    components: &[ApplicationComponent],
) -> Result<(), ApplicationRegistrationFailure> {
    let lookup_scope = if scope == RegistryScope::Private {
        LookupScope::PreferPrivate
    } else {
        LookupScope::PublicOnly
    };
    for component in components {
        for event_ref in component
            .contract
            .emits
            .iter()
            .chain(component.contract.consumes.iter())
        {
            let event_missing = events
                .find_exact(lookup_scope, &event_ref.event_id, &event_ref.version)
                .is_none();
            if !event_missing {
                continue;
            }
            let message = format!(
                "component {} references missing event {}@{}",
                component.manifest.component_id, event_ref.event_id, event_ref.version
            );
            return Err(single_registration_error(
                ApplicationRegistrationErrorCode::MissingRequiredEvent,
                component.contract_path.display().to_string(),
                &message,
            ));
        }
    }
    Ok(())
}

fn build_application_capability_registration(
    manifest: &ApplicationBundleManifest,
    component: &ApplicationComponent,
    request: &ApplicationRegistrationRequest,
) -> CapabilityRegistration {
    let artifact_ref = format!(
        "app:{}:{}:component:{}:{}",
        manifest.app_id,
        manifest.version,
        component.manifest.component_id,
        component.manifest.version
    );
    let artifact = CapabilityArtifactRecord {
        artifact_ref,
        implementation_kind: ImplementationKind::Executable,
        source: SourceReference {
            kind: SourceKind::Local,
            location: component.manifest_path.display().to_string(),
        },
        binary: Some(BinaryReference {
            format: BinaryFormat::Wasm,
            location: component.wasm_binary_path.display().to_string(),
            signature: None,
        }),
        workflow_ref: None,
        digests: ArtifactDigests {
            source_digest: governed_content_digest(&component.contract),
            binary_digest: Some(component.verified_wasm_digest.clone()),
        },
        provenance: RegistryProvenance {
            source: format!("application_bundle:{}", manifest.app_id),
            author: manifest.app_id.clone(),
            created_at: request.registered_at.clone(),
        },
    };

    CapabilityRegistration {
        scope: request.scope,
        contract: component.contract.clone(),
        contract_path: component.contract_path.display().to_string(),
        artifact,
        registered_at: request.registered_at.clone(),
        tags: vec![format!("app:{}", manifest.app_id)],
        composability: ComposabilityMetadata {
            kind: CompositionKind::Atomic,
            patterns: vec![CompositionPattern::Validation],
            provides: vec![component.contract.id.clone()],
            requires: component
                .manifest
                .dependencies
                .iter()
                .map(|dependency| dependency.component_id.clone())
                .collect(),
        },
        governing_spec: "044-application-bundle-manifest".to_string(),
        validator_version: request.validator_version.clone(),
    }
}

fn build_application_record(
    request: &ApplicationRegistrationRequest,
    manifest: &ApplicationBundleManifest,
    components: Vec<ApplicationRegisteredComponent>,
    workflows: Vec<ApplicationRegisteredWorkflow>,
) -> ApplicationRegistryRecord {
    let manifest_digest = application_manifest_digest(manifest);
    let bundle_digest = application_bundle_digest(manifest, &components, &workflows);
    ApplicationRegistryRecord {
        scope: request.scope,
        workspace_id: request.workspace_id.clone(),
        app_id: manifest.app_id.clone(),
        version: manifest.version.clone(),
        manifest_path: request.manifest_path.display().to_string(),
        manifest_digest,
        bundle_digest,
        registered_at: request.registered_at.clone(),
        readiness_status: ApplicationReadinessStatus::Ready,
        components,
        workflows,
        inspection_link: format!("/v1/apps/{}/{}", manifest.app_id, manifest.version),
        execution_links: manifest
            .workflows
            .iter()
            .map(|workflow| {
                format!(
                    "/v1/workflows/{}/{}",
                    workflow.workflow_id, workflow.workflow_version
                )
            })
            .collect(),
    }
}

fn application_manifest_digest(manifest: &ApplicationBundleManifest) -> String {
    let value = serde_json::json!({
        "app_id": manifest.app_id,
        "version": manifest.version,
        "schema_version": manifest.schema_version,
        "workspace_defaults": manifest.workspace_defaults,
        "components": manifest.components.iter().map(|component| serde_json::json!({
            "component_id": component.reference.component_id,
            "version": component.reference.version,
            "digest": component.reference.digest,
            "manifest_path": component.reference.manifest_path,
        })).collect::<Vec<_>>(),
        "workflows": manifest.workflows,
        "model_dependencies": manifest.model_dependencies,
        "config_schema": manifest.config_schema,
        "default_config": manifest.default_config,
        "placement_policy": manifest.placement_policy,
        "public_surfaces": manifest.public_surfaces,
    });
    format!("sha256:{}", sha256_hex(value.to_string().as_bytes()))
}

fn application_bundle_digest(
    manifest: &ApplicationBundleManifest,
    components: &[ApplicationRegisteredComponent],
    workflows: &[ApplicationRegisteredWorkflow],
) -> String {
    let value = serde_json::json!({
        "app_id": manifest.app_id,
        "version": manifest.version,
        "components": components,
        "workflows": workflows,
    });
    format!("sha256:{}", sha256_hex(value.to_string().as_bytes()))
}

fn map_manifest_failure(failure: ApplicationManifestFailure) -> ApplicationRegistrationFailure {
    ApplicationRegistrationFailure {
        errors: failure
            .errors
            .into_iter()
            .map(|error| ApplicationRegistrationError {
                code: ApplicationRegistrationErrorCode::ManifestValidationFailed,
                path: error.path,
                message: error.message,
                severity: ErrorSeverity::Error,
            })
            .collect(),
    }
}

fn map_capability_registration_failure(
    component: &ApplicationComponent,
    failure: crate::RegistryFailure,
) -> ApplicationRegistrationFailure {
    ApplicationRegistrationFailure {
        errors: failure
            .errors
            .into_iter()
            .map(|error| ApplicationRegistrationError {
                code: ApplicationRegistrationErrorCode::CapabilityRegistrationFailed,
                path: component.contract_path.display().to_string(),
                message: error.message,
                severity: error.severity,
            })
            .collect(),
    }
}

fn map_workflow_registration_failure(
    failure: crate::WorkflowFailure,
) -> ApplicationRegistrationFailure {
    ApplicationRegistrationFailure {
        errors: failure
            .errors
            .into_iter()
            .map(|error| ApplicationRegistrationError {
                code: ApplicationRegistrationErrorCode::WorkflowRegistrationFailed,
                path: error.path,
                message: error.message,
                severity: error.severity,
            })
            .collect(),
    }
}

fn ensure_unique_component_refs(
    components: &[ApplicationComponentRef],
) -> Result<(), ApplicationManifestFailure> {
    let mut seen = BTreeSet::new();
    for component in components {
        let key = format!("{}@{}", component.component_id, component.version);
        if !seen.insert(key.clone()) {
            return Err(single_error(
                ApplicationManifestErrorCode::DuplicateComponentReference,
                key.clone(),
                format!("duplicate component reference in application manifest: {key}"),
            ));
        }
    }
    Ok(())
}

fn load_component(
    manifest_dir: &Path,
    reference: &ApplicationComponentRef,
) -> Result<ApplicationComponent, ApplicationManifestFailure> {
    let manifest_path = manifest_dir.join(&reference.manifest_path);
    if !manifest_path.is_file() {
        return Err(single_error(
            ApplicationManifestErrorCode::AppComponentManifestMissing,
            manifest_path.display().to_string(),
            format!(
                "missing component manifest for {} at {}",
                reference.component_id,
                manifest_path.display()
            ),
        ));
    }

    let manifest_contents = fs::read_to_string(&manifest_path).map_err(|error| {
        single_error(
            ApplicationManifestErrorCode::ComponentManifestReadFailed,
            manifest_path.display().to_string(),
            format!(
                "failed to read component manifest {}: {error}",
                manifest_path.display()
            ),
        )
    })?;

    let component: WasmComponentManifestSerde =
        serde_json::from_str(&manifest_contents).map_err(|error| {
            single_error(
                ApplicationManifestErrorCode::ComponentManifestParseFailed,
                manifest_path.display().to_string(),
                format!(
                    "failed to parse component manifest {}: {error}",
                    manifest_path.display()
                ),
            )
        })?;

    let expected_wasm_digest =
        ensure_component_reference_matches(reference, &component, &manifest_path)?;
    ensure_concrete_component_dependencies(&component.dependencies, &manifest_path)?;

    let component_dir = manifest_path.parent().unwrap_or(manifest_dir);
    let contract_path = component_dir.join(&component.contract_path);
    let contract = load_component_contract(&contract_path, &component)?;
    let wasm_binary_path = component_dir.join(&component.wasm_binary_path);
    let verified_wasm_digest = verify_wasm_digest(&wasm_binary_path, &expected_wasm_digest)?;

    Ok(ApplicationComponent {
        reference: reference.clone(),
        manifest_path,
        manifest: to_component_manifest(component),
        contract_path,
        contract,
        wasm_binary_path,
        verified_wasm_digest,
    })
}

fn ensure_component_reference_matches(
    reference: &ApplicationComponentRef,
    component: &WasmComponentManifestSerde,
    manifest_path: &Path,
) -> Result<String, ApplicationManifestFailure> {
    if reference.component_id != component.component_id || reference.version != component.version {
        return Err(single_error(
            ApplicationManifestErrorCode::ComponentReferenceMismatch,
            manifest_path.display().to_string(),
            format!(
                "component reference mismatch for {}: app declared {}@{}, component manifest contains {}@{}",
                manifest_path.display(),
                reference.component_id,
                reference.version,
                component.component_id,
                component.version
            ),
        ));
    }
    let expected_digest = normalize_sha256_digest(&reference.digest).ok_or_else(|| {
        single_error(
            ApplicationManifestErrorCode::InvalidDigestMetadata,
            "$.components[].digest".to_string(),
            format!(
                "component reference {}@{} declares invalid digest metadata",
                reference.component_id, reference.version
            ),
        )
    })?;
    let component_digest = normalize_sha256_digest(&component.wasm_digest).ok_or_else(|| {
        single_error(
            ApplicationManifestErrorCode::InvalidDigestMetadata,
            "$.wasm_digest".to_string(),
            format!(
                "component manifest {}@{} declares invalid wasm_digest metadata",
                component.component_id, component.version
            ),
        )
    })?;
    if expected_digest != component_digest {
        return Err(single_error(
            ApplicationManifestErrorCode::ComponentDigestMismatch,
            manifest_path.display().to_string(),
            format!(
                "component digest mismatch for {}@{}: app declared {}, component manifest declared {}",
                component.component_id, component.version, reference.digest, component.wasm_digest
            ),
        ));
    }
    Ok(expected_digest)
}

fn ensure_concrete_component_dependencies(
    dependencies: &[WasmComponentDependency],
    manifest_path: &Path,
) -> Result<(), ApplicationManifestFailure> {
    for dependency in dependencies {
        let concrete = dependency.version.as_deref().is_some_and(has_text)
            && dependency.digest.as_deref().is_some_and(has_text)
            && dependency.version_range.is_none();
        if !concrete {
            return Err(single_error(
                ApplicationManifestErrorCode::ComponentDependencyMustBeConcrete,
                manifest_path.display().to_string(),
                format!(
                    "component dependency {} must declare concrete version and digest without version_range",
                    dependency.component_id
                ),
            ));
        }
    }
    Ok(())
}

fn load_component_contract(
    contract_path: &Path,
    component: &WasmComponentManifestSerde,
) -> Result<CapabilityContract, ApplicationManifestFailure> {
    if !contract_path.is_file() {
        return Err(single_error(
            ApplicationManifestErrorCode::ComponentContractMissing,
            contract_path.display().to_string(),
            format!(
                "missing capability contract for {} at {}",
                component.component_id,
                contract_path.display()
            ),
        ));
    }

    let contract_contents = fs::read_to_string(contract_path).map_err(|error| {
        single_error(
            ApplicationManifestErrorCode::ComponentContractMissing,
            contract_path.display().to_string(),
            format!(
                "failed to read capability contract {}: {error}",
                contract_path.display()
            ),
        )
    })?;

    let contract = parse_contract(&contract_contents).map_err(|failure| {
        let detail = failure
            .errors
            .into_iter()
            .map(|error| format!("{} at {}", error.message, error.path))
            .collect::<Vec<_>>()
            .join("; ");
        single_error(
            ApplicationManifestErrorCode::ComponentContractParseFailed,
            contract_path.display().to_string(),
            format!(
                "failed to parse capability contract for {}: {}",
                component.component_id, detail
            ),
        )
    })?;

    if contract.id != component.capability_id || contract.version != component.capability_version {
        return Err(single_error(
            ApplicationManifestErrorCode::ComponentContractMismatch,
            contract_path.display().to_string(),
            format!(
                "component contract mismatch for {}: manifest declared {}@{}, contract contains {}@{}",
                component.component_id,
                component.capability_id,
                component.capability_version,
                contract.id,
                contract.version
            ),
        ));
    }

    Ok(contract)
}

fn verify_wasm_digest(
    wasm_binary_path: &Path,
    expected: &str,
) -> Result<String, ApplicationManifestFailure> {
    if !wasm_binary_path.is_file() {
        return Err(single_error(
            ApplicationManifestErrorCode::ComponentWasmMissing,
            wasm_binary_path.display().to_string(),
            format!("missing WASM binary at {}", wasm_binary_path.display()),
        ));
    }
    let bytes = fs::read(wasm_binary_path).map_err(|error| {
        single_error(
            ApplicationManifestErrorCode::ComponentWasmMissing,
            wasm_binary_path.display().to_string(),
            format!(
                "failed to read WASM binary {}: {error}",
                wasm_binary_path.display()
            ),
        )
    })?;
    let actual = sha256_hex(&bytes);
    if expected != actual {
        return Err(single_error(
            ApplicationManifestErrorCode::ComponentDigestMismatch,
            wasm_binary_path.display().to_string(),
            format!(
                "WASM digest mismatch for {}: expected sha256:{expected}, got sha256:{actual}",
                wasm_binary_path.display()
            ),
        ));
    }
    Ok(format!("sha256:{actual}"))
}

fn to_component_manifest(component: WasmComponentManifestSerde) -> WasmComponentManifest {
    WasmComponentManifest {
        component_id: component.component_id,
        version: component.version,
        schema_version: component.schema_version,
        capability_id: component.capability_id,
        capability_version: component.capability_version,
        contract_path: component.contract_path,
        wasm_binary_path: component.wasm_binary_path,
        wasm_digest: component.wasm_digest,
        runtime_constraints: component.runtime_constraints,
        permitted_targets: component.permitted_targets,
        dependencies: component.dependencies,
        connector_requirements: component.connector_requirements,
        validation_evidence: component.validation_evidence,
    }
}

fn normalize_sha256_digest(value: &str) -> Option<String> {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(digest.to_ascii_lowercase())
    } else {
        None
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

fn single_error(
    code: ApplicationManifestErrorCode,
    path: String,
    message: String,
) -> ApplicationManifestFailure {
    ApplicationManifestFailure {
        errors: vec![ApplicationManifestError {
            code,
            path,
            message,
        }],
    }
}

fn single_registration_error(
    code: ApplicationRegistrationErrorCode,
    path: impl Into<String>,
    message: &str,
) -> ApplicationRegistrationFailure {
    ApplicationRegistrationFailure {
        errors: vec![ApplicationRegistrationError {
            code,
            path: path.into(),
            message: message.to_string(),
            severity: ErrorSeverity::Error,
        }],
    }
}
