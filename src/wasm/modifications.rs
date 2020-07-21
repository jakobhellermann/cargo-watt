use std::path::Path;
use toml_edit::{value, Document, InlineTable, Item, Table};

pub fn make_modifications(path: &Path) -> Result<Vec<ProcMacroFn>, anyhow::Error> {
    let toml_path = path.join("Cargo.toml");
    let toml = std::fs::read_to_string(&toml_path)?;
    let new_toml = cargo_toml(&toml)?;
    std::fs::write(toml_path, new_toml)?;

    let lib_path = path.join("src").join("lib.rs");
    let lib = std::fs::read_to_string(&lib_path)?;
    let (fns, new_lib) = librs(&lib)?;
    std::fs::write(lib_path, new_lib)?;

    let lock = path.join("Cargo.lock");
    if lock.exists() {
        std::fs::remove_file(lock)?;
    }

    dump_replace(path)?;

    Ok(fns)
}

fn dump_replace(directory: &Path) -> Result<(), std::io::Error> {
    let src_files = walkdir::WalkDir::new(directory.join("src"));
    for file in src_files {
        let file = file?;
        if !file.file_type().is_file() {
            continue;
        };

        let mut contents = std::fs::read_to_string(file.path())?;
        let mut changed = false;

        // TODO make this more efficient
        let from = ["proc_macro ::", "proc_macro::"];
        let to = "proc_macro2 ::";

        for needle in &from {
            if contents.contains(needle) {
                contents = contents.replace(needle, to);
                changed = true;
            }
        }

        if changed {
            std::fs::write(file.path(), contents)?;
        }
    }

    Ok(())
}

fn dependency(kind: &str, dep: &str) -> InlineTable {
    let mut table = InlineTable::default();
    table.get_or_insert(kind, dep);
    table
}

// returns the (possibly just generated) [patch.crates.io] section
fn implicit_table<'a>(manifest: &'a mut toml_edit::Document, a: &str, b: &str) -> &'a mut Table {
    let mut a_table = Table::new();
    a_table.set_implicit(true);

    let patch = manifest[a]
        .or_insert(Item::Table(a_table))
        .as_table_mut()
        .unwrap();

    let mut b_table = Table::new();
    b_table.set_implicit(true);

    patch[b]
        .or_insert(Item::Table(b_table))
        .as_table_mut()
        .unwrap()
}

const PATCHES: &[(&str, &str, &str)] = &[
    ("git", "proc-macro2", "https://github.com/dtolnay/watt"),
    ("git", "syn", "https://github.com/jakobhellermann/syn"),
];

/// changes `proc-macro = true` to `crate-type = ["cdylib"]`
/// adds a patch for proc-macro2 to point to dtolnay's watt crate.
pub fn cargo_toml(input: &str) -> Result<String, anyhow::Error> {
    let mut manifest: Document = input.parse()?;
    manifest["lib"]["proc-macro"] = value(false);

    let mut cdylib = toml_edit::Array::default();
    cdylib
        .push("cdylib")
        .map_err(|e| anyhow::anyhow!("crate-type array contains non-string type: {}", e))?;
    manifest["lib"]["crate-type"] = value(cdylib);

    let release_profile = implicit_table(&mut manifest, "profile", "release");
    release_profile["codegen-units"] = value(1);
    release_profile["opt-level"] = value("s");

    // ensure dependencies contain proc_macro so that we can patch it
    manifest["dependencies"]["proc-macro2"].or_insert(value("1.0"));

    let patch = implicit_table(&mut manifest, "patch", "crates-io");
    for (kind, patched_crate, dep) in PATCHES {
        patch[patched_crate] = value(dependency(kind, dep));
    }

    Ok(manifest.to_string_in_original_order())
}

pub struct ProcMacroFn {
    pub name: syn::Ident,
    pub attrs: Vec<syn::Attribute>,
    pub kind: ProcMacroKind,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ProcMacroKind {
    Macro,
    Derive,
    Attribute,
    ProcMacroHack,
}
impl quote::ToTokens for ProcMacroFn {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let ident = &self.name;
        let mut new_fn: syn::ItemFn = match self.kind {
            ProcMacroKind::Macro | ProcMacroKind::ProcMacroHack => syn::parse_quote! {
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
    remove_extern_procmacro(&mut file);

    let c_abi: syn::Abi = syn::parse_quote!(extern "C");
    let no_mangle = parse_attributes(quote::quote!(#[no_mangle]))?;

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
    let mut allow_warnings = parse_attributes(quote::quote!(#[allow(warnings)]))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    allow_warnings.style = syn::AttrStyle::Inner(syn::parse_quote!(!));
    file.attrs.push(allow_warnings);
}

fn remove_extern_procmacro(file: &mut syn::File) {
    file.items.retain(|item| match item {
        syn::Item::ExternCrate(extern_crate) => extern_crate.ident != "proc_macro",
        _ => true,
    })
}

fn macro_kind(item: &syn::ItemFn) -> Option<ProcMacroKind> {
    item.attrs
        .iter()
        .filter_map(|attr| attr.parse_meta().ok())
        .filter_map(|meta| macro_meta(&meta))
        .next()
}
/// either #[proc_macro], #[proc_macro_derive] or #[proc_macro_attribute].
/// can also be #[cfg_attr(..., *one of the above*)]
fn macro_meta(meta: &syn::Meta) -> Option<ProcMacroKind> {
    match meta {
        syn::Meta::Path(path) => match path.get_ident() {
            Some(ident) if ident == "proc_macro" => Some(ProcMacroKind::Macro),
            Some(ident) if ident == "proc_macro_attribute" => Some(ProcMacroKind::Attribute),
            Some(ident) if ident == "proc_macro_hack" => Some(ProcMacroKind::ProcMacroHack),
            _ => None,
        },
        syn::Meta::List(syn::MetaList { path, nested, .. }) => match path.get_ident() {
            Some(ident) if ident == "proc_macro_derive" => Some(ProcMacroKind::Derive),
            Some(ident) if ident == "cfg_attr" => {
                let cfg_meta = nested.iter().nth(1)?; // nth(0) is cfg condition
                match cfg_meta {
                    syn::NestedMeta::Meta(meta) => macro_meta(meta),
                    _ => None,
                }
            }
            _ => None,
        },
        _ => None,
    }
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

pub fn parse_attributes(
    token_stream: proc_macro2::TokenStream,
) -> syn::Result<Vec<syn::Attribute>> {
    struct AttrParser(Vec<syn::Attribute>);
    impl syn::parse::Parse for AttrParser {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            Ok(AttrParser(input.call(syn::Attribute::parse_outer)?))
        }
    }

    let AttrParser(attrs) = syn::parse2(token_stream)?;
    Ok(attrs)
}
