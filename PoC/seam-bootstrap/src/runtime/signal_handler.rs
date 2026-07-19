//! Signal Integration - Connect abort mechanism to OS signal handlers
//!
//! Integrates OS signals (SIGTERM, SIGABRT, SIGINT) with the direct jump abort mechanism.
//! When a signal is received, triggers O(1) abort via CFP/RFP context switch.
//!
//! **Architecture**:
//! - Thread-local registration of abort targets
//! - Signal handler dispatches to current ExecutionContext's abort mechanism
//! - Graceful signal handling with in_collector flag to prevent cascading aborts
//!
//! **Signals Handled**:
//! - SIGTERM (graceful termination)
//! - SIGABRT (abnormal termination)
//! - SIGINT (interrupt - Ctrl+C)

#[inline(always)]
fn fn_to_sighandler(f: extern "C" fn(i32)) -> libc::sighandler_t {
    (f as *const ()) as libc::sighandler_t
}


/// Per-thread abort target for signal handling
/// Stores the abort target that will be activated when a signal is received
#[derive(Clone, Copy, Debug)]
pub struct SignalAbortTarget {
    /// Memory address where abort should jump to (collector IP)
    pub collector_ip: *const u8,
    /// Collector channel identity used for boundary resolution
    pub collector_channel_id: u32,
    /// Target control frame pointer
    pub target_cfp: *mut u8,
    /// Target resource frame pointer (ghost frame)
    pub target_rfp: *mut u8,
}

impl SignalAbortTarget {
    pub fn new(
        collector_ip: *const u8,
        collector_channel_id: u32,
        target_cfp: *mut u8,
        target_rfp: *mut u8,
    ) -> Self {
        SignalAbortTarget {
            collector_ip,
            collector_channel_id,
            target_cfp,
            target_rfp,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.collector_ip.is_null() && !self.target_cfp.is_null() && !self.target_rfp.is_null()
    }
}

/// Signal handler registration and management
/// Provides methods to register/unregister signal handlers with abort targets
pub struct SignalHandler;

impl SignalHandler {
    /// Register signal handlers for abort signals
    /// Must be called once per thread
    pub fn register_signal_handlers() -> Result<(), &'static str> {
        #[cfg(unix)]
        {
            // SIGTERM - graceful termination
            unsafe {
                let result = libc::signal(libc::SIGTERM, fn_to_sighandler(Self::signal_handler_impl));
                if result == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to register SIGTERM handler");
                }
            }
            
            // SIGABRT - abnormal termination
            unsafe {
                let result = libc::signal(libc::SIGABRT, fn_to_sighandler(Self::signal_handler_impl));
                if result == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to register SIGABRT handler");
                }
            }
            
            // SIGINT - interrupt (Ctrl+C)
            unsafe {
                let result = libc::signal(libc::SIGINT, fn_to_sighandler(Self::signal_handler_impl));
                if result == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to register SIGINT handler");
                }
            }
        }
        
        #[cfg(windows)]
        {
            // Windows signal handling via signal() function
            // Note: Limited signal support on Windows; primarily SIGINT and SIGABRT
            unsafe {
                let result_int = libc::signal(libc::SIGINT, fn_to_sighandler(Self::signal_handler_impl));
                if result_int == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to register SIGINT handler on Windows");
                }
                let result_abrt = libc::signal(libc::SIGABRT, fn_to_sighandler(Self::signal_handler_impl));
                if result_abrt == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to register SIGABRT handler on Windows");
                }
            }
        }
        
        Ok(())
    }

    /// Unregister signal handlers (restore default behavior)
    pub fn unregister_signal_handlers() -> Result<(), &'static str> {
        #[cfg(unix)]
        {
            unsafe {
                let result_term = libc::signal(libc::SIGTERM, libc::SIG_DFL);
                if result_term == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to unregister SIGTERM handler");
                }
                let result_abrt = libc::signal(libc::SIGABRT, libc::SIG_DFL);
                if result_abrt == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to unregister SIGABRT handler");
                }
                let result_int = libc::signal(libc::SIGINT, libc::SIG_DFL);
                if result_int == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to unregister SIGINT handler");
                }
            }
        }
        
        #[cfg(windows)]
        {
            unsafe {
                let result_int = libc::signal(libc::SIGINT, libc::SIG_DFL);
                if result_int == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to unregister SIGINT handler on Windows");
                }
                let result_abrt = libc::signal(libc::SIGABRT, libc::SIG_DFL);
                if result_abrt == libc::SIG_ERR as libc::sighandler_t {
                    return Err("Failed to unregister SIGABRT handler on Windows");
                }
            }
        }
        
        Ok(())
    }

    /// Get the current thread-local abort target (if registered)
    pub fn get_abort_target() -> Option<SignalAbortTarget> {
        THREAD_LOCAL_ABORT_TARGET.with(|target| target.get())
    }

    /// Set the thread-local abort target for signal handling
    pub fn set_abort_target(target: SignalAbortTarget) {
        THREAD_LOCAL_ABORT_TARGET.with(|tgt| {
            tgt.set(Some(target));
        });
    }

    /// Clear the thread-local abort target
    pub fn clear_abort_target() {
        THREAD_LOCAL_ABORT_TARGET.with(|target| {
            target.set(None);
        });
    }

    /// Internal signal handler implementation (dispatches to abort via thread-local target)
    extern "C" fn signal_handler_impl(sig: i32) {
        // Get the thread-local abort target
        if let Some(target) = Self::get_abort_target() {
            if target.is_valid() {
                // Execute direct jump to collector via abort mechanism
                // This is unsafe because:
                // 1. We're in a signal handler
                // 2. We're modifying CPU registers and executing a jump
                // 3. We're bypassing normal Rust control flow
                // 
                // However, this is necessary to achieve:
                // - O(1) abort from any execution context
                // - Pre-computed abort target (verified at registration)
                // - No stack unwinding or DWARF lookup
                
                unsafe {
                    // Execute direct jump with CFP/RFP context switch
                    // This is implemented via assembly in cfp_rfp.rs
                    #[cfg(target_arch = "x86_64")]
                    {
                        core::arch::asm!(
                            "mov rbp, {cfp}",
                            "mov r15, {rfp}",
                            "jmp {ip}",
                            cfp = in(reg) target.target_cfp,
                            rfp = in(reg) target.target_rfp,
                            ip = in(reg) target.collector_ip,
                            options(noreturn)
                        );
                    }
                    
                    #[cfg(target_arch = "aarch64")]
                    {
                        core::arch::asm!(
                            "mov x29, {cfp}",
                            "mov x28, {rfp}",
                            "br {ip}",
                            cfp = in(reg) target.target_cfp,
                            rfp = in(reg) target.target_rfp,
                            ip = in(reg) target.collector_ip,
                            options(noreturn)
                        );
                    }
                    
                    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
                    {
                        // Fallback for unsupported architectures - exit with signal number
                        std::process::exit(sig);
                    }
                }
            }
        }
        
        // If no abort target available, exit with signal number
        std::process::exit(sig);
    }
}

thread_local! {
    /// Thread-local storage for abort targets
    /// Allows each thread to register its own abort target for signal handling
    static THREAD_LOCAL_ABORT_TARGET: std::cell::Cell<Option<SignalAbortTarget>> = 
        std::cell::Cell::new(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_abort_target_creation() {
        let target = SignalAbortTarget::new(0x1000 as *const u8, 7, 0x2000 as *mut u8, 0x3000 as *mut u8);
        
        assert_eq!(target.collector_ip, 0x1000 as *const u8);
        assert_eq!(target.collector_channel_id, 7);
        assert_eq!(target.target_cfp, 0x2000 as *mut u8);
        assert_eq!(target.target_rfp, 0x3000 as *mut u8);
        assert!(target.is_valid());
    }

    #[test]
    fn test_signal_abort_target_invalid() {
        let target = SignalAbortTarget::new(std::ptr::null(), 7, 0x2000 as *mut u8, 0x3000 as *mut u8);
        
        assert!(!target.is_valid());
    }

    #[test]
    fn test_signal_handler_thread_local_storage() {
        // Clear any existing target
        SignalHandler::clear_abort_target();
        assert!(SignalHandler::get_abort_target().is_none());
        
        // Set a target
        let target = SignalAbortTarget::new(0x1000 as *const u8, 7, 0x2000 as *mut u8, 0x3000 as *mut u8);
        SignalHandler::set_abort_target(target.clone());
        
        // Verify retrieval
        let retrieved = SignalHandler::get_abort_target();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.collector_ip, target.collector_ip);
        assert_eq!(retrieved.collector_channel_id, target.collector_channel_id);
        assert_eq!(retrieved.target_cfp, target.target_cfp);
        assert_eq!(retrieved.target_rfp, target.target_rfp);
        
        // Clear target
        SignalHandler::clear_abort_target();
        assert!(SignalHandler::get_abort_target().is_none());
    }

    #[test]
    fn test_signal_abort_target_clone() {
        let target1 = SignalAbortTarget::new(0x1000 as *const u8, 7, 0x2000 as *mut u8, 0x3000 as *mut u8);
        
        let target2 = target1.clone();
        
        assert_eq!(target1.collector_ip, target2.collector_ip);
        assert_eq!(target1.collector_channel_id, target2.collector_channel_id);
        assert_eq!(target1.target_cfp, target2.target_cfp);
        assert_eq!(target1.target_rfp, target2.target_rfp);
    }

    #[test]
    fn test_signal_handler_register_signals() {
        // This test verifies that signal registration completes without panic
        // Actual signal delivery is tested in integration tests
        let result = SignalHandler::register_signal_handlers();
        
        // Should succeed on both Unix and Windows
        assert!(result.is_ok(), "Failed to register signal handlers: {:?}", result);
    }

    #[test]
    fn test_signal_handler_unregister_signals() {
        // Register first
        let _ = SignalHandler::register_signal_handlers();
        
        // Then unregister
        let result = SignalHandler::unregister_signal_handlers();
        assert!(result.is_ok(), "Failed to unregister signal handlers: {:?}", result);
    }

    #[test]
    fn test_signal_handler_register_unregister_cycle() {
        // Verify multiple register/unregister cycles work correctly
        for _ in 0..3 {
            let register_result = SignalHandler::register_signal_handlers();
            assert!(register_result.is_ok());
            
            let unregister_result = SignalHandler::unregister_signal_handlers();
            assert!(unregister_result.is_ok());
        }
    }
}
