use crate::utils;
use std::path::Path;

pub fn make_modifications(path: &Path) -> Result<Vec<ProcMacroFn>, anyhow::Error> {
    let toml_path = path.join("Cargo.toml");
    let toml = std::fs::read_to_string(&toml_path)?;
    let new_toml = cargo_toml(&toml)?;
    std::fs::write(toml_path, new_toml)?;

    let lib_path = path.join("src").join("lib.rs");
    let lib = std::fs::read_to_string(&lib_path)?;
    let (fns, new_lib) = librs(&lib)?;
    std::fs::write(lib_path, new_lib)?;

    Ok(fns)
}

/// changes `proc-macro = true` to `crate-type = ["cdylib"]`
/// adds a patch for proc-macro2 to point to dtolnay's watt crate.
pub fn cargo_toml(input: &str) -> Result<String, anyhow::Error> {
    /*let mut manifest = cargo_toml::Manifest::from_str(input)?;
    if let Some(lib) = &mut manifest.lib {
        lib.proc_macro = false;
        lib.crate_type = vec!["cdylib".to_string()];
    }
    manifest
        .patch
        .entry("crates-io".into())
        .or_default()
        .insert(
            "proc-macro2".into(),
            cargo_toml::Dependency::Detailed(cargo_toml::DependencyDetail {
                git: Some("https://github.com/dtolnay/watt".into()),
                ..Default::default()
            }),
        );

    let toml = toml::to_string(&manifest)?;*/

    let mut toml = input.replace("proc-macro = true", "crate-type = [\"cdylib\"]");
    toml.push_str("\n[patch.crates-io]\n");
    toml.push_str("proc-macro2 = { git = \"https://github.com/dtolnay/watt\" }\n");
    toml.push_str("syn = { git = \"https://github.com/jakobhellermann/syn-watt\" }");

    Ok(toml)
}

pub struct ProcMacroFn {
    pub name: syn::Ident,
    pub attrs: Vec<syn::Attribute>,
    pub kind: ProcMacroKind,
}
pub enum ProcMacroKind {
    Macro,
    Derive,
    Attribute,
}
impl quote::ToTokens for ProcMacroFn {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.name;
        let mut new_fn: syn::ItemFn = match self.kind {
            ProcMacroKind::Macro => syn::parse_quote! {
                pub fn #ident(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
                    MACRO.proc_macro(stringify!(#ident), input)
                }
            },
            ProcMacroKind::Derive => syn::parse_quote! {
                pub fn #ident(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
                    MACRO.proc_macro_derive(stringify!(#ident), input)
                }
            },
            ProcMacroKind::Attribute => syn::parse_quote! {
                pub fn #ident(args: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
                    MACRO.proc_macro_attribute(stringify!(#ident), args, input)
                }
            },
        };
        new_fn.attrs = self.attrs.clone();
        tokens.extend(quote::quote! { #new_fn });
    }
}

pub fn librs(input: &str) -> Result<(Vec<ProcMacroFn>, String), anyhow::Error> {
    let mut file = syn::parse_str::<syn::File>(input)?;
    insert_allow_warnings(&mut file);

    let c_abi: syn::Abi = syn::parse_quote!(extern "C");
    let no_mangle = utils::parse_attributes(quote::quote!(#[no_mangle]))?;

    let mut fns = Vec::new();
    for (f, kind) in proc_macro_fns(&mut file) {
        // #[proc_macro]
        // pub fn my_macro(_input: TokenStream) -> proc_macro::TokenStream {
        //     ...
        // }
        // -->
        // #[no_mangle]
        // pub extern "C" fn my_macro(_input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        //    ...
        // }
        let old_attrs = std::mem::replace(&mut f.attrs, no_mangle.clone());
        f.sig.abi = Some(c_abi.clone());
        rename_tokenstream(&mut f.sig);

        fns.push(ProcMacroFn {
            name: f.sig.ident.clone(),
            attrs: old_attrs,
            kind,
        });
    }

    Ok((fns, quote::quote!(#file).to_string()))
}

fn proc_macro_fns(file: &mut syn::File) -> impl Iterator<Item = (&mut syn::ItemFn, ProcMacroKind)> {
    file.items
        .iter_mut()
        .filter_map(|item| match item {
            syn::Item::Fn(item_fn) => Some(item_fn),
            _ => None,
        })
        .filter_map(|item| match macro_kind(item) {
            Some(kind) => Some((item, kind)),
            _ => None,
        })
}

fn insert_allow_warnings(file: &mut syn::File) {
    let mut allow_warnings = utils::parse_attributes(quote::quote!(#[allow(warnings)]))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    allow_warnings.style = syn::AttrStyle::Inner(syn::parse_quote!(!));
    file.attrs.push(allow_warnings);
}

fn macro_kind(item: &syn::ItemFn) -> Option<ProcMacroKind> {
    item.attrs
        .iter()
        .filter_map(|attr| attr.parse_meta().ok())
        .filter_map(|meta| match meta {
            syn::Meta::Path(path) => match path.get_ident() {
                Some(ident) if ident == "proc_macro" => Some(ProcMacroKind::Macro),
                Some(ident) if ident == "proc_macro_attribute" => Some(ProcMacroKind::Attribute),
                _ => None,
            },
            syn::Meta::List(syn::MetaList { path, .. }) => match path.get_ident() {
                Some(ident) if ident == "proc_macro_derive" => Some(ProcMacroKind::Derive),
                _ => None,
            },
            _ => None,
        })
        .next()
}

fn rename_tokenstream(sig: &mut syn::Signature) {
    let token_stream: syn::Type = syn::parse_quote!(proc_macro2::TokenStream);

    for input in &mut sig.inputs {
        if let syn::FnArg::Typed(pat_type) = input {
            pat_type.ty = Box::new(token_stream.clone());
        }
    }

    sig.output = syn::ReturnType::Type(syn::parse_quote!(->), Box::new(token_stream));
}
