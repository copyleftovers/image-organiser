use crate::metadata::DateExtracted;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub version: u8,
    pub files: HashMap<String, FileEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct FileEntry {
    pub sha256: String,
    pub original_path: String,
    pub original_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_group: Option<String>,
    pub imported_at: String,
    pub file_size_bytes: u64,
}

pub fn load_manifest(dir: &Path) -> Manifest {
    let path = dir.join(".manifest.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            return Manifest {
                version: 1,
                files: HashMap::new(),
            };
        }
    };
    match serde_json::from_str(&content) {
        Ok(m) => m,
        Err(_) => {
            eprintln!(
                "WARNING: corrupt manifest at {}, starting fresh",
                path.display()
            );
            Manifest {
                version: 1,
                files: HashMap::new(),
            }
        }
    }
}

pub fn save_manifest(dir: &Path, manifest: &Manifest) -> std::io::Result<()> {
    let path = dir.join(".manifest.json");
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&path, json)
}

pub fn build_dedup_index(target: &Path) -> HashMap<String, PathBuf> {
    let mut index = HashMap::new();
    if !target.exists() {
        return index;
    }
    for entry in WalkDir::new(target)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if entry.file_name() == ".manifest.json" {
            let dir = entry.path().parent().unwrap_or(target);
            let manifest = load_manifest(dir);
            for (filename, file_entry) in &manifest.files {
                index.insert(file_entry.sha256.clone(), dir.join(filename));
            }
        }
    }
    index
}

pub fn generate_filename(
    date: &DateExtracted,
    extension: &str,
    hash: &[u8; 32],
    target_dir: &Path,
) -> String {
    if let DateExtracted::Found {
        year,
        month,
        day,
        hour,
        minute,
        second,
        ..
    } = date
    {
        let base = format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}",
            year, month, day, hour, minute, second
        );
        let candidate = format!("{}.{}", base, extension);
        if !target_dir.join(&candidate).exists() {
            return candidate;
        }
        let suffix = format!("{:02x}{:02x}", hash[0], hash[1]);
        let candidate = format!("{}_{}.{}", base, suffix, extension);
        if !target_dir.join(&candidate).exists() {
            return candidate;
        }
        for i in 1..10 {
            let suffix = format!("{:02x}{:02x}", hash[i], hash[i + 1]);
            let candidate = format!("{}_{}.{}", base, suffix, extension);
            if !target_dir.join(&candidate).exists() {
                return candidate;
            }
        }
        let long_suffix: String = hash[..4].iter().map(|b| format!("{:02x}", b)).collect();
        format!("{}_{}.{}", base, long_suffix, extension)
    } else {
        format!("undated.{}", extension)
    }
}
