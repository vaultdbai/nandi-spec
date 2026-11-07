//! Checks the JSON Schemas in `registry-schema/`, offline.
//!
//! - Every `*.schema.json` is a valid JSON Schema 2020-12 and compiles.
//! - Every file in `examples/` validates against its schema.
//! - Every file in `tests/invalid/` fails its schema.
//!
//! A file's schema comes from its name: `connector-fetch.json` is checked
//! against `connector.schema.json`. A `$ref` to a sibling schema file
//! resolves from this folder, never from the network.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jsonschema::{Retrieve, Uri, Validator};
use serde_json::{json, Value};

/// The base of every schema's `$id`.
const ID_BASE: &str = "https://github.com/vaultdbai/nandi-registry/registry-schema/";

fn schema_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../registry-schema")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

/// The schema files by name, e.g. `connector` for `connector.schema.json`.
fn schemas() -> BTreeMap<String, Value> {
    json_files(&schema_dir())
        .into_iter()
        .filter_map(|path| {
            let file = path.file_name()?.to_str()?;
            let name = file.strip_suffix(".schema.json")?.to_string();
            Some((name, read_json(&path)))
        })
        .collect()
}

/// Serves the sibling schema files by their `$id`, and nothing else.
struct LocalSchemas(BTreeMap<String, Value>);

impl Retrieve for LocalSchemas {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let uri = uri.as_str();
        self.0
            .values()
            .find(|schema| schema["$id"].as_str() == Some(uri))
            .cloned()
            .ok_or_else(|| format!("not a local schema: {uri}").into())
    }
}

fn validator(name: &str) -> Validator {
    let all = schemas();
    let schema = all
        .get(name)
        .unwrap_or_else(|| panic!("no {name}.schema.json"));
    jsonschema::options()
        .with_retriever(LocalSchemas(all.clone()))
        .build(schema)
        .unwrap_or_else(|e| panic!("{name}.schema.json does not compile: {e}"))
}

/// The schema a fixture is checked against: its file name up to the first `-`.
fn schema_name_of(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("file name");
    let (name, _) = stem
        .split_once('-')
        .unwrap_or_else(|| panic!("{stem}: name it <schema>-<case>.json"));
    name.to_string()
}

fn errors(validator: &Validator, instance: &Value) -> Vec<String> {
    validator
        .iter_errors(instance)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect()
}

#[test]
fn every_schema_is_valid_2020_12() {
    let all = schemas();
    assert!(all.len() >= 4, "found only {:?}", all.keys());
    for (name, schema) in &all {
        assert_eq!(
            schema["$schema"],
            json!("https://json-schema.org/draft/2020-12/schema"),
            "{name}.schema.json must declare draft 2020-12"
        );
        assert_eq!(
            schema["$id"],
            json!(format!("{ID_BASE}{name}.schema.json")),
            "{name}.schema.json has the wrong $id"
        );
        if let Err(e) = jsonschema::meta::validate(schema) {
            panic!("{name}.schema.json is not a valid schema: {e}");
        }
        validator(name);
    }
}

#[test]
fn every_example_validates() {
    let examples = json_files(&schema_dir().join("examples"));
    assert!(!examples.is_empty(), "no examples");
    for path in examples {
        let errors = errors(&validator(&schema_name_of(&path)), &read_json(&path));
        assert!(errors.is_empty(), "{}: {errors:#?}", path.display());
    }
}

#[test]
fn every_invalid_fixture_fails() {
    let fixtures = json_files(&schema_dir().join("tests/invalid"));
    assert!(!fixtures.is_empty(), "no invalid fixtures");
    for path in fixtures {
        let errors = errors(&validator(&schema_name_of(&path)), &read_json(&path));
        assert!(!errors.is_empty(), "{} should fail", path.display());
        println!("{}: {errors:?}", path.display());
    }
}

/// An npx package with a full version is fine; the version check for npx
/// adds to the general one and must not refuse a valid release.
#[test]
fn npx_with_an_exact_version_validates() {
    let mut entry = read_json(&schema_dir().join("examples/connector-fetch.json"));
    entry["run"] = json!({
        "command": "npx",
        "package": "@modelcontextprotocol/server-everything",
        "version": "2025.9.25",
        "args": ["stdio"]
    });
    let errors = errors(&validator("connector"), &entry);
    assert!(errors.is_empty(), "{errors:#?}");
}

/// A host connector is valid in the schema, so a company registry can list
/// one. The public registry refuses it in review.
#[test]
fn a_stdio_connector_may_run_on_the_host() {
    let mut entry = read_json(&schema_dir().join("examples/connector-fetch.json"));
    entry["placement"] = json!("host");
    let errors = errors(&validator("connector"), &entry);
    assert!(errors.is_empty(), "{errors:#?}");
}
