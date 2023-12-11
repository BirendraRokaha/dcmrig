use anyhow::{bail, Error, Result};
use dcmrig_rs::*;
use dicom::object::{open_file, FileDicomObject, InMemDicomObject};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::{prelude::*, ThreadPoolBuilder};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::exit,
};
use tracing::{debug, error, info, warn};
use walkdir::WalkDir;

pub fn dicom_deid(
    source_path: PathBuf,
    destination_path: PathBuf,
    mapping_table: PathBuf,
) -> Result<()> {
    info!(
        "Deidentifying the data for >> SOURCE: {} | DESTINATION: {} | MappingTable: {}",
        source_path.display(),
        destination_path.display(),
        mapping_table.display(),
    );

    let pool = ThreadPoolBuilder::new().num_threads(256).build().unwrap();
    pool.install(|| {
        check_source_path_exists(&source_path);
        check_destination_path_exists(&destination_path);
        let mapping_dict = generate_mapping_dict(mapping_table.clone()).unwrap_or_else(|_| {
            error!("Can't open the mapping table: {}", mapping_table.display());
            exit(1);
        });
        info!("Indexing files from: {}", source_path.display());
        let all_files: Vec<_> = WalkDir::new(source_path)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .par_bridge()
            .filter(|entry| entry.file_type().is_file())
            .collect();
        let total_len: u64 = all_files.len() as u64;
        info!("Total files found: {} | Starting deid", total_len);
        let pb = ProgressBar::new(total_len);
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] ({pos}/{len}, ETA {eta})").unwrap(),
        );
        all_files
            .par_iter()
            .enumerate()
            .for_each(|(_index, working_path)| {
                if let Ok(dcm_obj) = open_file(working_path.path()) {
                    deid_each_dcm_file(&dcm_obj, &destination_path, &mapping_dict)
                        .unwrap_or_else(|_| error!("Can't DeID {:#?}", &working_path.file_name()));
                } else {
                    copy_non_dicom_files(&working_path, &destination_path).unwrap_or_else(|_| {
                        error!("Can't copy non dicom file {:#?}", &working_path.file_name())
                    })
                }
                pb.inc(1);
            });
        pb.finish();
    });
    info!("DICOM DeID complete!");
    Ok(())
}

/// Deidentify each file based on the mapping dict
/// Generate filename and path based on DICOM tags
/// Save the file to the necessary directory
/// All Destination directories will be created recursively
fn deid_each_dcm_file(
    dcm_obj: &FileDicomObject<InMemDicomObject>,
    destination_path: &PathBuf,
    mapping_dict: &HashMap<String, String>,
) -> Result<(), Error> {
    let patient_id = dcm_obj.element_by_name("PatientID")?.to_str()?.to_string();
    let patient_deid = match mapping_dict.get(&patient_id) {
        Some(deid) => deid.to_string(),
        None => bail!("DeID for {patient_id} is not found"),
    };
    let new_dicom_object = modify_tags_with_id(dcm_obj.clone(), patient_deid)?;
    let dicom_tags_values = get_sanitized_tag_values(new_dicom_object.clone())?;
    let file_name = generate_dicom_file_name(dicom_tags_values.clone(), "DeID".to_string())?;
    let dir_path = generate_dicom_file_path(dicom_tags_values, &destination_path)?;
    create_target_dir(&dir_path)?;
    let mut full_path = format!("{}/{}", dir_path, file_name);
    full_path = check_if_dup_exists(full_path);
    debug!("Saving file: {} to: {}", file_name, dir_path);
    new_dicom_object.write_to_file(full_path)?;
    Ok(())
}

/// Generate a dictionary based on the Mapping table
/// Eg DeID001,U012345 >> {"U012345"; "DeID001"}
/// All lines that dont follow DeID,PatientID pattern will be ignored
fn generate_mapping_dict(mapping_table: PathBuf) -> Result<HashMap<String, String>> {
    let mut data_map: HashMap<String, String> = HashMap::new();
    if let Ok(file) = File::open(&mapping_table) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(line) = line {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() == 2 {
                    if parts[0].is_empty() || parts[1].is_empty() {
                        continue;
                    }
                    let key = parts[1].trim().to_string();
                    let value = parts[0].trim().to_string();
                    data_map.insert(key, value);
                } else {
                    warn!("Invalid line: {}", line);
                }
            }
        }
    } else {
        error!("Failed to open file {}", &mapping_table.display());
        bail!("")
    }
    Ok(data_map)
}
