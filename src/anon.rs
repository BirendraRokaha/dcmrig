use anyhow::Result;
use dcmrig_rs::*;
use dicom::{
    core::{DataElement, VR},
    dictionary_std::tags,
    object::{open_file, FileDicomObject, InMemDicomObject},
};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info};
use uuid::Uuid;
use walkdir::WalkDir;

pub fn dicom_anon(source_path: PathBuf, destination_path: PathBuf) -> Result<()> {
    info!(
        "Anonymizing the data for >> SOURCE: {} |DESTINATION: {}",
        source_path.display(),
        destination_path.display()
    );

    let anon_id_tracker: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    check_given_path_exists(&source_path, &destination_path)?;
    info!("Indexing files from: {}", source_path.display());
    let all_files: Vec<_> = WalkDir::new(source_path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .par_bridge()
        .filter(|entry| entry.file_type().is_file())
        .collect();
    let total_len: u64 = all_files.len() as u64;
    info!("Total files found: {} | Starting anon", total_len);
    let failed_case: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let non_dcm_cases: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));
    let pb = ProgressBar::new(total_len);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] ({pos}/{len}, ETA {eta})",
        )
        .unwrap(),
    );
    all_files
        .par_iter()
        .enumerate()
        .for_each(|(_index, working_path)| {
            if let Ok(dcm_obj) = open_file(working_path.path()) {
                let map_clone = Arc::clone(&anon_id_tracker);
                anon_each_dcm_file(&dcm_obj, &destination_path, map_clone).unwrap_or_else(|_| {
                    let mut map = failed_case.lock().unwrap();
                    *map += 1;
                    error!("Can't anon {:#?}", &working_path.file_name());
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
        "Anon".to_string(),
    )
    .unwrap();

    info!("DICOM Anon complete!");
    Ok(())
}

fn anon_each_dcm_file(
    dcm_obj: &FileDicomObject<InMemDicomObject>,
    destination_path: &PathBuf,
    map_clone: Arc<Mutex<HashMap<std::string::String, std::string::String>>>,
) -> Result<()> {
    let patient_id = dcm_obj.element_by_name("PatientID")?.to_str()?.to_string();
    let mut map = map_clone.lock().unwrap();
    match map.get(&patient_id) {
        Some(_) => (),
        None => {
            map.insert(patient_id.clone(), Uuid::new_v4().to_string());
            debug!("New AnonID for: {}", patient_id);
        }
    }
    let patient_anon_id = map.get(&patient_id).unwrap().to_string();
    let mut new_dicom_object = modify_tags_with_id(dcm_obj.clone(), patient_anon_id)?;
    new_dicom_object = dicom_anon_date_time(new_dicom_object)?;

    let dicom_tags_values: HashMap<String, String> = get_sanitized_tag_values(&new_dicom_object)?;
    let file_name = generate_dicom_file_name(&dicom_tags_values, "ANON".to_string())?;
    let dir_path = generate_dicom_file_path(dicom_tags_values, &destination_path)?;
    create_target_dir(&dir_path)?;
    let mut full_path = format!("{}/{}", dir_path, file_name);
    full_path = check_if_dup_exists(full_path);
    debug!("Saving file: {} to: {}", file_name, dir_path);
    new_dicom_object.write_to_file(full_path)?;
    Ok(())
}

fn dicom_anon_date_time(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    let dicom_date_tags = [
        (tags::STUDY_DATE, VR::DA),
        (tags::SERIES_DATE, VR::DA),
        (tags::ACQUISITION_DATE, VR::DA),
        (tags::PATIENT_BIRTH_DATE, VR::DA),
        (tags::SCHEDULED_PROCEDURE_STEP_START_DATE, VR::DA),
        (tags::SCHEDULED_PROCEDURE_STEP_END_DATE, VR::DA),
        (tags::PERFORMED_PROCEDURE_STEP_START_DATE, VR::DA),
        (tags::PERFORMED_PROCEDURE_STEP_END_DATE, VR::DA),
        (tags::CONTENT_DATE, VR::DA),
    ];
    let dicom_time_tags = [
        (tags::STUDY_TIME, VR::TM),
        (tags::SERIES_TIME, VR::TM),
        (tags::ACQUISITION_TIME, VR::TM),
        (tags::SCHEDULED_PROCEDURE_STEP_START_TIME, VR::TM),
        (tags::SCHEDULED_PROCEDURE_STEP_START_TIME, VR::TM),
        (tags::PERFORMED_PROCEDURE_STEP_START_TIME, VR::TM),
        (tags::PERFORMED_PROCEDURE_STEP_END_TIME, VR::TM),
        (tags::CONTENT_TIME, VR::TM),
    ];

    let dicom_date_data = "19000101";
    let dicom_time_data = "000000";

    for each_v in dicom_date_tags {
        dcm_obj.put(DataElement::new(each_v.0, each_v.1, dicom_date_data));
    }
    for each_v in dicom_time_tags {
        dcm_obj.put(DataElement::new(each_v.0, each_v.1, dicom_time_data));
    }
    dcm_obj.put(DataElement::new(tags::PATIENT_AGE, VR::AS, "099Y"));
    dcm_obj.put(DataElement::new(tags::PATIENT_SEX, VR::CS, "O"));

    Ok(dcm_obj)
}
