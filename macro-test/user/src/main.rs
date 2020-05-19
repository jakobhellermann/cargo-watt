#![feature(proc_macro_hygiene)]

#[allow(unused)]
#[derive(macro_test_watt::DeriveMacro)]
struct Test(#[test] u8);

fn main() {
    macro_test_watt::my_macro!();
    macro_test_watt::my_macro1!();

    println!("{}", answer());

    #[macro_test_watt::attribute_macro_twice]
    println!("twice");
}
