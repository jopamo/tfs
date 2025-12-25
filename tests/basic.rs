use anyhow::Result;
use serde_json::json;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_plan_validation() -> Result<()> {
    let dir = tempdir()?;
    let root = dir.path().to_path_buf();
    let manifest_path = root.join("plan.json");
    let manifest = json!({
        "root": root.to_str().unwrap(),
        "transaction": "all",
        "operations": [
            { "op": "mkdir", "dst": "subdir" },
            { "op": "move", "src": "a.txt", "dst": "subdir/a.txt" }
        ]
    });
    fs::write(&manifest_path, manifest.to_string())?;

    // Create source file
    fs::write(root.join("a.txt"), "hello")?;

    let plan = tfs::model::load_plan(&manifest_path)?;
    plan.validate()?;
    Ok(())
}

#[test]
fn test_schema_generation() {
    let schema = tfs::model::generate_schema();
    assert!(schema.contains("$schema"));
    assert!(schema.contains("Plan"));
}
