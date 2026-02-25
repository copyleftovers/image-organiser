# Personas: image-organiser

## Primary: The Dump Collector

**Profile**: Developer who periodically imports media from iPhone (and occasionally other devices) to macOS via Image Capture, AirDrop, or USB. Media accumulates in "raw import" folders with no organization.

**Jobs to be done**:
- Import a fresh dump of 200-2000 mixed files without worrying about duplicates from prior dumps
- Trust that nothing is lost or silently overwritten
- End up with a browsable date-sorted library
- Not think about file formats, metadata quirks, or edge cases

**Frustrations**:
- Previous tool crashed on non-standard EXIF dates, leaving a half-processed mess
- Duplicate files across dumps waste disk space
- Can't tell which files have been imported and which haven't
- Mixed file types (photos, videos, screenshots, Live Photos) create chaos

**Success criteria**: Run the tool, see a clean summary, pass `--execute`, done. No babysitting.

## Anti-Persona: The Photo Professional

This tool is NOT for someone who:
- Needs Lightroom-style catalogs with ratings, collections, keywords
- Wants non-destructive editing workflows with sidecar management
- Requires perceptual deduplication across re-encoded versions
- Needs DAM (digital asset management) features
- Manages hundreds of thousands of files with sub-second query performance
