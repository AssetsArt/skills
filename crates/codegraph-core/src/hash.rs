use std::fmt;

/// SHA-256 of a file's contents. Wraps a raw 32-byte array so callers get
/// hex `Display`, structural equality, and a clear type name in `Debug`
/// output instead of an opaque byte array.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHash([u8; 32]);

impl FileHash {
    /// Wrap a 32-byte digest.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the underlying digest bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for FileHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileHash({self})")
    }
}

impl fmt::Display for FileHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl AsRef<[u8]> for FileHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
