# Product Spec: image-organiser

## Identity

A Rust CLI tool that organizes media files from arbitrary sources (iPhone dumps, camera imports, existing libraries) into a clean, date-based directory structure with content-based deduplication across multiple import runs.

## Core Operation

Single command: `import`. Takes a source directory and a target directory. Scans source recursively, extracts metadata, deduplicates against previously imported files, and organizes media into the target.

```
image-organiser import <SOURCE> <TARGET> [--execute] [--move]
```

Dry-run by default. Must pass `--execute` to perform actual file operations. `--move` switches from copy (default) to move semantics.

## Decisions

### File Scope

Handles ALL files with recognized media extensions. No restriction to specific formats.

Recognized extensions (case-insensitive):
- **Photos**: heic, heif, jpeg, jpg, png, tiff, tif, webp, bmp, gif, avif
- **RAW**: cr2, cr3, nef, arw, raf, rw2, dng, orf, pef, srw, 3fr
- **Video**: mov, mp4, m4v, avi, mkv, 3gp
- **Sidecar**: aae (iPhone edit metadata)
- **Screenshots**: png (detected by metadata, not extension alone)

Files with unrecognized extensions are skipped with a warning.

### Directory Structure

Target layout:

```
<TARGET>/
  2024/
    01/
      20240115_143022.heic
      20240115_143022_a1b2.mov     # collision resolved with hash suffix
      .manifest.json               # per-month state
    02/
      ...
  undated/
    20240315_unknown_c3d4.jpg      # no extractable date
    .manifest.json
  duplicates/
    20240315_143022_a1b2.heic      # duplicate of existing file
    .manifest.json
  corrupt/
    broken_file.mov                # unreadable/corrupt
    .manifest.json
```

### File Naming

Files renamed to timestamp format: `YYYYMMDD_HHMMSS.ext`

On collision (same timestamp, different content): append 4-char hex prefix of SHA-256 hash: `YYYYMMDD_HHMMSS_a1b2.ext`

Original filename and path stored in manifest.

### Deduplication

SHA-256 hash of file content (full byte stream).

- On import: hash each source file, check against all manifests in target.
- If hash exists in target: file is a duplicate. Move/copy to `duplicates/` subfolder.
- Cross-run dedup: manifests persist between runs, so subsequent imports detect duplicates from all prior imports.

### Paired Files

Live Photos (HEIC + MOV) and edit sidecars (AAE) are treated as independent files in the filesystem. The manifest tracks relationships via original filename pattern matching (e.g., `IMG_1234.HEIC` and `IMG_1234.MOV` share a `source_group`).

### Metadata Extraction

Priority order for date extraction:
1. EXIF `DateTimeOriginal`
2. EXIF `DateTimeDigitized`
3. EXIF `DateTime`
4. QuickTime `CreationDate` (for MOV/MP4)
5. QuickTime `MediaCreateDate`

Multiple date format strings attempted (the image-organizer's single-format approach is what caused the panic).

If all metadata extraction fails: file goes to `undated/`.

### Error Handling

| Condition | Action |
|-----------|--------|
| File unreadable (permissions, I/O) | Quarantine to `corrupt/`, log warning, continue |
| Metadata unparseable | Attempt fallback fields, then `undated/` |
| Corrupt image/video data | Quarantine to `corrupt/`, log warning, continue |
| Hash collision (different content, same hash) | Practically impossible with SHA-256; log error if detected |
| Target disk full | Abort with clear error message |
| Source file disappears mid-import | Log warning, continue with remaining files |

No panics. All errors handled with Result types. The tool never crashes on bad input.

### State: Per-Month Manifests

Each month folder (and `undated/`, `duplicates/`, `corrupt/`) contains a `.manifest.json`:

```json
{
  "version": 1,
  "files": {
    "20240115_143022.heic": {
      "sha256": "a1b2c3d4...",
      "original_path": "/Users/ryzhakar/Pictures/raw import/IMG_1234.HEIC",
      "original_name": "IMG_1234.HEIC",
      "date_source": "exif_datetime_original",
      "source_group": "IMG_1234",
      "imported_at": "2026-02-25T15:30:00Z",
      "file_size_bytes": 4521984
    }
  }
}
```

**Why per-month, not global**: Manifests travel with the data. You can move/archive entire months. No single file becomes a bottleneck. For cross-run dedup, the tool scans all manifests in the target tree at startup (building an in-memory hash set).

### Dry-Run

Default mode. Prints what would happen without touching files:

```
[DRY RUN] COPY /source/IMG_1234.HEIC -> /target/2024/01/20240115_143022.heic
[DRY RUN] SKIP /source/IMG_1235.HEIC (duplicate of /target/2024/01/20240115_143025.heic)
[DRY RUN] QUARANTINE /source/corrupt.mov -> /target/corrupt/corrupt.mov

Summary: 142 files to import, 3 duplicates, 1 corrupt, 0 undated
```

Pass `--execute` to perform operations.

### Progress

Progress bar with file count. Per-file status output. Summary at end with counts by category (imported, duplicates, corrupt, undated, skipped).

## NOT Building (Explicit Scope Exclusions)

- No GUI, no web UI
- No perceptual/fuzzy deduplication (SHA-256 only)
- No AI-based categorization or tagging
- No cloud sync or remote storage
- No photo editing or conversion
- No thumbnail generation
- No multi-user or access control
- No watch mode or daemon
- No undo command (dry-run-by-default is the safety mechanism)
- No verify/status/dedup-scan subcommands (import only for MVP)
