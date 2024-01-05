use std::path::Path;

use log::info;

use crate::disk::{analyze_single, CrateInfo};

mod analyzer;
mod disk;
mod std_versions;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    info!(
        "{:?}",
        analyze_single(
            CrateInfo {
                name: "once_cell".to_string(),
                version: "1.0.0".to_string(),
                published_at: 0
            },
            Path::new("../once_cell")
        )?
    );

    Ok(())
}
