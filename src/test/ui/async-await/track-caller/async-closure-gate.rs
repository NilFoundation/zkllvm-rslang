// edition:2021

#![feature(async_closure, stmt_expr_attributes)]

fn main() {
    let _ = #[track_caller] async || {
        //~^ ERROR `#[track_caller]` on closures is currently unstable [E0658]
    };
}
