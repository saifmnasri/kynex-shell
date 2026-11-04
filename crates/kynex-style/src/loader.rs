//! Style pack loader and filesystem integration.

use crate::error::StyleError;
use crate::manifest::{StyleManifest, MAX_STYLE_FILE_SIZE};
use crate::tokens::{TokensManifest, MAX_TOKENS_FILE_SIZE};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Loads and validates a style pack from a given directory.
///
/// Ensures the manifest and tokens exist, are within strict size limits,
/// have supported schema versions, contain no path traversals or unsafe paths,
/// and that all referenced files resolve within the pack directory.
///
/// # Errors
///
/// Returns a [`StyleError`] if the style or tokens file cannot be read, exceeds size limits,
/// contains invalid syntax, has an unsupported schema version, or fails security path validation.
pub fn load_style_pack(pack_dir: &Path) -> Result<(StyleManifest, TokensManifest), StyleError> {
    let manifest_path = pack_dir.join("style.toml");
    let manifest_str = read_file_with_limit(&manifest_path, MAX_STYLE_FILE_SIZE)?;
    let manifest: StyleManifest =
        toml::from_str(&manifest_str).map_err(|e| StyleError::TomlDecode {
            path: manifest_path.clone(),
            source: e,
        })?;

    manifest.validate(&manifest_path, pack_dir)?;

    let tokens_path = pack_dir.join(&manifest.tokens.file);
    let tokens_str = read_file_with_limit(&tokens_path, MAX_TOKENS_FILE_SIZE)?;
    let tokens: TokensManifest =
        toml::from_str(&tokens_str).map_err(|e| StyleError::TomlDecode {
            path: tokens_path.clone(),
            source: e,
        })?;

    tokens.validate(&tokens_path)?;

    Ok((manifest, tokens))
}

/// Reads a file completely into a string while enforcing a strict maximum size limit.
///
/// # Errors
///
/// Returns a [`StyleError`] if the file cannot be accessed, exceeds `max_bytes`, or contains invalid UTF-8.
pub fn read_file_with_limit(path: &Path, max_bytes: u64) -> Result<String, StyleError> {
    let metadata = std::fs::metadata(path).map_err(|e| StyleError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    if metadata.len() > max_bytes {
        return Err(StyleError::FileTooLarge {
            path: path.to_path_buf(),
            max_bytes,
            found_bytes: metadata.len(),
        });
    }

    let mut file = File::open(path).map_err(|e| StyleError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| StyleError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

    Ok(content)
}
