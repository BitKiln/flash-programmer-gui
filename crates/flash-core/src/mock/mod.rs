pub mod backend;
pub mod fault;
pub mod memory;
pub mod profiles;

pub use backend::{MockFlashSession, MockProbeBackend};
pub use fault::{FaultInjector, InjectedFault};
pub use memory::MockFlashMemory;
pub use profiles::{
    generic_cortex_m, get_target_by_name, stm32f103c8, stm32f103rb, stm32f401re, stm32f411ce,
};
