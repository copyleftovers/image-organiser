mod manifest;
mod metadata;
mod scan;

use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

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
    }
}

fn now_iso8601() -> String {
    jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Import {
            source,
            target,
            execute,
            r#move: move_files,
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

            let progress = ProgressBar::new(recognized.len() as u64);
            progress
                .set_style(
                    ProgressStyle::default_bar()
                        .template("[{elapsed_precise}] [{bar:40}] {pos}/{len} ({eta})")
                        .unwrap_or_else(|_| ProgressStyle::default_bar()), // safe: static template string
                );

            let mut imported_count: usize = 0;
            let mut duplicate_count: usize = 0;
            let mut corrupt_count: usize = 0;
            let mut undated_count: usize = 0;

            let dry_run_prefix = if execute { "" } else { "[DRY RUN] " };
            let op_word = if move_files { "MOVE" } else { "COPY" };

            for (path, extension) in &recognized {
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
                        corrupt_count += 1;
                        progress.inc(1);
                        continue;
                    }
                };

                let hex_hash = format_hash(&hash);
                let source_group = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(scan::extract_source_group);

                if let Some(existing) = dedup_index.get(&hex_hash) {
                    if execute {
                        let dup_dir = target.join("duplicates");
                        let original_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "unknown".to_string());
                        match copy_to_dir(path, &dup_dir, &original_name) {
                            Ok(dest) => {
                                eprintln!(
                                    "DUPLICATE {} -> {} (same as {})",
                                    path.display(),
                                    dest.display(),
                                    existing.display()
                                );
                                update_manifest_for_file(
                                    &dup_dir, &dest, &hex_hash, path, &original_name,
                                    None, source_group.as_deref(), extension,
                                );
                                if move_files {
                                    remove_source_safely(path, &dest);
                                }
                            }
                            Err(e) => {
                                if e.kind() == std::io::ErrorKind::NotFound {
                                    eprintln!(
                                        "WARNING: Source file disappeared: {}",
                                        path.display()
                                    );
                                } else {
                                    eprintln!(
                                        "WARNING: Failed to copy duplicate {}: {}",
                                        path.display(),
                                        e
                                    );
                                }
                            }
                        }
                    } else {
                        eprintln!(
                            "{}DUPLICATE {} (same as {})",
                            dry_run_prefix,
                            path.display(),
                            existing.display()
                        );
                    }
                    duplicate_count += 1;
                    progress.inc(1);
                    continue;
                }

                let date = metadata::extract_date(path);

                match &date {
                    metadata::DateExtracted::Found { year, month, source, .. } => {
                        let dest_dir =
                            target.join(format!("{:04}", year)).join(format!("{:02}", month));
                        let filename =
                            manifest::generate_filename(&date, extension, &hash, &dest_dir);
                        let dest = dest_dir.join(&filename);

                        if execute {
                            match copy_file_to(path, &dest) {
                                Ok(()) => {
                                    eprintln!(
                                        "{} {} -> {}",
                                        op_word,
                                        path.display(),
                                        dest.display()
                                    );
                                    let original_name = path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "unknown".to_string());
                                    update_manifest_for_file(
                                        &dest_dir,
                                        &dest,
                                        &hex_hash,
                                        path,
                                        &original_name,
                                        Some(&date_source_string(source)),
                                        source_group.as_deref(),
                                        extension,
                                    );
                                    if move_files {
                                        remove_source_safely(path, &dest);
                                    }
                                }
                                Err(e) => {
                                    if is_disk_full(&e) {
                                        std::fs::remove_file(&dest).ok();
                                        progress.finish_and_clear();
                                        eprintln!(
                                            "ERROR: Target disk full. Import aborted after {} files.",
                                            imported_count
                                        );
                                        print_summary(
                                            imported_count,
                                            duplicate_count,
                                            corrupt_count,
                                            undated_count,
                                            skipped_count,
                                            execute,
                                        );
                                        return;
                                    }
                                    if e.kind() == std::io::ErrorKind::NotFound {
                                        eprintln!(
                                            "WARNING: Source file disappeared: {}",
                                            path.display()
                                        );
                                    } else {
                                        eprintln!(
                                            "CORRUPT: {} (copy failed: {})",
                                            path.display(),
                                            e
                                        );
                                    }
                                    corrupt_count += 1;
                                    progress.inc(1);
                                    continue;
                                }
                            }
                        } else {
                            eprintln!(
                                "{}{} {} -> {}",
                                dry_run_prefix,
                                op_word,
                                path.display(),
                                dest.display()
                            );
                        }
                        imported_count += 1;
                    }
                    metadata::DateExtracted::NotFound => {
                        let dest_dir = target.join("undated");
                        let original_stem = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let hash_suffix = format!("{:02x}{:02x}", hash[0], hash[1]);
                        let filename =
                            format!("{}_{}.{}", original_stem, hash_suffix, extension);
                        let dest = dest_dir.join(&filename);

                        if execute {
                            match copy_file_to(path, &dest) {
                                Ok(()) => {
                                    eprintln!(
                                        "{} {} -> {}",
                                        op_word,
                                        path.display(),
                                        dest.display()
                                    );
                                    let original_name = path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_else(|| "unknown".to_string());
                                    update_manifest_for_file(
                                        &dest_dir,
                                        &dest,
                                        &hex_hash,
                                        path,
                                        &original_name,
                                        None,
                                        source_group.as_deref(),
                                        extension,
                                    );
                                    if move_files {
                                        remove_source_safely(path, &dest);
                                    }
                                }
                                Err(e) => {
                                    if is_disk_full(&e) {
                                        std::fs::remove_file(&dest).ok();
                                        progress.finish_and_clear();
                                        eprintln!(
                                            "ERROR: Target disk full. Import aborted after {} files.",
                                            imported_count
                                        );
                                        print_summary(
                                            imported_count,
                                            duplicate_count,
                                            corrupt_count,
                                            undated_count,
                                            skipped_count,
                                            execute,
                                        );
                                        return;
                                    }
                                    if e.kind() == std::io::ErrorKind::NotFound {
                                        eprintln!(
                                            "WARNING: Source file disappeared: {}",
                                            path.display()
                                        );
                                    } else {
                                        eprintln!(
                                            "CORRUPT: {} (copy failed: {})",
                                            path.display(),
                                            e
                                        );
                                    }
                                    corrupt_count += 1;
                                    progress.inc(1);
                                    continue;
                                }
                            }
                        } else {
                            eprintln!(
                                "{}UNDATED {} -> {}",
                                dry_run_prefix,
                                path.display(),
                                dest.display()
                            );
                        }
                        undated_count += 1;
                    }
                }
                progress.inc(1);
            }

            progress.finish_and_clear();
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

fn update_manifest_for_file(
    dest_dir: &std::path::Path,
    dest: &std::path::Path,
    hex_hash: &str,
    source_path: &std::path::Path,
    original_name: &str,
    date_source: Option<&str>,
    source_group: Option<&str>,
    _extension: &str,
) {
    let mut m = manifest::load_manifest(dest_dir);
    let filename = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let file_size = source_path
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0);
    m.files.insert(
        filename,
        manifest::FileEntry {
            sha256: hex_hash.to_string(),
            original_path: source_path.to_string_lossy().into_owned(),
            original_name: original_name.to_string(),
            date_source: date_source.map(|s| s.to_string()),
            source_group: source_group.map(|s| s.to_string()),
            imported_at: now_iso8601(),
            file_size_bytes: file_size,
        },
    );
    if let Err(e) = manifest::save_manifest(dest_dir, &m) {
        eprintln!("WARNING: Failed to save manifest in {}: {}", dest_dir.display(), e);
    }
}

fn is_disk_full(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(28)
}
