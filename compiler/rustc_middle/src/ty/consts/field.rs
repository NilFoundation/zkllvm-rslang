use std::fmt;
use std::num::NonZeroU16;

use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use rustc_target::abi::Size;

use crypto_bigint::{U384, Encoding};

/// A `ScalarField` represents a field value. It's a lot similar to `Scalar`, but separated,
/// because it does not fits into 16 bytes.
///
/// It is backed by a [`U384`].
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ScalarField {
    // FIXME: (aleasims) remove external crate here
    /// The first `size` bytes of `data` are the value.
    data: U384,
    size: NonZeroU16,
}

impl fmt::Debug for ScalarField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:x}", self)
    }
}

impl fmt::Display for ScalarField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.data)
    }
}

impl fmt::LowerHex for ScalarField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.data)
    }
}

impl fmt::UpperHex for ScalarField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#X}", self.data)
    }
}

// Cannot derive these, as the derives take references to the fields, and we
// can't take references to fields of packed structs.
impl<CTX> crate::ty::HashStable<CTX> for ScalarField {
    fn hash_stable(&self, hcx: &mut CTX, hasher: &mut crate::ty::StableHasher) {
        // Using a block `{self.data}` here to force a copy instead of using `self.data`
        // directly, because `hash_stable` takes `&self` and would thus borrow `self.data`.
        // Since `Self` is a packed struct, that would create a possibly unaligned reference,
        // which is UB.
        { self.data.as_words() }.hash_stable(hcx, hasher);
        self.size.get().hash_stable(hcx, hasher);
    }
}

impl<S: Encoder> Encodable<S> for ScalarField {
    fn encode(&self, s: &mut S) {
        s.emit_raw_bytes(&self.data.to_be_bytes());
        s.emit_u16(self.size.get());
    }
}

impl<D: Decoder> Decodable<D> for ScalarField {
    fn decode(d: &mut D) -> ScalarField {
        // FIXME: remove this unwrap?
        let be_bytes: [u8; 48] = d.read_raw_bytes(48).try_into().unwrap();
        Self {
            data: U384::from_be_bytes(be_bytes),
            size: NonZeroU16::new(d.read_u16()).unwrap(),
        }
    }
}

impl ScalarField {
    pub fn from_be_bytes(bytes_be: &[u8; 48], size: Size) -> Self {
        let data = U384::from_be_slice(bytes_be);
        let Ok(size) = NonZeroU16::try_from(size.bytes() as u16) else {
            bug!("field type size is zero");
        };
        Self { data, size }
    }

    pub fn from_uint(i: impl Into<u128>, size: Size) -> Self {
        let i: u128 = i.into();
        let Ok(size) = NonZeroU16::try_from(size.bytes() as u16) else {
            bug!("field type size is zero");
        };
        Self { data: U384::from(i), size }
    }

    pub fn data(&self) -> U384 {
        self.data
    }

    pub fn size(&self) -> Size {
        Size::from_bytes(self.size.get())
    }

    /// Get limbs as an array of `u64`.
    pub fn words(&self) -> &[u64; 6] {
        self.data.as_words()
    }
}
