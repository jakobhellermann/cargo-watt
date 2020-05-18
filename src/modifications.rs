use crate::utils;

pub fn cargo_toml(input: &str) -> String {
    let mut toml = input.replace("proc-macro = true", "crate-type = [\"cdylib\"]");
    toml.push_str(
        "\n[patch.crates-io]\nproc-macro2 = { git = \"https://github.com/dtolnay/watt\" }",
    );
    toml
}

pub fn librs(input: &str) -> Result<String, anyhow::Error> {
    let mut parsed = syn::parse_str::<syn::File>(input)?;
    let proc_macro_fns = parsed
        .items
        .iter_mut()
        .filter_map(|item| match item {
            syn::Item::Fn(item_fn) => Some(item_fn),
            _ => None,
        })
        .filter_map(|item_fn| {
            let meta = item_fn
                .attrs
                .iter()
                .filter_map(|attr| attr.parse_meta().ok())
                .filter(|meta| match meta {
                    syn::Meta::Path(p) => {
                        p.get_ident().map_or(false, |ident| ident == "proc_macro")
                    }
                    _ => false,
                })
                .next()?;

            Some((item_fn, meta))
        });

    let c_abi: syn::Abi = syn::parse2(quote::quote!(extern "C"))?;
    let no_mangle = utils::parse_attributes(quote::quote!(#[no_mangle]))?;

    let mut new_fns = Vec::new();
    for (f, _meta) in proc_macro_fns {
        let first_arg = utils::first_ident_parameter(f);

        let fn_name_inner = syn::Ident::new(
            &format!("{}_inner", f.sig.ident.to_string()),
            proc_macro2::Span::call_site(),
        );

        let new_block: syn::Block = syn::parse2(quote::quote! { {
            #fn_name_inner(#first_arg.into()).into()
        }})?;

        let mut sig = f.sig.clone();
        let _old_attrs = std::mem::replace(&mut f.attrs, Vec::new());
        let vis = std::mem::replace(&mut f.vis, syn::Visibility::Inherited);

        f.sig.ident = fn_name_inner;

        sig.abi = Some(c_abi.clone());

        let new_fn = syn::Item::Fn(syn::ItemFn {
            attrs: no_mangle.clone(),
            vis,
            sig,
            block: Box::new(new_block),
        });

        new_fns.push(new_fn);
    }

    for new_fn in new_fns {
        parsed.items.push(new_fn);
    }

    Ok(quote::quote!(#parsed).to_string())
}
