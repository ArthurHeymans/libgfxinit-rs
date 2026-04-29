//! Vendored Ada sources for `libgfxinit-sys`.
//!
//! This crate mirrors the role of crates such as `openssl-src`: it does not
//! expose runtime APIs, it just gives the `-sys` crate a stable location for the
//! upstream source trees when the consumer enables a `vendored` feature.

use std::path::{Path, PathBuf};

/// Paths to the vendored Ada source trees.
#[derive(Debug, Clone)]
pub struct Sources {
    libhwbase: PathBuf,
    libgfxinit: PathBuf,
}

impl Sources {
    /// Return paths rooted at this crate's `vendor/` directory.
    pub fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor");
        Self {
            libhwbase: root.join("libhwbase"),
            libgfxinit: root.join("libgfxinit"),
        }
    }

    /// Path to the vendored `libhwbase` checkout.
    pub fn libhwbase(&self) -> &Path {
        self.libhwbase.as_path()
    }

    /// Path to the vendored `libgfxinit` checkout.
    pub fn libgfxinit(&self) -> &Path {
        self.libgfxinit.as_path()
    }
}

impl Default for Sources {
    fn default() -> Self {
        Self::new()
    }
}
