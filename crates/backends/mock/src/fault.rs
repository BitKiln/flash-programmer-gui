use flash_core::error::FlashError;

/// Deterministic hardware and protocol failure modes for test simulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectedFault {
    /// Connection fails when attempting to attach to target or probe.
    ConnectFailure(String),
    /// Active connection drops or terminates unexpectedly.
    ConnectionLost(String),
    /// Erasing fails at or overlapping a specified memory address.
    EraseFailure { address: u32, message: String },
    /// Programming fails at a specific target address.
    ProgramFailure { address: u32, message: String },
    /// Programming fails at a specific target address (alias for ProgramFailure).
    ProgrammingFailed { address: u32, message: String },
    /// Programming fails after transferring a given threshold of bytes.
    ProgramFailureAfterBytes { byte_limit: usize, message: String },
    /// Verification corrupts the read value at a specific target address.
    VerificationCorruption { address: u32, corrupt_byte: u8 },
    /// Verification corrupts the read value at a specific target address (alias for VerificationCorruption).
    VerificationFailed { address: u32, corrupt_byte: u8 },
    /// Flash memory is marked as write-protected at a specific address.
    FlashProtected { address: u32 },
    /// Flash memory is marked as write-protected at a specific address (alias for FlashProtected).
    WriteProtected { address: u32 },
    /// Reset trigger fails (simulates floating NRST pin or hardware lockout).
    ResetFailure(String),
    /// Operation triggers a timeout.
    Timeout { operation: String, message: String },
}

/// Engine for injecting deterministic faults into mock probe sessions.
#[derive(Debug, Default, Clone)]
pub struct FaultInjector {
    faults: Vec<InjectedFault>,
    bytes_programmed_counter: usize,
}

impl FaultInjector {
    pub fn new() -> Self {
        Self {
            faults: Vec::new(),
            bytes_programmed_counter: 0,
        }
    }

    pub fn with_faults(faults: Vec<InjectedFault>) -> Self {
        Self {
            faults,
            bytes_programmed_counter: 0,
        }
    }

    pub fn inject(&mut self, fault: InjectedFault) {
        self.faults.push(fault);
    }

    pub fn clear(&mut self) {
        self.faults.clear();
        self.bytes_programmed_counter = 0;
    }

    /// Evaluates if connection should fail.
    pub fn check_connect(&self) -> Result<(), FlashError> {
        for fault in &self.faults {
            match fault {
                InjectedFault::ConnectFailure(msg) => {
                    return Err(FlashError::ConnectError(msg.clone()));
                }
                InjectedFault::ConnectionLost(msg) => {
                    return Err(FlashError::ConnectionLost(msg.clone()));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Evaluates if active connection has been lost.
    pub fn check_connection(&self) -> Result<(), FlashError> {
        for fault in &self.faults {
            if let InjectedFault::ConnectionLost(msg) = fault {
                return Err(FlashError::ConnectionLost(msg.clone()));
            }
        }
        Ok(())
    }

    /// Evaluates if erase at given address or sector should fail.
    pub fn check_erase(&self, address: u32) -> Result<(), FlashError> {
        self.check_connection()?;
        for fault in &self.faults {
            match fault {
                InjectedFault::EraseFailure {
                    address: target_addr,
                    message,
                } if *target_addr == address => {
                    return Err(FlashError::EraseError(format!(
                        "Erase failed at 0x{:08X}: {}",
                        address, message
                    )));
                }
                InjectedFault::FlashProtected {
                    address: target_addr,
                }
                | InjectedFault::WriteProtected {
                    address: target_addr,
                } if *target_addr == address => {
                    return Err(FlashError::FlashProtected { address });
                }
                InjectedFault::Timeout { operation, message } if operation == "erase" => {
                    return Err(FlashError::Timeout(format!("Erase timeout: {}", message)));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Evaluates whether a program chunk should fail.
    pub fn check_program_chunk(
        &mut self,
        address: u32,
        length: usize,
    ) -> Result<(), FlashError> {
        self.check_connection()?;
        for fault in &self.faults {
            match fault {
                InjectedFault::ProgramFailure {
                    address: target_addr,
                    message,
                }
                | InjectedFault::ProgrammingFailed {
                    address: target_addr,
                    message,
                } => {
                    let end = address.saturating_add(length as u32);
                    if *target_addr >= address && *target_addr < end {
                        return Err(FlashError::ProgramError(format!(
                            "Program failed at 0x{:08X}: {}",
                            target_addr, message
                        )));
                    }
                }
                InjectedFault::FlashProtected {
                    address: target_addr,
                }
                | InjectedFault::WriteProtected {
                    address: target_addr,
                } => {
                    let end = address.saturating_add(length as u32);
                    if *target_addr >= address && *target_addr < end {
                        return Err(FlashError::FlashProtected {
                            address: *target_addr,
                        });
                    }
                }
                InjectedFault::Timeout { operation, message } if operation == "program" => {
                    return Err(FlashError::Timeout(format!("Program timeout: {}", message)));
                }
                _ => {}
            }
        }

        self.bytes_programmed_counter += length;
        for fault in &self.faults {
            if let InjectedFault::ProgramFailureAfterBytes {
                byte_limit,
                message,
            } = fault
            {
                if self.bytes_programmed_counter >= *byte_limit {
                    return Err(FlashError::ProgramError(format!(
                        "Byte limit {} exceeded: {}",
                        byte_limit, message
                    )));
                }
            }
        }

        Ok(())
    }

    /// Applies corruptions to verified byte readings.
    pub fn maybe_corrupt_verify_byte(&self, address: u32, actual_byte: u8) -> u8 {
        for fault in &self.faults {
            match fault {
                InjectedFault::VerificationCorruption {
                    address: target_addr,
                    corrupt_byte,
                }
                | InjectedFault::VerificationFailed {
                    address: target_addr,
                    corrupt_byte,
                } if *target_addr == address => {
                    return *corrupt_byte;
                }
                _ => {}
            }
        }
        actual_byte
    }

    /// Evaluates if reset should fail.
    pub fn check_reset(&self) -> Result<(), FlashError> {
        self.check_connection()?;
        for fault in &self.faults {
            if let InjectedFault::ResetFailure(msg) = fault {
                return Err(FlashError::Internal(format!("Reset failed: {}", msg)));
            }
        }
        Ok(())
    }
}
