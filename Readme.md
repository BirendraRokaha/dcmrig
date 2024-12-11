# DCMrig - DICOM DeIdentification tools in 🦀rust🦀

<img src="https://github.com/BirendraRokaha/dcmrig/blob/main/misc/DEID_TEST_RUN.gif">

### USAGE
Usage: `dcmrig [OPTIONS] <SUBCOMMAND> <ARGS>`\
Example: `dcmrig deid -m ./path_to_table ./source_path ./dest_path`

**sub-commands:**
- `sort`    Sort the given source with any combination of PatientID, PatientName or Modality
- `anon`    Anonymize the given source each PatientID will be given a unique AnonID
- `deid`    Deidentify the given source based on a mapping table
- `report`  [NON FUNCTIONAL] Generate a report for a sorted dataset
- `help`    Print this message or the help of the given subcommand(s)

**Options:**
- -v, --verbose  Verbose output
- -h, --help     Print help
- -V, --version  Print version

## Install
Needs cargo
```
git clone https://github.com/BirendraRokaha/dcmrig.git
cd dcmrig
cargo build --release
```
The binary will be generated at `target/release/dcmrig`

---
## TODO
### CORE
- [x] Read all DICOM files from a given source
- [x] Parse the DICOM file and get the metadata by TagID or Tag name
- [x] Create and save a new dicom file in the desired location
- [x] [Partially complete] Handle private tags

### Nice to have
- [x] Pretty output
- [x] Multithreaded
- [x] Robust args parser
- [x] TOML config file for defining mask, delete and add dicom tag values for DeID

### Roadmap
- [x] [CookBook] Choose the tag as the identifier for DeID or Anon > Partially complete. Need to add for Anon
- [ ] [CookBook] In the Add section, Select other tags or combination of other tags as the value
- [x] [CookBook] Selection to keep or remove private tags
- [ ] [CookBook] Add VR as the field to remove/mask

---
1. Deidentification
- [x] Read from a mapping table to create a mapping dictionary
- [x] Derive add, delete, and mask tags from a config Toml file
- [x] Match data with mapping table and change dicom tags
- [x] Handle missing tags gracefully > partially complete

Mapping table example. Only one pair per line is valid.
```
# MASK_ID,PatientID # 
DeID_001,U1423571
DeID_002,U3245327
DeID_003,U6124732
```

A sample cookbook toml file is created at the users home dir ~/.dcmrig/cookbook.toml during the first execution.
```toml
# Tags are case sensitive. Need to follow the DICOM Stadndard dictionary
# Unique ID to match on, PatientID and PatientName tags suggested. It will default to PatientID
[matchid]
tag = "PatientID"

# List of tags and VRs that will be masked by the DeID
# Only PN VR recommended to MASK
[mask]
tags = [ "PatientID","PatientName","InstitutionName","InstitutionAddress","StudyID","AccessionNumber" ]
vrs = ["PN"]

# List of tags that will be deleted
[delete]
tags = []
private_tags = false

# Dictionary of tags to be added along with their values
# Date should follow YYYYMMDD format >> 19900101
# Time should follow HHMMSS format >> 090000
# DateTime should floolw YYYYMMDDTHHMMSS format 19900101T090000
[add]
tags.PatientIdentityRemoved = "Yes"
tags.DeidentificationMethod = "DCMRig"
```
Example: `dcmrig deid -m ./path_to_table ./source_path ./dest_path`

2. Anonymisation
- [x] Track unique PatientID and assign a anonID for every unique ID\
Example: `dcmrig anon -p [ANON_ID PREFIX optional] ./source_path ./dest_path`

3. Sort
- [x] Create Paths from the given list
- [x] Save files to a generated destination path with the desired filename
- [x] Sanitize dicom tags for missing data
- [x] Sort based on PATIENTID, PATENTNAME, MODALITY. Default is PatientID

Valid Sort order is any combination of INM. Case insensitive.\
Example: `dcmrig sort -s [INM] ./source_path ./dest_path`

4. Report
- [ ] Sorted Data needed
- [ ] Generate a CSV report
---