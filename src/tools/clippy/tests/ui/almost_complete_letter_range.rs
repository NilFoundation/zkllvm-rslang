// run-rustfix
// edition:2018
// aux-build:macro_rules.rs

#![feature(exclusive_range_pattern)]
#![feature(stmt_expr_attributes)]
#![warn(clippy::almost_complete_letter_range)]
#![allow(ellipsis_inclusive_range_patterns)]
#![allow(clippy::needless_parens_on_range_literals)]

#[macro_use]
extern crate macro_rules;

macro_rules! a {
    () => {
        'a'
    };
}

macro_rules! b {
    () => {
        let _ = 'a'..'z';
    };
}

fn main() {
    #[rustfmt::skip]
    {
        let _ = ('a') ..'z';
        let _ = 'A' .. ('Z');
    }

    let _ = 'b'..'z';
    let _ = 'B'..'Z';

    let _ = (b'a')..(b'z');
    let _ = b'A'..b'Z';

    let _ = b'b'..b'z';
    let _ = b'B'..b'Z';

    let _ = a!()..'z';

    let _ = match 0u8 {
        b'a'..b'z' if true => 1,
        b'A'..b'Z' if true => 2,
        b'b'..b'z' => 3,
        b'B'..b'Z' => 4,
        _ => 5,
    };

    let _ = match 'x' {
        'a'..'z' if true => 1,
        'A'..'Z' if true => 2,
        'b'..'z' => 3,
        'B'..'Z' => 4,
        _ => 5,
    };

    almost_complete_letter_range!();
    b!();
}

#[clippy::msrv = "1.25"]
fn _under_msrv() {
    let _ = match 'a' {
        'a'..'z' => 1,
        _ => 2,
    };
}

#[clippy::msrv = "1.26"]
fn _meets_msrv() {
    let _ = 'a'..'z';
    let _ = match 'a' {
        'a'..'z' => 1,
        _ => 2,
    };
}
