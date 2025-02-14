//! Introduces additional ways for creating `arrayvec::ArrayString`s.

use arrayvec::{ArrayString, ArrayVec};

use crate::error::{Error, Result};

// Default length is 23 characters, but it can be restricted to just
// 10 characters using the `tiny_stringid` feature.
#[cfg(all(feature = "small_stringid", not(feature = "tiny_stringid")))]
pub const STRING_ID_SIZE: usize = 23;
#[cfg(all(not(feature = "small_stringid"), feature = "tiny_stringid"))]
pub const STRING_ID_SIZE: usize = 10;

/// Fixed-size string used internally for indexing objects.
#[cfg(any(feature = "small_stringid", feature = "tiny_stringid"))]
pub type StringId = ArrayString<STRING_ID_SIZE>;

#[cfg(not(any(feature = "small_stringid", feature = "tiny_stringid")))]
pub type StringId = String;

/// Short fixed-size string.
pub type ShortString = ArrayString<23>;
/// Long fixed-size string.
pub type LongString = ArrayString<100>;

#[cfg(feature = "small_stringid")]
pub fn new(s: &str) -> Result<ArrayString<STRING_ID_SIZE>> {
    ArrayString::from(s).map_err(|e| Error::Other(format!("{}", e)))
}
#[cfg(not(feature = "small_stringid"))]
pub fn new(s: &str) -> Result<String> {
    Ok(String::from(s))
}

#[cfg(any(feature = "small_stringid", feature = "tiny_stringid"))]
pub fn new_truncate(s: &str) -> ArrayString<STRING_ID_SIZE> {
    ArrayString::from(truncate_str(s, STRING_ID_SIZE as u8)).unwrap()
}
#[cfg(not(any(feature = "small_stringid", feature = "tiny_stringid")))]
pub fn new_truncate(s: &str) -> String {
    String::from(s)
}

/// Truncates string to specified size (ignoring last bytes if they form a
/// partial `char`).
#[inline]
pub(crate) fn truncate_str(slice: &str, size: u8) -> &str {
    if slice.is_char_boundary(size.into()) {
        unsafe { slice.get_unchecked(..size.into()) }
    } else if (size as usize) < slice.len() {
        let mut index = size.saturating_sub(1) as usize;
        while !slice.is_char_boundary(index) {
            index = index.saturating_sub(1);
        }
        unsafe { slice.get_unchecked(..index) }
    } else {
        slice
    }
}
