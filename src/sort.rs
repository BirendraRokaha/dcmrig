use anyhow::Result;
use crossbeam::sync::WaitGroup;
use dcmrig_rs::*;
use dicom::object::{open_file, FileDicomObject, InMemDicomObject};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, warn};
use walkdir::DirEntry;
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
    let sort_order_vec = generate_sort_order(sort_order)?;
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
                sort_each_dcm_file(
                    working_path,
                    &dcm_obj,
                    &destination_path,
                    &sort_order_vec,
                    wg.clone(),
                )
                .unwrap_or_else(|_| {
                    let mut map = failed_case.lock().expect("Failed to lock mutex");
                    *map += 1;
                    error!(
                        "Can't SORT {:#?} Copying to FAILED_CASES directory",
                        &working_path.file_name()
                    );
                    failed_case_copy(&working_path.clone().into_path(), &destination_path)
                        .expect("Failed to copy file to FAILED_CASES directory");
                });
            } else {
                let mut map = non_dcm_cases.lock().expect("Failed to lock mutex");
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
        *failed_case.lock().expect("Failed to lock mutex"),
        *non_dcm_cases.lock().expect("Failed to lock mutex"),
        "Sorted".to_string(),
    )?;
    wg.wait();
    info!("DICOM Sort complete!");
    Ok(())
}

// DICOM SORT
fn sort_each_dcm_file(
    source_path: &DirEntry,
    dcm_obj: &FileDicomObject<InMemDicomObject>,
    destination_path: &PathBuf,
    sort_order_vec: &Vec<String>,
    wg: WaitGroup,
) -> Result<()> {
    let dicom_tags_values = get_sanitized_tag_values(&dcm_obj)?;
    let order_level = generate_order_level(sort_order_vec, &dicom_tags_values, &dcm_obj)?;
    let file_name = generate_dicom_file_name(
        &dicom_tags_values,
        replace_non_alphanumeric(
            dicom_tags_values
                .get("PatientName")
                .expect("Failed to extract value")
                .trim(),
        ),
    )?;
    let dir_path = format!(
        "{}/{}{}T{}_{}/{:0>4}_{}",
        destination_path.display(),
        order_level,
        dicom_tags_values
            .get("StudyDate")
            .expect("Failed to extract value")
            .trim(),
        dicom_tags_values
            .get("StudyTime")
            .expect("Failed to extract value")
            .split(".")
            .next()
            .expect("Failed to extract value"),
        dicom_tags_values
            .get("StudyInstanceUID")
            .expect("Failed to extract value")
            .split(".")
            .last()
            .expect("Failed to extract value"),
        dicom_tags_values
            .get("SeriesNumber")
            .expect("Failed to extract value"),
        replace_non_alphanumeric(
            dicom_tags_values
                .get("SeriesDescription")
                .expect("Failed to extract value")
                .trim()
        )
    );

    let c_source_path = source_path.clone();
    rayon::spawn(move || {
        create_target_dir(&dir_path).expect("Failed to created target dir");
        let full_path = check_if_dup_exists(format!("{}/{}", dir_path, file_name));
        debug!("Saving file: {} to: {}", file_name, dir_path);
        fs::copy(c_source_path.into_path(), full_path)
            .expect("Failed to copy file to sorted destination");
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
    dcm_obj: &FileDicomObject<InMemDicomObject>,
) -> Result<String> {
    let mut order_level: String = "".to_string();

    for each in order_level_vec {
        dcm_obj.element_by_name(&each)?;
        order_level = format!(
            "{}{}/",
            order_level,
            replace_non_alphanumeric(
                dicom_tags_values
                    .get(each.as_str())
                    .expect("Failed to replace")
                    .trim()
            )
        )
    }
    Ok(order_level)
}
