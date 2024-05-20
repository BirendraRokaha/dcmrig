use crate::cookbook_parser::parse_toml_cookbook;
use anyhow::Result;
use crossbeam::sync::WaitGroup;
use dcmrig_rs::*;

use dicom::{
    core::{dictionary::DataDictionaryEntryRef, VR},
    object::{FileDicomObject, InMemDicomObject},
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
    let (
        match_id,
        mask_tag_config,
        mask_vr_config,
        add_config,
        delete_tag_config,
        private_tags_del,
    ) = parse_toml_cookbook()?;

    // Set up required variables
    let (all_files, total_len, pb) = preprocessing_setup(&source_path, &destination_path)?;
    let failed_case: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let non_dcm_cases: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let mapping_dict = generate_mapping_dict(&mapping_table).unwrap_or_else(|_| {
        error!("Can't open the mapping table: {}", mapping_table.display());
        exit(1);
    });
    let wg = WaitGroup::new();

    // Main Loop
    all_files
        .par_iter()
        .enumerate()
        .for_each(|(_index, working_path)| {
            if let Ok(dcm_obj) = dicom::object::OpenFileOptions::new()
                .read_all()
                .open_file(working_path.path())
            {
                deid_each_dcm_file(
                    &dcm_obj,
                    &destination_path,
                    mapping_dict.clone(),
                    match_id.clone(),
                    mask_tag_config.clone(),
                    mask_vr_config.clone(),
                    delete_tag_config.clone(),
                    add_config.clone(),
                    private_tags_del.clone(),
                    wg.clone(),
                )
                .unwrap_or_else(|_| {
                    let mut map = failed_case.lock().expect("Failed to lock mutex");
                    *map += 1;
                    error!(
                        "Can't DeID {:#?} Copying to FAILED_CASES directory",
                        &working_path.file_name()
                    );
                    failed_case_copy(&working_path.clone().into_path(), &destination_path)
                        .expect("Failed to copy file to FAILED_CASES directory");
                });
            } else {
                let nwg = wg.clone();
                let mut map = non_dcm_cases.lock().expect("Failed to lock mutex");
                *map += 1;
                copy_non_dicom_files(&working_path, &destination_path).unwrap_or_else(|_| {
                    error!("Can't copy non dicom file {:#?}", &working_path.file_name());
                });
                drop(nwg);
            }
            pb.inc(1);
        });
    pb.finish();
    print_status(
        total_len,
        *failed_case.lock().expect("Failed to lock mutex"),
        *non_dcm_cases.lock().expect("Failed to lock mutex"),
        "DeID".to_string(),
    )?;
    info!("Waiting for all threads to complete");
    wg.wait();
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
    mapping_dict: HashMap<String, String>,
    match_id: DataDictionaryEntryRef<'static>,
    mask_tag_config_list: Vec<DataDictionaryEntryRef<'static>>,
    mask_vr_config_list: Vec<VR>,
    delete_tag_config_list: Vec<DataDictionaryEntryRef<'static>>,
    add_config_list: HashMap<String, String>,
    private_tags_del: bool,
    wg: WaitGroup,
) -> Result<()> {
    let tag_to_match = dcm_obj.element(match_id.tag.inner())?.to_str()?.to_string();
    let patient_deid = match mapping_dict.get(&tag_to_match) {
        Some(deid) => deid.to_string(),
        None => "".to_string(),
    };

    if patient_deid.is_empty() {
        debug!("DeID for {tag_to_match} is not found");
        return Ok(());
    }
    let mut new_dicom_object = dcm_obj.clone();

    if private_tags_del {
        new_dicom_object = delete_private_tags(new_dicom_object)?
    }

    let new_dicom_object = match mask_tag_config_list.is_empty() {
        true => new_dicom_object,
        false => tags_to_mask(
            new_dicom_object.clone(),
            patient_deid.clone(),
            mask_tag_config_list,
        )?,
    };

    let new_dicom_object = match mask_vr_config_list.is_empty() {
        true => new_dicom_object,
        false => mask_vr(new_dicom_object, mask_vr_config_list, patient_deid.clone())?,
    };

    let new_dicom_object = match add_config_list.is_empty() {
        true => new_dicom_object,
        false => tags_to_add(new_dicom_object.clone(), add_config_list)?,
    };

    let new_dicom_object = match delete_tag_config_list.is_empty() {
        true => new_dicom_object,
        false => tags_to_delete(new_dicom_object.clone(), delete_tag_config_list)?,
    };

    let dicom_tags_values = get_sanitized_tag_values(&new_dicom_object)?;

    let new_dp = destination_path.clone();
    let dcm_obj_clone = new_dicom_object.clone();

    rayon::spawn(move || {
        let file_name = generate_dicom_file_name(&dicom_tags_values, "DeID".to_string())
            .expect("Failed to generate file name");
        let dir_path = generate_dicom_file_path(dicom_tags_values, &new_dp)
            .expect("Failed to generate DIR path");
        let full_path = check_if_dup_exists(format!("{}/{}", dir_path, file_name));
        debug!("Saving file: {} to: {}", file_name, dir_path);
        let dcm_buffer = File::create(full_path).expect("Failed to create file");
        dcm_obj_clone
            .write_all(dcm_buffer)
            .expect("Failed to add dcm value to buffer");
        drop(wg);
    });
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
        exit(1);
    }
    Ok(data_map)
}
