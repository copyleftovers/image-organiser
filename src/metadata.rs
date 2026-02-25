use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Debug, Clone)]
pub enum DateExtracted {
    Found {
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        source: DateSource,
    },
    NotFound,
}

#[derive(Debug, Clone, Copy)]
pub enum DateSource {
    ExifDateTimeOriginal,
    ExifDateTimeDigitized,
    ExifDateTime,
    QuickTimeCreationDate,
    QuickTimeMediaCreateDate,
    FilesystemCreated,
    FilesystemModified,
}

pub fn hash_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hasher.finalize().into())
}

pub fn extract_date(path: &Path) -> DateExtracted {
    if let Some(result) = try_exif_dates(path) {
        return result;
    }
    if let Some(result) = try_quicktime_dates(path) {
        return result;
    }
    if let Some(result) = try_filesystem_dates(path) {
        return result;
    }
    DateExtracted::NotFound
}

fn try_exif_dates(path: &Path) -> Option<DateExtracted> {
    let file = File::open(path).ok()?;
    let iter = nom_exif::parse_exif(file, None).ok()??;
    let exif: nom_exif::Exif = iter.into();

    let tag_chain = [
        (nom_exif::ExifTag::DateTimeOriginal, DateSource::ExifDateTimeOriginal),
        (nom_exif::ExifTag::CreateDate, DateSource::ExifDateTimeDigitized),
        (nom_exif::ExifTag::ModifyDate, DateSource::ExifDateTime),
    ];

    for (tag, source) in &tag_chain {
        if let Some(entry) = exif.get(*tag)
            && let Some(extracted) = entry_value_to_date(entry, *source) {
            return Some(extracted);
        }
    }
    None
}

fn try_quicktime_dates(path: &Path) -> Option<DateExtracted> {
    let file = File::open(path).ok()?;
    let entries = nom_exif::parse_metadata(file).ok()?;

    let qt_keys: &[(&str, DateSource)] = &[
        ("com.apple.quicktime.creationdate", DateSource::QuickTimeCreationDate),
        ("creation_time", DateSource::QuickTimeMediaCreateDate),
    ];

    for (key, source) in qt_keys {
        for (k, v) in &entries {
            if k == key
                && let Some(extracted) = entry_value_to_date(v, *source) {
                return Some(extracted);
            }
        }
    }
    None
}

fn try_filesystem_dates(path: &Path) -> Option<DateExtracted> {
    let metadata = std::fs::metadata(path).ok()?;

    // Try creation time first (not available on all platforms/filesystems)
    if let Ok(created) = metadata.created()
        && let Some(extracted) = system_time_to_date(created, DateSource::FilesystemCreated) {
        return Some(extracted);
    }

    // Fall back to modification time
    if let Ok(modified) = metadata.modified()
        && let Some(extracted) = system_time_to_date(modified, DateSource::FilesystemModified) {
        return Some(extracted);
    }

    None
}

fn system_time_to_date(time: std::time::SystemTime, source: DateSource) -> Option<DateExtracted> {
    use std::time::UNIX_EPOCH;

    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    let timestamp = jiff::Timestamp::from_second(duration.as_secs() as i64).ok()?;
    let dt = timestamp.to_zoned(jiff::tz::TimeZone::UTC).datetime();

    Some(DateExtracted::Found {
        year: dt.year() as u16,
        month: dt.month() as u8,
        day: dt.day() as u8,
        hour: dt.hour() as u8,
        minute: dt.minute() as u8,
        second: dt.second() as u8,
        source,
    })
}

fn entry_value_to_date(entry: &nom_exif::EntryValue, source: DateSource) -> Option<DateExtracted> {
    if let Some(dt) = entry.as_time() {
        let formatted = format!("{}", dt.format("%Y:%m:%d %H:%M:%S"));
        if let Some((year, month, day, hour, minute, second)) = parse_date_string(&formatted) {
            return Some(DateExtracted::Found {
                year,
                month,
                day,
                hour,
                minute,
                second,
                source,
            });
        }
    }
    if let Some(s) = entry.as_str()
        && let Some((year, month, day, hour, minute, second)) = parse_date_string(s) {
        return Some(DateExtracted::Found {
            year,
            month,
            day,
            hour,
            minute,
            second,
            source,
        });
    }
    None
}

fn parse_date_string(s: &str) -> Option<(u16, u8, u8, u8, u8, u8)> {
    let formats = [
        "%Y:%m:%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y:%m:%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%SZ",
        "%Y:%m:%d %H:%M",
        "%Y-%m-%d",
    ];

    let trimmed = s.trim();

    for fmt in &formats {
        if let Ok(parsed) = jiff::fmt::strtime::parse(fmt, trimmed)
            && let Ok(dt) = parsed.to_datetime() {
            return Some((
                dt.year() as u16,
                dt.month() as u8,
                dt.day() as u8,
                dt.hour() as u8,
                dt.minute() as u8,
                dt.second() as u8,
            ));
        }
    }
    None
}
