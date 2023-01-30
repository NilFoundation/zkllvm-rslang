// compile-flags: -Z simulate-remapped-rust-src-base=/rustc/FAKE_PREFIX -Z translate-remapped-path-to-local-path=no -Z ui-testing=no
// normalize-stderr-test "alloc[0-9]+" -> "ALLOC_ID"

#![feature(const_swap)]
#![feature(const_mut_refs)]
use std::{
    mem::{self, MaybeUninit},
    ptr,
};

const X: () = {
    let mut ptr1 = &1;
    let mut ptr2 = &2;

    // Swap them, bytewise.
    unsafe {
        ptr::swap_nonoverlapping(
            &mut ptr1 as *mut _ as *mut MaybeUninit<u8>,
            &mut ptr2 as *mut _ as *mut MaybeUninit<u8>,
            mem::size_of::<&i32>(),
        );
    }
};

fn main() {
    X
}
