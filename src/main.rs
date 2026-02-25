mod manifest;
mod metadata;
mod scan;

use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use rayon::prelude::*;

#[derive(Parser)]
#[command(name = "image-organiser")]
#[command(about = "Organize media files into date-based directory structure")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import media files from source into organized target
    Import {
        /// Source directory to scan
        source: PathBuf,
        /// Target directory for organized library
        target: PathBuf,
        /// Actually perform file operations (default: dry-run)
        #[arg(long)]
        execute: bool,
        /// Move files instead of copying (default: copy)
        #[arg(long, rename_all = "kebab-case")]
        r#move: bool,
        /// Suppress per-file output (show only progress bar and summary)
        #[arg(long, short)]
        quiet: bool,
    },
}

fn format_hash(hash: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for byte in hash {
        s.push_str(&format!("{:02x}", byte));
    }
    s
}

fn date_source_string(source: &metadata::DateSource) -> &'static str {
    match source {
        metadata::DateSource::ExifDateTimeOriginal => "exif_datetime_original",
        metadata::DateSource::ExifDateTimeDigitized => "exif_datetime_digitized",
        metadata::DateSource::ExifDateTime => "exif_datetime",
        metadata::DateSource::QuickTimeCreationDate => "quicktime_creation_date",
        metadata::DateSource::QuickTimeMediaCreateDate => "quicktime_media_create_date",
        metadata::DateSource::FilesystemCreated => "filesystem_created",
        metadata::DateSource::FilesystemModified => "filesystem_modified",
    }
}

#[derive(Debug, Clone)]
struct ManifestEntry {
    dir: PathBuf,
    filename: String,
    entry: manifest::FileEntry,
}

#[derive(Debug)]
enum FileProcessingResult {
    Imported {
        manifest_entry: Option<ManifestEntry>,
    },
    Duplicate {
        manifest_entry: Option<ManifestEntry>,
    },
    Undated {
        manifest_entry: Option<ManifestEntry>,
    },
    Corrupt,
}

fn now_iso8601() -> String {
    jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn create_manifest_entry(
    dest: &Path,
    hex_hash: &str,
    source_path: &Path,
    original_name: &str,
    date_source: Option<&str>,
    source_group: Option<&str>,
) -> ManifestEntry {
    let file_size = source_path
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0);
    let filename = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = dest.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    ManifestEntry {
        dir,
        filename,
        entry: manifest::FileEntry {
            sha256: hex_hash.to_string(),
            original_path: source_path.to_string_lossy().into_owned(),
            original_name: original_name.to_string(),
            date_source: date_source.map(|s| s.to_string()),
            source_group: source_group.map(|s| s.to_string()),
            imported_at: now_iso8601(),
            file_size_bytes: file_size,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn process_file_for_copy(
    path: &Path,
    extension: &str,
    dedup_index: &std::collections::HashMap<String, PathBuf>,
    target: &Path,
    execute: bool,
    move_files: bool,
    file_op_lock: &std::sync::Arc<std::sync::Mutex<()>>,
    quiet: bool,
) -> FileProcessingResult {
    let dry_run_prefix = if execute { "" } else { "[DRY RUN] " };
    let op_word = if move_files { "MOVE" } else { "COPY" };
    // Extract source_group from filename
    let source_group = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(scan::extract_source_group);

    // Step 1: Hash file
    let hash = match metadata::hash_file(path) {
        Ok(h) => h,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                eprintln!(
                    "WARNING: Source file disappeared: {}",
                    path.display()
                );
            } else {
                eprintln!("CORRUPT: {} ({})", path.display(), err);
                if execute {
                    let corrupt_dir = target.join("corrupt");
                    let original_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "unknown".to_string());
                    if let Err(e) = copy_to_dir(path, &corrupt_dir, &original_name) {
                        eprintln!(
                            "WARNING: Failed to quarantine {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
            return FileProcessingResult::Corrupt;
        }
    };

    let hex_hash = format_hash(&hash);

    // Step 2: Check for duplicates
    if let Some(existing) = dedup_index.get(&hex_hash) {
        if execute {
            let dup_dir = target.join("duplicates");
            let original_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "unknown".to_string());
            match copy_to_dir(path, &dup_dir, &original_name) {
                Ok(dest) => {
                    if !quiet {
                        eprintln!(
                            "DUPLICATE {} -> {} (same as {})",
                            path.display(),
                            dest.display(),
                            existing.display()
                        );
                    }
                    let manifest_entry = create_manifest_entry(&dest, &hex_hash, path, &original_name, None, source_group.as_deref());
                    if move_files {
                        remove_source_safely(path, &dest);
                    }
                    return FileProcessingResult::Duplicate {
                        manifest_entry: Some(manifest_entry),
                    };
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        eprintln!(
                            "WARNING: Failed to copy duplicate {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        } else if !quiet {
            eprintln!(
                "{}DUPLICATE {} (same as {})",
                dry_run_prefix,
                path.display(),
                existing.display()
            );
        }
        return FileProcessingResult::Duplicate {
            manifest_entry: None,
        };
    }

    // Step 3: Extract date
    let date = metadata::extract_date(path);

    match &date {
        metadata::DateExtracted::Found { year, month, source, .. } => {
            let dest_dir = target.join(format!("{:04}", year)).join(format!("{:02}", month));

            // Lock to prevent race condition in filename generation + copy
            let (_filename, dest) = {
                let _lock = file_op_lock.lock().unwrap();
                let filename = manifest::generate_filename(&date, extension, &hash, &dest_dir);
                let dest = dest_dir.join(&filename);
                (filename, dest)
            };

            if execute {
                let _lock = file_op_lock.lock().unwrap();
                match copy_file_to(path, &dest) {
                    Ok(()) => {
                        if !quiet {
                            eprintln!(
                                "{} {} -> {}",
                                op_word,
                                path.display(),
                                dest.display()
                            );
                        }
                        let original_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "unknown".to_string());
                        let manifest_entry = create_manifest_entry(
                            &dest,
                            &hex_hash,
                            path,
                            &original_name,
                            Some(date_source_string(source)),
                            source_group.as_deref(),
                        );
                        if move_files {
                            remove_source_safely(path, &dest);
                        }
                        FileProcessingResult::Imported {
                            manifest_entry: Some(manifest_entry),
                        }
                    }
                    Err(e) => {
                        if is_disk_full(&e) {
                            std::fs::remove_file(&dest).ok();
                            eprintln!("ERROR: Target disk full");
                        }
                        if e.kind() != std::io::ErrorKind::NotFound {
                            eprintln!(
                                "CORRUPT: {} (copy failed: {})",
                                path.display(),
                                e
                            );
                        }
                        FileProcessingResult::Corrupt
                    }
                }
            } else {
                if !quiet {
                    eprintln!(
                        "{}{} {} -> {}",
                        dry_run_prefix,
                        op_word,
                        path.display(),
                        dest.display()
                    );
                }
                FileProcessingResult::Imported {
                    manifest_entry: None,
                }
            }
        }
        metadata::DateExtracted::NotFound => {
            let dest_dir = target.join("undated");
            let original_stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let hash_suffix = format!("{:02x}{:02x}", hash[0], hash[1]);
            let filename = format!("{}_{}.{}", original_stem, hash_suffix, extension);
            let dest = dest_dir.join(&filename);

            if execute {
                let _lock = file_op_lock.lock().unwrap();
                match copy_file_to(path, &dest) {
                    Ok(()) => {
                        if !quiet {
                            eprintln!(
                                "{} {} -> {}",
                                op_word,
                                path.display(),
                                dest.display()
                            );
                        }
                        let original_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "unknown".to_string());
                        let manifest_entry =
                            create_manifest_entry(&dest, &hex_hash, path, &original_name, None, source_group.as_deref());
                        if move_files {
                            remove_source_safely(path, &dest);
                        }
                        FileProcessingResult::Undated {
                            manifest_entry: Some(manifest_entry),
                        }
                    }
                    Err(e) => {
                        if is_disk_full(&e) {
                            std::fs::remove_file(&dest).ok();
                            eprintln!("ERROR: Target disk full");
                        }
                        if e.kind() != std::io::ErrorKind::NotFound {
                            eprintln!(
                                "CORRUPT: {} (copy failed: {})",
                                path.display(),
                                e
                            );
                        }
                        FileProcessingResult::Corrupt
                    }
                }
            } else {
                if !quiet {
                    eprintln!(
                        "{}UNDATED {} -> {}",
                        dry_run_prefix,
                        path.display(),
                        dest.display()
                    );
                }
                FileProcessingResult::Undated {
                    manifest_entry: None,
                }
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Import {
            source,
            target,
            execute,
            r#move: move_files,
            quiet,
        } => {
            let files = scan::discover_files(&source);
            let dedup_index = manifest::build_dedup_index(&target);

            let mut recognized: Vec<(PathBuf, String)> = Vec::new();
            let mut skipped_count: usize = 0;
            for file in &files {
                match scan::classify_file(file) {
                    scan::MediaFile::Recognized { path, extension } => {
                        recognized.push((path, extension));
                    }
                    scan::MediaFile::Unrecognized { path, extension } => {
                        if extension.is_empty() {
                            eprintln!("SKIPPED: {} (no extension)", path.display());
                        } else {
                            eprintln!("SKIPPED: {} (.{} unrecognized)", path.display(), extension);
                        }
                        skipped_count += 1;
                    }
                }
            }

            let progress = Arc::new(ProgressBar::new(recognized.len() as u64));
            progress
                .set_style(
                    ProgressStyle::default_bar()
                        .template("[{elapsed_precise}] [{bar:40}] {pos}/{len} ({eta})")
                        .unwrap_or_else(|_| ProgressStyle::default_bar()), // safe: static template string
                );

            // Atomic counters for results
            let imported_count = Arc::new(AtomicUsize::new(0));
            let duplicate_count = Arc::new(AtomicUsize::new(0));
            let corrupt_count = Arc::new(AtomicUsize::new(0));
            let undated_count = Arc::new(AtomicUsize::new(0));

            // Synchronize file operations to prevent race conditions in parallel mode
            use std::sync::Mutex;
            let file_op_lock = Arc::new(Mutex::new(()));

            // Parallel processing
            let results: Vec<_> = recognized
                .par_iter()
                .map(|(path, extension)| {
                    let result = process_file_for_copy(
                        path,
                        extension,
                        &dedup_index,
                        &target,
                        execute,
                        move_files,
                        &file_op_lock,
                        quiet,
                    );

                    // Update counters
                    match &result {
                        FileProcessingResult::Imported { .. } => {
                            imported_count.fetch_add(1, Ordering::Relaxed);
                        }
                        FileProcessingResult::Duplicate { .. } => {
                            duplicate_count.fetch_add(1, Ordering::Relaxed);
                        }
                        FileProcessingResult::Undated { .. } => {
                            undated_count.fetch_add(1, Ordering::Relaxed);
                        }
                        FileProcessingResult::Corrupt => {
                            corrupt_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }

                    // Thread-safe progress update
                    progress.inc(1);

                    result
                })
                .collect();

            progress.finish_and_clear();

            // Batch manifest updates
            if execute {
                let mut manifest_batches: std::collections::HashMap<PathBuf, Vec<(String, manifest::FileEntry)>> =
                    std::collections::HashMap::new();

                for result in &results {
                    match result {
                        FileProcessingResult::Imported { manifest_entry }
                        | FileProcessingResult::Duplicate { manifest_entry }
                        | FileProcessingResult::Undated { manifest_entry } => {
                            if let Some(entry) = manifest_entry {
                                manifest_batches
                                    .entry(entry.dir.clone())
                                    .or_default()
                                    .push((entry.filename.clone(), entry.entry.clone()));
                            }
                        }
                        FileProcessingResult::Corrupt => {}
                    }
                }

                // Write all manifests
                for (dir, entries) in manifest_batches {
                    let mut m = manifest::load_manifest(&dir);
                    for (filename, file_entry) in entries {
                        m.files.insert(filename, file_entry);
                    }
                    if let Err(e) = manifest::save_manifest(&dir, &m) {
                        eprintln!("WARNING: Failed to save manifest in {}: {}", dir.display(), e);
                    }
                }
            }

            let imported_count = imported_count.load(Ordering::SeqCst);
            let duplicate_count = duplicate_count.load(Ordering::SeqCst);
            let corrupt_count = corrupt_count.load(Ordering::SeqCst);
            let undated_count = undated_count.load(Ordering::SeqCst);
            print_summary(
                imported_count,
                duplicate_count,
                corrupt_count,
                undated_count,
                skipped_count,
                execute,
            );
        }
    }
}

fn print_summary(
    imported: usize,
    duplicates: usize,
    corrupt: usize,
    undated: usize,
    skipped: usize,
    execute: bool,
) {
    if execute {
        println!(
            "{} imported, {} duplicates, {} corrupt, {} undated, {} skipped",
            imported, duplicates, corrupt, undated, skipped
        );
    } else {
        println!(
            "[DRY RUN] {} imported, {} duplicates, {} corrupt, {} undated, {} skipped",
            imported, duplicates, corrupt, undated, skipped
        );
        println!("\nPass --execute to perform operations.");
    }
}

fn copy_file_to(source: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, dest)?;
    Ok(())
}

fn copy_to_dir(
    source: &std::path::Path,
    dir: &std::path::Path,
    name: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut dest = dir.join(name);
    if dest.exists() {
        let stem = std::path::Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = std::path::Path::new(name)
            .extension()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut counter = 1u32;
        loop {
            let candidate = if ext.is_empty() {
                format!("{}_{}", stem, counter)
            } else {
                format!("{}_{}.{}", stem, counter, ext)
            };
            dest = dir.join(&candidate);
            if !dest.exists() {
                break;
            }
            counter += 1;
            if counter > 1000 {
                break;
            }
        }
    }
    std::fs::copy(source, &dest)?;
    Ok(dest)
}

fn remove_source_safely(
    source: &std::path::Path,
    dest: &std::path::Path,
) {
    match (dest.exists(), dest.metadata()) {
        (true, Ok(dest_meta)) => match source.metadata() {
            Ok(src_meta) if dest_meta.len() == src_meta.len() => {
                if let Err(e) = std::fs::remove_file(source) {
                    eprintln!(
                        "WARNING: Failed to remove source {}: {}",
                        source.display(),
                        e
                    );
                }
            }
            _ => {
                eprintln!(
                    "WARNING: Size mismatch after copy, source preserved: {}",
                    source.display()
                );
            }
        },
        _ => {
            eprintln!(
                "WARNING: Dest verification failed, source preserved: {}",
                source.display()
            );
        }
    }
}


fn is_disk_full(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(28)
}
