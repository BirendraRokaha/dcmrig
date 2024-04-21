use std::{
    collections::HashMap,
    fmt::Write,
    fs::{canonicalize, copy, create_dir_all},
    path::PathBuf,
    process::exit,
};

use anyhow::Result;
use dicom::{
    core::{dictionary::DataDictionaryEntryRef, DataDictionary, DataElement, VR},
    dictionary_std::tags::{self},
    object::{FileDicomObject, InMemDicomObject, StandardDataDictionary, Tag},
};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::{
    current_num_threads,
    iter::{ParallelBridge, ParallelIterator},
};
use regex::Regex;
use tracing::{error, info, warn};
use walkdir::{DirEntry, WalkDir};

// Tags to get data for
static DICOM_TAGS_SANITIZED: [&str; 10] = [
    "PatientID",
    "PatientName",
    "Modality",
    "StudyDate",
    "StudyTime",
    "SeriesNumber",
    "SeriesInstanceUID",
    "StudyInstanceUID",
    "InstanceNumber",
    "SeriesDescription",
];

// Tags to change data for
static DICOM_TAGS_CHANGE: [(Tag, VR); 6] = [
    (tags::PATIENT_ID, VR::LO),
    (tags::PATIENT_NAME, VR::PN),
    (tags::INSTITUTION_NAME, VR::LO),
    (tags::INSTITUTION_ADDRESS, VR::ST),
    (tags::ACCESSION_NUMBER, VR::SH),
    (tags::STUDY_ID, VR::SH),
];

// Logo
pub fn print_logo() {
    let app_version = env!("CARGO_PKG_VERSION");
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

    println!("{} Ver: {}", art, app_version);
}

// Initial setup before starting the action
pub fn preprocessing_setup(
    source_path: &PathBuf,
    destination_path: &PathBuf,
) -> Result<(Vec<DirEntry>, u64, ProgressBar)> {
    check_given_path_exists(&source_path, &destination_path)?;
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
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] ({pos}/{len}, ETA {eta})",
        )
        .unwrap(),
    );
    info!("Current number of threads: {}", current_num_threads());
    Ok((all_files, total_len, pb))
}

fn check_given_path_exists(src_path: &PathBuf, dest_path: &PathBuf) -> Result<()> {
    // Source Path
    match canonicalize(src_path) {
        Ok(_) => (),
        Err(e) => {
            error!(
                "Given source Path doesnot exist: {}\n{}",
                src_path.display(),
                e
            );
            exit(1)
        }
    }
    // Destination Path
    match canonicalize(dest_path) {
        Ok(_) => (),
        Err(_) => create_dir_all(dest_path).unwrap_or_else(|_| {
            error!("Can't create dir: {}", dest_path.display());
            exit(1)
        }),
    }
    Ok(())
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
    dcm_obj: &FileDicomObject<InMemDicomObject>,
) -> Result<HashMap<String, String>> {
    let mut dicom_tags_values = HashMap::new();
    for each_tag in DICOM_TAGS_SANITIZED {
        let tag_value = dcm_obj.element_by_name(each_tag);
        // println!("{:#?}", tag_value);
        match tag_value {
            Ok(_) => {
                // let f_tag_value: &str = tag_value?.to_str()?.as_ref();
                dicom_tags_values.insert(each_tag.to_string(), tag_value?.to_str()?.to_string());
            }
            Err(_) => {
                warn!("No value for {}", each_tag);
                let final_value = format!("NoValue_{}", each_tag);
                dicom_tags_values.insert(each_tag.to_string(), final_value);
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
pub fn mask_tags_with_id(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    patient_deid: String,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_v in DICOM_TAGS_CHANGE {
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
        "DCMRig",
    ));
    dcm_obj.put(DataElement::new(
        tags::PATIENT_IDENTITY_REMOVED,
        VR::CS,
        "YES",
    ));
    Ok(dcm_obj)
}

pub fn tags_to_mask(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    patient_deid: String,
    mask_config_list: &Vec<DataDictionaryEntryRef<'static>>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_tag in mask_config_list {
        let each_tag_tag = each_tag.tag.inner();
        let each_tag_vr = each_tag.vr;
        match dcm_obj.put(DataElement::new(
            each_tag_tag,
            each_tag_vr,
            patient_deid.as_ref(),
        )) {
            Some(_) => (),
            None => warn!("Mask Tag : Failed to mask tag {:?}", each_tag_tag),
        }
    }
    Ok(dcm_obj)
}

pub fn tags_to_add(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    add_config_list: &HashMap<String, String>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_element in add_config_list {
        let config_tag = each_element.0;
        let config_value = each_element.1;
        let (each_tag, each_vr) = extract_tag_vr_from_str(&config_tag)?;
        dcm_obj.put(DataElement::new(each_tag, each_vr, config_value.as_ref()));
    }
    Ok(dcm_obj)
}

pub fn tags_to_delete(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    delete_config_list: &Vec<DataDictionaryEntryRef<'static>>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_tag in delete_config_list {
        match dcm_obj.remove_element(each_tag.tag.inner()) {
            true => (),
            // false => warn!("Delete Tag: {:?} not valid/found", each_tag.tag.inner()),
            false => (),
        }
    }
    Ok(dcm_obj)
}

pub fn delete_private_tags(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    // Mask all SQ values with the given ID
    for each_element in dcm_obj.clone() {
        if each_element.header().vr() == VR::SQ {
            dcm_obj.remove_element(each_element.header().tag);
        }
    }
    Ok(dcm_obj)
}

// Generate the Dicom filename based on the dicom tags
pub fn generate_dicom_file_name(
    dicom_tags_values: &HashMap<String, String>,
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
    create_target_dir(&dir_path)?;
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

pub fn extract_tag_vr_from_str(tag_name: &String) -> Result<(Tag, VR)> {
    match DataDictionary::by_name(&StandardDataDictionary, &tag_name) {
        Some(v) => return Ok((v.tag.inner(), v.vr)),
        None => {
            warn!("Tag: {} is not valid!", tag_name);
            return Err(anyhow::Error::msg("Tag Not Valid, VR not found!!"));
        }
    };
}

//
// #[derive(Debug, Clone)]
// pub struct DcmToStore {
//     pub dcm_obj: FileDicomObject<InMemDicomObject>,
//     pub dest_path: String,
// }

// impl DcmToStore {
//     pub fn new(dcm_obj: FileDicomObject<InMemDicomObject>, dest_path: String) -> Self {
//         DcmToStore { dcm_obj, dest_path }
//     }

//     pub fn push_for_store(&self) {
//         self.dcm_obj
//             .write_to_file(&self.dest_path)
//             .unwrap_or_else(|_| error!("Failed to store dicom file: {}", &self.dest_path))
//     }
// }
