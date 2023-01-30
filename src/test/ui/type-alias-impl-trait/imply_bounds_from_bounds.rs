// check-pass

#![feature(type_alias_impl_trait)]

trait Callable {
    type Output;
    fn call() -> Self::Output;
}

impl<'a> Callable for &'a () {
    type Output = impl Sized;
    fn call() -> Self::Output {}
}

fn test<'a>() -> impl Sized {
    <&'a () as Callable>::call()
}

fn want_static<T: 'static>(_: T) {}

fn test2<'a>() {
    want_static(<&'a () as Callable>::call());
}

fn main() {}
