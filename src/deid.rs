use crate::cookbook_parser::parse_toml_cookbook;
use anyhow::{bail, Error, Result};
use dcmrig_rs::*;
use dicom::{
    core::dictionary::DataDictionaryEntryRef,
    object::{open_file, FileDicomObject, InMemDicomObject},
};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::exit,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, warn};

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

    // Get cookbook configs
    let (mask_config, add_config, delete_config) = parse_toml_cookbook()?;

    // Set up required variables
    let (all_files, total_len, pb) = preprocessing_setup(&source_path, &destination_path)?;
    let failed_case: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let non_dcm_cases: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let mapping_dict = generate_mapping_dict(&mapping_table).unwrap_or_else(|_| {
        error!("Can't open the mapping table: {}", mapping_table.display());
        exit(1);
    });

    // Main Loop
    all_files
        .par_iter()
        .enumerate()
        .for_each(|(_index, working_path)| {
            if let Ok(dcm_obj) = open_file(working_path.path()) {
                deid_each_dcm_file(
                    &dcm_obj,
                    &destination_path,
                    &mapping_dict,
                    &mask_config,
                    &delete_config,
                    &add_config,
                )
                .unwrap_or_else(|_| {
                    let mut map = failed_case.lock().unwrap();
                    *map += 1;
                    error!("Can't DeID {:#?}", &working_path.file_name());
                });
            } else {
                let mut map = non_dcm_cases.lock().unwrap();
                *map += 1;
                copy_non_dicom_files(&working_path, &destination_path).unwrap_or_else(|_| {
                    error!("Can't copy non dicom file {:#?}", &working_path.file_name());
                })
            }
            pb.inc(1);
        });
    pb.finish();
    print_status(
        total_len,
        *failed_case.lock().unwrap(),
        *non_dcm_cases.lock().unwrap(),
        "DeID".to_string(),
    )
    .unwrap();

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
    mask_config_list: &Vec<DataDictionaryEntryRef<'static>>,
    delete_config_list: &Vec<DataDictionaryEntryRef<'static>>,
    add_config_list: &HashMap<String, String>,
) -> Result<(), Error> {
    let patient_id = dcm_obj.element_by_name("PatientID")?.to_str()?.to_string();
    let patient_deid = match mapping_dict.get(&patient_id) {
        Some(deid) => deid.to_string(),
        None => bail!("DeID for {patient_id} is not found"),
    };

    let new_dicom_object = match mask_config_list.is_empty() {
        true => dcm_obj.clone(),
        false => tags_to_mask(dcm_obj.clone(), patient_deid, mask_config_list)?,
    };

    let new_dicom_object = match add_config_list.is_empty() {
        true => new_dicom_object.clone(),
        false => tags_to_add(new_dicom_object.clone(), add_config_list)?,
    };

    let new_dicom_object = match delete_config_list.is_empty() {
        true => new_dicom_object.clone(),
        false => tags_to_delete(new_dicom_object.clone(), delete_config_list)?,
    };

    let dicom_tags_values = get_sanitized_tag_values(&new_dicom_object)?;
    let file_name = generate_dicom_file_name(&dicom_tags_values, "DeID".to_string())?;
    let dir_path = generate_dicom_file_path(dicom_tags_values, &destination_path)?;
    let full_path = check_if_dup_exists(format!("{}/{}", dir_path, file_name));
    debug!("Saving file: {} to: {}", file_name, dir_path);
    new_dicom_object.write_to_file(full_path)?;
    Ok(())
}

/// Generate a dictionary based on the Mapping table
/// Eg DeID001,U012345 >> {"U012345"; "DeID001"}
/// All lines that dont follow DeID,PatientID pattern will be ignored
fn generate_mapping_dict(mapping_table: &PathBuf) -> Result<HashMap<String, String>> {
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
