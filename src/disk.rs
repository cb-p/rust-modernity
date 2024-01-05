use std::{collections::HashMap, path::Path, process::Command};

use anyhow::{anyhow, ensure, Context};
use log::{debug, trace};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

use crate::{
    analyzer::VersionAnalyzer,
    std_versions::{load_version_constructor, VersionConstructor},
};

static VERSION_CONSTRUCTOR: Lazy<VersionConstructor> =
    Lazy::new(|| load_version_constructor().expect("could not process std versions"));

static WARNING_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^warning: `[A-Za-z_-]+` \\(\\w+\\) generated (\\d+) warning").unwrap()
});

#[derive(Debug, Serialize)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub published_at: i64,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    // Ideally we would use a CrateInfo here, but csv doesn't support flatten.
    pub name: String,
    pub version: String,
    pub published_at: i64,

    pub edition: usize,
    pub reported_msrv: Option<usize>,
    pub version_signature: f32,

    pub unsafe_exprs: usize,
    pub total_exprs: usize,
    pub unsafe_fraction: f32,

    pub clippy_warnings: usize,
    pub clippy_warnings_per_expr: f32,
}

fn rust_version_to_number(version: &str) -> Option<usize> {
    version
        .split('.')
        .nth(1)
        .and_then(|s| s.parse::<usize>().ok())
}

fn edition_id(edition: cargo_toml::Edition) -> usize {
    match edition {
        cargo_toml::Edition::E2015 => 0,
        cargo_toml::Edition::E2018 => 1,
        cargo_toml::Edition::E2021 => 2,
    }
}

fn normalize_versions(versions: &HashMap<String, usize>) -> f32 {
    if versions.is_empty() {
        return 1.0;
    }

    let max = versions.values().max().copied().unwrap_or(1) as f32;

    let mut acc = 0.0;
    let mut weight_acc = 0.0;
    for (version, amount) in versions {
        // We normalize using log to emphasize usage of newer versions.
        let weight = (*amount as f32).ln() / max.ln();

        let Some(version_number) = rust_version_to_number(version) else {
            continue;
        };

        acc += version_number as f32 * weight;
        weight_acc += weight;
    }

    acc / weight_acc
}

fn count_clippy_warnings(manifest_path: &Path, all_features: bool) -> anyhow::Result<usize> {
    let mut clippy = Command::new("cargo");
    clippy.arg("clippy");
    if all_features {
        clippy.arg("--all-features");
    }
    let clippy = clippy
        .arg("--manifest-path")
        .arg(manifest_path)
        .output()
        .context("failed to execute cargo clippy")?;

    let out = String::from_utf8(clippy.stderr)?;

    Ok(out
        .lines()
        .filter_map(|line| WARNING_REGEX.captures(line))
        .filter_map(|captures| captures.get(1))
        .filter_map(|n| n.as_str().parse::<usize>().ok())
        .sum())
}

pub fn analyze_single(info: CrateInfo, path: &Path, all_features: bool) -> anyhow::Result<Stats> {
    ensure!(path.is_dir(), "path should be a directory");

    debug!("analyzing {} {}..", info.name, info.version);

    let manifest_path = path.join("Cargo.toml");

    trace!("expanding code...");
    let mut expand = Command::new("cargo");
    expand.arg("expand");
    if all_features {
        expand.arg("--all-features");
    }
    let expand = expand
        .arg("--manifest-path")
        .arg(&manifest_path)
        .output()
        .context("failed to execute cargo-expand")?;

    if !expand.status.success() {
        let error = String::from_utf8(expand.stderr)?;
        let concise_error = error.lines().last().context("no last error line")?;
        return Err(anyhow!("{}", concise_error)).context("could not expand crate");
    }

    let expanded_source_code = String::from_utf8(expand.stdout)?;

    let file: syn::File =
        syn::parse_str(&expanded_source_code).context("could not parse expanded source code")?;

    trace!("analyzing versions...");
    let mut version_analyzer = VersionAnalyzer::new(&VERSION_CONSTRUCTOR);
    version_analyzer.process_file(file);

    let manifest =
        cargo_toml::Manifest::from_path(&manifest_path).context("could not read manifest")?;

    let package = manifest
        .package
        .context("no `package` header in manifest")?;

    debug!("{:?}", version_analyzer.version_counts);
    debug!(
        "unsafe: {}/{}",
        version_analyzer.unsafe_exprs, version_analyzer.total_exprs
    );

    trace!("counting warnings with clippy...");
    let clippy_warnings = count_clippy_warnings(&manifest_path, all_features)
        .context("failed to count clippy warnings")?;

    trace!("finishing up...");
    Ok(Stats {
        name: info.name,
        version: info.version,
        published_at: info.published_at,

        edition: edition_id(
            package
                .edition
                .get()
                .copied()
                .unwrap_or(cargo_toml::Edition::E2015),
        ),
        reported_msrv: package
            .rust_version
            .as_ref()
            .and_then(|v| v.get().ok())
            .and_then(|v| rust_version_to_number(v)),
        version_signature: normalize_versions(&version_analyzer.version_counts),

        unsafe_exprs: version_analyzer.unsafe_exprs,
        total_exprs: version_analyzer.total_exprs,
        unsafe_fraction: version_analyzer.unsafe_exprs as f32 / version_analyzer.total_exprs as f32,

        clippy_warnings,
        clippy_warnings_per_expr: clippy_warnings as f32 / version_analyzer.total_exprs as f32,
    })
}
