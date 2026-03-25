use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{Result, SxmcError};

#[derive(Clone, Debug)]
pub struct DiscoverySnapshotEntry {
    pub path: PathBuf,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct DiscoveryResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
    pub content: String,
    pub path: PathBuf,
    pub source_type: String,
}

pub fn load_snapshot(path: &Path) -> Result<Value> {
    let raw = fs::read_to_string(path).map_err(|error| {
        SxmcError::Other(format!(
            "Failed to read discovery snapshot '{}': {}",
            path.display(),
            error
        ))
    })?;
    let value: Value = serde_json::from_str(&raw).map_err(|error| {
        SxmcError::Other(format!(
            "Discovery snapshot '{}' is not valid JSON: {}",
            path.display(),
            error
        ))
    })?;
    if value["discovery_schema"].is_null() || value["source_type"].is_null() {
        return Err(SxmcError::Other(format!(
            "Discovery snapshot '{}' is missing `discovery_schema` or `source_type`. Save it with `sxmc discover ... --output <file>` first.",
            path.display()
        )));
    }
    Ok(value)
}

pub fn load_snapshot_inputs(path: &Path) -> Result<Vec<DiscoverySnapshotEntry>> {
    if path.is_dir() {
        let mut entries = fs::read_dir(path)?
            .filter_map(|entry| entry.ok().map(|item| item.path()))
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        entries.sort();

        let mut loaded = Vec::new();
        for entry in entries {
            let value = load_snapshot(&entry)?;
            loaded.push(DiscoverySnapshotEntry { path: entry, value });
        }

        if loaded.is_empty() {
            return Err(SxmcError::Other(format!(
                "Discovery snapshot directory '{}' did not contain any valid *.json snapshots.",
                path.display()
            )));
        }
        Ok(loaded)
    } else {
        Ok(vec![DiscoverySnapshotEntry {
            path: path.to_path_buf(),
            value: load_snapshot(path)?,
        }])
    }
}

pub fn build_resources(paths: &[PathBuf]) -> Result<Vec<DiscoveryResource>> {
    let mut resources = Vec::new();
    let mut seen = HashMap::<String, usize>::new();

    for input in paths {
        for entry in load_snapshot_inputs(input)? {
            let source_type = entry.value["source_type"]
                .as_str()
                .unwrap_or("discovery")
                .to_string();
            let stem = entry
                .path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("snapshot");
            let base_slug = slugify(&format!("{source_type}-{stem}"));
            let count = seen.entry(base_slug.clone()).or_insert(0);
            let slug = if *count == 0 {
                base_slug
            } else {
                format!("{base_slug}-{}", *count + 1)
            };
            *count += 1;

            resources.push(DiscoveryResource {
                uri: format!("sxmc-discovery://snapshots/{slug}"),
                name: format!(
                    "{} discovery snapshot ({})",
                    source_type.to_uppercase(),
                    entry
                        .path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("snapshot.json")
                ),
                description: format!(
                    "Mounted {} discovery snapshot from {}",
                    source_type,
                    entry.path.display()
                ),
                mime_type: "application/json".into(),
                content: serde_json::to_string_pretty(&entry.value)?,
                path: entry.path,
                source_type,
            });
        }
    }

    Ok(resources)
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_alphanumeric() {
            out.push(lowered);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}
