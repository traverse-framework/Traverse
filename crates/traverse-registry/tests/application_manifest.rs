#![allow(clippy::expect_used)]

use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use traverse_registry::{ApplicationManifestErrorCode, load_application_bundle_manifest};

#[test]
fn loads_checked_in_application_manifest_with_real_wasm_component() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/applications/expedition-readiness/app.manifest.json");

    let bundle = load_application_bundle_manifest(&manifest_path)
        .expect("checked-in application bundle should validate");

    assert_eq!(bundle.app_id, "expedition.readiness");
    assert_eq!(bundle.version, "1.0.0");
    assert_eq!(bundle.components.len(), 1);
    assert_eq!(
        bundle.components[0].manifest.component_id,
        "expedition.readiness.validate-team-readiness-component"
    );
    assert_eq!(
        bundle.components[0].contract.id,
        "expedition.planning.validate-team-readiness"
    );
    assert_eq!(
        bundle.components[0].verified_wasm_digest,
        "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99"
    );
}

#[test]
fn rejects_manifest_path_without_parent_directory() {
    let failure = load_application_bundle_manifest(PathBuf::new().as_path())
        .expect_err("empty manifest path should fail before reading");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ManifestParentMissing
    );
}

#[test]
fn rejects_missing_application_manifest_file() {
    let fixture = AppFixture::new("missing-app-manifest");

    let failure = load_application_bundle_manifest(&fixture.root.join("missing.manifest.json"))
        .expect_err("missing app manifest should fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ManifestReadFailed
    );
}

#[test]
fn rejects_invalid_application_manifest_json() {
    let fixture = AppFixture::new("invalid-app-manifest");
    fs::write(fixture.app_manifest_path(), "{ not valid json ")
        .expect("invalid app manifest fixture should write");

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("invalid app manifest json should fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ManifestParseFailed
    );
}

#[test]
fn rejects_missing_component_manifest() {
    let fixture = AppFixture::new("missing-component");
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99",
        "components/missing/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("missing component manifest must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::AppComponentManifestMissing
    );
}

#[test]
fn rejects_duplicate_component_references() {
    let fixture = AppFixture::new("duplicate-components");
    let refs = json!([
        component_ref(
            "expedition.readiness.validate-team-readiness-component",
            "1.0.0",
            "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99",
            "components/a/component.manifest.json",
        ),
        component_ref(
            "expedition.readiness.validate-team-readiness-component",
            "1.0.0",
            "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99",
            "components/b/component.manifest.json",
        )
    ]);
    fixture.write_app_manifest(&refs);

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("duplicate component references must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::DuplicateComponentReference
    );
}

#[test]
fn rejects_unreadable_component_manifest() {
    let fixture = AppFixture::new("unreadable-component-manifest");
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99",
        "components/validate-team-readiness/component.manifest.json",
    )]));
    fs::write(fixture.component_manifest_path(), "{}")
        .expect("component manifest fixture should write");
    make_unreadable(&fixture.component_manifest_path());

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("unreadable component manifest must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentManifestReadFailed
    );
}

#[test]
fn rejects_invalid_component_manifest_json() {
    let fixture = AppFixture::new("invalid-component-manifest");
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99",
        "components/validate-team-readiness/component.manifest.json",
    )]));
    fs::write(fixture.component_manifest_path(), "{ not valid json ")
        .expect("invalid component manifest fixture should write");

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("invalid component manifest json must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentManifestParseFailed
    );
}

#[test]
fn rejects_component_reference_identity_mismatch() {
    let fixture = AppFixture::new("component-reference-mismatch");
    let wasm_digest = fixture.write_wasm("component bytes");
    fixture.write_component_manifest(&json!({
        "component_id": "expedition.readiness.other-component",
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("component reference mismatch must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentReferenceMismatch
    );
}

#[test]
fn rejects_component_contract_identity_mismatch() {
    let fixture = AppFixture::new("contract-mismatch");
    let wasm_digest = fixture.write_wasm("component bytes");
    fixture.write_component_manifest(&json!({
        "capability_id": "expedition.planning.not-the-contract",
        "capability_version": "1.0.0",
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("contract mismatch must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentContractMismatch
    );
}

#[test]
fn rejects_component_manifest_digest_mismatch() {
    let fixture = AppFixture::new("component-manifest-digest-mismatch");
    let wasm_digest = fixture.write_wasm("component bytes");
    fixture.write_component_manifest(&json!({
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99",
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("app/component digest mismatch must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentDigestMismatch
    );
}

#[test]
fn rejects_invalid_component_manifest_digest_metadata() {
    let fixture = AppFixture::new("invalid-component-digest");
    fixture.write_component_manifest(&json!({
        "wasm_digest": "fnv1a64:dffc31d6401c84d6",
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99",
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("invalid component digest metadata must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::InvalidDigestMetadata
    );
}

#[test]
fn rejects_invalid_digest_metadata() {
    let fixture = AppFixture::new("invalid-digest");
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        "fnv1a64:dffc31d6401c84d6",
        "components/validate-team-readiness/component.manifest.json",
    )]));
    fixture.write_component_manifest(&json!({
        "wasm_digest": "fnv1a64:dffc31d6401c84d6",
        "dependencies": []
    }));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("invalid digest metadata must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::InvalidDigestMetadata
    );
}

#[test]
fn rejects_missing_component_contract() {
    let fixture = AppFixture::new("missing-contract");
    let wasm_digest = fixture.write_wasm("component bytes");
    fixture.write_component_manifest(&json!({
        "contract_path": "missing-contract.json",
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("missing component contract must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentContractMissing
    );
}

#[test]
fn rejects_unreadable_component_contract() {
    let fixture = AppFixture::new("unreadable-contract");
    let wasm_digest = fixture.write_wasm("component bytes");
    let contract_path = fixture
        .root
        .join("components/validate-team-readiness/unreadable-contract.json");
    fs::write(&contract_path, "{}").expect("contract fixture should write");
    make_unreadable(&contract_path);
    fixture.write_component_manifest(&json!({
        "contract_path": "unreadable-contract.json",
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("unreadable component contract must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentContractMissing
    );
}

#[test]
fn rejects_invalid_component_contract_json() {
    let fixture = AppFixture::new("invalid-contract");
    let wasm_digest = fixture.write_wasm("component bytes");
    let contract_path = fixture
        .root
        .join("components/validate-team-readiness/invalid-contract.json");
    fs::write(&contract_path, "{}").expect("contract fixture should write");
    fixture.write_component_manifest(&json!({
        "contract_path": "invalid-contract.json",
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("invalid component contract must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentContractParseFailed
    );
}

#[test]
fn rejects_missing_wasm_binary() {
    let fixture = AppFixture::new("missing-wasm");
    let digest = "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99";
    fixture.write_component_manifest(&json!({
        "wasm_digest": digest,
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        digest,
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("missing WASM binary must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentWasmMissing
    );
}

#[test]
fn rejects_unreadable_wasm_binary() {
    let fixture = AppFixture::new("unreadable-wasm");
    let wasm_digest = fixture.write_wasm("component bytes");
    make_unreadable(&fixture.wasm_path());
    fixture.write_component_manifest(&json!({
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("unreadable WASM binary must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentWasmMissing
    );
}

#[test]
fn rejects_wasm_digest_mismatch() {
    let fixture = AppFixture::new("digest-mismatch");
    let _digest = fixture.write_wasm("different bytes");
    let wrong_digest = "sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99";
    fixture.write_component_manifest(&json!({
        "wasm_digest": wrong_digest,
        "dependencies": []
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        wrong_digest,
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("digest mismatch must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentDigestMismatch
    );
}

#[test]
fn rejects_version_range_component_dependencies() {
    let fixture = AppFixture::new("range-dependency");
    let wasm_digest = fixture.write_wasm("component bytes");
    fixture.write_component_manifest(&json!({
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": [
            {
                "component_id": "expedition.readiness.other-component",
                "version_range": "^1.0.0"
            }
        ]
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("version range dependency must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentDependencyMustBeConcrete
    );
}

#[test]
fn accepts_concrete_component_dependencies() {
    let fixture = AppFixture::new("concrete-dependency");
    let wasm_digest = fixture.write_wasm("component bytes");
    fixture.write_component_manifest(&json!({
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": [
            {
                "component_id": "expedition.readiness.other-component",
                "version": "1.0.0",
                "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }
        ]
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let bundle = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect("concrete component dependencies should validate");

    assert_eq!(bundle.components[0].manifest.dependencies.len(), 1);
}

#[test]
fn rejects_component_dependencies_without_concrete_version_and_digest() {
    let fixture = AppFixture::new("missing-concrete-dependency");
    let wasm_digest = fixture.write_wasm("component bytes");
    fixture.write_component_manifest(&json!({
        "wasm_digest": format!("sha256:{wasm_digest}"),
        "dependencies": [
            {
                "component_id": "expedition.readiness.other-component",
                "version": " "
            }
        ]
    }));
    fixture.write_app_manifest(&json!([component_ref(
        "expedition.readiness.validate-team-readiness-component",
        "1.0.0",
        &format!("sha256:{wasm_digest}"),
        "components/validate-team-readiness/component.manifest.json",
    )]));

    let failure = load_application_bundle_manifest(&fixture.app_manifest_path())
        .expect_err("non-concrete dependency must fail");

    assert_eq!(
        failure.errors[0].code,
        ApplicationManifestErrorCode::ComponentDependencyMustBeConcrete
    );
}

struct AppFixture {
    root: PathBuf,
}

impl AppFixture {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("traverse-app-manifest-{name}-{nanos}"));
        fs::create_dir_all(root.join("components/validate-team-readiness"))
            .expect("fixture directories should be created");
        Self { root }
    }

    fn app_manifest_path(&self) -> PathBuf {
        self.root.join("app.manifest.json")
    }

    fn component_manifest_path(&self) -> PathBuf {
        self.root
            .join("components/validate-team-readiness/component.manifest.json")
    }

    fn wasm_path(&self) -> PathBuf {
        self.root
            .join("components/validate-team-readiness/component.wasm")
    }

    fn write_app_manifest(&self, components: &serde_json::Value) {
        let app = json!({
            "app_id": "expedition.readiness",
            "version": "1.0.0",
            "schema_version": "1.0.0",
            "workspace_defaults": {
                "workspace_id": "test"
            },
            "components": components,
            "workflows": [],
            "model_dependencies": [],
            "config_schema": {
                "type": "object"
            },
            "default_config": {},
            "placement_policy": {
                "preferred_targets": ["local"]
            },
            "public_surfaces": ["cli"]
        });
        fs::write(self.app_manifest_path(), app.to_string()).expect("app manifest should write");
    }

    fn write_component_manifest(&self, overrides: &serde_json::Value) {
        let component_id = overrides
            .get("component_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("expedition.readiness.validate-team-readiness-component");
        let version = overrides
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("1.0.0");
        let wasm_digest = overrides
            .get("wasm_digest")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("sha256:5647c39a1d25d8728350f9619025292a62e78a602068a2ad9b6f075751c93d99");
        let capability_id = overrides
            .get("capability_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("expedition.planning.validate-team-readiness");
        let capability_version = overrides
            .get("capability_version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("1.0.0");
        let dependencies = overrides
            .get("dependencies")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let default_contract_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../contracts/examples/expedition/capabilities/validate-team-readiness/contract.json",
        );
        let contract_path = overrides
            .get("contract_path")
            .cloned()
            .unwrap_or_else(|| json!(default_contract_path));
        let wasm_binary_path = overrides
            .get("wasm_binary_path")
            .cloned()
            .unwrap_or_else(|| json!("component.wasm"));
        let component = json!({
            "component_id": component_id,
            "version": version,
            "schema_version": "1.0.0",
            "capability_id": capability_id,
            "capability_version": capability_version,
            "contract_path": contract_path,
            "wasm_binary_path": wasm_binary_path,
            "wasm_digest": wasm_digest,
            "runtime_constraints": {
                "host_api_access": "none",
                "network_access": "forbidden",
                "filesystem_access": "none"
            },
            "permitted_targets": ["local"],
            "dependencies": dependencies,
            "connector_requirements": [],
            "validation_evidence": []
        });
        fs::write(self.component_manifest_path(), component.to_string())
            .expect("component manifest should write");
    }

    fn write_wasm(&self, contents: &str) -> String {
        fs::write(self.wasm_path(), contents.as_bytes()).expect("wasm fixture should write");
        sha256_hex(contents.as_bytes())
    }
}

fn make_unreadable(path: &PathBuf) {
    let mut permissions = fs::metadata(path)
        .expect("fixture metadata should be available")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(path, permissions).expect("fixture should become unreadable");
}

fn component_ref(id: &str, version: &str, digest: &str, manifest_path: &str) -> serde_json::Value {
    json!({
        "component_id": id,
        "version": version,
        "digest": digest,
        "manifest_path": manifest_path
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
