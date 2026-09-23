use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const REQUIRED_EDITION: &str = "2024";
const EDITION_EXEMPT: &[&str] = &[
    "neoutl-shared-abi",
    "neoutl-effect-shader-build",
    "neoutl-object-shader-build",
];
const DEP_TABLES: &[&str] = &["dependencies", "build-dependencies", "dev-dependencies"];

struct Manifest {
    path: PathBuf,
    doc: toml::Table,
    package_name: Option<String>,
}

fn discover_manifests(root: &Path) -> Vec<Manifest> {
    let mut result = Vec::new();
    collect_manifests(&root.join("crates"), &mut result);
    if let Ok(text) = fs::read_to_string(root.join("Cargo.toml")) {
        if let Ok(doc) = text.parse::<toml::Table>() {
            result.push(Manifest {
                path: root.join("Cargo.toml"),
                package_name: doc
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .map(str::to_owned),
                doc,
            });
        }
    }
    result
}

fn collect_manifests(dir: &Path, out: &mut Vec<Manifest>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_manifests(&path, out);
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) != Some("Cargo.toml") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = text.parse::<toml::Table>() else {
            continue;
        };
        let package_name = doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(str::to_owned);
        out.push(Manifest {
            path,
            doc,
            package_name,
        });
    }
}

fn is_workspace_true(value: &toml::Value) -> bool {
    value
        .get("workspace")
        .and_then(|w| w.as_bool())
        .unwrap_or(false)
}

fn dep_tables_in<'a>(doc: &'a toml::Table) -> Vec<(&'a str, &'a toml::Table)> {
    let mut out = Vec::new();
    for name in DEP_TABLES {
        if let Some(toml::Value::Table(t)) = doc.get(*name) {
            out.push((*name, t));
        }
    }
    if let Some(toml::Value::Table(targets)) = doc.get("target") {
        for (_, cfg) in targets {
            if let toml::Value::Table(cfg_table) = cfg {
                for name in DEP_TABLES {
                    if let Some(toml::Value::Table(t)) = cfg_table.get(*name) {
                        out.push((*name, t));
                    }
                }
            }
        }
    }
    out
}

pub fn run(root: &Path) -> bool {
    let manifests = discover_manifests(root);
    let member_names: BTreeSet<String> = manifests
        .iter()
        .filter_map(|m| m.package_name.clone())
        .collect();

    let workspace_dep_names: BTreeSet<String> = manifests
        .iter()
        .find(|m| m.path == root.join("Cargo.toml"))
        .and_then(|m| m.doc.get("workspace"))
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default();

    let mut violations: Vec<String> = Vec::new();
    let mut external_direct: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for m in &manifests {
        let rel = m
            .path
            .strip_prefix(root)
            .unwrap_or(&m.path)
            .display()
            .to_string();

        if let Some(edition) = m
            .doc
            .get("package")
            .and_then(|p| p.get("edition"))
            .and_then(|e| e.as_str())
        {
            let exempt = m
                .package_name
                .as_deref()
                .map(|n| EDITION_EXEMPT.contains(&n))
                .unwrap_or(false);
            if edition != REQUIRED_EDITION && !exempt {
                violations.push(format!(
                    "{rel}: edition={edition} (期待値={REQUIRED_EDITION})"
                ));
            }
        }

        for (table_name, table) in dep_tables_in(&m.doc) {
            for (dep_name, value) in table {
                if member_names.contains(dep_name) {
                    if !is_workspace_true(value) {
                        violations.push(format!(
                            "{rel} [{table_name}] {dep_name}: workspace.dependencies未使用 (path/version直書き)"
                        ));
                    }
                    continue;
                }
                if workspace_dep_names.contains(dep_name) {
                    if !is_workspace_true(value) {
                        violations.push(format!(
                            "{rel} [{table_name}] {dep_name}: workspace.dependencies登録済みだが直書き"
                        ));
                    }
                    continue;
                }
                if !is_workspace_true(value) {
                    external_direct
                        .entry(dep_name.clone())
                        .or_default()
                        .push(rel.clone());
                }
            }
        }
    }

    for (dep_name, locations) in &external_direct {
        if locations.len() > 1 {
            violations.push(format!(
                "{dep_name}: 複数crateで直書き重複 ({}) — workspace.dependencies集約対象",
                locations.join(", ")
            ));
        }
    }

    if violations.is_empty() {
        eprintln!("[xtask lint] 違反0件");
        true
    } else {
        eprintln!("[xtask lint] 違反{}件", violations.len());
        for v in &violations {
            eprintln!("  - {v}");
        }
        false
    }
}
