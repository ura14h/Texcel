use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum StoredPath {
    Text(String),
    Tagged(TaggedPath),
}

#[derive(Serialize, Deserialize)]
struct TaggedPath {
    encoding: String,
    units: Vec<u32>,
}

impl StoredPath {
    /// Stores ordinary paths as plain strings and other paths as OS units.
    fn from_path(path: &Path) -> Self {
        if let Some(text) = path.to_str() {
            return Self::Text(text.to_owned());
        }
        #[cfg(unix)]
        let (encoding, units) = {
            use std::os::unix::ffi::OsStrExt;
            (
                "unix_bytes",
                path.as_os_str()
                    .as_bytes()
                    .iter()
                    .map(|byte| u32::from(*byte))
                    .collect(),
            )
        };
        #[cfg(windows)]
        let (encoding, units) = {
            use std::os::windows::ffi::OsStrExt;
            (
                "windows_wide",
                path.as_os_str().encode_wide().map(u32::from).collect(),
            )
        };
        #[cfg(not(any(unix, windows)))]
        let (encoding, units) = (
            "lossy",
            path.to_string_lossy().chars().map(u32::from).collect(),
        );
        Self::Tagged(TaggedPath {
            encoding: encoding.to_owned(),
            units,
        })
    }

    /// Restores a path only when its saved OS representation is supported here.
    fn into_path(self) -> Option<PathBuf> {
        match self {
            Self::Text(text) => Some(PathBuf::from(text)),
            Self::Tagged(tagged) => decode_tagged_path(tagged),
        }
    }
}

/// Keeps existing string-array settings readable while encoding non-UTF8 paths safely.
pub(super) mod recent_paths_serde {
    use super::{PathBuf, StoredPath};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Stores recent paths without losing non-text operating system units.
    pub fn serialize<S: Serializer>(paths: &[PathBuf], serializer: S) -> Result<S::Ok, S::Error> {
        paths
            .iter()
            .map(|path| StoredPath::from_path(path))
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    /// Restores each recent path that this operating system can read.
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<PathBuf>, D::Error> {
        Ok(Vec::<StoredPath>::deserialize(deserializer)?
            .into_iter()
            .filter_map(StoredPath::into_path)
            .collect())
    }
}

/// Decodes a tagged native path representation for the current operating system.
fn decode_tagged_path(tagged: TaggedPath) -> Option<PathBuf> {
    match tagged.encoding.as_str() {
        #[cfg(unix)]
        "unix_bytes" => {
            use std::os::unix::ffi::OsStringExt;
            let bytes = tagged
                .units
                .into_iter()
                .map(u8::try_from)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
        }
        #[cfg(windows)]
        "windows_wide" => {
            use std::os::windows::ffi::OsStringExt;
            let wide = tagged
                .units
                .into_iter()
                .map(u16::try_from)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            Some(PathBuf::from(std::ffi::OsString::from_wide(&wide)))
        }
        #[cfg(not(any(unix, windows)))]
        "lossy" => Some(PathBuf::from(
            tagged
                .units
                .into_iter()
                .filter_map(char::from_u32)
                .collect::<String>(),
        )),
        _ => None,
    }
}
