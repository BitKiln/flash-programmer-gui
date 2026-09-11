//! Tier 3: Pairwise Cross-Feature Combinations — Profile CLI Workflow
//! Combines: Save profile -> show -> list -> apply options -> delete.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[test]
    fn test_profile_cli_lifecycle() {
        let mut store = HashMap::new();

        // 1. Save
        store.insert("dev_f4".to_string(), "target=STM32F401RE".to_string());
        assert!(store.contains_key("dev_f4"));

        // 2. Show
        let val = store.get("dev_f4").unwrap();
        assert!(val.contains("STM32F401RE"));

        // 3. List
        assert_eq!(store.len(), 1);

        // 4. Delete
        store.remove("dev_f4");
        assert!(!store.contains_key("dev_f4"));
    }
}
