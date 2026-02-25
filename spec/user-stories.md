# User Stories: image-organiser

## S1: First Import (Happy Path)

**As** a dump collector,
**I want to** see exactly what will happen before any files are touched,
**so that** I can verify the plan matches my expectations before committing.

**Acceptance criteria**:

Given a source folder with 50 mixed media files (HEIC, JPEG, MOV, MP4, PNG, AAE)
When I run `image-organiser import <source> <target>` (no --execute flag)
Then I see a preview of all 50 operations showing destination paths
And I see a summary: "50 files to import, 0 duplicates, 0 corrupt, 0 undated"
And no files are copied/moved (source and target unchanged)

Given the same source after reviewing the preview
When I run `image-organiser import <source> <target> --execute`
Then all 50 files appear in target organized as `YYYY/MM/YYYYMMDD_HHMMSS.ext`
And each month folder contains a `.manifest.json` with file metadata
And I see the same summary as dry-run but with confirmation of completion
And I can browse target chronologically by folder structure

**Verification**:
- Pick a file, check EXIF date matches folder location and filename
- Open `.manifest.json`, verify `original_path` and `sha256` fields present
- Run command again (dry-run), should show 50 duplicates, 0 to import

---

## S2: Deduplication Across Multiple Imports

**As** a dump collector importing weekly iPhone dumps,
**I want** files I've already imported to be detected and quarantined,
**so that** I don't waste disk space or clutter my library with duplicates.

**Acceptance criteria**:

Given a target with 100 previously imported files (with manifests)
When I run `image-organiser import <new-dump> <target>` (dry-run)
And the new dump contains 30 new files + 20 files that match SHA-256 hashes from previous imports
Then preview shows "30 files to import, 20 duplicates, 0 corrupt, 0 undated"
And preview lists each duplicate with source path and existing target path

Given the same scenario with --execute
When import completes
Then 30 new files land in dated folders
And 20 duplicates land in `duplicates/` folder with original naming scheme
And `duplicates/.manifest.json` records their sha256 and original_path
And I can verify duplicates by comparing hashes: files in duplicates/ match files in dated folders

**Edge case**: If duplicate appears in multiple prior months, detection still works (hash set built from all manifests).

---

## S3: Resilient Date Extraction

**As** a dump collector with files from various devices and apps,
**I want** the tool to extract dates from multiple metadata formats without crashing,
**so that** one weird file doesn't block my entire import.

**Acceptance criteria**:

Given a source with:
- 10 iPhone photos (EXIF DateTimeOriginal in format 1)
- 10 camera RAW files (EXIF DateTimeDigitized in format 2)
- 10 videos (QuickTime CreationDate)
- 5 screenshots with no metadata
When I run import --execute
Then all 40 files are categorized correctly
And 35 files land in dated folders (YYYY/MM/) based on extracted dates
And 5 screenshots land in `undated/` folder
And no panics occur regardless of date format variations

**Verification**:
- Inspect undated/.manifest.json: files have no `date_source` field or it's `null`
- Inspect dated file manifests: `date_source` field shows which EXIF/QuickTime tag was used

---

## S4: Corrupt File Quarantine

**As** a dump collector,
**I want** corrupt or unreadable files to be isolated without stopping my import,
**so that** one bad file doesn't block hundreds of good files.

**Acceptance criteria**:

Given a source with 100 files where 3 are corrupt (unreadable bytes, I/O errors, or malformed media)
When I run import --execute
Then 97 good files are organized normally
And 3 corrupt files are moved/copied to `corrupt/` folder
And I see warnings in output: "WARNING: Quarantined corrupt file: <filename>"
And summary shows "97 imported, 0 duplicates, 3 corrupt, 0 undated"
And `corrupt/.manifest.json` records the failures with error descriptions

**Edge case**: If corruption is detected mid-hash (e.g., partial read), file still quarantined and import continues.

---

## S5: Timestamp Collision Resolution

**As** a dump collector with burst photos or synchronized multi-device captures,
**I want** files with identical timestamps to coexist without overwriting,
**so that** no photos are lost due to same-second captures.

**Acceptance criteria**:

Given a source with 5 photos all timestamped 2024-01-15 14:30:22 (burst mode)
When I run import --execute
Then all 5 files land in `2024/01/`
And filenames are:
  - `20240115_143022.heic` (first file processed)
  - `20240115_143022_a1b2.heic` (collision, 4-char hash suffix added)
  - `20240115_143022_c3d4.heic` (collision, different hash)
  - (etc. for all 5 files)
And each file has a unique SHA-256 hash in manifest
And I can verify all 5 files are distinct by comparing file sizes or opening them

---

## S6: Move vs Copy Semantics

**As** a dump collector wanting to reclaim disk space,
**I want to** move files instead of copying them during import,
**so that** I can clean up the source directory automatically.

**Acceptance criteria**:

Given a source with 50 files (default behavior: copy)
When I run `import <source> <target> --execute`
Then all 50 files exist in target organized structure
And all 50 files still exist in source (unchanged)

Given the same source (reset scenario)
When I run `import <source> <target> --execute --move`
Then all 50 files exist in target organized structure
And source directory is empty (or only contains unrecognized/skipped files)
And no data loss: SHA-256 hashes of target files match what was in source

**Safety**: If move operation fails mid-transfer (e.g., cross-filesystem move issues), error is clear and partial state is obvious.

---

## S7: Undated File Handling

**As** a dump collector,
**I want** files with no extractable date to be preserved in a dedicated folder,
**so that** they're not lost but also don't pollute dated folders with wrong timestamps.

**Acceptance criteria**:

Given a source with:
- 20 files with valid EXIF/QuickTime dates
- 5 files with no metadata (e.g., screenshots, edited images)
When I run import --execute
Then 20 files land in dated folders (YYYY/MM/)
And 5 files land in `undated/` folder
And undated files are renamed with source filename + hash suffix for uniqueness: `original_name_a1b2.ext`
And `undated/.manifest.json` shows `date_source: null` for these files
And I can browse undated/ to manually sort these files later

**No guessing**: Filesystem timestamps (created/modified) are never used for dating—only metadata.

---

## S8: Unrecognized File Types (Defensive Handling)

**As** a dump collector with mixed content in source folders,
**I want** files with unknown extensions to be skipped with a clear warning,
**so that** I understand what's not being imported and why.

**Acceptance criteria**:

Given a source with:
- 40 recognized media files (jpg, heic, mov, mp4, png)
- 3 unrecognized files (txt, pdf, zip)
When I run import (dry-run)
Then preview shows "40 files to import"
And I see warnings: "SKIPPED: <filename>.txt (unrecognized extension)"
And summary shows "40 to import, 3 skipped"

When I run with --execute
Then only 40 recognized files are imported
And 3 unrecognized files remain in source untouched
And I can review the warnings to decide if I need to handle those files separately

**Recognized extensions** (from spec): heic, heif, jpeg, jpg, png, tiff, tif, webp, bmp, gif, avif, cr2, cr3, nef, arw, raf, rw2, dng, orf, pef, srw, 3fr, mov, mp4, m4v, avi, mkv, 3gp, aae

---

## S9: Progress Visibility for Large Imports

**As** a dump collector importing thousands of files,
**I want** to see real-time progress during import,
**so that** I know the tool is working and can estimate completion time.

**Acceptance criteria**:

Given a source with 2000 files
When I run import --execute
Then I see a progress bar: "[====>    ] 1247/2000 files (62%)"
And progress updates as each file is processed
And per-file status shows briefly: "COPY /source/IMG_1234.HEIC -> /target/2024/01/20240115_143022.heic"
And final summary appears when complete: "2000 imported, 45 duplicates, 2 corrupt, 8 undated, 3 skipped"

**Performance**: Progress updates don't slow down import (updates batched or throttled if needed).

---

## S10: Live Photo and Sidecar Tracking

**As** a dump collector importing iPhone Live Photos,
**I want** the tool to track relationships between paired files (HEIC+MOV, AAE sidecars),
**so that** I can later identify which files belong together even after renaming.

**Acceptance criteria**:

Given a source with:
- `IMG_1234.HEIC` (photo)
- `IMG_1234.MOV` (Live Photo video component)
- `IMG_1234.AAE` (edit sidecar)
When I run import --execute
Then all 3 files are imported to dated folders (independently, based on their own timestamps)
And all 3 are renamed to timestamp format
And manifest shows `source_group: "IMG_1234"` for all 3 files
And I can grep manifests for `"source_group": "IMG_1234"` to find all related files

**No special treatment**: Files are independent in filesystem (no bundling). Relationship preserved only in manifest metadata.

---

## S11: Target Disk Full (Graceful Failure)

**As** a dump collector,
**I want** a clear error if target disk runs out of space,
**so that** I don't end up with a partially-completed import and no explanation.

**Acceptance criteria**:

Given a target with 100 MB free space
When I run import --execute with source containing 200 MB of files
Then import processes files until disk full error occurs
And I see error message: "ERROR: Target disk full. Import aborted after N files."
And I see summary of what was completed: "N imported, M duplicates, ..."
And no corrupt/partial files left in target (failed operations cleaned up)
And I can run dry-run again to see what remains to be imported

**Recovery path**: User can free space and re-run import (dedup ensures completed files not reimported).

---

## S12: Source File Disappears Mid-Import

**As** a dump collector,
**I want** the import to continue if a source file vanishes during processing,
**so that** external interference (e.g., another process modifying source) doesn't crash the tool.

**Acceptance criteria**:

Given an import in progress (or simulated via test)
When a source file is deleted after scan but before copy/move
Then I see warning: "WARNING: Source file disappeared: <path>"
And import continues with remaining files
And summary accounts for the skipped file: "N imported, 1 failed, ..."
And no panic or crash

**Edge case**: If file disappears during hashing, same behavior (warning + continue).

---

## Notes on Story Sequencing

**Minimal Viable Import**: S1 + S3 + S4 (happy path, date extraction, error handling)

**Deduplication Value**: S2 (core persona pain point)

**Safety & Polish**: S5 (collisions), S6 (move semantics), S7 (undated), S9 (progress)

**Defensive Completeness**: S8 (unknown files), S11 (disk full), S12 (source changes)

**Advanced Tracking**: S10 (Live Photos) - nice-to-have, not blocking for core value
