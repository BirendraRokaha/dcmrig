use anyhow::Result;
use crossbeam::sync::WaitGroup;
use dcmrig_rs::*;
use dicom::object::{open_file, FileDicomObject, InMemDicomObject};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, warn};

pub fn dicom_sort(
    source_path: PathBuf,
    destination_path: PathBuf,
    sort_order: String,
) -> Result<()> {
    info!(
        "Sorting the data for >> SOURCE: {} | DESTINATION: {}",
        source_path.display(),
        destination_path.display()
    );

    // Set up required variables
    let (all_files, total_len, pb) = preprocessing_setup(&source_path, &destination_path)?;
    let sort_order_vec = generate_sort_order(sort_order).unwrap();
    let failed_case: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let non_dcm_cases: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    info!("Sort Order {:?}", sort_order_vec);

    let wg = WaitGroup::new();
    // Main loop
    all_files
        .par_iter()
        .enumerate()
        .for_each(|(_index, working_path)| {
            if let Ok(dcm_obj) = open_file(working_path.path()) {
                sort_each_dcm_file(&dcm_obj, &destination_path, &sort_order_vec, wg.clone())
                    .unwrap_or_else(|_| {
                        let mut map = failed_case.lock().unwrap();
                        *map += 1;
                        error!("Cannot sort {:#?}", &working_path.file_name())
                    });
            } else {
                let mut map = non_dcm_cases.lock().unwrap();
                *map += 1;
                copy_non_dicom_files(&working_path, &destination_path).unwrap_or_else(|_| {
                    error!("Can't copy non dicom file {:#?}", &working_path.file_name())
                })
            }
            pb.inc(1);
        });
    pb.finish();
    print_status(
        total_len,
        *failed_case.lock().unwrap(),
        *non_dcm_cases.lock().unwrap(),
        "Sorted".to_string(),
    )
    .unwrap();
    wg.wait();
    info!("DICOM Sort complete!");
    Ok(())
}

// DICOM SORT
fn sort_each_dcm_file(
    dcm_obj: &FileDicomObject<InMemDicomObject>,
    destination_path: &PathBuf,
    sort_order_vec: &Vec<String>,
    wg: WaitGroup,
) -> Result<()> {
    let dicom_tags_values = get_sanitized_tag_values(&dcm_obj)?;
    let order_level = generate_order_level(sort_order_vec, &dicom_tags_values);
    let file_name = generate_dicom_file_name(
        &dicom_tags_values,
        replace_non_alphanumeric(dicom_tags_values.get("PatientName").unwrap().trim()),
    )?;
    let dir_path = format!(
        "{}/{}{}T{}_{}/{:0>4}_{}",
        destination_path.display(),
        order_level?,
        dicom_tags_values.get("StudyDate").unwrap().trim(),
        dicom_tags_values
            .get("StudyTime")
            .unwrap()
            .split(".")
            .next()
            .unwrap(),
        dicom_tags_values
            .get("StudyInstanceUID")
            .unwrap()
            .split(".")
            .last()
            .unwrap(),
        dicom_tags_values.get("SeriesNumber").unwrap(),
        replace_non_alphanumeric(dicom_tags_values.get("SeriesDescription").unwrap().trim())
    );
    // create_target_dir(&dir_path)?;
    // let full_path = check_if_dup_exists(format!("{}/{}", dir_path, file_name));
    // debug!("Saving file: {} to: {}", file_name, dir_path);
    // dcm_obj.write_to_file(full_path)?;
    let dcm_obj_clone = dcm_obj.clone();
    rayon::spawn(move || {
        create_target_dir(&dir_path).expect("Failed to created target dir");
        let full_path = check_if_dup_exists(format!("{}/{}", dir_path, file_name));
        debug!("Saving file: {} to: {}", file_name, dir_path);
        dcm_obj_clone
            .write_to_file(full_path)
            .expect("Failed to save file");
        drop(wg);
    });
    Ok(())
}

// Generate the DIR order level from the given input
// Any combination if I=PatientID, N=PatientName, or M=Modality PatientID is the default
fn generate_sort_order(ord_input: String) -> Result<Vec<String>> {
    let mut order_level_vec: Vec<String> = vec![];
    for each in ord_input.to_uppercase().chars().into_iter() {
        match each.to_string().as_str() {
            "I" => order_level_vec.push("PatientID".to_string()),
            "N" => order_level_vec.push("PatientName".to_string()),
            "M" => order_level_vec.push("Modality".to_string()),
            &_ => (),
        }
    }
    if order_level_vec.is_empty() {
        warn!("Valid SortOrder not found, Default PatientID will be used");
        order_level_vec.push("PatientID".to_string())
    }
    Ok(order_level_vec)
}

fn generate_order_level(
    order_level_vec: &Vec<String>,
    dicom_tags_values: &HashMap<String, String>,
) -> Result<String> {
    let mut order_level: String = "".to_string();
    for each in order_level_vec {
        order_level = format!(
            "{}{}/",
            order_level,
            replace_non_alphanumeric(dicom_tags_values.get(each.as_str()).unwrap().trim())
        )
    }
    Ok(order_level)
}
