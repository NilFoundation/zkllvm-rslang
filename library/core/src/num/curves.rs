//! Elliptic curve functions.

#![stable(feature = "rust1", since = "1.0.0")]

use crate::intrinsics;

macro_rules! impl_zero_one {
    ($($t:ty, $f:ty, $zero_x:expr, $zero_y:expr, $one_x:expr, $one_y:expr)*) => {
        $(
            #[stable(feature = "rust1", since = "1.0.0")]
            impl $t {
                /// Returns curve neutral element.
                #[inline(always)]
                #[stable(feature = "rust1", since = "1.0.0")]
                pub fn zero() -> Self {
                    unsafe { intrinsics::curve_init::<$f, $t>($zero_x, $zero_y) }
                }

                /// Returns curve generator (`one`).
                #[inline(always)]
                #[stable(feature = "rust1", since = "1.0.0")]
                pub fn one() -> Self {
                    unsafe { intrinsics::curve_init::<$f, $t>($one_x, $one_y) }
                }
            }
        )*
    }
}

// FIXME: (aleasims) replace these dummy initialization values with real one
// These are here only because I cannot place todo!() here.
impl_zero_one! {
    __zkllvm_curve_bls12381, __zkllvm_field_bls12381_base, 0g, 0g, 1g, 1g
    __zkllvm_curve_curve25519, __zkllvm_field_curve25519_base, 0g, 0g, 1g, 1g
    __zkllvm_curve_pallas, __zkllvm_field_pallas_base, 0g, 0g, 1g, 1g
    __zkllvm_curve_vesta, __zkllvm_field_pallas_scalar, 0g, 0g, 1g, 1g
}
