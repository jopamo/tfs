use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Root execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    /// Absolute root directory; all operations are confined under this root.
    pub root: PathBuf,
    /// Transaction mode.
    #[serde(default = "default_transaction_mode")]
    pub transaction: TransactionMode,
    /// Default collision policy.
    #[serde(default = "default_collision_policy")]
    pub collision_policy: CollisionPolicy,
    /// Symlink handling policy.
    #[serde(default = "default_symlink_policy")]
    pub symlink_policy: SymlinkPolicy,
    /// Allow overwrite policies (requires explicit opt-in).
    #[serde(default)]
    pub allow_overwrite: bool,
    /// List of operations to execute.
    pub operations: Vec<Operation>,
}

fn default_transaction_mode() -> TransactionMode {
    TransactionMode::All
}

fn default_collision_policy() -> CollisionPolicy {
    CollisionPolicy::Fail
}

fn default_symlink_policy() -> SymlinkPolicy {
    SymlinkPolicy::Error
}

impl Plan {
    /// Validate the plan (basic sanity checks).
    pub fn validate(&self) -> Result<()> {
        if !self.root.is_absolute() {
            anyhow::bail!("root must be an absolute path");
        }
        // TODO: more validation
        Ok(())
    }
}

/// Transaction atomicity mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum TransactionMode {
    /// All operations succeed or none are applied.
    #[serde(rename = "all")]
    All,
    /// Each operation commits independently.
    #[serde(rename = "op")]
    Op,
}

/// Collision resolution policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum CollisionPolicy {
    /// Fail the operation.
    #[serde(rename = "fail")]
    Fail,
    /// Append numeric suffix (_2, _3, …).
    #[serde(rename = "suffix")]
    Suffix,
    /// Append short hash of file contents.
    #[serde(rename = "hash8")]
    Hash8,
    /// Overwrite destination, backing up original.
    #[serde(rename = "overwrite_with_backup")]
    OverwriteWithBackup,
}

/// Symlink handling policy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum SymlinkPolicy {
    /// Follow symlinks.
    #[serde(rename = "follow")]
    Follow,
    /// Skip symlinks (treat as missing).
    #[serde(rename = "skip")]
    Skip,
    /// Treat symlinks as errors.
    #[serde(rename = "error")]
    Error,
}

/// A single filesystem operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    /// Create a directory.
    Mkdir {
        /// Destination path (relative to root).
        dst: PathBuf,
        /// Create parent directories as needed.
        #[serde(default)]
        parents: bool,
    },
    /// Move a file or directory.
    Move {
        /// Source path (relative to root).
        src: PathBuf,
        /// Destination path (relative to root).
        dst: PathBuf,
        /// Whether to allow cross-device move (copy+delete).
        #[serde(default)]
        cross_device: bool,
    },
    /// Copy a file or directory.
    Copy {
        /// Source path (relative to root).
        src: PathBuf,
        /// Destination path (relative to root).
        dst: PathBuf,
        /// Whether to copy recursively for directories.
        #[serde(default)]
        recursive: bool,
    },
    /// Rename (alias for move within same directory).
    Rename {
        /// Source path (relative to root).
        src: PathBuf,
        /// Destination path (relative to root).
        dst: PathBuf,
    },
    /// Move to trash/quarantine (optional).
    Trash {
        /// Source path (relative to root).
        src: PathBuf,
    },
}

/// Generate JSON Schema for the Plan type.
pub fn generate_schema() -> String {
    let schema = schemars::schema_for!(Plan);
    serde_json::to_string_pretty(&schema).expect("failed to serialize schema")
}

/// Load a Plan from a JSON file.
pub fn load_plan(path: &std::path::Path) -> Result<Plan> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let plan = serde_json::from_reader(reader)?;
    Ok(plan)
}

/// Create a Plan from a JSON string.
pub fn from_json(json: &str) -> Result<Plan> {
    let plan = serde_json::from_str(json)?;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_validation() {
        let plan = Plan {
            root: "/absolute/path".into(),
            transaction: TransactionMode::All,
            collision_policy: CollisionPolicy::Fail,
            symlink_policy: SymlinkPolicy::Error,
            allow_overwrite: false,
            operations: vec![],
        };
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn test_plan_relative_root_fails() {
        let plan = Plan {
            root: "relative/path".into(),
            transaction: TransactionMode::All,
            collision_policy: CollisionPolicy::Fail,
            symlink_policy: SymlinkPolicy::Error,
            allow_overwrite: false,
            operations: vec![],
        };
        assert!(plan.validate().is_err());
    }
}