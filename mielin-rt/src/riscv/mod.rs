//! RISC-V runtime support for IMAC and GC variants.
//!
//! Provides bare-metal runtime abstractions for RISC-V embedded systems,
//! covering the CLINT (Core Local Interruptor), PLIC (Platform-Level Interrupt
//! Controller), privilege levels, and RISC-V ISA variant detection.
//!
//! ## Supported ISA Variants
//!
//! - `Rv32Imac`: 32-bit base ISA + Multiply + Atomic + Compressed (most common embedded)
//! - `Rv32Gc`: 32-bit with G=IMAFD + C
//! - `Rv64Imac`: 64-bit IMAC (Linux-capable embedded)
//! - `Rv64Gc`: 64-bit GCIMAFDC (full Linux platform)

pub mod rv32imac;
pub mod rv64imac;

use core::fmt;

// ---------------------------------------------------------------------------
// ISA Variant
// ---------------------------------------------------------------------------

/// RISC-V ISA variant classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiscvVariant {
    /// 32-bit base ISA + Multiply + Atomic + Compressed — most common embedded MCU target
    Rv32Imac,
    /// 32-bit with G extension (= IMAFD) plus Compressed — full soft-float embedded
    Rv32Gc,
    /// 64-bit IMAC — smallest 64-bit Linux-capable embedded profile
    Rv64Imac,
    /// 64-bit GCIMAFDC — full Linux platform (e.g., VisionFive 2, SiFive U74)
    Rv64Gc,
}

impl RiscvVariant {
    /// Returns `true` if this is a 32-bit variant
    pub fn is_32bit(self) -> bool {
        matches!(self, RiscvVariant::Rv32Imac | RiscvVariant::Rv32Gc)
    }

    /// Returns `true` if this is a 64-bit variant
    pub fn is_64bit(self) -> bool {
        matches!(self, RiscvVariant::Rv64Imac | RiscvVariant::Rv64Gc)
    }

    /// Returns `true` if this variant has the Atomic ('A') extension
    pub fn has_atomic(self) -> bool {
        // All listed variants include the 'A' extension
        true
    }

    /// Returns `true` if this variant has the Compressed ('C') extension
    pub fn has_compressed(self) -> bool {
        // All listed variants include the 'C' extension
        true
    }

    /// Returns `true` if this variant has the Multiply ('M') extension
    pub fn has_multiply(self) -> bool {
        true
    }

    /// Returns `true` if this variant has the single-precision FP ('F') extension
    pub fn has_float(self) -> bool {
        matches!(self, RiscvVariant::Rv32Gc | RiscvVariant::Rv64Gc)
    }

    /// Returns `true` if this variant has the double-precision FP ('D') extension
    pub fn has_double(self) -> bool {
        matches!(self, RiscvVariant::Rv32Gc | RiscvVariant::Rv64Gc)
    }

    /// XLEN — the native integer register width in bits
    pub fn xlen(self) -> u8 {
        match self {
            RiscvVariant::Rv32Imac | RiscvVariant::Rv32Gc => 32,
            RiscvVariant::Rv64Imac | RiscvVariant::Rv64Gc => 64,
        }
    }
}

impl fmt::Display for RiscvVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiscvVariant::Rv32Imac => write!(f, "RV32IMAC"),
            RiscvVariant::Rv32Gc => write!(f, "RV32GC"),
            RiscvVariant::Rv64Imac => write!(f, "RV64IMAC"),
            RiscvVariant::Rv64Gc => write!(f, "RV64GC"),
        }
    }
}

// ---------------------------------------------------------------------------
// Privilege Level
// ---------------------------------------------------------------------------

/// RISC-V hardware privilege level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivilegeLevel {
    /// U-mode: unprivileged user-space applications
    User,
    /// S-mode: supervisor (OS kernel) — requires MMU support
    Supervisor,
    /// M-mode: machine mode, bare metal, highest privilege
    Machine,
}

impl fmt::Display for PrivilegeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrivilegeLevel::User => write!(f, "U-mode"),
            PrivilegeLevel::Supervisor => write!(f, "S-mode"),
            PrivilegeLevel::Machine => write!(f, "M-mode"),
        }
    }
}

// ---------------------------------------------------------------------------
// Interrupt Cause (mcause)
// ---------------------------------------------------------------------------

/// RISC-V interrupt cause decoded from the `mcause` CSR.
///
/// When bit 63 of `mcause` is set the entry is an interrupt; otherwise it is
/// an exception (trap).  This enum covers only the interrupt side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptCause {
    /// Interrupt 3 — Machine Software Interrupt (via CLINT msip)
    MachineSoftware,
    /// Interrupt 7 — Machine Timer Interrupt (via CLINT mtime/mtimecmp)
    MachineTimer,
    /// Interrupt 11 — Machine External Interrupt (via PLIC)
    MachineExternal,
    /// Interrupt 1 in S-mode — Supervisor Software Interrupt
    SupervisorSoftware,
    /// Interrupt 5 in S-mode — Supervisor Timer Interrupt
    SupervisorTimer,
    /// Interrupt 9 — Supervisor External Interrupt
    SupervisorExternal,
    /// Any other cause code not covered by the standard spec
    Unknown(u64),
}

impl From<u64> for InterruptCause {
    /// Decode an `mcause` register value into an [`InterruptCause`].
    ///
    /// The highest bit indicates an interrupt (vs exception).  The lower
    /// bits give the interrupt number per the RISC-V Privileged Spec §3.1.15.
    fn from(mcause: u64) -> Self {
        // Bit 63 set → interrupt; clear → exception (not handled here)
        let is_interrupt = mcause >> 63 != 0;
        let cause_code = mcause & !(1u64 << 63);

        if !is_interrupt {
            return InterruptCause::Unknown(mcause);
        }

        match cause_code {
            1 => InterruptCause::SupervisorSoftware,
            3 => InterruptCause::MachineSoftware,
            5 => InterruptCause::SupervisorTimer,
            7 => InterruptCause::MachineTimer,
            9 => InterruptCause::SupervisorExternal,
            11 => InterruptCause::MachineExternal,
            other => InterruptCause::Unknown(other),
        }
    }
}

impl fmt::Display for InterruptCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InterruptCause::MachineSoftware => write!(f, "Machine Software Interrupt"),
            InterruptCause::MachineTimer => write!(f, "Machine Timer Interrupt"),
            InterruptCause::MachineExternal => write!(f, "Machine External Interrupt"),
            InterruptCause::SupervisorSoftware => write!(f, "Supervisor Software Interrupt"),
            InterruptCause::SupervisorTimer => write!(f, "Supervisor Timer Interrupt"),
            InterruptCause::SupervisorExternal => write!(f, "Supervisor External Interrupt"),
            InterruptCause::Unknown(v) => write!(f, "Unknown Cause({v:#x})"),
        }
    }
}

// ---------------------------------------------------------------------------
// CLINT Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Core Local Interruptor (CLINT) memory-mapped peripheral.
///
/// The CLINT provides machine-mode software interrupts and the `mtime` /
/// `mtimecmp` registers used for timer interrupts.
///
/// Memory layout (SiFive standard):
/// ```text
/// base + 0x0000 .. 0x3FFF   msip[hart] registers  (4 bytes each)
/// base + 0x4000 .. 0xBFF7   mtimecmp[hart]         (8 bytes each)
/// base + 0xBFF8              mtime                  (8 bytes)
/// ```
#[derive(Debug, Clone)]
pub struct ClintConfig {
    /// MMIO base address of the CLINT
    pub base_addr: usize,
    /// Byte offset of the `mtime` register from base (default 0xBFF8)
    pub mtime_offset: usize,
    /// Byte offset of the `mtimecmp` array from base (default 0x4000)
    pub mtimecmp_offset: usize,
    /// Number of hardware threads served by this CLINT
    pub num_harts: usize,
}

impl ClintConfig {
    /// Build a CLINT config using the canonical SiFive CLINT memory layout.
    ///
    /// SiFive Freedom SDK and most open-source RISC-V SoCs follow this layout.
    pub fn sifive(base: usize) -> Self {
        Self {
            base_addr: base,
            mtime_offset: 0xBFF8,
            mtimecmp_offset: 0x4000,
            num_harts: 1,
        }
    }

    /// Build a CLINT config for the QEMU `virt` machine.
    ///
    /// The QEMU virt machine places its CLINT at 0x0200_0000 and supports
    /// up to 8 virtual harts.
    pub fn virt(base: usize) -> Self {
        Self {
            base_addr: base,
            mtime_offset: 0xBFF8,
            mtimecmp_offset: 0x4000,
            num_harts: 8,
        }
    }

    /// Override the hart count (builder-style).
    pub fn with_num_harts(mut self, n: usize) -> Self {
        self.num_harts = n;
        self
    }

    /// Address of the `mtime` register
    pub fn mtime_addr(&self) -> usize {
        self.base_addr + self.mtime_offset
    }

    /// Address of `mtimecmp` for the given hart index
    pub fn mtimecmp_addr(&self, hart: usize) -> usize {
        self.base_addr + self.mtimecmp_offset + hart * 8
    }

    /// Address of the `msip` register for the given hart index
    pub fn msip_addr(&self, hart: usize) -> usize {
        self.base_addr + hart * 4
    }
}

// ---------------------------------------------------------------------------
// PLIC Configuration
// ---------------------------------------------------------------------------

/// Configuration for the Platform-Level Interrupt Controller (PLIC).
///
/// The PLIC arbitrates between all external interrupt sources (GPIO, UART,
/// Ethernet, etc.) and delivers them to harts at their configured priority
/// threshold.
///
/// Standard PLIC memory map (based on SiFive/RISC-V spec):
/// ```text
/// base + 0x000000   source priority[1..N]  (4 bytes each)
/// base + 0x001000   pending bits            (1 bit per source)
/// base + 0x002000   enable bits[context]    (1 bit per source per context)
/// base + 0x200000   threshold[context]      (4 bytes per context)
/// base + 0x200004   claim/complete[context] (4 bytes per context)
/// ```
#[derive(Debug, Clone)]
pub struct PlicConfig {
    /// MMIO base address of the PLIC
    pub base_addr: usize,
    /// Total number of interrupt sources (excluding source 0 which is reserved)
    pub num_sources: usize,
    /// Number of contexts (typically 2 per hart: M-mode + S-mode)
    pub num_contexts: usize,
}

impl PlicConfig {
    /// Standard PLIC configuration with 128 sources and 2 contexts
    pub fn new(base_addr: usize) -> Self {
        Self {
            base_addr,
            num_sources: 128,
            num_contexts: 2,
        }
    }

    /// PLIC for the QEMU `virt` machine (base 0x0C00_0000, 96 sources, 2*8 contexts)
    pub fn qemu_virt() -> Self {
        Self {
            base_addr: 0x0C00_0000,
            num_sources: 96,
            num_contexts: 16, // 2 contexts × 8 harts
        }
    }

    /// Address of the priority register for `irq` (1-based source index)
    pub fn priority_addr(&self, irq: u32) -> usize {
        self.base_addr + irq as usize * 4
    }

    /// Base address of the enable bits for `context`
    pub fn enable_base(&self, context: usize) -> usize {
        self.base_addr + 0x2000 + context * 0x80
    }

    /// Address of the claim/complete register for `context`
    pub fn claim_addr(&self, context: usize) -> usize {
        self.base_addr + 0x20_0004 + context * 0x1000
    }

    /// Address of the threshold register for `context`
    pub fn threshold_addr(&self, context: usize) -> usize {
        self.base_addr + 0x20_0000 + context * 0x1000
    }
}

// ---------------------------------------------------------------------------
// RISC-V Runtime Error
// ---------------------------------------------------------------------------

/// Errors produced by the RISC-V runtime
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiscvError {
    /// A CLINT operation was attempted but no CLINT has been configured
    ClintNotConfigured,
    /// A PLIC operation was attempted but no PLIC has been configured
    PlicNotConfigured,
    /// The specified hart index exceeds the number of harts supported by the CLINT
    InvalidHart { hart: usize, max_harts: usize },
    /// The specified IRQ number exceeds the number of sources in the PLIC
    InvalidIrq { irq: u32, max_sources: u32 },
    /// An address is insufficiently aligned for the required access width
    AlignmentError { addr: usize, required: usize },
    /// The operation requires a privilege level that has not been configured
    UnsupportedPrivilegeLevel,
}

impl fmt::Display for RiscvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiscvError::ClintNotConfigured => {
                write!(f, "RISC-V CLINT not configured — call with_clint() first")
            }
            RiscvError::PlicNotConfigured => {
                write!(f, "RISC-V PLIC not configured — call with_plic() first")
            }
            RiscvError::InvalidHart { hart, max_harts } => {
                write!(f, "Invalid hart index {hart} (max supported: {max_harts})")
            }
            RiscvError::InvalidIrq { irq, max_sources } => {
                write!(f, "Invalid IRQ {irq} (max sources: {max_sources})")
            }
            RiscvError::AlignmentError { addr, required } => {
                write!(
                    f,
                    "Address {addr:#x} violates required alignment of {required} bytes"
                )
            }
            RiscvError::UnsupportedPrivilegeLevel => {
                write!(
                    f,
                    "Required privilege level is not available on this platform"
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RISC-V Runtime
// ---------------------------------------------------------------------------

/// Bare-metal RISC-V runtime manager.
///
/// Encapsulates the ISA variant, current privilege level, CLINT, and PLIC
/// configuration needed to manage interrupts, timers, and system state on
/// a RISC-V SoC.
///
/// This type is intentionally platform-agnostic and `no_std`-safe; all
/// hardware register accesses use raw pointer operations guarded by target
/// feature detection so that unit tests run on the host.
pub struct RiscvRuntime {
    /// ISA variant this runtime is targeting
    pub variant: RiscvVariant,
    /// Current privilege level (M/S/U)
    pub privilege: PrivilegeLevel,
    /// Hart index this runtime instance manages
    pub hart_id: usize,
    /// Optional CLINT peripheral configuration
    pub clint: Option<ClintConfig>,
    /// Optional PLIC peripheral configuration
    pub plic: Option<PlicConfig>,
    /// Reference frequency of the `mtime` counter in Hz
    ///
    /// Typical values: 10 MHz (SiFive) or 32 768 Hz (low-power RTC CLINT)
    pub mtime_freq_hz: u64,
}

impl RiscvRuntime {
    /// Construct a new runtime for the given ISA variant.
    ///
    /// Defaults to M-mode, hart 0, 10 MHz mtime, no CLINT/PLIC configured.
    pub fn new(variant: RiscvVariant) -> Self {
        Self {
            variant,
            privilege: PrivilegeLevel::Machine,
            hart_id: 0,
            clint: None,
            plic: None,
            mtime_freq_hz: 10_000_000, // 10 MHz — SiFive/OpenSBI default
        }
    }

    /// Builder: attach a CLINT configuration
    pub fn with_clint(mut self, clint: ClintConfig) -> Self {
        self.clint = Some(clint);
        self
    }

    /// Builder: attach a PLIC configuration
    pub fn with_plic(mut self, plic: PlicConfig) -> Self {
        self.plic = Some(plic);
        self
    }

    /// Builder: override the hart ID (for multi-hart SoCs)
    pub fn with_hart_id(mut self, hart_id: usize) -> Self {
        self.hart_id = hart_id;
        self
    }

    /// Builder: set the `mtime` reference frequency
    pub fn with_mtime_freq(mut self, hz: u64) -> Self {
        self.mtime_freq_hz = hz;
        self
    }

    /// Initialize the runtime.
    ///
    /// On real hardware this would set `mtvec`, configure `mstatus.MIE`, and
    /// prime the PLIC thresholds.  In simulation/test mode it is a no-op that
    /// returns `Ok(())`.
    pub fn init(&mut self) -> Result<(), RiscvError> {
        // In a real bare-metal context:
        //   1. write mtvec with the trap vector address
        //   2. clear mip / set mie bits for desired interrupt sources
        //   3. set PLIC threshold to 0 (allow all priorities) for this context
        //   4. set global interrupt enable in mstatus.MIE
        //
        // For simulation we simply succeed.
        Ok(())
    }

    /// Enable global machine-mode interrupts (set `mstatus.MIE`).
    ///
    /// On non-RISC-V targets this is a no-op to allow host unit testing.
    pub fn enable_interrupts(&self) {
        #[cfg(target_arch = "riscv32")]
        unsafe {
            core::arch::asm!("csrsi mstatus, 0x8");
        }
        #[cfg(target_arch = "riscv64")]
        unsafe {
            core::arch::asm!("csrsi mstatus, 0x8");
        }
        // no-op on host / other architectures
    }

    /// Disable global machine-mode interrupts (clear `mstatus.MIE`).
    pub fn disable_interrupts(&self) {
        #[cfg(target_arch = "riscv32")]
        unsafe {
            core::arch::asm!("csrci mstatus, 0x8");
        }
        #[cfg(target_arch = "riscv64")]
        unsafe {
            core::arch::asm!("csrci mstatus, 0x8");
        }
        // no-op on host / other architectures
    }

    /// Execute the `wfi` (Wait For Interrupt) instruction.
    ///
    /// On non-RISC-V targets this is a no-op.
    pub fn wfi(&self) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            core::arch::asm!("wfi");
        }
        // no-op on host
    }

    /// Write the `mtvec` CSR to point at a trap handler.
    ///
    /// `handler_addr` must be 4-byte aligned (direct mode).  Vectored mode
    /// (bit 0 set to 1) is not set automatically — callers must OR the mode bits
    /// themselves before passing the address if needed.
    pub fn set_mtvec(&self, handler_addr: usize) -> Result<(), RiscvError> {
        if handler_addr & 0x3 != 0 {
            return Err(RiscvError::AlignmentError {
                addr: handler_addr,
                required: 4,
            });
        }

        #[cfg(target_arch = "riscv32")]
        unsafe {
            core::arch::asm!(
                "csrw mtvec, {0}",
                in(reg) handler_addr,
            );
        }
        #[cfg(target_arch = "riscv64")]
        unsafe {
            core::arch::asm!(
                "csrw mtvec, {0}",
                in(reg) handler_addr,
            );
        }

        Ok(())
    }

    /// Read the `mtime` counter from the CLINT.
    ///
    /// Returns `None` if no CLINT has been configured.
    pub fn read_mtime(&self) -> Option<u64> {
        let clint = self.clint.as_ref()?;
        let addr = clint.mtime_addr();

        // On real hardware: volatile 64-bit read from MMIO
        // In simulation: return a synthetic value (0 for tests)
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // Safety: MMIO read of properly aligned CLINT register
            Some(unsafe { core::ptr::read_volatile(addr as *const u64) })
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = addr;
            Some(0)
        }
    }

    /// Set the `mtimecmp` register for a specific hart.
    ///
    /// Writing a value >= `mtime` clears a pending timer interrupt; writing a
    /// future `mtime` value schedules the next interrupt.
    pub fn set_mtimecmp(&self, hart: usize, cmp: u64) -> Result<(), RiscvError> {
        let clint = self.clint.as_ref().ok_or(RiscvError::ClintNotConfigured)?;

        if hart >= clint.num_harts {
            return Err(RiscvError::InvalidHart {
                hart,
                max_harts: clint.num_harts,
            });
        }

        let addr = clint.mtimecmp_addr(hart);

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            #[cfg(target_arch = "riscv32")]
            {
                let lo_addr = addr as *mut u32;
                let hi_addr = (addr + 4) as *mut u32;
                // Write high word first with all-ones to prevent spurious trigger
                core::ptr::write_volatile(hi_addr, u32::MAX);
                core::ptr::write_volatile(lo_addr, cmp as u32);
                core::ptr::write_volatile(hi_addr, (cmp >> 32) as u32);
            }
            #[cfg(target_arch = "riscv64")]
            {
                core::ptr::write_volatile(addr as *mut u64, cmp);
            }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = (addr, cmp);
        }

        Ok(())
    }

    /// Enable an IRQ source in the PLIC and set its priority.
    ///
    /// Priority 0 means "never interrupt"; 1-7 are valid with 7 being highest.
    pub fn plic_enable_irq(&self, irq: u32, priority: u8) -> Result<(), RiscvError> {
        let plic = self.plic.as_ref().ok_or(RiscvError::PlicNotConfigured)?;

        if irq == 0 || irq as usize > plic.num_sources {
            return Err(RiscvError::InvalidIrq {
                irq,
                max_sources: plic.num_sources as u32,
            });
        }

        let priority_addr = plic.priority_addr(irq);
        let enable_base = plic.enable_base(0); // context 0 = M-mode hart 0
        let enable_addr = enable_base + (irq as usize / 32) * 4;
        let enable_bit = 1u32 << (irq % 32);

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            core::ptr::write_volatile(priority_addr as *mut u32, priority as u32);
            let current = core::ptr::read_volatile(enable_addr as *const u32);
            core::ptr::write_volatile(enable_addr as *mut u32, current | enable_bit);
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = (priority_addr, enable_addr, enable_bit, priority);
        }

        Ok(())
    }

    /// Claim (read) the highest-priority pending interrupt for `context`.
    ///
    /// Returns `Some(irq)` if an interrupt is pending; `None` if the PLIC is
    /// not configured or no interrupt is pending.
    pub fn plic_claim(&self, context: usize) -> Option<u32> {
        let plic = self.plic.as_ref()?;
        let claim_addr = plic.claim_addr(context);

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let irq = unsafe { core::ptr::read_volatile(claim_addr as *const u32) };
            if irq == 0 {
                None
            } else {
                Some(irq)
            }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = claim_addr;
            None
        }
    }

    /// Complete (acknowledge) an interrupt claim for `context`.
    ///
    /// Must be called after the ISR has handled the IRQ to allow the PLIC to
    /// re-assert the interrupt if it remains pending.
    pub fn plic_complete(&self, context: usize, irq: u32) -> Result<(), RiscvError> {
        let plic = self.plic.as_ref().ok_or(RiscvError::PlicNotConfigured)?;

        if irq == 0 || irq as usize > plic.num_sources {
            return Err(RiscvError::InvalidIrq {
                irq,
                max_sources: plic.num_sources as u32,
            });
        }

        let claim_addr = plic.claim_addr(context);

        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        unsafe {
            core::ptr::write_volatile(claim_addr as *mut u32, irq);
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = (claim_addr, irq);
        }

        Ok(())
    }

    /// Convert microseconds to `mtime` ticks at the configured frequency.
    pub fn micros_to_ticks(&self, us: u64) -> u64 {
        us.saturating_mul(self.mtime_freq_hz) / 1_000_000
    }

    /// Convert `mtime` ticks to microseconds at the configured frequency.
    pub fn ticks_to_micros(&self, ticks: u64) -> u64 {
        ticks.saturating_mul(1_000_000) / self.mtime_freq_hz
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    extern crate alloc;

    // --- RISC-V variant creation ---

    #[test]
    fn test_riscv_variant_rv32imac_create() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac);
        assert_eq!(rt.variant, RiscvVariant::Rv32Imac);
        assert_eq!(rt.variant.xlen(), 32);
        assert!(rt.variant.is_32bit());
        assert!(!rt.variant.is_64bit());
        assert!(rt.variant.has_atomic());
        assert!(rt.variant.has_compressed());
        assert!(rt.variant.has_multiply());
        assert!(!rt.variant.has_float());
    }

    #[test]
    fn test_riscv_variant_rv64imac_create() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv64Imac);
        assert_eq!(rt.variant, RiscvVariant::Rv64Imac);
        assert_eq!(rt.variant.xlen(), 64);
        assert!(rt.variant.is_64bit());
        assert!(!rt.variant.is_32bit());
        assert!(!rt.variant.has_float());
        assert!(!rt.variant.has_double());
    }

    // --- CLINT configuration ---

    #[test]
    fn test_clint_config_sifive() {
        let clint = ClintConfig::sifive(0x0200_0000);
        assert_eq!(clint.base_addr, 0x0200_0000);
        assert_eq!(clint.mtime_offset, 0xBFF8);
        assert_eq!(clint.mtimecmp_offset, 0x4000);
        assert_eq!(clint.num_harts, 1);

        // Derived address helpers
        assert_eq!(clint.mtime_addr(), 0x0200_0000 + 0xBFF8);
        assert_eq!(clint.mtimecmp_addr(0), 0x0200_0000 + 0x4000);
        assert_eq!(clint.mtimecmp_addr(1), 0x0200_0000 + 0x4000 + 8);
        assert_eq!(clint.msip_addr(0), 0x0200_0000);
        assert_eq!(clint.msip_addr(1), 0x0200_0000 + 4);
    }

    #[test]
    fn test_clint_config_virt() {
        let clint = ClintConfig::virt(0x0200_0000);
        assert_eq!(clint.base_addr, 0x0200_0000);
        // QEMU virt supports 8 harts
        assert_eq!(clint.num_harts, 8);
        // Same offsets as SiFive for the virt machine
        assert_eq!(clint.mtime_offset, 0xBFF8);
        assert_eq!(clint.mtimecmp_offset, 0x4000);
    }

    // --- InterruptCause decode ---

    #[test]
    fn test_interrupt_cause_from_mcause() {
        // Bit 63 set = interrupt; code 7 = MachineTimer
        let mcause: u64 = (1u64 << 63) | 7;
        let cause = InterruptCause::from(mcause);
        assert_eq!(cause, InterruptCause::MachineTimer);

        // Code 3 = MachineSoftware
        let mcause_sw: u64 = (1u64 << 63) | 3;
        assert_eq!(
            InterruptCause::from(mcause_sw),
            InterruptCause::MachineSoftware
        );

        // Code 11 = MachineExternal
        let mcause_ext: u64 = (1u64 << 63) | 11;
        assert_eq!(
            InterruptCause::from(mcause_ext),
            InterruptCause::MachineExternal
        );

        // Exception (bit 63 clear) -> Unknown
        let mcause_exc: u64 = 2; // Store/AMO access fault
        assert!(matches!(
            InterruptCause::from(mcause_exc),
            InterruptCause::Unknown(_)
        ));

        // Supervisor variants
        let mcause_stimer: u64 = (1u64 << 63) | 5;
        assert_eq!(
            InterruptCause::from(mcause_stimer),
            InterruptCause::SupervisorTimer
        );
    }

    // --- PLIC configuration ---

    #[test]
    fn test_plic_config_defaults() {
        let plic = PlicConfig::new(0x0C00_0000);
        assert_eq!(plic.base_addr, 0x0C00_0000);
        assert_eq!(plic.num_sources, 128);
        assert_eq!(plic.num_contexts, 2);

        // Address helpers
        assert_eq!(plic.priority_addr(1), 0x0C00_0000 + 4);
        assert_eq!(plic.priority_addr(2), 0x0C00_0000 + 8);
        assert_eq!(plic.enable_base(0), 0x0C00_0000 + 0x2000);
        assert_eq!(plic.threshold_addr(0), 0x0C00_0000 + 0x20_0000);
        assert_eq!(plic.claim_addr(0), 0x0C00_0000 + 0x20_0004);
    }

    // --- Runtime init ---

    #[test]
    fn test_riscv_runtime_init() {
        let mut rt = RiscvRuntime::new(RiscvVariant::Rv32Imac)
            .with_clint(ClintConfig::sifive(0x0200_0000))
            .with_plic(PlicConfig::new(0x0C00_0000));
        assert!(rt.init().is_ok());
    }

    // --- mtime frequency default ---

    #[test]
    fn test_mtime_freq_default() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac);
        // Default is 10 MHz (SiFive/OpenSBI convention)
        assert_eq!(rt.mtime_freq_hz, 10_000_000);

        // Override to RTC frequency
        let rt_rtc = RiscvRuntime::new(RiscvVariant::Rv32Imac).with_mtime_freq(32_768);
        assert_eq!(rt_rtc.mtime_freq_hz, 32_768);
    }

    // --- Tick / microsecond conversion ---

    #[test]
    fn test_mtime_tick_conversion_round_trip() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac).with_mtime_freq(10_000_000);
        let us = 1_000u64;
        let ticks = rt.micros_to_ticks(us);
        assert_eq!(ticks, 10_000);
        assert_eq!(rt.ticks_to_micros(ticks), us);
    }

    // --- Error variants display ---

    #[test]
    fn test_riscv_error_display_clint_not_configured() {
        let e = RiscvError::ClintNotConfigured;
        let s = alloc::format!("{e}");
        assert!(s.contains("CLINT"));
    }

    #[test]
    fn test_riscv_error_display_invalid_hart() {
        let e = RiscvError::InvalidHart {
            hart: 5,
            max_harts: 4,
        };
        let s = alloc::format!("{e}");
        assert!(s.contains("hart") || s.contains('5'));
    }

    // --- Privilege level ordering ---

    #[test]
    fn test_privilege_level_ordering() {
        assert!(PrivilegeLevel::Machine > PrivilegeLevel::Supervisor);
        assert!(PrivilegeLevel::Supervisor > PrivilegeLevel::User);
        assert!(PrivilegeLevel::Machine > PrivilegeLevel::User);
    }

    // --- Read mtime returns Some(0) with CLINT in sim, None without ---

    #[test]
    fn test_read_mtime_no_clint_returns_none() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac);
        // No CLINT configured -> None on any platform
        assert!(rt.read_mtime().is_none());
    }

    #[test]
    fn test_read_mtime_with_clint_returns_some() {
        let rt =
            RiscvRuntime::new(RiscvVariant::Rv32Imac).with_clint(ClintConfig::sifive(0x0200_0000));
        // On host simulation path returns Some(0)
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        assert_eq!(rt.read_mtime(), Some(0));
    }

    // --- set_mtimecmp error paths ---

    #[test]
    fn test_set_mtimecmp_no_clint_returns_error() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac);
        let result = rt.set_mtimecmp(0, 100);
        assert_eq!(result, Err(RiscvError::ClintNotConfigured));
    }

    #[test]
    fn test_set_mtimecmp_invalid_hart() {
        let rt =
            RiscvRuntime::new(RiscvVariant::Rv32Imac).with_clint(ClintConfig::sifive(0x0200_0000)); // 1 hart
                                                                                                    // Hart 5 exceeds the configured 1-hart CLINT
        let result = rt.set_mtimecmp(5, 9999);
        assert!(matches!(result, Err(RiscvError::InvalidHart { .. })));
    }

    // --- PLIC error paths ---

    #[test]
    fn test_plic_enable_irq_no_plic_returns_error() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac);
        assert_eq!(rt.plic_enable_irq(1, 7), Err(RiscvError::PlicNotConfigured));
    }

    #[test]
    fn test_plic_enable_irq_zero_reserved() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac).with_plic(PlicConfig::new(0x0C00_0000));
        // IRQ 0 is reserved in PLIC spec
        assert!(matches!(
            rt.plic_enable_irq(0, 7),
            Err(RiscvError::InvalidIrq { .. })
        ));
    }

    // --- set_mtvec alignment check ---

    #[test]
    fn test_set_mtvec_unaligned_returns_error() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac);
        // Address not 4-byte aligned
        let result = rt.set_mtvec(0x1001);
        assert!(matches!(result, Err(RiscvError::AlignmentError { .. })));
    }

    #[test]
    fn test_set_mtvec_aligned_ok() {
        let rt = RiscvRuntime::new(RiscvVariant::Rv32Imac);
        // Address is 4-byte aligned -- succeeds on host (no CSR write)
        assert!(rt.set_mtvec(0x1000).is_ok());
    }

    // --- RiscvVariant GC extensions ---

    #[test]
    fn test_rv32gc_has_float_and_double() {
        let v = RiscvVariant::Rv32Gc;
        assert!(v.has_float());
        assert!(v.has_double());
        assert_eq!(v.xlen(), 32);
    }

    #[test]
    fn test_rv64gc_full_extension_set() {
        let v = RiscvVariant::Rv64Gc;
        assert!(v.has_float());
        assert!(v.has_double());
        assert!(v.has_atomic());
        assert!(v.has_compressed());
        assert_eq!(v.xlen(), 64);
    }
}
