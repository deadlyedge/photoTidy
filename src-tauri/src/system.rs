use std::path::Path;

use serde::Serialize;

use crate::error::Result;
use crate::utils::path::to_posix_string;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStatus {
    pub path: String,
    #[serde(rename = "availableBytes")]
    pub available_bytes: u64,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
}

pub fn disk_status(path: &Path) -> Result<DiskStatus> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }

    let available_bytes = fs2::available_space(path)?;
    let total_bytes = fs2::total_space(path)?;

    Ok(DiskStatus {
        path: to_posix_string(path).into_owned(),
        available_bytes,
        total_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn disk_status_reports_space() -> Result<()> {
        let dir = tempdir()?;
        let status = disk_status(dir.path())?;
        assert!(status.total_bytes >= status.available_bytes);
        assert!(!status.path.is_empty());
        Ok(())
    }
}
