//! Elliptic curve elements.

#![stable(feature = "rust1", since = "1.0.0")]

#[cfg(not(bootstrap))]
macro_rules! impl_curve_elem {
    ($($t:ty)*) => {
        $(
            #[stable(feature = "rust1", since = "1.0.0")]
            impl $t {
                /// Returns curve neutral element.
                #[inline(always)]
                #[stable(feature = "rust1", since = "1.0.0")]
                pub fn zero() -> Self {
                    todo!();
                }

                /// Returns curve generator (`one`).
                #[inline(always)]
                #[stable(feature = "rust1", since = "1.0.0")]
                pub fn one() -> Self {
                    todo!();
                }
            }
        )*
    }
}

#[cfg(not(bootstrap))]
impl_curve_elem! {
    __zkllvm_curve_bls12381
    __zkllvm_curve_curve25519
    __zkllvm_curve_pallas
    __zkllvm_curve_vesta
}
