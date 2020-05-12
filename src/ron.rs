//! RON universal implementors.
//!
//! This module provides you with universal implementation for any type that implements
//! [`serde::Deserialize`] for encoded objects with [ron].
//!
//! [`serde::Deserialize`]: https://docs.rs/serde/1.0.85/serde/trait.Deserialize.html
//! [ron]: https://crates.io/crates/ron

use ron::de::{self, from_str};
use serde::Deserialize;
use std::fmt;
use std::fs::read_to_string;
use std::io;
use std::path::PathBuf;

use crate::key::Key;
use crate::load::{Load, Loaded, Storage};

/// The RON universal method. Use this with [`Storage::get_by`] or [`Storage::get_proxied_by`] to
/// benefit from the automatic implementors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ron;

/// Possible error that might occur while loading and reloading RON formatted scarce resources.
#[derive(Debug)]
pub enum RonError {
  /// An error in [ron](https://crates.io/crates/ron).
  RonError(de::Error),
  /// The file specified by the key failed to open or could not be read.
  CannotReadFile(PathBuf, io::Error),
  /// The input key doesn’t provide enough information to open a file.
  NoKey,
}

impl fmt::Display for RonError {
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    match *self {
      RonError::RonError(ref e) => write!(f, "RON error: {}", e),

      RonError::CannotReadFile(ref path, ref e) => {
        write!(f, "cannot read file {}: {}", path.display(), e)
      }

      RonError::NoKey => f.write_str("no path key available"),
    }
  }
}

impl<C, K, T> Load<C, K, Ron> for T
where
  K: Key + Into<Option<PathBuf>>,
  T: 'static + for<'de> Deserialize<'de>,
{
  type Error = RonError;

  fn load(key: K, _: &mut Storage<C, K>, _: &mut C) -> Result<Loaded<Self, K>, Self::Error> {
    if let Some(path) = key.into() {
      let file_content =
        read_to_string(&path).map_err(|ioerr| RonError::CannotReadFile(path, ioerr))?;

      from_str(&file_content)
        .map(Loaded::without_dep)
        .map_err(RonError::RonError)
    } else {
      Err(RonError::NoKey)
    }
  }
}
