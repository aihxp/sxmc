use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use glob::glob;
use serde_json::{json, Value};

use crate::error::{Result, SxmcError};

struct CodebaseCollections<'a> {
    manifests: &'a mut Vec<Value>,
    task_runners: &'a mut Vec<Value>,
    entrypoints: &'a mut Vec<Value>,
    project_kinds: &'a mut BTreeSet<String>,
    recommended_commands: &'a mut Vec<Value>,
}

pub fn inspect_codebase(root: &Path, compact: bool) -> Result<Value> {
    let root = fs::canonicalize(root).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to resolve codebase root '{}': {}",
            root.display(),
            e
        ))
    })?;

    let cargo_manifest = root.join("Cargo.toml");
    let package_manifest = root.join("package.json");
    let npm_package_manifest = root.join("packaging").join("npm").join("package.json");
    let workflows_dir = root.join(".github").join("workflows");

    let mut manifests = Vec::new();
    let mut task_runners = Vec::new();
    let mut entrypoints = Vec::new();
    let mut configs = Vec::new();
    let mut project_kinds = BTreeSet::new();
    let mut recommended_commands = Vec::new();

    if cargo_manifest.exists() {
        project_kinds.insert("rust".to_string());
        manifests.push(file_entry("cargo", &cargo_manifest, None));
        task_runners.push(json!({
            "name": "cargo",
            "kind": "rust",
            "path": cargo_manifest.display().to_string(),
        }));
        let cargo_contents = fs::read_to_string(&cargo_manifest).map_err(|e| {
            SxmcError::Other(format!(
                "Failed to read Cargo manifest '{}': {}",
                cargo_manifest.display(),
                e
            ))
        })?;
        let cargo_value: toml::Value = cargo_contents.parse().map_err(|e| {
            SxmcError::Other(format!(
                "Failed to parse Cargo manifest '{}': {}",
                cargo_manifest.display(),
                e
            ))
        })?;
        if let Some(package_name) = cargo_value
            .get("package")
            .and_then(|value| value.get("name"))
            .and_then(toml::Value::as_str)
        {
            entrypoints.push(json!({
                "kind": "cargo-package",
                "name": package_name,
                "path": cargo_manifest.display().to_string(),
            }));
        }
        if let Some(bins) = cargo_value.get("bin").and_then(toml::Value::as_array) {
            for bin in bins {
                entrypoints.push(json!({
                    "kind": "cargo-bin",
                    "name": bin.get("name").and_then(toml::Value::as_str).unwrap_or("<unnamed>"),
                    "path": bin.get("path").and_then(toml::Value::as_str).map(|p| root.join(p).display().to_string()).unwrap_or_else(|| cargo_manifest.display().to_string()),
                }));
            }
        }
        recommended_commands.push(json!({
            "name": "build",
            "command": "cargo build",
            "kind": "cargo",
        }));
        recommended_commands.push(json!({
            "name": "test",
            "command": "cargo test",
            "kind": "cargo",
        }));
    }

    if package_manifest.exists() {
        collect_package_manifest(
            &root,
            &package_manifest,
            "package-json",
            &mut CodebaseCollections {
                manifests: &mut manifests,
                task_runners: &mut task_runners,
                entrypoints: &mut entrypoints,
                project_kinds: &mut project_kinds,
                recommended_commands: &mut recommended_commands,
            },
        )?;
    }
    if npm_package_manifest.exists() {
        collect_package_manifest(
            &root,
            &npm_package_manifest,
            "package-json",
            &mut CodebaseCollections {
                manifests: &mut manifests,
                task_runners: &mut task_runners,
                entrypoints: &mut entrypoints,
                project_kinds: &mut project_kinds,
                recommended_commands: &mut recommended_commands,
            },
        )?;
    }

    let pyproject_manifest = root.join("pyproject.toml");
    if pyproject_manifest.exists() {
        collect_pyproject_manifest(
            &root,
            &pyproject_manifest,
            &mut CodebaseCollections {
                manifests: &mut manifests,
                task_runners: &mut task_runners,
                entrypoints: &mut entrypoints,
                project_kinds: &mut project_kinds,
                recommended_commands: &mut recommended_commands,
            },
        )?;
    }

    for requirements_path in glob_relative_paths(
        &root,
        &[
            "requirements.txt",
            "requirements-*.txt",
            "requirements/*.txt",
        ],
    )? {
        manifests.push(file_entry("requirements", &requirements_path, None));
        project_kinds.insert("python".to_string());
        task_runners.push(json!({
            "name": "pip",
            "kind": "python",
            "path": requirements_path.display().to_string(),
        }));
    }

    let makefile_path = root.join("Makefile");
    if makefile_path.exists() {
        configs.push(file_entry("makefile", &makefile_path, None));
        task_runners.push(json!({
            "name": "make",
            "kind": "task-runner",
            "path": makefile_path.display().to_string(),
        }));
        for target in parse_makefile_targets(&makefile_path)? {
            entrypoints.push(json!({
                "kind": "make-target",
                "name": target,
                "path": makefile_path.display().to_string(),
            }));
        }
        recommended_commands.push(json!({
            "name": "make-help",
            "command": "make",
            "kind": "make",
        }));
    }

    let justfile_path = root.join("Justfile");
    if justfile_path.exists() {
        configs.push(file_entry("justfile", &justfile_path, None));
        task_runners.push(json!({
            "name": "just",
            "kind": "task-runner",
            "path": justfile_path.display().to_string(),
        }));
        for target in parse_justfile_targets(&justfile_path)? {
            entrypoints.push(json!({
                "kind": "just-recipe",
                "name": target,
                "path": justfile_path.display().to_string(),
            }));
        }
        recommended_commands.push(json!({
            "name": "just-default",
            "command": "just",
            "kind": "just",
        }));
    }

    for taskfile_path in glob_relative_paths(&root, &["Taskfile.yml", "Taskfile.yaml"])? {
        configs.push(file_entry("taskfile", &taskfile_path, None));
        task_runners.push(json!({
            "name": "task",
            "kind": "task-runner",
            "path": taskfile_path.display().to_string(),
        }));
        for task_name in parse_taskfile_tasks(&taskfile_path)? {
            entrypoints.push(json!({
                "kind": "taskfile-task",
                "name": task_name,
                "path": taskfile_path.display().to_string(),
            }));
        }
        recommended_commands.push(json!({
            "name": "task-default",
            "command": "task",
            "kind": "taskfile",
        }));
    }

    if workflows_dir.exists() {
        let mut workflow_count = 0usize;
        for workflow in read_sorted_files(&workflows_dir)? {
            if matches!(
                workflow.extension().and_then(|value| value.to_str()),
                Some("yml") | Some("yaml")
            ) {
                configs.push(file_entry("github-workflow", &workflow, None));
                workflow_count += 1;
            }
        }
        if workflow_count > 0 {
            project_kinds.insert("ci".to_string());
            task_runners.push(json!({
                "name": "github-actions",
                "kind": "workflow",
                "path": workflows_dir.display().to_string(),
            }));
        }
    }

    for compose_path in glob_relative_paths(
        &root,
        &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ],
    )? {
        project_kinds.insert("containerized".to_string());
        configs.push(file_entry("compose", &compose_path, None));
        task_runners.push(json!({
            "name": "docker-compose",
            "kind": "container",
            "path": compose_path.display().to_string(),
        }));
        recommended_commands.push(json!({
            "name": "compose-up",
            "command": "docker compose up",
            "kind": "container",
        }));
    }

    for turbo_path in glob_relative_paths(&root, &["turbo.json"])? {
        project_kinds.insert("monorepo".to_string());
        configs.push(file_entry("turbo", &turbo_path, None));
        task_runners.push(json!({
            "name": "turbo",
            "kind": "monorepo",
            "path": turbo_path.display().to_string(),
        }));
    }

    for nx_path in glob_relative_paths(&root, &["nx.json"])? {
        project_kinds.insert("monorepo".to_string());
        configs.push(file_entry("nx", &nx_path, None));
        task_runners.push(json!({
            "name": "nx",
            "kind": "monorepo",
            "path": nx_path.display().to_string(),
        }));
    }

    for tsconfig_path in glob_relative_paths(&root, &["tsconfig.json"])? {
        project_kinds.insert("typescript".to_string());
        configs.push(file_entry("tsconfig", &tsconfig_path, None));
    }

    for vite_path in glob_relative_paths(
        &root,
        &[
            "vite.config.js",
            "vite.config.ts",
            "vite.config.mjs",
            "vite.config.cjs",
        ],
    )? {
        project_kinds.insert("frontend".to_string());
        project_kinds.insert("vite".to_string());
        configs.push(file_entry("vite", &vite_path, None));
    }

    for next_path in glob_relative_paths(
        &root,
        &[
            "next.config.js",
            "next.config.ts",
            "next.config.mjs",
            "next.config.cjs",
        ],
    )? {
        project_kinds.insert("frontend".to_string());
        project_kinds.insert("nextjs".to_string());
        configs.push(file_entry("nextjs", &next_path, None));
    }

    for relative in [
        "README.md",
        "LICENSE",
        ".cursor/rules/sxmc-cli-ai.md",
        ".github/copilot-instructions.md",
        "CLAUDE.md",
        "AGENTS.md",
        "GEMINI.md",
    ] {
        let path = root.join(relative);
        if path.exists() {
            configs.push(file_entry("project-config", &path, Some(relative)));
        }
    }

    let value = json!({
        "discovery_schema": "sxmc_discover_codebase_v1",
        "source_type": "codebase",
        "root": root.display().to_string(),
        "manifest_count": manifests.len(),
        "task_runner_count": task_runners.len(),
        "entrypoint_count": entrypoints.len(),
        "config_count": configs.len(),
        "project_kind_count": project_kinds.len(),
        "recommended_command_count": recommended_commands.len(),
        "project_kinds": project_kinds.into_iter().collect::<Vec<_>>(),
        "recommended_commands": recommended_commands,
        "manifests": manifests,
        "task_runners": task_runners,
        "entrypoints": entrypoints,
        "configs": configs,
    });

    if compact {
        Ok(json!({
            "discovery_schema": value["discovery_schema"],
            "source_type": value["source_type"],
            "root": value["root"],
            "manifest_count": value["manifest_count"],
            "task_runner_count": value["task_runner_count"],
            "entrypoint_count": value["entrypoint_count"],
            "config_count": value["config_count"],
            "project_kind_count": value["project_kind_count"],
            "recommended_command_count": value["recommended_command_count"],
            "project_kinds": value["project_kinds"],
            "recommended_command_names": summarize_names(value["recommended_commands"].as_array()),
            "manifest_kinds": summarize_names(value["manifests"].as_array()),
            "task_runner_names": summarize_names(value["task_runners"].as_array()),
            "entrypoint_names": summarize_names(value["entrypoints"].as_array()),
        }))
    } else {
        Ok(value)
    }
}

pub fn load_codebase_snapshot(path: &Path) -> Result<Value> {
    let contents = fs::read_to_string(path).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read codebase snapshot '{}': {}",
            path.display(),
            e
        ))
    })?;
    let value: Value = serde_json::from_str(&contents).map_err(|e| {
        SxmcError::Other(format!(
            "Codebase snapshot '{}' is not valid JSON: {}",
            path.display(),
            e
        ))
    })?;
    if value["discovery_schema"] != "sxmc_discover_codebase_v1"
        || value["source_type"] != "codebase"
    {
        return Err(SxmcError::Other(format!(
            "Codebase snapshot '{}' is not a valid sxmc codebase discovery artifact.",
            path.display()
        )));
    }
    Ok(value)
}

pub fn diff_codebase_value(before: &Value, after: &Value) -> Value {
    json!({
        "discovery_schema": "sxmc_discover_codebase_diff_v1",
        "source_type": "codebase-diff",
        "before_root": before["root"],
        "after_root": after["root"],
        "manifest_count_changed": before["manifest_count"] != after["manifest_count"],
        "task_runner_count_changed": before["task_runner_count"] != after["task_runner_count"],
        "entrypoint_count_changed": before["entrypoint_count"] != after["entrypoint_count"],
        "config_count_changed": before["config_count"] != after["config_count"],
        "project_kinds_added": string_set_diff(after["project_kinds"].as_array(), before["project_kinds"].as_array()),
        "project_kinds_removed": string_set_diff(before["project_kinds"].as_array(), after["project_kinds"].as_array()),
        "manifests_added": named_entry_diff(after["manifests"].as_array(), before["manifests"].as_array()),
        "manifests_removed": named_entry_diff(before["manifests"].as_array(), after["manifests"].as_array()),
        "task_runners_added": named_entry_diff(after["task_runners"].as_array(), before["task_runners"].as_array()),
        "task_runners_removed": named_entry_diff(before["task_runners"].as_array(), after["task_runners"].as_array()),
        "entrypoints_added": named_entry_diff(after["entrypoints"].as_array(), before["entrypoints"].as_array()),
        "entrypoints_removed": named_entry_diff(before["entrypoints"].as_array(), after["entrypoints"].as_array()),
        "configs_added": named_entry_diff(after["configs"].as_array(), before["configs"].as_array()),
        "configs_removed": named_entry_diff(before["configs"].as_array(), after["configs"].as_array()),
        "recommended_commands_added": command_entry_diff(after["recommended_commands"].as_array(), before["recommended_commands"].as_array()),
        "recommended_commands_removed": command_entry_diff(before["recommended_commands"].as_array(), after["recommended_commands"].as_array()),
    })
}

fn collect_package_manifest(
    root: &Path,
    manifest_path: &Path,
    manifest_kind: &str,
    collections: &mut CodebaseCollections<'_>,
) -> Result<()> {
    collections.project_kinds.insert("node".to_string());
    collections
        .manifests
        .push(file_entry(manifest_kind, manifest_path, None));
    let contents = fs::read_to_string(manifest_path).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read package manifest '{}': {}",
            manifest_path.display(),
            e
        ))
    })?;
    let value: Value = serde_json::from_str(&contents).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to parse package manifest '{}': {}",
            manifest_path.display(),
            e
        ))
    })?;

    let package_name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("<unnamed>");
    collections.task_runners.push(json!({
        "name": package_name,
        "kind": "npm",
        "path": manifest_path.display().to_string(),
    }));
    if let Some(scripts) = value.get("scripts").and_then(Value::as_object) {
        for (name, command) in scripts {
            collections.entrypoints.push(json!({
                "kind": "npm-script",
                "name": name,
                "command": command,
                "path": manifest_path.display().to_string(),
                "workspace_root": root.display().to_string(),
            }));
            if matches!(name.as_str(), "dev" | "start" | "build" | "test" | "lint") {
                collections.recommended_commands.push(json!({
                    "name": name,
                    "command": format!("npm run {name}"),
                    "kind": "npm",
                }));
            }
        }
    }
    Ok(())
}

fn collect_pyproject_manifest(
    root: &Path,
    manifest_path: &Path,
    collections: &mut CodebaseCollections<'_>,
) -> Result<()> {
    collections.project_kinds.insert("python".to_string());
    collections
        .manifests
        .push(file_entry("pyproject", manifest_path, None));
    let contents = fs::read_to_string(manifest_path).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read pyproject manifest '{}': {}",
            manifest_path.display(),
            e
        ))
    })?;
    let value: toml::Value = contents.parse().map_err(|e| {
        SxmcError::Other(format!(
            "Failed to parse pyproject manifest '{}': {}",
            manifest_path.display(),
            e
        ))
    })?;

    let project_name = value
        .get("project")
        .and_then(|section| section.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            value
                .get("tool")
                .and_then(|section| section.get("poetry"))
                .and_then(|section| section.get("name"))
                .and_then(toml::Value::as_str)
        })
        .unwrap_or("<unnamed>");

    collections.task_runners.push(json!({
        "name": project_name,
        "kind": "python",
        "path": manifest_path.display().to_string(),
        "workspace_root": root.display().to_string(),
    }));

    if let Some(scripts) = value
        .get("project")
        .and_then(|section| section.get("scripts"))
        .and_then(toml::Value::as_table)
    {
        for (name, target) in scripts {
            collections.entrypoints.push(json!({
                "kind": "python-script",
                "name": name,
                "command": target.as_str().unwrap_or_default(),
                "path": manifest_path.display().to_string(),
            }));
        }
    }

    if let Some(scripts) = value
        .get("tool")
        .and_then(|section| section.get("poetry"))
        .and_then(|section| section.get("scripts"))
        .and_then(toml::Value::as_table)
    {
        for (name, target) in scripts {
            collections.entrypoints.push(json!({
                "kind": "poetry-script",
                "name": name,
                "command": target.as_str().unwrap_or_default(),
                "path": manifest_path.display().to_string(),
            }));
        }
    }

    collections.recommended_commands.push(json!({
        "name": "python-install",
        "command": "python -m pip install -r requirements.txt",
        "kind": "python",
    }));

    Ok(())
}

fn summarize_names(values: Option<&Vec<Value>>) -> Value {
    let names = values
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .or_else(|| item.get("kind").and_then(Value::as_str))
                })
                .map(|name| Value::String(name.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(names)
}

fn string_set_diff(left: Option<&Vec<Value>>, right: Option<&Vec<Value>>) -> Value {
    let left = left
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let right = right
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    Value::Array(
        left.difference(&right)
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>(),
    )
}

fn named_entry_diff(left: Option<&Vec<Value>>, right: Option<&Vec<Value>>) -> Value {
    let left = entry_identity_set(left);
    let right = entry_identity_set(right);
    Value::Array(
        left.difference(&right)
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>(),
    )
}

fn command_entry_diff(left: Option<&Vec<Value>>, right: Option<&Vec<Value>>) -> Value {
    let left = left
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let name = item["name"].as_str().unwrap_or("<unknown>");
                    let command = item["command"].as_str().unwrap_or("<unknown>");
                    format!("{name}: {command}")
                })
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let right = right
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let name = item["name"].as_str().unwrap_or("<unknown>");
                    let command = item["command"].as_str().unwrap_or("<unknown>");
                    format!("{name}: {command}")
                })
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    Value::Array(
        left.difference(&right)
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>(),
    )
}

fn entry_identity_set(values: Option<&Vec<Value>>) -> BTreeSet<String> {
    values
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let name = item["name"].as_str().unwrap_or("<unknown>");
                    let kind = item["kind"].as_str().unwrap_or("<unknown>");
                    format!("{kind}:{name}")
                })
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default()
}

fn glob_relative_paths(root: &Path, patterns: &[&str]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for pattern in patterns {
        let absolute_pattern = root.join(pattern).display().to_string();
        let matches = glob(&absolute_pattern)
            .map_err(|e| SxmcError::Other(format!("Invalid glob pattern '{pattern}': {e}")))?;
        for entry in matches {
            match entry {
                Ok(path) if path.exists() => paths.push(path),
                Ok(_) => {}
                Err(error) => {
                    return Err(SxmcError::Other(format!(
                        "Failed to match glob pattern '{pattern}': {error}"
                    )))
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn parse_makefile_targets(path: &Path) -> Result<Vec<String>> {
    let contents = fs::read_to_string(path).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read Makefile '{}': {}",
            path.display(),
            e
        ))
    })?;
    let mut targets = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty()
            || trimmed.starts_with('\t')
            || trimmed.starts_with(' ')
            || trimmed.starts_with('#')
            || trimmed.starts_with('.')
            || trimmed.contains(":=")
            || trimmed.contains("?=")
            || trimmed.contains("+=")
        {
            continue;
        }
        if let Some((target, _)) = trimmed.split_once(':') {
            let name = target.trim();
            if !name.is_empty() && !name.contains(' ') && !name.contains('=') {
                targets.push(name.to_string());
            }
        }
    }
    targets.sort();
    targets.dedup();
    Ok(targets)
}

fn parse_justfile_targets(path: &Path) -> Result<Vec<String>> {
    let contents = fs::read_to_string(path).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read Justfile '{}': {}",
            path.display(),
            e
        ))
    })?;
    let mut targets = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("set ")
            || trimmed.starts_with("import ")
        {
            continue;
        }
        if let Some((target, _)) = trimmed.split_once(':') {
            let name = target
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches('@');
            if !name.is_empty()
                && !name.starts_with('.')
                && name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                targets.push(name.to_string());
            }
        }
    }
    targets.sort();
    targets.dedup();
    Ok(targets)
}

fn parse_taskfile_tasks(path: &Path) -> Result<Vec<String>> {
    let contents = fs::read_to_string(path).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to read Taskfile '{}': {}",
            path.display(),
            e
        ))
    })?;
    let value: Value = serde_yaml::from_str(&contents).map_err(|e| {
        SxmcError::Other(format!(
            "Failed to parse Taskfile '{}': {}",
            path.display(),
            e
        ))
    })?;
    let mut tasks = value
        .get("tasks")
        .and_then(Value::as_object)
        .map(|tasks| tasks.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    tasks.sort();
    Ok(tasks)
}

fn file_entry(kind: &str, path: &Path, label: Option<&str>) -> Value {
    json!({
        "kind": kind,
        "name": label.unwrap_or_else(|| path.file_name().and_then(|value| value.to_str()).unwrap_or("<unknown>")),
        "path": path.display().to_string(),
    })
}

fn read_sorted_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = fs::read_dir(dir)
        .map_err(|e| SxmcError::Other(format!("Failed to read '{}': {}", dir.display(), e)))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}
