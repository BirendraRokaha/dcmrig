use anyhow::Result;
use dicom::core::DataDictionary;
use dicom::object::StandardDataDictionary;
use home::{self, home_dir};
use serde::Deserialize;
use std::io::Write;
use std::{
    collections::HashMap,
    fs::{self, canonicalize, create_dir_all, File},
    process::exit,
};
use toml;
use tracing::{error, info, warn};

#[derive(Debug, Deserialize)]
struct CookBook {
    mask: Option<MaskDelTags>,
    delete: Option<MaskDelTags>,
    add: Option<AddTags>,
}

#[derive(Debug, Deserialize)]
struct MaskDelTags {
    tags: Vec<String>,
}

impl MaskDelTags {
    fn default() -> Self {
        // let default_v = vec!["PatientID".to_string()];
        MaskDelTags { tags: Vec::new() }
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

fn create_default_config(config_file_path: &String) -> Result<String> {
    warn!("Config file not found, Creating a default config file");
    let default_config_raw = r#"# The chain of application is mask > add > delete
# The tags are case sensitive. They should match the DICOM standard dictionary specification
# Mask and delete only work with the tags already present in the dicom file

# List of tags that will be masked by the DeID
[mask]
tags = ["PatientID", "PatientName"]

# List of tags that will be deleted
[delete]
tags = []

# Dictionary of tags to be added along with their values
[add]
tags.PatientIdentityRemoved = "Yes"
tags.DeidentificationMethod = "DCMRig"
"#;
    let mut file_to_save = File::create(config_file_path).unwrap();
    write!(file_to_save, "{}", default_config_raw.to_string())?;
    Ok(default_config_raw.to_string())
}

fn check_for_config() -> Result<String> {
    let home_path = home_dir().unwrap();
    let config_home = format!("{}/.dcmrig", home_path.display());
    let config_file_path = format!("{}/config.toml", config_home);

    match canonicalize(&config_file_path) {
        Ok(_) => (),
        Err(_) => create_dir_all(&config_home).unwrap_or_else(|_| {
            error!("Can't create dir: {}", &config_home);
            exit(1)
        }),
    }

    let file_content: String = match fs::read_to_string(&config_file_path) {
        Ok(v) => {
            info!("Reading from the config toml file at {}", config_file_path);
            v
        }
        Err(_) => create_default_config(&config_file_path)?,
    };

    Ok(file_content)
}

fn check_valid_tag_vec(tag_vec: Vec<String>) -> Vec<String> {
    let mut tags_vec = tag_vec.clone();
    for each in tag_vec.iter().enumerate() {
        match DataDictionary::by_name(&StandardDataDictionary, &each.1) {
            Some(_) => (),
            None => {
                tags_vec.remove(each.0);
                warn!("Tag {} is not valid", each.1)
            }
        }
    }
    tags_vec
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

pub fn parse_toml_config() -> Result<(Vec<String>, HashMap<String, String>, Vec<String>)> {
    let file_content = check_for_config()?;
    let toml_des: CookBook =
        toml::from_str(&file_content).expect("Failed to deserialize Cargo.toml");
    let mask_list = toml_des.mask.unwrap_or_else(|| MaskDelTags::default()).tags;

    let add_list = toml_des.add.unwrap_or_else(|| AddTags::default()).tags;

    let delete_list = toml_des
        .delete
        .unwrap_or_else(|| MaskDelTags::default())
        .tags;

    let mask_list = match mask_list.is_empty() {
        true => {
            warn!("The Mask config is empty or corrupted");
            MaskDelTags::default().tags
        }
        false => {
            let mask_list = check_valid_tag_vec(mask_list);
            info!("Tags to mask {:?}", mask_list);
            mask_list
        }
    };

    let delete_list = match delete_list.is_empty() {
        true => {
            warn!("The Delete config is empty or corrupted");
            MaskDelTags::default().tags
        }
        false => {
            let delete_list = check_valid_tag_vec(delete_list);
            info!("Tags to delete {:?}", delete_list);
            delete_list
        }
    };

    let add_list = match add_list.is_empty() {
        true => {
            warn!("The Add config is empty or corrupted");
            AddTags::default().tags
        }
        false => {
            let add_list = check_valid_tag_hashmap(add_list);
            info!("Tags to add {:?}", add_list);
            add_list
        }
    };

    Ok((mask_list, add_list, delete_list))
}
