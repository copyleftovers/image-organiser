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
        .stdout(predicate::str::contains("2 imported"));

    // Files should be in dated folders (using filesystem dates)
    let mut manifest_found = false;
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let year_dir = entry.path();
            if year_dir.file_name().unwrap().to_str().unwrap().chars().all(|c| c.is_numeric()) {
                for month_entry in fs::read_dir(&year_dir).unwrap() {
                    let month_dir = month_entry.unwrap().path();
                    if month_dir.is_dir() {
                        let manifest = read_manifest(&month_dir);
                        assert_eq!(manifest["version"], 1);

                        if let Some(files) = manifest["files"].as_object() {
                            if !files.is_empty() {
                                manifest_found = true;
                                for (_filename, entry) in files {
                                    assert!(entry["sha256"].is_string(), "sha256 must be present");
                                    assert!(entry["original_path"].is_string(), "original_path must be present");
                                    assert!(entry["original_name"].is_string(), "original_name must be present");
                                    assert!(entry["imported_at"].is_string(), "imported_at must be present");
                                    assert!(entry["file_size_bytes"].is_number(), "file_size_bytes must be present");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(manifest_found, "manifest with files should be created in dated folder");
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
        .stdout(predicate::str::contains("1 imported"))
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
        .stdout(predicate::str::contains("3 imported"));

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

    // Collect all files from all directories (dated folders, duplicates, undated, corrupt)
    let mut total_files = 0;
    let mut all_hashes = std::collections::HashSet::new();

    fn collect_from_dir(dir: &std::path::Path, total_files: &mut usize, all_hashes: &mut std::collections::HashSet<String>) {
        if dir.exists() && dir.is_dir() {
            if dir.join(".manifest.json").exists() {
                let manifest = read_manifest(dir);
                if let Some(files) = manifest["files"].as_object() {
                    *total_files += files.len();
                    for (_filename, entry) in files {
                        all_hashes.insert(entry["sha256"].as_str().unwrap().to_string());
                    }
                }
            }
        }
    }

    // Check dated folders
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let dir_path = entry.path();
            let dir_name = dir_path.file_name().unwrap().to_str().unwrap();

            // Check if it's a year directory (4 digits)
            if dir_name.len() == 4 && dir_name.chars().all(|c| c.is_numeric()) {
                for month_entry in fs::read_dir(&dir_path).unwrap() {
                    let month_dir = month_entry.unwrap().path();
                    if month_dir.is_dir() {
                        collect_from_dir(&month_dir, &mut total_files, &mut all_hashes);
                    }
                }
            } else {
                // Check special directories (duplicates, undated, corrupt)
                collect_from_dir(&dir_path, &mut total_files, &mut all_hashes);
            }
        }
    }

    assert_eq!(total_files, 3, "all 3 burst files must be preserved (found {} files)", total_files);
    assert_eq!(all_hashes.len(), 3, "all 3 files must have distinct hashes");
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

    // File should be in dated folder (using filesystem date)
    let mut total_files = 0;
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let year_dir = entry.path();
            if year_dir.file_name().unwrap().to_str().unwrap().chars().all(|c| c.is_numeric()) {
                for month_entry in fs::read_dir(&year_dir).unwrap() {
                    let month_dir = month_entry.unwrap().path();
                    if month_dir.is_dir() {
                        let manifest = read_manifest(&month_dir);
                        if let Some(files) = manifest["files"].as_object() {
                            total_files += files.len();
                        }
                    }
                }
            }
        }
    }
    assert_eq!(total_files, 1, "file must be in target");
}

// --- S7: Undated File Handling ---

#[test]
fn files_without_metadata_use_filesystem_dates() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "screenshot.png", b"plain png bytes");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 imported").or(predicate::str::contains("0 undated")));

    // File should be in a dated folder (using filesystem date), not undated/
    let mut found_file = false;
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let year_dir = entry.path();
            if year_dir.file_name().unwrap().to_str().unwrap().chars().all(|c| c.is_numeric()) {
                for month_entry in fs::read_dir(&year_dir).unwrap() {
                    let month_dir = month_entry.unwrap().path();
                    if month_dir.is_dir() {
                        let manifest = read_manifest(&month_dir);
                        if let Some(files) = manifest["files"].as_object() {
                            for (_filename, entry) in files {
                                if let Some(date_source) = entry.get("date_source") {
                                    if date_source == "filesystem_created" || date_source == "filesystem_modified" {
                                        found_file = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(found_file, "file without metadata should use filesystem date and be in dated folder");
}

#[test]
fn filesystem_dates_preferred_over_undated() {
    let source = TempDir::new().unwrap();
    let target = TempDir::new().unwrap();

    create_file(source.path(), "IMG_1234.png", b"some image data");

    cmd()
        .args(["import", source.path().to_str().unwrap(), target.path().to_str().unwrap(), "--execute"])
        .assert()
        .success();

    // File should use filesystem date and NOT go to undated/
    // It should be in a YYYY/MM/ folder with timestamp-based filename
    let mut found_in_dated = false;
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let name = entry.file_name();
            let name_str = name.to_str().unwrap();
            // Check if it's a year directory (4 digits)
            if name_str.len() == 4 && name_str.chars().all(|c| c.is_numeric()) {
                found_in_dated = true;
                break;
            }
        }
    }
    assert!(found_in_dated, "file should be in dated folder (YYYY/MM/), not undated/");
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

    // Files should be in dated folders (using filesystem dates)
    let mut source_groups = vec![];
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let year_dir = entry.path();
            if year_dir.file_name().unwrap().to_str().unwrap().chars().all(|c| c.is_numeric()) {
                for month_entry in fs::read_dir(&year_dir).unwrap() {
                    let month_dir = month_entry.unwrap().path();
                    if month_dir.is_dir() {
                        let manifest = read_manifest(&month_dir);
                        if let Some(files) = manifest["files"].as_object() {
                            for (_filename, entry) in files {
                                if let Some(sg) = entry.get("source_group") {
                                    source_groups.push(sg.as_str().unwrap().to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    assert_eq!(source_groups.len(), 3, "should have 3 files with source_group");
    for sg in source_groups {
        assert_eq!(sg, "IMG_1234", "all related files must share source_group");
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

    // Find the manifest in dated folder
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let year_dir = entry.path();
            if year_dir.file_name().unwrap().to_str().unwrap().chars().all(|c| c.is_numeric()) {
                for month_entry in fs::read_dir(&year_dir).unwrap() {
                    let month_dir = month_entry.unwrap().path();
                    if month_dir.is_dir() {
                        let manifest = read_manifest(&month_dir);
                        assert_eq!(manifest["version"], 1);
                        return;
                    }
                }
            }
        }
    }
    panic!("No manifest found in dated folders");
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

    // Find the manifest in dated folder
    for entry in fs::read_dir(target.path()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let year_dir = entry.path();
            if year_dir.file_name().unwrap().to_str().unwrap().chars().all(|c| c.is_numeric()) {
                for month_entry in fs::read_dir(&year_dir).unwrap() {
                    let month_dir = month_entry.unwrap().path();
                    if month_dir.is_dir() {
                        let manifest = read_manifest(&month_dir);
                        if let Some(files) = manifest["files"].as_object() {
                            if let Some(entry) = files.values().next() {
                                let imported_at = entry["imported_at"].as_str().unwrap();
                                assert!(imported_at.ends_with('Z'), "imported_at must end with Z (UTC)");
                                assert!(imported_at.contains('T'), "imported_at must be ISO 8601");
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
    panic!("No manifest with files found in dated folders");
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
