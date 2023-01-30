#![feature(dyn_star)]
#![allow(incomplete_features)]

use std::fmt::Debug;

fn dyn_debug(_: (dyn* Debug + '_)) {

}

fn polymorphic<T: Debug + ?Sized>(t: &T) {
    dyn_debug(t);
    //~^ ERROR `&T` needs to be a pointer-sized type
}

fn main() {}
