use std::fs::{create_dir_all, File, OpenOptions};
use std::path::Path;

use crate::Months;

pub fn read_archive<P: AsRef<Path>>(archive_path: P) -> Result<Months, String> {
    return match File::open(archive_path) {
        Ok(archive_file) => {
            match serde_json::from_reader(archive_file) {
                Ok(archive_months) => Ok(archive_months),
                Err(error) => {
                    Err(format!("Failed to parse archive: {:?}", error))
                }
            }
        }
        Err(error) => Err(format!("Failed to open archive file: {:?}", error))
    };
}

pub fn write_archive<P: AsRef<Path>>(archive_path: P, months: &Months) -> Result<(), String> {
    let archive_path = archive_path.as_ref();
    if let Some(error) = archive_path.parent().and_then(|parent_dir| {
        create_dir_all(parent_dir).err()
    }) {
        return Err(format!("Failed to create archive directory: {:?}", error));
    }

    return match OpenOptions::new().write(true).truncate(true).read(false).create(true).open(archive_path) {
        Ok(archive_file) => {
            serde_json::to_writer(archive_file, months).map_err(|error|
                format!("Failed to convert archive to JSON: {:?}", error)
            )
        }
        Err(error) => {
            Err(format!("Failed to write to archive file: {:?}", error))
        }
    };
}
