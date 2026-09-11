//! Lists the chips this build can program.
//!
//! The list comes from the probe-rs target registry compiled into the binary,
//! not from any table in this repo.
//!
//! Usage:
//!   cargo run -p flash-core --example list_targets            # summary per family
//!   cargo run -p flash-core --example list_targets -- stm32u5 # variants matching a prefix
use probe_rs::config::Registry;

fn main() {
    let registry = Registry::from_builtin_families();
    let filter = std::env::args().nth(1);

    match filter {
        Some(query) => {
            let matches = registry.search_chips(&query);
            println!("{} chips match '{}':", matches.len(), query);
            for name in matches {
                println!("  {name}");
            }
        }
        None => {
            let families = registry.families();
            let chips: usize = families.iter().map(|f| f.variants.len()).sum();
            println!("{} families, {} chip variants\n", families.len(), chips);
            for family in families {
                println!("{:<45} {:>4} variants", family.name, family.variants.len());
            }
        }
    }
}
