use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::Result;
use crate::utils::fs::ensure_parent_dir;

pub fn read_json<T>(path: impl AsRef<Path>) -> Result<T>
where
    T: DeserializeOwned,
{
    let data = fs::read(path.as_ref())?;
    let value = serde_json::from_slice(&data)?;
    Ok(value)
}

pub fn write_json<T>(path: impl AsRef<Path>, value: &T) -> Result<()>
where
    T: Serialize,
{
    let path = path.as_ref();
    ensure_parent_dir(path)?;
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, json)?;
    Ok(())
}
