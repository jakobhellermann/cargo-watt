use std::path::Path;
use walkdir::WalkDir;

pub fn copy_all(from: &Path, to: &Path) -> Result<(), anyhow::Error> {
    anyhow::ensure!(from.is_dir(), "from path should be a directory");
    if to.exists() {
        std::fs::remove_dir_all(&to)?;
    }

    let files = WalkDir::new(from);
    for file in files {
        let entry = file?;
        let file_type = entry.file_type();

        if file_type.is_symlink() || entry.path().components().any(|c| c.as_os_str() == ".git") {
            continue;
        }

        let new_file = entry
            .path()
            .components()
            .skip(1)
            .fold(to.to_path_buf(), |acc, item| acc.join(item));
        if file_type.is_dir() {
            std::fs::create_dir(new_file)?;
        } else {
            std::fs::copy(entry.path(), new_file)?;
        }
    }

    Ok(())
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

pub fn first_ident_parameter(f: &syn::ItemFn) -> Option<&syn::Ident> {
    f.sig
        .inputs
        .iter()
        .filter_map(|fn_arg| match fn_arg {
            syn::FnArg::Typed(t) => Some(t),
            _ => None,
        })
        .filter_map(|pat_type| match pat_type.pat.as_ref() {
            syn::Pat::Ident(ident) => Some(&ident.ident),
            _ => None,
        })
        .next()
}
