use std::{
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use clap::Parser;
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

static API_CLIENT: Lazy<SyncClient> = Lazy::new(|| {
    SyncClient::new(
        "rust-modernity (GitHub @chrrs)",
        Duration::from_millis(2000),
    )
    .expect("failed to initialize crates.io API client")
});

#[derive(Parser)]
#[command(version)]
struct Args {
    /// Crate name on crates.io to analyze
    crate_: String,

    /// Amount of versions to fetch and analyze
    #[arg(short, long, default_value_t = 20)]
    versions: usize,

    /// Location of the output CSV file
    #[arg(short, long)]
    out_file: Option<PathBuf>,

    /// Analyze using only the default crate features
    #[arg(short, long)]
    not_all_features: bool,
}

fn analyze_version(version: &Version, all_features: bool) -> anyhow::Result<Stats> {
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
        all_features,
    )
    .context("failed to analyze");

    std::fs::remove_dir_all(temp_dir).context("failed to delete temp dir")?;

    stats
}

fn analyze_from_crates_io(
    progress: ProgressBar,
    name: &str,
    count: usize,
    all_features: bool,
) -> anyhow::Result<Vec<Stats>> {
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
    if versions.len() > count {
        let indices = (0..count)
            .map(|i| (i * (versions.len() - 1)) / (count - 1))
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

        let stat = match analyze_version(version, all_features) {
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
    let args = Args::parse();

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

    // Prepare output file
    let csv_path = args.out_file.unwrap_or_else(|| {
        let out_dir = Path::new(OUT_DIR);
        std::fs::create_dir_all(out_dir).expect("failed to create results dir");
        out_dir.join(format!("{}.csv", args.crate_))
    });

    // Analyze the crate versions
    let name = &args.crate_;

    let progress = multi.add(
        ProgressBar::new(args.versions as u64)
            .with_style(style)
            .with_prefix(name.to_string()),
    );

    let stats = analyze_from_crates_io(
        progress.clone(),
        name,
        args.versions,
        !args.not_all_features,
    )?;

    progress.abandon_with_message(format!("analyzed with {} versions", stats.len()));

    // Write results to CSV
    let mut writer = csv::Writer::from_path(csv_path)?;

    for stat in stats {
        writer.serialize(stat)?;
    }

    writer.flush()?;

    Ok(())
}
