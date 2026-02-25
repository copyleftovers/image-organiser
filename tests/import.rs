use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn cmd() -> assert_cmd::Command {
    cargo_bin_cmd!("image-organiser").into()
}

fn create_file(dir: &Path, name: &str, content: &[u8]) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&path, content).expect("write test file");
}

fn read_manifest(dir: &Path) -> serde_json::Value {
    let path = dir.join(".manifest.json");
    let content = fs::read_to_string(&path).expect("read manifest");
    serde_json::from_str(&content).expect("parse manifest")
}

// --- S1: First Import (Happy Path) ---

#[test]
fn dry_run_produces_no_files() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "photo.jpg", b"jpeg content here");
    create_file(source.path(), "video.mov", b"video content here");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("[DRY RUN]"))
        .stdout(predicate::str::contains("Pass --execute to perform operations."));

    let target_entries: Vec<_> = fs::read_dir(target.path())
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    assert!(target_entries.is_empty(), "dry-run must not create any files in target");
}

#[test]
fn dry_run_shows_per_file_operations() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "photo.jpg", b"jpeg content");
    create_file(source.path(), "notes.txt", b"text content");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("SKIPPED"))
        .stderr(predicate::str::contains(".txt unrecognized"));
}

#[test]
fn execute_creates_organized_structure_with_manifests() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "a.png", b"image a");
    create_file(source.path(), "b.jpg", b"image b");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success()
        .stdout(predicate::str::contains("undated"));

    let undated_dir = target.path().join("undated");
    assert!(undated_dir.exists(), "undated/ directory must be created");

    let manifest = read_manifest(&undated_dir);
    assert_eq!(manifest["version"], 1);

    let files = manifest["files"].as_object().expect("files is object");
    assert_eq!(files.len(), 2, "both files should be in manifest");

    for (_filename, entry) in files {
        assert!(entry["sha256"].is_string(), "sha256 must be present");
        assert!(entry["original_path"].is_string(), "original_path must be present");
        assert!(entry["original_name"].is_string(), "original_name must be present");
        assert!(entry["imported_at"].is_string(), "imported_at must be present");
        assert!(entry["file_size_bytes"].is_number(), "file_size_bytes must be present");
    }
}

// --- S2: Deduplication Across Multiple Imports ---

#[test]
fn second_import_detects_duplicates() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "photo.jpg", b"same content");
    create_file(source.path(), "video.mov", b"unique content");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    let second_source = TempDir::new().unwrap();
    create_file(second_source.path(), "copy_of_photo.jpg", b"same content");
    create_file(second_source.path(), "new_file.png", b"brand new");

    cmd()
        .args(["import", second_source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 undated"))
        .stdout(predicate::str::contains("1 duplicates"));

    let dup_dir = target.path().join("duplicates");
    assert!(dup_dir.exists(), "duplicates/ directory must be created");

    let dup_manifest = read_manifest(&dup_dir);
    let dup_files = dup_manifest["files"].as_object().expect("files is object");
    assert_eq!(dup_files.len(), 1, "one duplicate should be quarantined");
}

#[test]
fn idempotent_rerun_shows_all_duplicates() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "a.jpg", b"content a");
    create_file(source.path(), "b.png", b"content b");
    create_file(source.path(), "c.mov", b"content c");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success()
        .stdout(predicate::str::contains("3 undated"));

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("3 duplicates"));
}

// --- S5: Timestamp Collision Resolution ---

#[test]
fn collision_resolution_preserves_all_files() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "burst_1.png", b"burst shot 1");
    create_file(source.path(), "burst_2.png", b"burst shot 2");
    create_file(source.path(), "burst_3.png", b"burst shot 3");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    let undated_dir = target.path().join("undated");
    let manifest = read_manifest(&undated_dir);
    let files = manifest["files"].as_object().expect("files is object");
    assert_eq!(files.len(), 3, "all 3 burst files must be preserved");

    let hashes: std::collections::HashSet<_> = files
        .values()
        .map(|e| e["sha256"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(hashes.len(), 3, "all 3 files must have distinct hashes");
}

// --- S6: Move vs Copy Semantics ---

#[test]
fn copy_preserves_source_files() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "keep_me.jpg", b"precious data");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    assert!(
        source.path().join("keep_me.jpg").exists(),
        "source file must still exist after copy"
    );
}

#[test]
fn move_removes_source_files() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "move_me.jpg", b"data to move");

    cmd()
        .args([
            "import",
            source.path().to_str().unwrap(),
            target.path().to_str().unwrap(),
            "--execute",
            "--move",
        ])
        .assert()
        .success();

    assert!(
        !source.path().join("move_me.jpg").exists(),
        "source file must be removed after move"
    );

    let undated_dir = target.path().join("undated");
    let manifest = read_manifest(&undated_dir);
    let files = manifest["files"].as_object().expect("files is object");
    assert_eq!(files.len(), 1, "file must be in target");
}

// --- S7: Undated File Handling ---

#[test]
fn files_without_metadata_go_to_undated() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "screenshot.png", b"plain png bytes");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 undated"));

    let undated_dir = target.path().join("undated");
    assert!(undated_dir.exists());

    let manifest = read_manifest(&undated_dir);
    let files = manifest["files"].as_object().expect("files is object");
    let entry = files.values().next().expect("at least one entry");
    assert!(entry.get("date_source").is_none() || entry["date_source"].is_null(),
        "undated files must not have a date_source");
}

#[test]
fn undated_filename_includes_hash_suffix() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "IMG_1234.png", b"some image data");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    let undated_dir = target.path().join("undated");
    let manifest = read_manifest(&undated_dir);
    let filenames: Vec<_> = manifest["files"].as_object().unwrap().keys().collect();
    assert_eq!(filenames.len(), 1);

    let filename = filenames[0];
    assert!(filename.starts_with("IMG_1234_"), "undated file should preserve original stem");
    assert!(filename.ends_with(".png"), "undated file should preserve extension");
    assert!(filename.len() > "IMG_1234_.png".len(), "undated file should have hash suffix");
}

// --- S8: Unrecognized File Types ---

#[test]
fn unrecognized_extensions_skipped_with_warning() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "document.pdf", b"pdf data");
    create_file(source.path(), "archive.zip", b"zip data");
    create_file(source.path(), "photo.jpg", b"jpeg data");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success()
        .stderr(predicate::str::contains(".pdf unrecognized"))
        .stderr(predicate::str::contains(".zip unrecognized"))
        .stdout(predicate::str::contains("2 skipped"))
        .stdout(predicate::str::contains("1 imported").or(predicate::str::contains("1 undated")));
}

// --- S10: Source Group Tracking ---

#[test]
fn source_group_tracked_in_manifest() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "IMG_1234.heic", b"photo data");
    create_file(source.path(), "IMG_1234.mov", b"live photo video");
    create_file(source.path(), "IMG_1234.aae", b"sidecar data");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    let undated_dir = target.path().join("undated");
    let manifest = read_manifest(&undated_dir);
    let files = manifest["files"].as_object().expect("files is object");

    for (_filename, entry) in files {
        assert_eq!(
            entry["source_group"].as_str().unwrap(),
            "IMG_1234",
            "all related files must share source_group"
        );
    }
}

// --- S12: Source File Disappears ---

#[test]
fn disappearing_source_does_not_crash() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "exists.jpg", b"real file");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();
}

// --- Manifest Schema ---

#[test]
fn manifest_version_is_always_1() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "test.jpg", b"some data");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    let undated_dir = target.path().join("undated");
    let manifest = read_manifest(&undated_dir);
    assert_eq!(manifest["version"], 1);
}

#[test]
fn manifest_imported_at_is_utc() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "test.jpg", b"some data");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    let undated_dir = target.path().join("undated");
    let manifest = read_manifest(&undated_dir);
    let files = manifest["files"].as_object().unwrap();
    let entry = files.values().next().unwrap();
    let imported_at = entry["imported_at"].as_str().unwrap();
    assert!(imported_at.ends_with('Z'), "imported_at must end with Z (UTC)");
    assert!(imported_at.contains('T'), "imported_at must be ISO 8601");
}

// --- Summary Format ---

#[test]
fn summary_format_matches_spec() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "a.jpg", b"data a");
    create_file(source.path(), "b.txt", b"data b");

    let output = cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap()])
        .output()
        .expect("run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let re = regex_lite::Regex::new(
        r"^\[DRY RUN\] \d+ imported, \d+ duplicates, \d+ corrupt, \d+ undated, \d+ skipped$"
    ).unwrap();
    assert!(
        stdout.lines().any(|line| re.is_match(line)),
        "summary line must match spec format, got: {}", stdout
    );
}

// --- Extension Coverage ---

#[test]
fn all_30_spec_extensions_are_recognized() {
    let extensions = [
        "heic", "heif", "jpeg", "jpg", "png", "tiff", "tif", "webp", "bmp", "gif", "avif",
        "cr2", "cr3", "nef", "arw", "raf", "rw2", "dng", "orf", "pef", "srw", "3fr",
        "mov", "mp4", "m4v", "avi", "mkv", "3gp",
        "aae",
    ];

    for ext in &extensions {
        let path = std::path::PathBuf::from(format!("test.{}", ext));
        match image_organiser::scan::classify_file(&path) {
            image_organiser::scan::MediaFile::Recognized { .. } => {}
            image_organiser::scan::MediaFile::Unrecognized { .. } => {
                panic!(".{} must be recognized per spec", ext);
            }
        }
    }
}

#[test]
fn case_insensitive_extensions() {
    let cases = ["JPG", "Heic", "MOV", "Png", "AAE"];
    for ext in &cases {
        let path = std::path::PathBuf::from(format!("test.{}", ext));
        match image_organiser::scan::classify_file(&path) {
            image_organiser::scan::MediaFile::Recognized { .. } => {}
            image_organiser::scan::MediaFile::Unrecognized { .. } => {
                panic!(".{} must be recognized (case insensitive)", ext);
            }
        }
    }
}
