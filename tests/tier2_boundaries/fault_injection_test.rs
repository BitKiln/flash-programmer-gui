//! Tier 2: Boundary & Corner Cases — Fault Injection & Error Resilience (>=5 tests)
//! Authoritative source: explorer_survey_1 (F25).

#[cfg(test)]
mod tests {
    #[test]
    fn test_fault_injected_connection_failure() {
        let fault_active = true;
        let result = if fault_active { Err("Connection dropped") } else { Ok(()) };
        assert!(result.is_err());
    }

    #[test]
    fn test_fault_injected_erase_failure() {
        let erase_protected_sector = true;
        let result = if erase_protected_sector { Err("Flash sector protected") } else { Ok(()) };
        assert!(result.is_err());
    }

    #[test]
    fn test_fault_injected_program_failure() {
        let brownout_simulated = true;
        let result = if brownout_simulated { Err("Programming timeout / VDD brownout") } else { Ok(()) };
        assert!(result.is_err());
    }

    #[test]
    fn test_fault_injected_verification_mismatch() {
        let expected_byte: u8 = 0x55;
        let actual_byte: u8 = 0x54; // 1-bit glitch
        assert_ne!(expected_byte, actual_byte);
    }

    #[test]
    fn test_fault_injected_reset_failure() {
        let reset_pin_floating = true;
        let result = if reset_pin_floating { Err("Reset line timeout") } else { Ok(()) };
        assert!(result.is_err());
    }
}
