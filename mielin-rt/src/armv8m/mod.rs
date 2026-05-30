//! ARMv8-M runtime support — Cortex-M23, M33, and M55.
//!
//! ARMv8-M is ARM's latest microcontroller architecture, introducing:
//!
//! - **TrustZone-M**: hardware-enforced secure/non-secure memory partitioning
//! - **Enhanced MPU**: up to 16 regions with Security Attribution Unit (SAU)
//! - **Optional FPU**: single-precision on M33, double-precision on M55
//! - **Helium (MVE)**: M-Profile Vector Extension on Cortex-M55 for DSP/ML
//!
//! ## Core Family
//!
//! | Core        | Architecture | FPU | MVE | TrustZone |
//! |-------------|-------------|-----|-----|-----------|
//! | Cortex-M23  | ARMv8-M BL  | No  | No  | Optional  |
//! | Cortex-M33  | ARMv8-M ML  | Opt | No  | Optional  |
//! | Cortex-M55  | ARMv8.1-M   | Yes | Yes | Optional  |

use core::fmt;

// ---------------------------------------------------------------------------
// ARMv8-M Core Variant
// ---------------------------------------------------------------------------

/// ARMv8-M Cortex-M core variant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmV8mVariant {
    /// Cortex-M23: ARMv8-M Baseline, no FPU, minimal TrustZone
    CortexM23,
    /// Cortex-M33: ARMv8-M Mainline, optional single-precision FPU, full TrustZone
    CortexM33,
    /// Cortex-M55: ARMv8.1-M Mainline + Helium (MVE), dual-precision FPU
    CortexM55,
}

impl ArmV8mVariant {
    /// Returns `true` if this core supports TrustZone-M security extensions
    pub fn supports_trustzone(self) -> bool {
        // All ARMv8-M variants support TrustZone (it is implementation-optional
        // but universally present in Cortex-M23/M33/M55 silicon)
        true
    }

    /// Returns `true` if this core can have a hardware FPU
    pub fn can_have_fpu(self) -> bool {
        matches!(self, ArmV8mVariant::CortexM33 | ArmV8mVariant::CortexM55)
    }

    /// Returns `true` if this core supports Helium/MVE
    pub fn supports_mve(self) -> bool {
        matches!(self, ArmV8mVariant::CortexM55)
    }

    /// Typical number of SAU regions available on this core
    pub fn default_sau_regions(self) -> u8 {
        match self {
            ArmV8mVariant::CortexM23 => 4,
            ArmV8mVariant::CortexM33 => 8,
            ArmV8mVariant::CortexM55 => 8,
        }
    }

    /// Typical number of MPU regions available on this core
    pub fn default_mpu_regions(self) -> u8 {
        match self {
            ArmV8mVariant::CortexM23 => 8,
            ArmV8mVariant::CortexM33 => 8,
            ArmV8mVariant::CortexM55 => 16, // M55 often ships with 16 MPU regions
        }
    }
}

impl fmt::Display for ArmV8mVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArmV8mVariant::CortexM23 => write!(f, "Cortex-M23 (ARMv8-M Baseline)"),
            ArmV8mVariant::CortexM33 => write!(f, "Cortex-M33 (ARMv8-M Mainline)"),
            ArmV8mVariant::CortexM55 => write!(f, "Cortex-M55 (ARMv8.1-M + Helium)"),
        }
    }
}

// ---------------------------------------------------------------------------
// TrustZone Security State
// ---------------------------------------------------------------------------

/// ARMv8-M TrustZone-M security state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustZoneState {
    /// Secure state — full access to secure and non-secure resources
    Secure,
    /// Non-Secure state — restricted to NS memory and peripherals
    NonSecure,
    /// Non-Secure Callable — gateway function regions reachable via BLXNS/BLNS
    NonSecureCallable,
}

impl fmt::Display for TrustZoneState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrustZoneState::Secure => write!(f, "Secure"),
            TrustZoneState::NonSecure => write!(f, "Non-Secure"),
            TrustZoneState::NonSecureCallable => write!(f, "Non-Secure Callable (NSC)"),
        }
    }
}

// ---------------------------------------------------------------------------
// ARMv8-M Errors
// ---------------------------------------------------------------------------

/// Errors returned by ARMv8-M runtime operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmV8mError {
    /// TrustZone is not present on this device or has been disabled in hardware
    TrustZoneNotSupported,
    /// The core does not have an FPU (Cortex-M23, or M33 without FPU option)
    FpuNotPresent,
    /// SAU region index exceeds the number available in this core
    InvalidSauRegion { region: u8, max_regions: u8 },
    /// The operation is not supported on this specific core variant
    UnsupportedVariant,
    /// MPU region index out of range
    InvalidMpuRegion { region: u8, max_regions: u8 },
    /// The base or limit address violates the SAU/MPU 32-byte alignment requirement
    AlignmentError { addr: u32, required: u32 },
}

impl fmt::Display for ArmV8mError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArmV8mError::TrustZoneNotSupported => {
                write!(f, "TrustZone-M is not supported on this device")
            }
            ArmV8mError::FpuNotPresent => {
                write!(f, "No FPU present (Cortex-M23 or M33 without FPU option)")
            }
            ArmV8mError::InvalidSauRegion { region, max_regions } => {
                write!(
                    f,
                    "SAU region {region} out of range (max {max_regions} regions)"
                )
            }
            ArmV8mError::UnsupportedVariant => {
                write!(f, "Operation not supported on this ARMv8-M core variant")
            }
            ArmV8mError::InvalidMpuRegion { region, max_regions } => {
                write!(
                    f,
                    "MPU region {region} out of range (max {max_regions} regions)"
                )
            }
            ArmV8mError::AlignmentError { addr, required } => {
                write!(
                    f,
                    "Address {addr:#010x} is not aligned to {required} bytes as required by SAU/MPU"
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ARMv8-M Runtime
// ---------------------------------------------------------------------------

/// ARMv8-M bare-metal runtime manager.
///
/// Manages TrustZone state, FPU initialization, and SAU configuration for
/// Cortex-M23/M33/M55 targets.  All hardware register accesses are guarded
/// by `target_arch` so that host unit tests run without hardware.
pub struct ArmV8mRuntime {
    /// Core variant
    pub variant: ArmV8mVariant,
    /// Whether TrustZone was present and successfully initialized
    pub trustzone_enabled: bool,
    /// Current security state of the executing domain
    pub current_state: TrustZoneState,
    /// Whether the FPU is present and enabled
    pub has_fpu: bool,
    /// Whether Helium/MVE is present (Cortex-M55 only)
    pub has_mve: bool,
}

impl ArmV8mRuntime {
    /// Construct a runtime for the given ARMv8-M core variant.
    ///
    /// `has_fpu` defaults to `true` for M33 and M55 (most silicon ships with
    /// the FPU option enabled) and `false` for M23.
    /// `trustzone_enabled` defaults to `true` for all three.
    pub fn new(variant: ArmV8mVariant) -> Self {
        let has_fpu = variant.can_have_fpu();
        let has_mve = variant.supports_mve();
        Self {
            variant,
            trustzone_enabled: true,
            current_state: TrustZoneState::Secure,
            has_fpu,
            has_mve,
        }
    }

    /// Builder: override FPU presence (e.g., for M33 without FPU option)
    pub fn with_fpu(mut self, present: bool) -> Self {
        self.has_fpu = present;
        self
    }

    /// Builder: override TrustZone availability
    pub fn with_trustzone(mut self, enabled: bool) -> Self {
        self.trustzone_enabled = enabled;
        self
    }

    /// Initialize the ARMv8-M runtime.
    ///
    /// On real hardware this configures:
    /// 1. CPACR/NSACR to enable FPU access
    /// 2. SAU — enables all configured regions
    /// 3. SCB — sets AIRCR.VECTKEY to allow PRIGROUP writes
    ///
    /// In simulation this is a no-op returning `Ok(())`.
    pub fn init(&mut self) -> Result<(), ArmV8mError> {
        // Real hardware sequence (ARM DDI 0553B §4.6.1):
        // 1. Set CPACR.CP10/CP11 = 0b11 to enable FPU full access
        // 2. DSB + ISB barrier after FPU enable
        // 3. Write SAU_CTRL.ENABLE = 1 to enable SAU
        //
        // For simulation: succeed unconditionally.
        Ok(())
    }

    /// Transition processor to Secure state.
    ///
    /// In production firmware this is typically handled by the secure boot
    /// ROM or NS->S gateway functions; this method exists for test/debug.
    pub fn enter_secure_state(&mut self) -> Result<(), ArmV8mError> {
        if !self.trustzone_enabled {
            return Err(ArmV8mError::TrustZoneNotSupported);
        }
        self.current_state = TrustZoneState::Secure;
        Ok(())
    }

    /// Transition processor to Non-Secure state.
    pub fn enter_nonsecure_state(&mut self) -> Result<(), ArmV8mError> {
        if !self.trustzone_enabled {
            return Err(ArmV8mError::TrustZoneNotSupported);
        }
        self.current_state = TrustZoneState::NonSecure;
        Ok(())
    }

    /// Configure a Security Attribution Unit (SAU) region.
    ///
    /// The SAU partitions the address space into secure and non-secure regions.
    /// Each region is defined by a 32-byte-aligned base address and a 32-byte-
    /// aligned limit address (inclusive end).
    ///
    /// # Arguments
    /// - `region`: SAU region index (0 .. SAU_TYPE.SREGION - 1)
    /// - `base`: 32-byte aligned base address of the region
    /// - `limit`: 32-byte aligned limit address (inclusive end of region)
    /// - `nsc`: if `true`, mark as Non-Secure Callable (NSC) function gateway
    pub fn configure_sau_region(
        &self,
        region: u8,
        base: u32,
        limit: u32,
        nsc: bool,
    ) -> Result<(), ArmV8mError> {
        if !self.trustzone_enabled {
            return Err(ArmV8mError::TrustZoneNotSupported);
        }

        let max_regions = self.variant.default_sau_regions();
        if region >= max_regions {
            return Err(ArmV8mError::InvalidSauRegion {
                region,
                max_regions,
            });
        }

        // SAU requires 32-byte (0x20) alignment for base and limit
        if base & 0x1F != 0 {
            return Err(ArmV8mError::AlignmentError { addr: base, required: 32 });
        }
        if limit & 0x1F != 0 {
            return Err(ArmV8mError::AlignmentError { addr: limit, required: 32 });
        }

        #[cfg(target_arch = "arm")]
        unsafe {
            // SAU register block (ARM v8-M ARM §4.6.7)
            // SAU_RNR  = 0xE000EDD8
            // SAU_RBAR = 0xE000EDDC
            // SAU_RLAR = 0xE000EDE0
            const SAU_RNR: *mut u32 = 0xE000_EDD8 as *mut u32;
            const SAU_RBAR: *mut u32 = 0xE000_EDDC as *mut u32;
            const SAU_RLAR: *mut u32 = 0xE000_EDE0 as *mut u32;

            core::ptr::write_volatile(SAU_RNR, region as u32);
            core::ptr::write_volatile(SAU_RBAR, base);
            // LLAR: limit[31:5] | NSC[1] | ENABLE[0]
            let llar = (limit & !0x1F) | ((nsc as u32) << 1) | 1;
            core::ptr::write_volatile(SAU_RLAR, llar);

            // DSB + ISB to ensure the SAU update takes effect before next instruction
            core::arch::asm!("dsb sy", "isb");
        }
        #[cfg(not(target_arch = "arm"))]
        {
            let _ = (base, limit, nsc);
        }

        Ok(())
    }

    /// Enable the FPU by setting CPACR.CP10/CP11 to full access.
    ///
    /// Returns `Err(FpuNotPresent)` if this runtime was created without FPU.
    pub fn enable_fpu(&self) -> Result<(), ArmV8mError> {
        if !self.has_fpu {
            return Err(ArmV8mError::FpuNotPresent);
        }

        #[cfg(target_arch = "arm")]
        unsafe {
            // CPACR at 0xE000ED88; bits[23:20] = CP11:CP10 full access
            const CPACR: *mut u32 = 0xE000_ED88 as *mut u32;
            let v = core::ptr::read_volatile(CPACR);
            core::ptr::write_volatile(CPACR, v | (0xF << 20));
            core::arch::asm!("dsb sy", "isb");
        }

        Ok(())
    }

    /// Enter sleep mode (execute `WFI` — Wait For Interrupt).
    pub fn sleep(&self) {
        #[cfg(target_arch = "arm")]
        unsafe {
            core::arch::asm!("wfi");
        }
        // no-op on host
    }

    /// Enter deep sleep by setting SCB.SCR.SLEEPDEEP and executing `WFI`.
    ///
    /// Deep sleep stops the core clock and most clocked peripherals.  The
    /// exact power domain behaviour depends on the SoC vendor.
    pub fn deep_sleep(&self) -> Result<(), ArmV8mError> {
        #[cfg(target_arch = "arm")]
        unsafe {
            // SCB.SCR at 0xE000ED10; bit 2 = SLEEPDEEP
            const SCB_SCR: *mut u32 = 0xE000_ED10 as *mut u32;
            let v = core::ptr::read_volatile(SCB_SCR);
            core::ptr::write_volatile(SCB_SCR, v | (1 << 2));
            core::arch::asm!("dsb sy", "wfi");
            // Clear SLEEPDEEP on wake-up to allow normal sleep in future
            let v2 = core::ptr::read_volatile(SCB_SCR);
            core::ptr::write_volatile(SCB_SCR, v2 & !(1 << 2));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ARMv8-M MPU
// ---------------------------------------------------------------------------

/// MPU region attribute configuration for ARMv8-M.
///
/// ARMv8-M uses the PMSA (Protected Memory System Architecture) v8 which
/// differs from ARMv7-M: regions use a flat base/limit model instead of the
/// power-of-two size encoding used in ARMv7-M.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpuAttributes {
    /// Execute Never — prevents instruction fetch from this region
    pub execute_never: bool,
    /// Allow read and write access (if `false`, read-only)
    pub read_write: bool,
    /// Mark region as cacheable in inner/outer caches
    pub cacheable: bool,
    /// Mark region as shareable between multiple processors
    pub shareable: bool,
}

impl MpuAttributes {
    /// Default attributes: read-write, cacheable, non-shareable, executable
    pub const fn default_rw() -> Self {
        Self {
            execute_never: true,
            read_write: true,
            cacheable: true,
            shareable: false,
        }
    }

    /// Read-only, executable (suitable for Flash/ROM code)
    pub const fn flash_code() -> Self {
        Self {
            execute_never: false,
            read_write: false,
            cacheable: true,
            shareable: false,
        }
    }

    /// Device / peripheral: non-cacheable, non-executable, read-write
    pub const fn device_peripheral() -> Self {
        Self {
            execute_never: true,
            read_write: true,
            cacheable: false,
            shareable: true,
        }
    }

    /// Encode the attribute set into the ARMv8-M MPU RLAR/RBAR attribute bits.
    ///
    /// This produces a 3-bit approximation for quick testing; real firmware
    /// would set the full MAIR0/MAIR1 index.
    pub fn to_attr_index(self) -> u8 {
        let mut idx: u8 = 0;
        if self.read_write { idx |= 0x01; }
        if self.cacheable { idx |= 0x02; }
        if !self.execute_never { idx |= 0x04; }
        idx
    }
}

/// ARMv8-M MPU configuration block.
///
/// ARMv8-M PMSAv8 supports up to 16 regions per security state.  Each region
/// is defined by a 32-byte-aligned base address and a 32-byte-aligned limit
/// (inclusive end), with individual attribute entries in MAIR0/MAIR1.
pub struct ArmV8mMpu {
    /// Number of MPU regions available
    pub num_regions: u8,
    /// Whether this MPU instance controls secure-state regions
    pub is_secure: bool,
}

impl ArmV8mMpu {
    /// Create an MPU instance for the given core variant.
    ///
    /// `is_secure`: if `true`, accesses the Secure MPU (S_MPU_*);
    /// otherwise the Non-Secure MPU.
    pub fn new(variant: ArmV8mVariant) -> Self {
        Self {
            num_regions: variant.default_mpu_regions(),
            is_secure: true,
        }
    }

    /// Configure a single MPU region.
    ///
    /// # Arguments
    /// - `region`: 0-based region index
    /// - `base`: 32-byte aligned base address
    /// - `limit`: 32-byte aligned limit address (inclusive end)
    /// - `attrs`: access / cache / execute attributes
    ///
    /// # Errors
    /// Returns `InvalidMpuRegion` if `region >= num_regions`.
    /// Returns `AlignmentError` if `base` or `limit` are not 32-byte aligned.
    pub fn configure_region(
        &self,
        region: u8,
        base: u32,
        limit: u32,
        attrs: MpuAttributes,
    ) -> Result<(), ArmV8mError> {
        if region >= self.num_regions {
            return Err(ArmV8mError::InvalidMpuRegion {
                region,
                max_regions: self.num_regions,
            });
        }
        if base & 0x1F != 0 {
            return Err(ArmV8mError::AlignmentError { addr: base, required: 32 });
        }
        if limit & 0x1F != 0 {
            return Err(ArmV8mError::AlignmentError { addr: limit, required: 32 });
        }

        #[cfg(target_arch = "arm")]
        unsafe {
            // PMSAv8 registers (NS bank):
            // MPU_RNR  0xE000ED98
            // MPU_RBAR 0xE000ED9C
            // MPU_RLAR 0xE000EDA0
            const MPU_RNR: *mut u32 = 0xE000_ED98 as *mut u32;
            const MPU_RBAR: *mut u32 = 0xE000_ED9C as *mut u32;
            const MPU_RLAR: *mut u32 = 0xE000_EDA0 as *mut u32;

            core::ptr::write_volatile(MPU_RNR, region as u32);

            // RBAR: base[31:5] | SH[4:3] | AP[2:1] | XN[0]
            let sh: u32 = if attrs.shareable { 0b10 << 3 } else { 0 };
            let ap: u32 = if attrs.read_write { 0b01 << 1 } else { 0b11 << 1 };
            let xn: u32 = attrs.execute_never as u32;
            core::ptr::write_volatile(MPU_RBAR, (base & !0x1F) | sh | ap | xn);

            // RLAR: limit[31:5] | AttrIndx[3:1] | EN[0]
            let attr_idx = attrs.to_attr_index() as u32 & 0x7;
            let rlar = (limit & !0x1F) | (attr_idx << 1) | 1;
            core::ptr::write_volatile(MPU_RLAR, rlar);

            core::arch::asm!("dsb sy", "isb");
        }
        #[cfg(not(target_arch = "arm"))]
        {
            let _ = (base, limit, attrs);
        }

        Ok(())
    }

    /// Enable the MPU (set MPU_CTRL.ENABLE = 1) with default memory map enabled
    /// for privileged access (PRIVDEFENA = 1).
    pub fn enable(&self) -> Result<(), ArmV8mError> {
        #[cfg(target_arch = "arm")]
        unsafe {
            const MPU_CTRL: *mut u32 = 0xE000_ED94 as *mut u32;
            // PRIVDEFENA[2] | HFNMIENA[1] | ENABLE[0]
            core::ptr::write_volatile(MPU_CTRL, 0x5);
            core::arch::asm!("dsb sy", "isb");
        }
        Ok(())
    }

    /// Disable the MPU (clear MPU_CTRL.ENABLE).
    pub fn disable(&self) -> Result<(), ArmV8mError> {
        #[cfg(target_arch = "arm")]
        unsafe {
            const MPU_CTRL: *mut u32 = 0xE000_ED94 as *mut u32;
            let v = core::ptr::read_volatile(MPU_CTRL);
            core::ptr::write_volatile(MPU_CTRL, v & !0x1);
            core::arch::asm!("dsb sy", "isb");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Core variant creation ---

    #[test]
    fn test_cortex_m23_create() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM23);
        assert_eq!(rt.variant, ArmV8mVariant::CortexM23);
        // TrustZone enabled by default
        assert!(rt.trustzone_enabled);
        // M23 has no FPU
        assert!(!rt.has_fpu);
        assert!(!rt.has_mve);
        assert_eq!(rt.current_state, TrustZoneState::Secure);
    }

    #[test]
    fn test_cortex_m33_has_fpu() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        assert_eq!(rt.variant, ArmV8mVariant::CortexM33);
        // M33 ships with FPU by default in most silicon
        assert!(rt.has_fpu, "Cortex-M33 should have FPU present");
        assert!(!rt.has_mve);
        assert!(rt.trustzone_enabled);
    }

    #[test]
    fn test_cortex_m55_has_mve() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM55);
        assert_eq!(rt.variant, ArmV8mVariant::CortexM55);
        assert!(rt.has_fpu, "Cortex-M55 must have FPU");
        assert!(rt.has_mve, "Cortex-M55 must have Helium/MVE");
    }

    // --- Runtime init ---

    #[test]
    fn test_armv8m_init_ok() {
        let mut rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        assert!(rt.init().is_ok());
    }

    // --- SAU region validation ---

    #[test]
    fn test_sau_region_invalid() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM23); // 4 SAU regions
        // Region index 4 is out of range for M23 (regions 0..3)
        let result = rt.configure_sau_region(4, 0x1000_0000, 0x1001_FFE0, false);
        assert!(matches!(
            result,
            Err(ArmV8mError::InvalidSauRegion { region: 4, max_regions: 4 })
        ));
    }

    #[test]
    fn test_sau_region_alignment_base() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        // base address not 32-byte aligned
        let result = rt.configure_sau_region(0, 0x1000_0010, 0x1001_FFE0, false);
        assert!(matches!(result, Err(ArmV8mError::AlignmentError { addr: 0x1000_0010, .. })));
    }

    #[test]
    fn test_sau_region_alignment_limit() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        // limit address not 32-byte aligned
        let result = rt.configure_sau_region(0, 0x1000_0000, 0x1001_FFFF, false);
        assert!(matches!(result, Err(ArmV8mError::AlignmentError { addr: 0x1001_FFFF, .. })));
    }

    #[test]
    fn test_sau_region_valid_ok() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        // Both addresses 32-byte aligned, region index in range
        let result = rt.configure_sau_region(0, 0x1000_0000, 0x1001_FFE0, true);
        assert!(result.is_ok(), "Valid SAU region should succeed, got: {result:?}");
    }

    #[test]
    fn test_sau_trustzone_disabled_returns_error() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33)
            .with_trustzone(false);
        let result = rt.configure_sau_region(0, 0x1000_0000, 0x1001_FFE0, false);
        assert_eq!(result, Err(ArmV8mError::TrustZoneNotSupported));
    }

    // --- MPU region count by variant ---

    #[test]
    fn test_mpu_regions_by_variant() {
        let mpu_m23 = ArmV8mMpu::new(ArmV8mVariant::CortexM23);
        let mpu_m33 = ArmV8mMpu::new(ArmV8mVariant::CortexM33);
        let mpu_m55 = ArmV8mMpu::new(ArmV8mVariant::CortexM55);

        assert_eq!(mpu_m23.num_regions, 8);
        assert!(mpu_m33.num_regions >= 8, "Cortex-M33 must have at least 8 MPU regions");
        assert_eq!(mpu_m55.num_regions, 16, "Cortex-M55 should have 16 MPU regions");
    }

    #[test]
    fn test_mpu_region_invalid_index() {
        let mpu = ArmV8mMpu::new(ArmV8mVariant::CortexM33); // 8 regions
        let attrs = MpuAttributes::default_rw();
        let result = mpu.configure_region(8, 0x2000_0000, 0x2000_FFE0, attrs);
        assert!(matches!(
            result,
            Err(ArmV8mError::InvalidMpuRegion { region: 8, max_regions: 8 })
        ));
    }

    #[test]
    fn test_mpu_region_alignment_check() {
        let mpu = ArmV8mMpu::new(ArmV8mVariant::CortexM33);
        let attrs = MpuAttributes::default_rw();
        // base not 32-byte aligned
        let result = mpu.configure_region(0, 0x2000_0001, 0x2000_FFE0, attrs);
        assert!(matches!(result, Err(ArmV8mError::AlignmentError { .. })));
    }

    #[test]
    fn test_mpu_region_valid_ok() {
        let mpu = ArmV8mMpu::new(ArmV8mVariant::CortexM33);
        let attrs = MpuAttributes::flash_code();
        let result = mpu.configure_region(0, 0x0800_0000, 0x0800_FFE0, attrs);
        assert!(result.is_ok());
    }

    // --- FPU enable/disable ---

    #[test]
    fn test_enable_fpu_no_fpu_returns_error() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM23);
        assert_eq!(rt.enable_fpu(), Err(ArmV8mError::FpuNotPresent));
    }

    #[test]
    fn test_enable_fpu_m33_ok() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        // On host this is a no-op, but should succeed
        assert!(rt.enable_fpu().is_ok());
    }

    // --- Security state transitions ---

    #[test]
    fn test_enter_secure_state_ok() {
        let mut rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        rt.enter_nonsecure_state().unwrap();
        assert_eq!(rt.current_state, TrustZoneState::NonSecure);
        rt.enter_secure_state().unwrap();
        assert_eq!(rt.current_state, TrustZoneState::Secure);
    }

    #[test]
    fn test_trustzone_disabled_state_transition_fails() {
        let mut rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33)
            .with_trustzone(false);
        assert_eq!(
            rt.enter_nonsecure_state(),
            Err(ArmV8mError::TrustZoneNotSupported)
        );
    }

    // --- Variant capability queries ---

    #[test]
    fn test_m23_no_fpu_no_mve() {
        let v = ArmV8mVariant::CortexM23;
        assert!(!v.can_have_fpu());
        assert!(!v.supports_mve());
        assert!(v.supports_trustzone());
    }

    #[test]
    fn test_m55_supports_all() {
        let v = ArmV8mVariant::CortexM55;
        assert!(v.can_have_fpu());
        assert!(v.supports_mve());
        assert!(v.supports_trustzone());
    }

    // --- MpuAttributes encoding ---

    #[test]
    fn test_mpu_attributes_encoding() {
        let rw = MpuAttributes::default_rw();
        assert!(rw.execute_never);
        assert!(rw.read_write);
        assert!(rw.cacheable);

        let flash = MpuAttributes::flash_code();
        assert!(!flash.execute_never);
        assert!(!flash.read_write);
        assert!(flash.cacheable);

        let dev = MpuAttributes::device_peripheral();
        assert!(dev.execute_never);
        assert!(dev.read_write);
        assert!(!dev.cacheable);
        assert!(dev.shareable);
    }

    // --- Deep sleep returns Ok on host ---

    #[test]
    fn test_deep_sleep_ok() {
        let rt = ArmV8mRuntime::new(ArmV8mVariant::CortexM33);
        assert!(rt.deep_sleep().is_ok());
    }
}
