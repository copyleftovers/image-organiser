# User Stories: image-organiser

## S1: First Import

**As** a dump collector,
**I want to** import a folder of mixed iPhone media into an organized library,
**so that** my files are sorted by date and I can browse them chronologically.

**Acceptance criteria**:
- HEIC, JPEG, MOV, MP4, PNG, AAE files all processed
- Files land in `YYYY/MM/` folders based on date taken
- Files renamed to `YYYYMMDD_HHMMSS.ext`
- Manifest created in each month folder
- Summary printed with counts

## S2: Second Import (Cross-Dump Dedup)

**As** a dump collector,
**I want to** import a second dump into the same library without duplicating files from the first dump,
**so that** I don't waste disk space or have duplicate files cluttering my library.

**Acceptance criteria**:
- Files matching SHA-256 hashes from prior imports are detected as duplicates
- Duplicates moved/copied to `duplicates/` folder
- New files organized normally
- Summary shows duplicate count

## S3: Corrupt File Handling

**As** a dump collector,
**I want** corrupt or unreadable files to be quarantined instead of crashing the import,
**so that** one bad file doesn't block the rest of my import.

**Acceptance criteria**:
- Corrupt files placed in `corrupt/` folder
- Warning logged per corrupt file
- Import continues for remaining files
- No panics, no crashes

## S4: Undated Files

**As** a dump collector,
**I want** files with no extractable date to go to a dedicated folder,
**so that** they're not lost but also don't pollute dated folders with wrong timestamps.

**Acceptance criteria**:
- Files with no EXIF/QuickTime date metadata go to `undated/`
- Original filename preserved in manifest
- No filesystem date guessing

## S5: Dry Run

**As** a dump collector,
**I want to** preview what the import will do before it touches any files,
**so that** I can verify the plan before committing.

**Acceptance criteria**:
- Default mode (no flags) is dry-run
- Shows each file operation (copy/move/skip/quarantine)
- Shows summary counts
- `--execute` flag required to perform operations
- No files touched without `--execute`

## S6: Adopt Existing Library

**As** a dump collector who previously used image-organizer,
**I want to** re-organize my existing (partially organized) library into the new structure,
**so that** everything is consistent and deduplicated.

**Acceptance criteria**:
- Can point import at the old library as source, new location as target
- Files re-organized into `YYYY/MM/` structure
- Internal duplicates within the old library detected
- Manifests created for all organized files

## S7: Timestamp Collisions

**As** a dump collector with burst photos or multiple devices,
**I want** files with identical timestamps to coexist without overwriting,
**so that** no photos are lost.

**Acceptance criteria**:
- Collision resolved by appending 4-char hex hash suffix
- Both files preserved
- Manifest records both entries
