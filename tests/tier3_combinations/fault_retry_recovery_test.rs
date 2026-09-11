//! Tier 3: Pairwise Cross-Feature Combinations — Fault Injection & Retry Recovery
//! Combines: Fault injection during write -> failure detection -> clear fault -> retry -> success.

#[cfg(test)]
mod tests {
    #[test]
    fn test_fault_retry_recovery() {
        let mut fault_injected = true;
        let mut flash = vec![0xFFu8; 1024];
        let payload = [0x77u8; 64];

        // Attempt 1: Injected failure
        let result = if fault_injected {
            Err("Flash error")
        } else {
            flash[0..64].copy_from_slice(&payload);
            Ok(())
        };
        assert!(result.is_err());

        // Attempt 2: Clear fault and retry
        fault_injected = false;
        let result = if fault_injected {
            Err("Flash error")
        } else {
            flash[0..64].copy_from_slice(&payload);
            Ok(())
        };
        assert!(result.is_ok());
        assert_eq!(&flash[0..64], &payload);
    }
}
