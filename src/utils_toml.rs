use toml_edit::{Document, InlineTable, Item, Table};

// returns the (possibly just generated) [patch.crates.io] section
pub fn implicit_table<'a>(manifest: &'a mut Document, a: &str, b: &str) -> &'a mut Table {
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

pub fn dependency(kind: &str, dep: &str) -> InlineTable {
    let mut table = InlineTable::default();
    table.get_or_insert(kind, dep);
    table
}
