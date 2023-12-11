/*!
The main entry point into dcmrig.
*/

mod anon;
mod args;
mod deid;
mod sort;

use crate::args::EntityType;
use anon::dicom_anon;
use anyhow::{Ok, Result};
use args::ArgsParser;
use clap::Parser;
use dcmrig_rs::print_logo;
use deid::dicom_deid;
use sort::dicom_sort;
use tracing::{error, warn, Level};

fn app() -> Result<()> {
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
        EntityType::Anon(anon_command) => {
            dicom_anon(anon_command.source, anon_command.destination)?
        }
        EntityType::Report(_report_command) => {
            warn!("Report function Not setup yet");
        }
    }
    Ok(())
}

fn main() {
    app().unwrap_or_else(|_| error!("Unexpected error!"))
}
