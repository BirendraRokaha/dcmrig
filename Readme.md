# DICOM Toolkit - DICOM corelab tools in 🦀rust🦀

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

## TODO
### CORE
- [x] Read all DICOM files from a given source
- [x] Create and save a new dicom file in the desired location
- [ ] Handle private tags

### Nice to have
- [x] Pretty output
- [x] Multithreaded
- [x] Robust args parser
- [x] TOML config file for defining mask, delete and add dicom tag values for DeID

---
1. Deidentification
- [x] Read from a mapping table to create a mapping dictionary
- [x] Match data with mapping table and change dicom tags
- [x] Handle missing tags gracefully > partially complete
- [x] Derive add, delete, and mask tags from a config Toml file

Mapping table example. Only one pair per line is valid.
```
# MASK_ID,PatientID # 
DeID_001,U1423571
DeID_002,U3245327
DeID_003,U6124732
```

A sample cookbook toml file is created at the users home dir ~/.dcmrig/cookbook.toml during the first execution.
```toml
# The chain of application is mask > add > delete
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
```
Example: `dcmrig deid -m ./path_to_table ./source_path ./dest_path`

2. Anonymisation
- [x] Track unique PatientID and assign a UUID per unique ID\
Example: `dcmrig anon ./source_path ./dest_path`

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