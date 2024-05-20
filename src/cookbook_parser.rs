use anyhow::Result;
use dicom::core::dictionary::DataDictionaryEntryRef;
use dicom::core::{DataDictionary, VR};
use dicom::object::StandardDataDictionary;
use home::{self, home_dir};
use serde::Deserialize;
use std::io::Write;
use std::str::FromStr;
use std::{
    collections::HashMap,
    fs::{self, canonicalize, create_dir_all, File},
    process::exit,
};
use toml;
use tracing::{error, info, warn};

#[derive(Debug, Deserialize)]
struct CookBook {
    matchid: Option<MatchIDTag>,
    mask: Option<MaskTags>,
    delete: Option<DelTags>,
    add: Option<AddTags>,
}

#[derive(Debug, Deserialize)]
struct MatchIDTag {
    tag: String,
}

#[derive(Debug, Deserialize, Clone)]
struct MaskTags {
    tags: Vec<String>,
    vrs: Vec<String>,
}

impl MaskTags {
    fn default() -> Self {
        MaskTags {
            tags: Vec::new(),
            vrs: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct DelTags {
    tags: Vec<String>,
    private_tags: bool,
}

impl DelTags {
    fn default() -> Self {
        DelTags {
            tags: Vec::new(),
            private_tags: false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AddTags {
    tags: std::collections::HashMap<String, String>,
}

impl AddTags {
    fn default() -> Self {
        AddTags {
            tags: HashMap::new(),
        }
    }
}

fn create_default_cookbook(cookbook_file_path: &String) -> Result<String> {
    warn!("Cookbook file not found, Creating a default cookbook file");
    let default_cookbook_raw = r#"#The chain of application is mask > add > delete
# The tags are case sensitive. They should match the DICOM standard dictionary specification
# Mask and delete only work with the tags already present in the dicom file

# Tags are case sensitive. Need to follow the DICOM Stadndard dictionary
# Unique ID to match on, PatientID and PatientName tags suggested. It will default to PatientID
[matchid]
tag = "PatientID"

# List of tags and VRs that will be masked by the DeID
# Only PN VR recommended
[mask]
tags = ["PatientID", "PatientName", "InstitutionName", "InstitutionAddress", "StudyID", "AccessionNumber"]
vrs = ["PN"]

# List of tags that will be deleted
[delete]
tags = []
private_tags = false

# Dictionary of tags to be added along with their values
# Date should follow YYYYMMDD format >> 19900101
# Time should follow HHMMSS format >> 090000
# DateTime should floolw YYYYMMDDTHHMMSS format >> 19900101T090000
[add]
tags.PatientIdentityRemoved = "Yes"
tags.DeidentificationMethod = "DCMRig"
tags.ClinicalTrialSponsorName = "TrialName"
# Timepoint is a special field which follows the following pattern
# PatientID_StudyDateTStudyTime_Modality
tags.ClinicalTrialTimePointID = "PatientID_StudyDateTStudyTime_Modality"
"#;
    let mut file_to_save =
        File::create(cookbook_file_path).expect("Failed to create cookbook path");
    write!(file_to_save, "{}", default_cookbook_raw.to_string())?;
    info!("Default cookbook created: {}", cookbook_file_path);
    Ok(default_cookbook_raw.to_string())
}

fn check_for_cookbook() -> Result<String> {
    let home_path = home_dir().expect("Home path not found");
    let cookbook_home = format!("{}/.dcmrig", home_path.display());
    let cookbook_file_path = format!("{}/cookbook.toml", cookbook_home);

    match canonicalize(&cookbook_file_path) {
        Ok(_) => (),
        Err(_) => create_dir_all(&cookbook_home).unwrap_or_else(|_| {
            error!("Can't create cookbook root dir: {}", &cookbook_home);
            exit(1)
        }),
    }

    let file_content: String = match fs::read_to_string(&cookbook_file_path) {
        Ok(v) => {
            info!(
                "Reading from the cookbook toml file at {}",
                cookbook_file_path
            );
            v
        }
        Err(_) => create_default_cookbook(&cookbook_file_path)?,
    };

    Ok(file_content)
}

fn check_valid_tag_vec(tag_vec: Vec<String>) -> Vec<DataDictionaryEntryRef<'static>> {
    let mut std_tag_list = Vec::new();
    for each in tag_vec {
        match DataDictionary::by_name(&StandardDataDictionary, &each) {
            Some(tag) => std_tag_list.push(tag.to_owned()),
            None => warn!("Tag {} is not valid", each),
        }
    }
    // tags_vec
    std_tag_list
}

fn check_valid_vr_vec(vrs_vec: Vec<String>) -> Vec<VR> {
    let mut std_vr_list = Vec::new();
    for each in vrs_vec {
        match VR::from_str(&each) {
            Ok(vr) => std_vr_list.push(vr.to_owned()),
            Err(_) => warn!("VR {} is not valid", each),
        }
    }
    // tags_vec
    std_vr_list
}

fn check_valid_tag_hashmap(tag_hash: HashMap<String, String>) -> HashMap<String, String> {
    let mut tags_hash_m = tag_hash.clone();
    for each in tag_hash {
        match DataDictionary::by_name(&StandardDataDictionary, &each.0) {
            Some(_) => (),
            None => {
                tags_hash_m.remove(&each.0);
                warn!("Tag {} is not valid", each.0)
            }
        }
    }
    tags_hash_m
}

fn check_tag_list(action: &str, tag_list: Vec<String>) -> Vec<DataDictionaryEntryRef<'static>> {
    match tag_list.is_empty() {
        true => {
            warn!("The {} cookbook is empty or corrupted", action);
            return vec![];
        }
        false => {
            info!("Checking Mask list");
            let tag_list: Vec<DataDictionaryEntryRef<'_>> = check_valid_tag_vec(tag_list);
            tag_list
                .iter()
                .enumerate()
                .for_each(|(_i, v)| info!("Tags to {} {}", action, v.alias));
            return tag_list;
        }
    }
}

fn check_vr_list(vr_list: Vec<String>) -> Vec<VR> {
    match vr_list.is_empty() {
        true => {
            warn!("The Mask VR cookbook is empty or corrupted");
            return vec![];
        }
        false => {
            info!("Checking Mask list");
            let vr_list = check_valid_vr_vec(vr_list);
            // info!("Tags to mask {:?}", mask_list);
            vr_list
                .iter()
                .enumerate()
                .for_each(|(_i, v)| info!("VR to mask {}", v));
            return vr_list;
        }
    }
}

pub fn parse_toml_cookbook() -> Result<(
    DataDictionaryEntryRef<'static>,
    Vec<DataDictionaryEntryRef<'static>>,
    Vec<VR>,
    HashMap<String, String>,
    Vec<DataDictionaryEntryRef<'static>>,
    bool,
)> {
    let file_content = check_for_cookbook()?;
    let toml_des: CookBook =
        toml::from_str(&file_content).expect("Failed to deserialize Cargo.toml");

    // Setting up variables
    let matchid = toml_des.matchid.unwrap_or_else(|| MatchIDTag {
        tag: "PatientID".to_string(),
    });
    let mask_list = toml_des
        .mask
        .clone()
        .unwrap_or_else(|| MaskTags::default())
        .tags;
    let mask_vrs_list = toml_des
        .mask
        .clone()
        .unwrap_or_else(|| MaskTags::default())
        .vrs;

    let add_list = toml_des.add.unwrap_or_else(|| AddTags::default()).tags;

    let delete_list = toml_des
        .delete
        .clone()
        .unwrap_or_else(|| DelTags::default())
        .tags;
    let private_tags_del = toml_des
        .delete
        .unwrap_or_else(|| DelTags::default())
        .private_tags;

    // Validating the lists
    info!("Checking MatchID tag");
    let matchid = match matchid.tag.as_str() {
        "PatientID" => DataDictionary::by_name(&StandardDataDictionary, "PatientID")
            .expect("Failed to extract tag"),
        "PatientName" => DataDictionary::by_name(&StandardDataDictionary, "PatientName")
            .expect("Failed to extract tag"),
        &_ => {
            warn!("MatchID empty or corrupted. PatientID will be used as default");
            DataDictionary::by_name(&StandardDataDictionary, "PatientID")
                .expect("Failed to extract tag")
        }
    };
    info!("MatchID > {}", matchid.alias);

    let mask_tag_list = check_tag_list("mask", mask_list);
    let delete_tag_list = check_tag_list("delete", delete_list);

    let mask_vr_list = check_vr_list(mask_vrs_list);

    let add_list = match add_list.is_empty() {
        true => {
            warn!("The Add cookbook is empty or corrupted");
            AddTags::default().tags
        }
        false => {
            info!("Checking Add list");
            let add_list = check_valid_tag_hashmap(add_list);
            // info!("Tags to add {:?}", add_list);
            add_list
                .iter()
                .enumerate()
                .for_each(|(_i, v)| info!("Tags to add {} > {}", v.0, v.1));
            add_list
        }
    };

    Ok((
        matchid.to_owned(),
        mask_tag_list,
        mask_vr_list,
        add_list,
        delete_tag_list,
        private_tags_del,
    ))
}
