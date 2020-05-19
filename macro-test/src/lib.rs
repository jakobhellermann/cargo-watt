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
