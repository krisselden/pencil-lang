//! A compiled figure as bytes: a build compiles a document once, and a page or a video reads the
//! result back without parsing anything.
//!
//! The bytes are a header and a body. The body is the `Composition` in postcard, a compact
//! binary serde format, with no names in it, so it is only readable by the exact types that wrote
//! it. The header makes sure of that: Pencil promises no compatibility between builds, so a file
//! says which build wrote it and any other build refuses it. Which build is not a version number
//! someone has to remember to bump. It is a hash of the core's source, made by `build.rs`, so
//! any change to the types, the compiler or the styles that could change a figure changes it.
//! The native tools and the WASM host are built from the same source and so agree.
//!
//! ```text
//! "PENCILFG"   8 bytes   what this is
//! fingerprint  8 bytes   little-endian; the build that wrote it
//! body length  8 bytes   little-endian; so a cut-off file is told from a damaged one
//! body                   the Composition
//! ```
//!
//! The same document and sheets give the same bytes: nothing in a `Composition` is a map, so
//! nothing depends on an order that isn't written down.

use crate::{Composition, Error};

mod fingerprint {
    include!(concat!(env!("OUT_DIR"), "/fingerprint.rs"));
}

const MAGIC: &[u8; 8] = b"PENCILFG";
const HEADER: usize = 24;

/// The hash of the source this core was built from. Files written by another number are refused.
pub const FINGERPRINT: u64 = fingerprint::FINGERPRINT;

/// Whether bytes claim to be a compiled figure, so a host can tell one from a document's text
/// without trying to read it. Whether this build can read them is for [`Composition::from_bytes`].
pub fn is_compiled(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

impl Composition {
    /// This figure as bytes, for [`Composition::from_bytes`] to read back. What it computes
    /// while it's drawn (`Figure::solved`) isn't part of it.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let body = postcard::to_allocvec(self)
            .map_err(|e| Error::new(format!("couldn't write the compiled figure: {e}")))?;
        let mut out = Vec::with_capacity(HEADER + body.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FINGERPRINT.to_le_bytes());
        out.extend_from_slice(&(body.len() as u64).to_le_bytes());
        out.extend_from_slice(&body);
        Ok(out)
    }

    /// Reads what [`Composition::to_bytes`] wrote. Anything else is an error that says what is
    /// wrong, never a figure that isn't the one that was compiled.
    pub fn from_bytes(bytes: &[u8]) -> Result<Composition, Error> {
        let Some((header, body)) = bytes.split_at_checked(HEADER) else {
            return Err(Error::new(
                "this isn't a compiled Pencil figure: it's too short",
            ));
        };
        if &header[..8] != MAGIC {
            return Err(Error::new("this isn't a compiled Pencil figure"));
        }
        let word = |at: usize| u64::from_le_bytes(header[at..at + 8].try_into().expect("8 bytes"));
        if word(8) != FINGERPRINT {
            return Err(Error::new(format!(
                "this figure was compiled by a different build of Pencil ({:016x}, and this is \
                 {FINGERPRINT:016x}): compile it again",
                word(8),
            )));
        }
        if word(16) != body.len() as u64 {
            return Err(Error::new(
                "this compiled figure is cut short or has extra bytes after it: compile it again",
            ));
        }
        let mut composition: Composition = postcard::from_bytes(body).map_err(|e| {
            Error::new(format!(
                "this compiled figure is damaged ({e}): compile it again"
            ))
        })?;
        share_solved(&mut composition);
        Ok(composition)
    }
}

/// Panels with the same systems share their answers, as `Sources::panels` makes them: each
/// shares the first panel's.
fn share_solved(composition: &mut Composition) {
    let Some((first, rest)) = composition.panels.split_first_mut() else {
        return;
    };
    for panel in rest {
        if panel.figure.scene.systems == first.figure.scene.systems {
            panel.figure.solved = first.figure.solved.clone();
        }
    }
}
