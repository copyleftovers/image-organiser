use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub enum MediaFile {
    Recognized { path: PathBuf, extension: String },
    Unrecognized { path: PathBuf, extension: String },
}

pub fn discover_files(source: &Path) -> Vec<PathBuf> {
    WalkDir::new(source)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect()
}

pub fn classify_file(path: &Path) -> MediaFile {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let recognized = matches!(extension.as_str(), "heic" | "heif" | "jpeg" | "jpg" | "png" | "tiff" | "tif" | "webp" | "bmp" | "gif"
        | "avif" | "cr2" | "cr3" | "nef" | "arw" | "raf" | "rw2" | "dng" | "orf" | "pef"
        | "srw" | "3fr" | "mov" | "mp4" | "m4v" | "avi" | "mkv" | "3gp" | "aae");

    if recognized {
        MediaFile::Recognized {
            path: path.to_path_buf(),
            extension,
        }
    } else {
        MediaFile::Unrecognized {
            path: path.to_path_buf(),
            extension,
        }
    }
}

pub fn extract_source_group(filename: &str) -> Option<String> {
    if filename.is_empty() {
        return None;
    }
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned());
    stem.filter(|s| !s.is_empty())
}
