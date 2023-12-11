use std::{
    collections::HashMap,
    fmt::Write,
    fs::{canonicalize, copy, create_dir_all},
    path::PathBuf,
    process::exit,
};

use anyhow::Result;
use dicom::{
    core::{DataElement, VR},
    dictionary_std::tags,
    object::{FileDicomObject, InMemDicomObject},
};
use regex::Regex;
use tracing::{error, info, warn};
use walkdir::DirEntry;

// Logo
pub fn print_logo() {
    let mut art = String::new();

    write!(
        art,
        "
██████╗  ██████╗███╗   ███╗    ██████╗ ██╗ ██████╗ 
██╔══██╗██╔════╝████╗ ████║    ██╔══██╗██║██╔════╝ 
██║  ██║██║     ██╔████╔██║    ██████╔╝██║██║  ███╗
██║  ██║██║     ██║╚██╔╝██║    ██╔══██╗██║██║   ██║
██████╔╝╚██████╗██║ ╚═╝ ██║    ██║  ██║██║╚██████╔╝
"
    )
    .unwrap();

    println!("{}", art);
}

// Check if the given source path exists, exits in failure
pub fn check_source_path_exists(dir_path: &PathBuf) {
    match canonicalize(dir_path) {
        Ok(_) => (),
        Err(e) => {
            error!(
                "Given source Path doesnot exist: {}\n{}",
                dir_path.display(),
                e
            );
            exit(1)
        }
    }
}

// Check if the given destination path exists, create a new one recursively if it does not exist
pub fn check_destination_path_exists(dir_path: &PathBuf) {
    match canonicalize(dir_path) {
        Ok(_) => (),
        Err(_) => create_dir_all(dir_path).unwrap_or_else(|_| {
            error!("Can't create dir: {}", dir_path.display());
            exit(1)
        }),
    }
}

// For all non DICOM files, Copy them to a NON_DICOM directory in the destination path
pub fn copy_non_dicom_files(each_file: &DirEntry, destination_path: &PathBuf) -> Result<()> {
    let non_dicom_path: PathBuf =
        PathBuf::from(format!("{}/NON_DICOM", &destination_path.to_string_lossy()));
    if !non_dicom_path.exists() {
        create_dir_all(&non_dicom_path)?;
    }
    let non_dicom_file_path = PathBuf::from(format!(
        "{}/NON_DICOM/{}",
        &destination_path.to_string_lossy(),
        &each_file.file_name().to_str().unwrap()
    ));
    copy(each_file.clone().into_path(), non_dicom_file_path)?;
    Ok(())
}

// Replace all non_alphanumeric characters with an underscore '_'
pub fn replace_non_alphanumeric(input: &str) -> String {
    let re = Regex::new(r"[^a-zA-Z0-9]+").unwrap();
    let modified_chars: String = input
        .chars()
        .map(|c| if re.is_match(&c.to_string()) { '_' } else { c })
        .collect();
    re.replace_all(&modified_chars, "_").to_string()
}

// For a given list of tags. Get the sanitized values.
// Removes all unnecessary characters and adds NoValue_ if value is not found for the tag
pub fn get_sanitized_tag_values(
    dcm_obj: FileDicomObject<InMemDicomObject>,
) -> Result<HashMap<String, String>> {
    let dicom_tags = vec![
        "PatientID".to_string(),
        "PatientName".to_string(),
        "Modality".to_string(),
        "StudyDate".to_string(),
        "StudyTime".to_string(),
        "SeriesNumber".to_string(),
        "SeriesInstanceUID".to_string(),
        "StudyInstanceUID".to_string(),
        "InstanceNumber".to_string(),
        "SeriesDescription".to_string(),
    ];
    let mut dicom_tags_values = HashMap::new();
    for each_tag in dicom_tags {
        let tag_value = dcm_obj.element_by_name(each_tag.as_str());
        match tag_value {
            Ok(_) => {
                dicom_tags_values.insert(each_tag, tag_value?.to_str()?.to_string());
            }
            Err(_) => {
                warn!("No value for {}", each_tag);
                let final_value = format!("NoValue_{}", each_tag);
                dicom_tags_values.insert(each_tag, final_value);
            }
        }
    }
    Ok(dicom_tags_values)
}

// Check if the target directory exists and create a new one recursively if it does not exist
pub fn create_target_dir(dir_path: &String) -> Result<()> {
    if !PathBuf::from(dir_path).exists() {
        create_dir_all(PathBuf::from(dir_path))?
    }
    Ok(())
}

// Check if a file already exist and add ~ to end of the file if it does recursively.
pub fn check_if_dup_exists(full_path: String) -> String {
    let new_path = full_path;
    if PathBuf::from(&new_path).exists() {
        let change_path = format!("{}~", new_path.clone());
        check_if_dup_exists(change_path)
    } else {
        return new_path;
    }
}

// Change certain tags to the given ID and add deidentified tags.
// Returns a cloned dicom object with modified values
pub fn modify_tags_with_id(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    patient_deid: String,
) -> Result<FileDicomObject<InMemDicomObject>> {
    let dicom_tags = vec![
        (tags::PATIENT_ID, VR::LO),
        (tags::PATIENT_NAME, VR::PN),
        (tags::INSTITUTION_NAME, VR::LO),
        (tags::INSTITUTION_ADDRESS, VR::ST),
        (tags::ACCESSION_NUMBER, VR::SH),
        (tags::STUDY_ID, VR::SH),
    ];
    for each_v in dicom_tags {
        dcm_obj.put(DataElement::new(each_v.0, each_v.1, patient_deid.as_ref()));
    }
    // Mask all PN values with the given ID
    for each_element in dcm_obj.clone() {
        if each_element.header().vr() == VR::PN {
            dcm_obj.put(DataElement::new(
                each_element.header().tag,
                VR::PN,
                patient_deid.as_ref(),
            ));
        }
    }
    // Add deidentified Info
    dcm_obj.put(DataElement::new(
        tags::DEIDENTIFICATION_METHOD,
        VR::LO,
        "DICOM_TOOLS_RS",
    ));
    dcm_obj.put(DataElement::new(
        tags::PATIENT_IDENTITY_REMOVED,
        VR::CS,
        "YES",
    ));
    Ok(dcm_obj)
}

// Generate the Dicom filename based on the dicom tags
pub fn generate_dicom_file_name(
    dicom_tags_values: HashMap<String, String>,
    prefix: String,
) -> Result<String> {
    let file_name = format!(
        "{}_{}_{}_{}T{}_{}_{}_{:0>5}.dcm",
        prefix,
        dicom_tags_values.get("PatientID").unwrap().trim(),
        dicom_tags_values.get("Modality").unwrap(),
        dicom_tags_values.get("StudyDate").unwrap(),
        dicom_tags_values
            .get("StudyTime")
            .unwrap()
            .split(".")
            .next()
            .unwrap(),
        dicom_tags_values.get("SeriesNumber").unwrap(),
        dicom_tags_values.get("SeriesInstanceUID").unwrap(),
        dicom_tags_values.get("InstanceNumber").unwrap()
    );
    Ok(file_name)
}

// Generate the path for the dicom files
pub fn generate_dicom_file_path(
    dicom_tags_values: HashMap<String, String>,
    destination_path: &PathBuf,
) -> Result<String> {
    let dir_path = format!(
        "{}/{}/{}T{}_{}/{:0>4}_{}",
        destination_path.display(),
        dicom_tags_values.get("PatientID").unwrap().trim(),
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
    Ok(dir_path)
}

pub fn print_status(
    total_len: u64,
    total_proc_failed_files: u64,
    total_non_dcm_files: u64,
    action: String,
) -> Result<()> {
    // let total_failed: u64 = *failed_case.lock().unwrap();
    // let total_non_dcm: u64 = *non_dcm_cases.lock().unwrap();
    let total_processed = total_len - { total_proc_failed_files + total_non_dcm_files };
    info!("Total Files: {}", total_len);
    info!("Failed Cases: {}", total_proc_failed_files);
    info!("NON-DCM files: {}", total_non_dcm_files);
    info!("Total {}: {}", action, total_processed);
    Ok(())
}
