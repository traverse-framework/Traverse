use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use traverse_contracts::{
    CapabilityContract, ConnectorRequirement, ExecutionTarget, parse_contract,
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
pub struct ApplicationComponent {
    pub reference: ApplicationComponentRef,
    pub manifest_path: PathBuf,
    pub manifest: WasmComponentManifest,
    pub contract_path: PathBuf,
    pub contract: CapabilityContract,
    pub wasm_binary_path: PathBuf,
    pub verified_wasm_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApplicationComponentRef {
    pub component_id: String,
    pub version: String,
    pub digest: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApplicationWorkflowRef {
    pub workflow_id: String,
    pub workflow_version: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
