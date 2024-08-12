use nanoid::nanoid;
use std::{
    collections::HashMap,
    fmt::Write,
    fs::{self, canonicalize, copy, create_dir_all},
    path::PathBuf,
    process::exit,
};

use anyhow::Result;
use dicom::{
    core::{
        chrono::NaiveDate,
        dictionary::DataDictionaryEntryRef,
        header::Header,
        value::{DicomDate, DicomDateTime, DicomTime},
        DataDictionary, DataElement, PrimitiveValue, VR,
    },
    dicom_value,
    dictionary_std::tags::{self, ORIGINAL_ATTRIBUTES_SEQUENCE},
    object::{FileDicomObject, InMemDicomObject, StandardDataDictionary, Tag},
};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::{
    current_num_threads,
    iter::{ParallelBridge, ParallelIterator},
};
use regex::Regex;
use tracing::{debug, error, info, warn};
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
static DICOM_TAGS_CHANGE: [(Tag, VR); 7] = [
    (tags::PATIENT_ID, VR::LO),
    (tags::PATIENT_NAME, VR::PN),
    (tags::INSTITUTION_NAME, VR::LO),
    (tags::INSTITUTION_ADDRESS, VR::ST),
    (tags::ACCESSION_NUMBER, VR::SH),
    (tags::STUDY_ID, VR::SH),
    (tags::PATIENT_COMMENTS, VR::LT),
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
██║  ██║██║     ██╔████╔██║ ██ ██████╔╝██║██║  ███╗
██║  ██║██║     ██║╚██╔╝██║    ██╔══██╗██║██║   ██║
██████╔╝╚██████╗██║ ╚═╝ ██║    ██║  ██║██║╚██████╔╝
"
    )
    .expect("Failed to write logo");
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
            "{spinner:.green} {percent}% [{elapsed_precise}] [{wide_bar:.cyan/blue}] ({pos}/{len}, ETA {eta})",
        )?,
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
        &each_file
            .file_name()
            .to_str()
            .expect("Failed to extract filename")
    ));
    copy(each_file.clone().into_path(), non_dicom_file_path)?;
    Ok(())
}

pub fn failed_case_copy(source_path: &PathBuf, dest_path: &PathBuf) -> Result<()> {
    let failed_cases_path = format!("{}/FAILED_CASES", dest_path.display());
    match canonicalize(failed_cases_path.clone()) {
        Ok(_) => (),
        Err(_) => create_dir_all(&failed_cases_path).unwrap_or_else(|_| {
            error!("Can't create dir: {}", failed_cases_path);
            exit(1)
        }),
    }
    let failed_cases_full_name = format!(
        "{}/{}",
        failed_cases_path,
        source_path
            .file_name()
            .expect("Failed to extract file name")
            .to_str()
            .expect("Failed to convert filename to str")
    );

    let final_failed_path = check_if_dup_exists(failed_cases_full_name);
    fs::copy(source_path, final_failed_path)?;
    Ok(())
}
// Replace all non_alphanumeric characters with an underscore '_'
pub fn replace_non_alphanumeric(input: &str) -> String {
    let re = Regex::new(r"[^a-zA-Z0-9]+").expect("Failed to set up Regex");
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
        match dcm_obj.element_by_name(each_tag) {
            Ok(tv) => {
                dicom_tags_values.insert(
                    each_tag.to_string(),
                    tv.to_str()?.to_string().replace(&['-', ':'][..], ""),
                );
            }
            Err(_) => {
                warn!("No value for {}", each_tag);
                let final_value = format!("NoValue_{}", each_tag);
                dicom_tags_values.insert(each_tag.to_string(), final_value);
            }
        }
    }
    dicom_tags_values.insert("ImagePlane".to_string(), determine_plane(&dcm_obj)?);
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
    let p_value = dicom_vr_corrected_value(VR::PN, &patient_deid)?;
    // Mask all PN values with the given ID
    dcm_obj = mask_all_vr(dcm_obj.clone(), VR::PN, p_value.clone())?;

    for each_v in DICOM_TAGS_CHANGE {
        let p_value = dicom_vr_corrected_value(each_v.1, &patient_deid)?;
        dcm_obj.put(DataElement::new(each_v.0, each_v.1, p_value.clone()));
    }
    // Add deidentified Info
    dcm_obj.put(DataElement::new(
        tags::DEIDENTIFICATION_METHOD,
        VR::LO,
        p_value.clone(),
    ));
    dcm_obj.put(DataElement::new(
        tags::PATIENT_IDENTITY_REMOVED,
        VR::CS,
        p_value,
    ));
    Ok(dcm_obj)
}

pub fn tags_to_mask(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    patient_deid: String,
    mask_config_list: Vec<DataDictionaryEntryRef<'static>>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_tag in mask_config_list {
        let each_tag_tag = each_tag.tag.inner();
        let each_tag_vr: VR = each_tag.vr.relaxed();
        let value = dicom_vr_corrected_value(each_tag_vr, &patient_deid)?;
        match dcm_obj.put(DataElement::new(each_tag_tag, each_tag_vr, value.clone())) {
            Some(_) => (),
            None => error!("Mask Tag : Failed to mask tag {:?}", each_tag_tag),
        }

        fn mask_sq_vrs(
            data_element: &DataElement<InMemDicomObject>,
            // dcm_obj: &mut FileDicomObject<InMemDicomObject>,
            value: PrimitiveValue,
            tag_to_check: Tag,
        ) {
            for each_sq_element in data_element.items().into_iter() {
                for each_element in each_sq_element.into_iter() {
                    for sq_inner_element in each_element.to_owned() {
                        if sq_inner_element.vr() == VR::SQ {
                            mask_sq_vrs(
                                &sq_inner_element,
                                // mut dcm_obj,
                                value.clone(),
                                tag_to_check,
                            );
                        }
                        if sq_inner_element.tag() == tag_to_check {
                            // sq_inner_element.update_value(|e| {
                            //     e.primitive_mut().unwrap().truncate(0);
                            // });
                            // TODO
                        }
                    }
                }
            }
        }

        for data_element in &dcm_obj {
            if data_element.vr() == VR::SQ {
                mask_sq_vrs(data_element, value.clone(), each_tag_tag);
            }
        }
    }

    Ok(dcm_obj)
}

pub fn tags_to_add(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    add_config_list: HashMap<String, String>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_element in add_config_list {
        let config_tag = each_element.0;
        let config_value = each_element.1;
        let (each_tag, each_vr) = extract_tag_vr_from_str(&config_tag)?;
        let value = dicom_vr_corrected_value(each_vr, &config_value)?;
        dcm_obj.put(DataElement::new(each_tag, each_vr, value));
    }
    Ok(dcm_obj)
}

pub fn tags_to_delete(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    delete_config_list: Vec<DataDictionaryEntryRef<'static>>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_tag in delete_config_list {
        match dcm_obj.remove_element(each_tag.tag.inner()) {
            true => (),
            false => debug!("Delete Tag: {:?} not valid/found", each_tag.tag.inner()),
        }
    }
    Ok(dcm_obj)
}

pub fn delete_private_tags(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    fn is_private(tag: Tag) -> bool {
        return tag.group() % 2 == 1;
    }

    let mut private_tags: Vec<Tag> = vec![];
    for each_element in dcm_obj.clone() {
        collect_tags(each_element, &mut private_tags);
    }

    fn collect_tags(data_element: DataElement<InMemDicomObject>, private_tags: &mut Vec<Tag>) {
        let tag = data_element.tag();
        if is_private(tag) {
            private_tags.push(tag);
        };

        if data_element.vr() == VR::SQ {
            for each_sq_element in data_element.items().into_iter() {
                for each_element in each_sq_element.into_iter() {
                    for each_tag in each_element {
                        collect_tags(each_tag.to_owned(), private_tags)
                    }
                }
            }
        }
    }

    for each in private_tags {
        dcm_obj.remove_element(each);
    }

    dcm_obj.remove_element(ORIGINAL_ATTRIBUTES_SEQUENCE);

    Ok(dcm_obj)
}

pub fn anon_dicom_uids(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
) -> Result<FileDicomObject<InMemDicomObject>> {
    let uid_tag_list = [
        "SOPInstanceUID".to_string(),
        "StudyInstanceUID".to_string(),
        "SeriesInstanceUID".to_string(),
        "FrameOfReferenceUID".to_string(),
    ];
    let anon_uid_prefix: Vec<_> = "1.2.999.999999.9999.9.9.9.9999".split(".").collect();

    for each_uid in uid_tag_list {
        let (each_tag, each_vr) = extract_tag_vr_from_str(&each_uid)?;
        let org_uid_val = dcm_obj.element(each_tag)?.to_str()?;
        let org_uid_vec: Vec<_> = org_uid_val.split(".").collect();
        let mut new_uid_parts = anon_uid_prefix.clone();
        new_uid_parts.extend_from_slice(&org_uid_vec[8..]);
        let new_uid_val = new_uid_parts.join(".");
        let value = dicom_vr_corrected_value(each_vr, &new_uid_val)?;
        dcm_obj.put(DataElement::new(each_tag, each_vr, value));
    }
    Ok(dcm_obj)
}

pub fn mask_all_vr(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    vr: VR,
    val: PrimitiveValue,
) -> Result<FileDicomObject<InMemDicomObject>> {
    for each_element in dcm_obj.clone() {
        if each_element.header().vr() == vr {
            dcm_obj.put(DataElement::new(
                each_element.tag(),
                each_element.vr(),
                val.clone(),
            ));
        }
    }
    Ok(dcm_obj)
}

pub fn mask_vr(
    mut dcm_obj: FileDicomObject<InMemDicomObject>,
    vr_list: Vec<VR>,
    val: String,
) -> Result<FileDicomObject<InMemDicomObject>> {
    let p_value = dicom_value!(Strs, [val]);
    for each_vr in vr_list {
        dcm_obj = mask_all_vr(dcm_obj.clone(), each_vr, p_value.clone())?;
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
        dicom_tags_values
            .get("PatientID")
            .expect("Failed to extract value")
            .trim(),
        dicom_tags_values
            .get("Modality")
            .expect("Failed to extract value"),
        dicom_tags_values
            .get("StudyDate")
            .expect("Failed to extract value"),
        dicom_tags_values
            .get("StudyTime")
            .expect("Failed to extract value")
            .split(".")
            .next()
            .expect("Failed to extract value"),
        dicom_tags_values
            .get("SeriesNumber")
            .expect("Failed to extract value"),
        dicom_tags_values
            .get("SeriesInstanceUID")
            .expect("Failed to extract value"),
        dicom_tags_values
            .get("InstanceNumber")
            .expect("Failed to extract value")
    );
    Ok(file_name)
}

// Generate the path for the dicom files
pub fn generate_dicom_file_path(
    dicom_tags_values: HashMap<String, String>,
    destination_path: &PathBuf,
) -> Result<String> {
    let temp_trimmed_study_uid = dicom_tags_values
        .get("StudyInstanceUID")
        .expect("Failed to extract value")
        .split(".")
        .last()
        .expect("Failed to extract value");

    let final_trimmed_uid = if temp_trimmed_study_uid.len() > 5 {
        temp_trimmed_study_uid[temp_trimmed_study_uid.len() - 5..].to_string()
    } else {
        temp_trimmed_study_uid.to_string()
    };
    let dir_path = format!(
        "{}/{}/{}T{}_{:0>5}/{:0>4}_{}_{}",
        destination_path.display(),
        dicom_tags_values
            .get("PatientID")
            .expect("Failed to extract value")
            .trim()
            .replace(" ", "_")
            .replace("^", "_"),
        dicom_tags_values
            .get("StudyDate")
            .expect("Failed to extract value")
            .trim(),
        dicom_tags_values
            .get("StudyTime")
            .expect("Failed to extract value")
            .trim(),
        final_trimmed_uid,
        dicom_tags_values
            .get("SeriesNumber")
            .expect("Failed to extract value"),
        replace_non_alphanumeric(
            dicom_tags_values
                .get("SeriesDescription")
                .expect("Failed to extract value")
                .trim()
        )
        .to_uppercase(),
        dicom_tags_values
            .get("ImagePlane")
            .expect("Failed to extract value")
            .trim()
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
    let total_processed = total_len - { total_proc_failed_files + total_non_dcm_files };
    info!("Total Files: {}", total_len);
    info!("Failed Cases: {}", total_proc_failed_files);
    info!("NON-DCM files: {}", total_non_dcm_files);
    info!("Total {}: {}", action, total_processed);
    Ok(())
}

pub fn extract_tag_vr_from_str(tag_name: &String) -> Result<(Tag, VR)> {
    match DataDictionary::by_name(&StandardDataDictionary, &tag_name) {
        Some(v) => return Ok((v.tag.inner(), v.vr.relaxed())),
        None => {
            warn!("Tag: {} is not valid!", tag_name);
            return Err(anyhow::Error::msg("Tag Not Valid, VR not found!!"));
        }
    };
}

// Generate ANON ID
pub fn gen_id() -> String {
    let alpha_numeric = &nanoid::alphabet::SAFE[2..];
    nanoid!(10, &alpha_numeric)
}

fn determine_plane(dcm_obj: &FileDicomObject<InMemDicomObject>) -> Result<String> {
    let orientation: Vec<f64> = match dcm_obj.element_by_name("ImageOrientationPatient") {
        Ok(value) => value.to_multi_float64()?,
        Err(_) => vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    };
    let row_x = orientation[0].round() as i8;
    let row_y = orientation[1].round() as i8;
    let col_y = orientation[4].round() as i8;
    let col_z = orientation[5].round() as i8;
    let plane = if row_x == 1 && col_z == -1 {
        "COR".to_string()
    } else if row_x == 1 && col_y == 1 {
        "AX".to_string()
    } else if row_y == 1 && col_z == -1 {
        "SAG".to_string()
    } else {
        "NA".to_string()
    };
    Ok(plane)
}

pub fn dicom_vr_corrected_value(vr: VR, value: &String) -> Result<PrimitiveValue> {
    let r_value = match vr {
        VR::AE | VR::AS | VR::PN | VR::SH | VR::CS | VR::LO | VR::UI | VR::UC => {
            dicom_value!(Strs, [value.clone()])
        }
        VR::ST | VR::LT | VR::UT | VR::UR => {
            dicom_value!(Str, value.clone())
        }
        VR::DA => {
            if value.len() != 8 {
                error!(
                    "Issue With Date value Does it follow this format YYYYMMDD: {}",
                    value
                );
                exit(1);
            }
            let d_date = DicomDate::try_from(&NaiveDate::parse_from_str(&value, "%Y%m%d")?)?;
            dicom_value!(Date, d_date)
        }
        VR::TM => {
            if value.len() != 6 {
                error!(
                    "Issue With Time value Does it follow this format HHMMSS: {}",
                    value
                );
                exit(1);
            }
            let hr: u8 = value[0..2].to_string().parse()?;
            let min: u8 = value[2..4].to_string().parse()?;
            let sec: u8 = value[4..6].to_string().parse()?;
            let d_time = DicomTime::from_hms(hr, min, sec)?;
            dicom_value!(Time, d_time)
        }
        VR::DT => {
            let split_value: Vec<&str> = value.split("T").collect();
            let t_date = split_value[0];
            let t_time = split_value[1];

            if t_date.len() != 8 {
                error!(
                    "Issue With Date value Does it follow this format YYYYMMDD: {}",
                    value
                );
                exit(1);
            }
            let d_date = DicomDate::try_from(&NaiveDate::parse_from_str(&t_date, "%Y%m%d")?)?;

            if t_time.len() != 6 {
                error!(
                    "Issue With Time value Does it follow this format HHMMSS: {}",
                    value
                );
                exit(1);
            }
            let hr: u8 = t_time[0..2].to_string().parse()?;
            let min: u8 = t_time[2..4].to_string().parse()?;
            let sec: u8 = t_time[4..6].to_string().parse()?;
            let d_time = DicomTime::from_hms(hr, min, sec)?;
            dicom_value!(DateTime, DicomDateTime::from_date_and_time(d_date, d_time)?)
        }
        _ => dicom_value!(Str, value.clone()),
    };
    Ok(r_value)
}
