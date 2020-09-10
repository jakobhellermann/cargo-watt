use proc_macro::TokenStream;

extern crate proc_macro;

#[proc_macro]
pub fn my_macro(_input: TokenStream) -> proc_macro::TokenStream {
    let stream = quote::quote! {
        println!("{}", 42);
    };

    stream.into()
}

#[proc_macro]
pub fn my_macro1(_input: TokenStream) -> proc_macro::TokenStream {
    let stream = quote::quote! {
        println!("{}", 1337);
    };

    stream.into()
}

#[proc_macro_derive(DeriveMacro, attributes(test))]
pub fn derive_macro(_item: TokenStream) -> TokenStream {
    "fn answer() -> u32 { 42 }".parse().unwrap()
}

#[proc_macro_attribute]
pub fn attribute_macro_twice(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut stream = input.clone();
    stream.extend(input);
    stream
}
