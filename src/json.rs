//! JSON universal implementors.
//!
//! This module provides you with universal implementation for any type that implements [`serde::Deserialize`].
//!
//! [`serde::Deserialize`]: https://docs.rs/serde/1.0.85/serde/trait.Deserialize.html

use serde::Deserialize;
use serde_json::{self, from_reader};
use std::fmt;
use std::fs::File;
use std::io;
use std::path::PathBuf;

use crate::key::Key;
use crate::load::{Load, Loaded, Storage};

/// The JSON universal method. Use this with [`Storage::get_by`] or [`Storage::get_proxied_by`] to
/// benefit from the automatic implementors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Json;

/// Possible error that might occur while loading and reloading JSON formatted scarce resources.
#[derive(Debug)]
pub enum JsonError {
  /// An error in [serde_json](https://crates.io/crates/serde-json).
  JsonError(serde_json::Error),
  /// The file specified by the key failed to open.
  CannotOpenFile(PathBuf, io::Error),
  /// The input key doesn’t provide enough information to open a file.
  NoKey,
}

impl fmt::Display for JsonError {
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    match *self {
      JsonError::JsonError(ref e) => write!(f, "JSON error: {}", e),

      JsonError::CannotOpenFile(ref path, ref e) => {
        write!(f, "cannot open file {}: {}", path.display(), e)
      }

      JsonError::NoKey => f.write_str("no path key available"),
    }
  }
}

impl<C, K, T> Load<C, K, Json> for T
where
  K: Key + Into<Option<PathBuf>>,
  T: 'static + for<'de> Deserialize<'de>,
{
  type Error = JsonError;

  fn load(key: K, _: &mut Storage<C, K>, _: &mut C) -> Result<Loaded<Self, K>, Self::Error> {
    if let Some(path) = key.into() {
      let file = File::open(&path).map_err(|ioerr| JsonError::CannotOpenFile(path, ioerr))?;

      from_reader(file)
        .map(Loaded::without_dep)
        .map_err(JsonError::JsonError)
    } else {
      Err(JsonError::NoKey)
    }
  }
}
