use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(
    author = "Birendra Rokaha <birenrokaha1@gmail.com>",
    version,
    about = "DCMRig >> High performance DICOM corelab tools"
)]
pub struct ArgsParser {
    #[clap(subcommand)]
    pub action_type: EntityType,
    /// Verbose output
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum EntityType {
    /// Sort the given source with any combination of PatientID, PatientName or Modality
    Sort(SortCommand),
    /// Anonymize the given source each PatientID will be given a unique AnonID.
    Anon(AnonCommand),
    /// Deidentify the given source based on a mapping table
    Deid(DeidCommand),
    /// [NON FUNCTIONAL] Generate a report for a sorted dataset
    Report(ReportCommand),
}

#[derive(Debug, Args)]
pub struct SortCommand {
    /// Sort order can be any combination of I=PatientID, N=PatientName, and M=Modality
    #[clap(short, long, default_value = "I")]
    pub sort_order: String,
    /// Source data path, All files will be recursively indexed
    pub source: PathBuf,
    /// Destination data path, the paths will be recursively created
    pub destination: PathBuf,
}

#[derive(Debug, Args)]
pub struct AnonCommand {
    /// Prefix for the ANON ID, Default Blank
    #[clap(short, long, default_value = "")]
    pub prefix: String,
    /// Source data path, All files will be recursively indexed
    pub source: PathBuf,
    /// Destination data path, the paths will be recursively created
    pub destination: PathBuf,
}

#[derive(Debug, Args)]
pub struct DeidCommand {
    /// Mapping table in the following order separated by line DEID,PatientID eg DEID_001,U012345
    #[clap(short, long)]
    pub mapping_table: PathBuf,
    /// Source data path, All files will be recursively indexed
    pub source: PathBuf,
    /// Destination data path, the paths will be recursively created
    pub destination: PathBuf,
}

#[derive(Debug, Args)]
pub struct ReportCommand {
    /// Source data path, All files will be recursively indexed
    pub source: PathBuf,
    /// Destination data path for the csv file
    pub destination: PathBuf,
}
