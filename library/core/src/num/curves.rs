//! Elliptic curve functions.

#![stable(feature = "rust1", since = "1.0.0")]

use crate::intrinsics;

macro_rules! impl_from_coordinates {
    ($($t:ty, $f:ty)*) => {
        $(
            impl $t {
                /// Create curve element from its base field coordinates.
                #[inline(always)]
                #[stable(feature = "rust1", since = "1.0.0")]
                pub unsafe fn from_coordinates(x: $f, y: $f) -> Self {
                    unsafe { intrinsics::curve_init::<$f, $t>(x, y) }
                }
            }
        )*
    }
}

impl_from_coordinates! {
    __zkllvm_curve_bls12381, __zkllvm_field_bls12381_base
    __zkllvm_curve_curve25519, __zkllvm_field_curve25519_base
    __zkllvm_curve_pallas, __zkllvm_field_pallas_base
    __zkllvm_curve_vesta, __zkllvm_field_pallas_scalar
}

/// Definitions of base curve element coordinates.
///
/// These constants will be used to get `zero()` and `one()` curve values.
#[rustfmt::skip]
mod consts {
    pub const BLS12381_CURVE_ZERO_X: __zkllvm_field_bls12381_base = 0x0g;
    pub const BLS12381_CURVE_ZERO_Y: __zkllvm_field_bls12381_base = 0x1g;
    pub const BLS12381_CURVE_ONE_X: __zkllvm_field_bls12381_base = 0x17f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bbg;
    pub const BLS12381_CURVE_ONE_Y: __zkllvm_field_bls12381_base = 0x8b3f481e3aaa0f1a09e30ed741d8ae4fcf5e095d5d00af600db18cb2c04b3edd03cc744a2888ae40caa232946c5e7e1g;

    pub const CURVE25519_CURVE_ZERO_X: __zkllvm_field_curve25519_base = 0x0g;
    pub const CURVE25519_CURVE_ZERO_Y: __zkllvm_field_curve25519_base = 0x1g;
    pub const CURVE25519_CURVE_ONE_X: __zkllvm_field_curve25519_base = 0x216936d3cd6e53fec0a4e231fdd6dc5c692cc7609525a7b2c9562d608f25d51ag;
    pub const CURVE25519_CURVE_ONE_Y: __zkllvm_field_curve25519_base = 0x6666666666666666666666666666666666666666666666666666666666666658g;

    pub const PALLAS_CURVE_ZERO_X: __zkllvm_field_pallas_base = 0x0g;
    pub const PALLAS_CURVE_ZERO_Y: __zkllvm_field_pallas_base = 0x1g;
    pub const PALLAS_CURVE_ONE_X: __zkllvm_field_pallas_base = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000000g;
    pub const PALLAS_CURVE_ONE_Y: __zkllvm_field_pallas_base = 0x2g;

    pub const VESTA_CURVE_ZERO_X: __zkllvm_field_pallas_scalar = 0x0g;
    pub const VESTA_CURVE_ZERO_Y: __zkllvm_field_pallas_scalar = 0x1g;
    pub const VESTA_CURVE_ONE_X: __zkllvm_field_pallas_scalar = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000000g;
    pub const VESTA_CURVE_ONE_Y: __zkllvm_field_pallas_scalar = 0x2g;
}

use consts::*;

macro_rules! impl_zero_one {
    ($($t:ty, $f:ty, $zero_x:expr, $zero_y:expr, $one_x:expr, $one_y:expr)*) => {
        $(
            impl $t {
                /// Returns curve neutral element.
                #[inline(always)]
                #[stable(feature = "rust1", since = "1.0.0")]
                pub fn zero() -> Self {
                    unsafe { Self::from_coordinates($zero_x, $zero_y) }
                }

                /// Returns curve generator (`one`).
                #[inline(always)]
                #[stable(feature = "rust1", since = "1.0.0")]
                pub fn one() -> Self {
                    unsafe { Self::from_coordinates($one_x, $one_y) }
                }
            }
        )*
    }
}

impl_zero_one! {
    __zkllvm_curve_bls12381, __zkllvm_field_bls12381_base, BLS12381_CURVE_ZERO_X, BLS12381_CURVE_ZERO_Y, BLS12381_CURVE_ONE_X, BLS12381_CURVE_ONE_Y
    __zkllvm_curve_curve25519, __zkllvm_field_curve25519_base, CURVE25519_CURVE_ZERO_X, CURVE25519_CURVE_ZERO_Y, CURVE25519_CURVE_ONE_X, CURVE25519_CURVE_ONE_Y
    __zkllvm_curve_pallas, __zkllvm_field_pallas_base, PALLAS_CURVE_ZERO_X, PALLAS_CURVE_ZERO_Y, PALLAS_CURVE_ONE_X, PALLAS_CURVE_ONE_Y
    __zkllvm_curve_vesta, __zkllvm_field_pallas_scalar, VESTA_CURVE_ZERO_X, VESTA_CURVE_ZERO_Y, VESTA_CURVE_ONE_X, VESTA_CURVE_ONE_Y
}
