use std::{io::Cursor, path::Path, time::Duration};

use anyhow::Context;
use crates_io_api::{SyncClient, Version};
use disk::{analyze_single, CrateInfo, Stats};
use flate2::read::GzDecoder;
use indicatif::{MultiProgress, ProgressBar, ProgressIterator, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
use log::{debug, error, trace, LevelFilter};
use once_cell::sync::Lazy;
use reqwest::Url;
use tar::Archive;

mod analyzer;
mod disk;
mod std_versions;

const TEMP_DIR: &str = ".current_crate";
const OUT_DIR: &str = "results";
const VERSION_COUNT: usize = 20;

static API_CLIENT: Lazy<SyncClient> = Lazy::new(|| {
    SyncClient::new(
        "rust-modernity (GitHub @chrrs)",
        Duration::from_millis(2000),
    )
    .expect("failed to initialize crates.io API client")
});

fn analyze_version(version: &Version) -> anyhow::Result<Stats> {
    let url = Url::parse("https://crates.io/")?.join(&version.dl_path)?;
    trace!("downloading from {url}...");
    let res = reqwest::blocking::get(url).and_then(|res| res.bytes())?;

    trace!("extracting archive...");
    let temp_dir = Path::new(TEMP_DIR);
    let decoder = GzDecoder::new(Cursor::new(res));
    let mut archive = Archive::new(decoder);
    archive.unpack(temp_dir).context("failed to unpack")?;

    let stats = analyze_single(
        CrateInfo {
            name: version.crate_name.clone(),
            version: version.num.clone(),
            published_at: version.created_at.timestamp(),
        },
        &temp_dir.join(format!("{}-{}", version.crate_name, version.num)),
    )
    .context("failed to analyze");

    std::fs::remove_dir_all(temp_dir).context("failed to delete temp dir")?;

    stats
}

fn analyze_from_crates_io(progress: ProgressBar, name: &str) -> anyhow::Result<Vec<Stats>> {
    let res = API_CLIENT
        .get_crate(name)
        .context("failed to get crate information from API")?;

    trace!("{} has {} available versions", name, res.versions.len());

    let mut versions = res
        .versions
        .into_iter()
        .filter(|version| !version.yanked)
        .collect::<Vec<_>>();

    // FIXME: Multiple versions released in a short time might make the
    //        version selection inaccurate.
    if versions.len() > VERSION_COUNT {
        let indices = (0..VERSION_COUNT)
            .map(|i| (i * (versions.len() - 1)) / (VERSION_COUNT - 1))
            .collect::<Vec<_>>();

        for i in (0..versions.len()).rev() {
            if !indices.contains(&i) {
                versions.remove(i);
            }
        }
    }

    debug!(
        "selected {} versions {:?}",
        name,
        versions.iter().map(|v| &v.num).collect::<Vec<_>>()
    );

    let mut stats = Vec::with_capacity(versions.len());
    for version in versions.iter().progress_with(progress.clone()) {
        progress.set_message(version.num.clone());

        let stat = match analyze_version(version) {
            Ok(stat) => stat,
            Err(err) => {
                error!("could not analyze {name} {}: {err:#}", version.num);
                continue;
            }
        };

        debug!("{stat:?}");
        stats.push(stat);
    }

    Ok(stats)
}

fn main() -> anyhow::Result<()> {
    let multi = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>2}/{len:2} {prefix} {msg}",
    )
    .unwrap()
    .progress_chars("##-");

    let logger = env_logger::builder()
        .filter(Some("reqwest"), LevelFilter::Info)
        .build();

    LogWrapper::new(multi.clone(), logger).try_init().unwrap();

    // Create output directory beforehand
    let out_dir = Path::new(OUT_DIR);
    std::fs::create_dir_all(out_dir)?;

    // Analyze the crate versions
    let name = "regex";

    let progress = multi.add(
        ProgressBar::new(VERSION_COUNT as u64)
            .with_style(style)
            .with_prefix(name.to_string()),
    );

    let stats = analyze_from_crates_io(progress.clone(), name)?;

    progress.abandon_with_message(format!("analyzed with {VERSION_COUNT} versions"));

    // Write results to CSV
    let mut writer = csv::Writer::from_path(out_dir.join(format!("{name}.csv")))?;

    for stat in stats {
        writer.serialize(stat)?;
    }

    writer.flush()?;

    Ok(())
}
