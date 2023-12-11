# DICOM Toolkit - DICOM corelab tools.

## TOOLS
1. Deidentification
2. Anonymization
3. Dicom Sort
4. Report

---
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