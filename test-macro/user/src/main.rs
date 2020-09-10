#![feature(proc_macro_hygiene)]

#[allow(unused)]
#[derive(macro_test::DeriveMacro)]
struct Test(#[test] u8);

fn main() {
    macro_test::my_macro!();
    macro_test::my_macro1!();

    println!("{}", answer());

    #[macro_test::attribute_macro_twice]
    println!("twice");
}
