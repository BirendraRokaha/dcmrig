/*!
The main entry point into dcmrig.
*/
mod anon;
mod args;
mod cookbook_parser;
mod deid;
mod sort;

use crate::args::EntityType;

use anon::dicom_anon;
use deid::dicom_deid;
use sort::dicom_sort;

use anyhow::{Ok, Result};
use args::ArgsParser;
use clap::Parser;
use dcmrig_rs::print_logo;
use tracing::{error, info, warn, Level};

fn app() -> Result<()> {
    let start_time = std::time::Instant::now();
    let args = ArgsParser::parse();

    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .without_time()
            .with_max_level(if args.verbose {
                Level::DEBUG
            } else {
                Level::INFO
            })
            .finish(),
    )?;
    print_logo();
    // Only executes if one of the 4 subcommands are provided
    match args.action_type {
        EntityType::Sort(sort_command) => dicom_sort(
            sort_command.source,
            sort_command.destination,
            sort_command.sort_order,
        )?,
        EntityType::Deid(deid_command) => dicom_deid(
            deid_command.source,
            deid_command.destination,
            deid_command.mapping_table,
        )?,
        EntityType::Anon(anon_command) => dicom_anon(
            anon_command.source,
            anon_command.destination,
            anon_command.prefix,
        )?,
        EntityType::Report(_report_command) => {
            warn!("Report function Not setup yet");
        }
    }

    let elapsed_time = std::time::Instant::now() - start_time;
    info!(
        "Total time: {}.{:03} seconds",
        elapsed_time.as_secs(),
        elapsed_time.subsec_millis()
    );
    Ok(())
}

fn main() -> Result<()> {
    app().unwrap_or_else(|_| error!("Unexpected error during execution!"));
    Ok(())
}
