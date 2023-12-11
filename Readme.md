# DICOM Toolkit - DICOM corelab tools.

### USAGE
Usage: `dcmrig [OPTIONS] <COMMAND>`\
Example: `dcmrig deid -m ./path_to_table ./source_path ./dest_path`

**Commands:**
- `sort`    [TESTING PHASE] Sort the given source with any combination of PatientID, PatientName or Modality
- `anon`    [TESTING PHASE] Anonymize the given source each PatientID will be given a unique AnonID
- `deid`    [TESTING PHASE] Deidentify the given source based on a mapping table
- `report`  [UNDER CONSTRUCTION] [NON FUNCTIONAL] Generate a report for a sorted dataset
- `help`    Print this message or the help of the given subcommand(s)

**Options:**
- -v, --verbose  Verbose output
- -h, --help     Print help
- -V, --version  Print version

## TOOLS
1. Deidentification
2. Anonymization
3. Dicom Sort
4. Report

## TODO
### CORE
- [x] Read all DICOM files from a given source
- [x] Create and save new dicom file in the desired location
### Nice to have
- [x] Pretty output
- [x] Multithreaded
- [x] Robust args parser

---
1. Deidentification
- [x] Read from a mapping table to create a mapping dictionary
- [x] Match data with mapping table and change dicom tags
- [x] Handle missing tags gracefully > partially complete

2. Anonymization
- [x] Track unique PatientID and assigne an UUID per ID

3. Sort
- [x] Ceeate Paths from the given list
- [x] Save files to a generated destination path with desired filename
- [x] Sanitize dicom tags for missing data
- [x] Sort based on PATIENTID, PATENTNAME, MODALITY

4. Report
- [ ] Sorted Data needed
- [ ] Generate a CSV report
---