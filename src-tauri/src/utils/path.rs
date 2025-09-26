use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use crate::error::Result;

pub fn normalize(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(clean_path(&absolute))
}

pub fn clean_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                components.pop();
            }
            Component::CurDir => {}
            _ => components.push(component.as_os_str().to_os_string()),
        }
    }

    let mut cleaned = PathBuf::new();
    for component in components {
        cleaned.push(component);
    }
    cleaned
}

pub fn ensure_trailing_separator(path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() {
        return PathBuf::new();
    }

    let mut buf = path.to_path_buf();
    if !path.ends_with(std::path::MAIN_SEPARATOR_STR) {
        buf.push("");
    }
    buf
}

pub fn to_posix_string(path: &Path) -> Cow<'_, str> {
    let path_str = path.to_string_lossy();
    if path_str.contains('\\') {
        Cow::Owned(path_str.replace('\\', "/"))
    } else {
        path_str
    }
}

pub fn join_and_normalize(base: impl AsRef<Path>, segment: impl AsRef<Path>) -> Result<PathBuf> {
    let joined = base.as_ref().join(segment);
    normalize(joined)
}
