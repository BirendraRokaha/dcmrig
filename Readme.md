# DICOM Toolkit - DICOM corelab tools.

### USAGE
Usage: `dcmrig [OPTIONS] <SUBCOMMAND> <ARGS>`\
Example: `dcmrig deid -m ./path_to_table ./source_path ./dest_path`

**sub-commands:**
- `sort`    [TESTING] Sort the given source with any combination of PatientID, PatientName or Modality
- `anon`    [TESTING] Anonymize the given source each PatientID will be given a unique AnonID
- `deid`    [TESTING] Deidentify the given source based on a mapping table
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
### Nice to have
- [x] Pretty output
- [x] Multithreaded
- [x] Robust args parser

---
1. Deidentification
- [x] Read from a mapping table to create a mapping dictionary
- [x] Match data with mapping table and change dicom tags
- [x] Handle missing tags gracefully > partially complete

2. Anonymisation
- [x] Track unique PatientID and assign a UUID per unique ID

3. Sort
- [x] Create Paths from the given list
- [x] Save files to a generated destination path with the desired filename
- [x] Sanitize dicom tags for missing data
- [x] Sort based on PATIENTID, PATENTNAME, MODALITY

4. Report
- [ ] Sorted Data needed
- [ ] Generate a CSV report
---