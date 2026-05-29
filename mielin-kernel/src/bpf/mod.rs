#![allow(dead_code)]
//! MielinBPF — sandboxed bytecode virtual machine.
//!
//! Provides a compact ISA, a static verifier, and an interpreter that can be
//! attached to kernel tracepoints.  Programs are verified before being stored
//! in the global registry, and the interpreter is called from
//! `observability::trace_event`.
//!
//! # Quick example
//!
//! ```no_run
//! use mielin_kernel::bpf::{self, isa::{AluOp, Instruction, Reg, Source}};
//! use mielin_kernel::observability::TracepointId;
//!
//! let prog = &[
//!     Instruction::Alu { op: AluOp::Mov, dst: Reg(0), src: Source::Imm(1) },
//!     Instruction::Exit,
//! ];
//! bpf::attach(TracepointId::TaskSpawn, prog).expect("attach");
//! ```

use core::sync::atomic::{AtomicU64, Ordering};

pub mod interp;
pub mod isa;
pub mod verifier;

pub use interp::{BpfContext, Vm};
pub use isa::*;
pub use verifier::{verify, VerifiedProgram};

use crate::observability::TracepointId;

// ---------------------------------------------------------------------------
// BpfError
// ---------------------------------------------------------------------------

/// Errors produced by the BPF verifier, encoder/decoder, and interpreter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BpfError {
    /// The instruction slice was empty.
    EmptyProgram,
    /// The program exceeds `MAX_PROGRAM_LEN`.
    ProgramTooLong { len: usize, max: usize },
    /// The last instruction is not `Exit`.
    MissingExit,
    /// A register index is out of range.
    InvalidRegister { pc: usize, reg: u8 },
    /// An instruction attempts to write to the frame-pointer register (R10).
    WriteToFramePointer { pc: usize },
    /// A scratch-memory offset is out of bounds or misaligned.
    ScratchOutOfBounds { pc: usize, offset: u16, max: usize },
    /// A jump target falls outside the program bounds.
    JumpOutOfBounds { pc: usize, target: i64 },
    /// A jump has a non-positive offset (backward or self-loop).
    BackwardJump { pc: usize, off: i16 },
    /// A division or modulo instruction uses an immediate zero divisor.
    DivisionByZero { pc: usize },
    /// A helper that requires the `bpf-maps` feature was used without it.
    HelperNotAllowed { pc: usize, helper: u8 },
    /// An unknown opcode byte was encountered during decoding.
    InvalidOpcode { word: u64 },
    /// An immediate value does not fit in the 32-bit field of the encoding.
    ImmediateTooWide { imm: i64 },
    /// The program ran out of fuel.
    FuelExhausted,
    /// A BPF map is full.
    MapFull,
    /// An internal defensive check detected unexpected corrupt state.
    InternalCorrupt,
}

impl core::fmt::Display for BpfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyProgram => write!(f, "BPF program is empty"),
            Self::ProgramTooLong { len, max } => write!(
                f,
                "BPF program too long: {} instructions (max {})",
                len, max
            ),
            Self::MissingExit => write!(f, "BPF program missing Exit as last instruction"),
            Self::InvalidRegister { pc, reg } => {
                write!(f, "BPF pc={}: invalid register R{}", pc, reg)
            }
            Self::WriteToFramePointer { pc } => {
                write!(f, "BPF pc={}: write to read-only frame pointer R10", pc)
            }
            Self::ScratchOutOfBounds { pc, offset, max } => write!(
                f,
                "BPF pc={}: scratch offset {} out of bounds (max {})",
                pc, offset, max
            ),
            Self::JumpOutOfBounds { pc, target } => {
                write!(f, "BPF pc={}: jump target {} out of bounds", pc, target)
            }
            Self::BackwardJump { pc, off } => {
                write!(f, "BPF pc={}: backward/self jump (off={})", pc, off)
            }
            Self::DivisionByZero { pc } => write!(f, "BPF pc={}: static division by zero", pc),
            Self::HelperNotAllowed { pc, helper } => write!(
                f,
                "BPF pc={}: helper {} not allowed (missing bpf-maps feature)",
                pc, helper
            ),
            Self::InvalidOpcode { word } => write!(f, "BPF invalid opcode in word {:#018x}", word),
            Self::ImmediateTooWide { imm } => {
                write!(f, "BPF immediate {} does not fit in i32", imm)
            }
            Self::FuelExhausted => write!(f, "BPF program ran out of fuel"),
            Self::MapFull => write!(f, "BPF map is full"),
            Self::InternalCorrupt => write!(f, "BPF internal state is corrupt"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BpfError {}

// ---------------------------------------------------------------------------
// BpfRegistry — global per-tracepoint program store
// ---------------------------------------------------------------------------

/// Statistics for a single tracepoint attachment.
#[derive(Debug, Clone, Copy)]
pub struct BpfStats {
    /// Number of times the program was executed.
    pub runs: u64,
    /// Number of times the program returned an error.
    pub errors: u64,
    /// Sum of R0 values returned by successful runs.
    pub output: u64,
}

/// Per-slot state: one slot per `TracepointId`.
struct RegistrySlot {
    program: spin::Mutex<Option<VerifiedProgram>>,
    runs: AtomicU64,
    errors: AtomicU64,
    output: AtomicU64,
}

impl RegistrySlot {
    const fn new() -> Self {
        Self {
            program: spin::Mutex::new(None),
            runs: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            output: AtomicU64::new(0),
        }
    }
}

// We need exactly TracepointId::COUNT = 20 slots.
// Rust const generics allow a const-size array; we initialise each element
// manually because `Copy` is not derivable for `spin::Mutex`.
macro_rules! registry_slots {
    () => {
        [
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
            RegistrySlot::new(),
        ]
    };
}

struct BpfRegistry {
    slots: [RegistrySlot; TracepointId::COUNT],
}

// SAFETY: `spin::Mutex` is `Send + Sync`; `AtomicU64` is `Send + Sync`.
unsafe impl Send for BpfRegistry {}
unsafe impl Sync for BpfRegistry {}

impl BpfRegistry {
    const fn new() -> Self {
        Self {
            slots: registry_slots!(),
        }
    }
}

lazy_static::lazy_static! {
    static ref BPF_REGISTRY: BpfRegistry = BpfRegistry::new();
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Verify `prog` and attach it to tracepoint `tp`.
///
/// Replaces any previously attached program.
///
/// # Errors
///
/// Returns the first `BpfError` from the verifier.
pub fn attach(tp: TracepointId, prog: &[Instruction]) -> Result<(), BpfError> {
    let vp = verify(prog)?;
    attach_verified(tp, vp);
    Ok(())
}

/// Attach a pre-verified program to tracepoint `tp`.
pub fn attach_verified(tp: TracepointId, prog: VerifiedProgram) {
    let idx = tp as usize;
    *BPF_REGISTRY.slots[idx].program.lock() = Some(prog);
}

/// Detach and drop the program attached to tracepoint `tp`.
pub fn detach(tp: TracepointId) {
    let idx = tp as usize;
    *BPF_REGISTRY.slots[idx].program.lock() = None;
}

/// Return the accumulated output (sum of R0 return values) for `tp`.
pub fn read_output(tp: TracepointId) -> u64 {
    BPF_REGISTRY.slots[tp as usize]
        .output
        .load(Ordering::Relaxed)
}

/// Return run/error/output statistics for `tp`.
pub fn read_stats(tp: TracepointId) -> BpfStats {
    let s = &BPF_REGISTRY.slots[tp as usize];
    BpfStats {
        runs: s.runs.load(Ordering::Relaxed),
        errors: s.errors.load(Ordering::Relaxed),
        output: s.output.load(Ordering::Relaxed),
    }
}

/// Reset the output accumulator for `tp` to zero.
pub fn reset_output(tp: TracepointId) {
    BPF_REGISTRY.slots[tp as usize]
        .output
        .store(0, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Hot path: called from observability::trace_event
// ---------------------------------------------------------------------------

/// Execute the BPF program attached to the tracepoint of `ev`, if any.
///
/// This function never panics. A failed `try_lock` is silently skipped
/// (non-blocking).
#[inline]
pub(crate) fn run_attached(ev: &crate::observability::TraceEvent) {
    let tp_idx = ev.event.tracepoint_id() as usize;
    let slot = match BPF_REGISTRY.slots.get(tp_idx) {
        Some(s) => s,
        None => return,
    };

    // Non-blocking: if the slot is currently being written (attach/detach),
    // we skip this invocation rather than stalling an interrupt context.
    if let Some(guard) = slot.program.try_lock() {
        if let Some(prog) = guard.as_ref() {
            let ctx = BpfContext::from_trace_event(ev);
            let result = Vm::new().run(prog, &ctx);
            slot.runs.fetch_add(1, Ordering::Relaxed);
            match result {
                Ok(r0) => {
                    slot.output.fetch_add(r0, Ordering::Relaxed);
                }
                Err(_) => {
                    slot.errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::{EventType, TraceEvent, TracepointId};

    // Use a tracepoint that is unlikely to conflict with other tests.
    const TP: TracepointId = TracepointId::LockRelease;

    fn clean_slot() {
        detach(TP);
        reset_output(TP);
        BPF_REGISTRY.slots[TP as usize]
            .runs
            .store(0, Ordering::Relaxed);
        BPF_REGISTRY.slots[TP as usize]
            .errors
            .store(0, Ordering::Relaxed);
    }

    fn make_event(tp: TracepointId) -> TraceEvent {
        TraceEvent {
            timestamp: 1234,
            cpu_id: 0,
            event: event_for(tp),
        }
    }

    fn event_for(tp: TracepointId) -> EventType {
        match tp {
            TracepointId::LockRelease => EventType::LockRelease {
                lock_addr: 0xDEAD,
                task_id: 1,
            },
            TracepointId::TaskSpawn => EventType::TaskSpawn {
                task_id: 1,
                priority: 10,
            },
            _ => EventType::TaskSpawn {
                task_id: 0,
                priority: 0,
            },
        }
    }

    #[test]
    fn test_attach_verified_and_detach() {
        clean_slot();
        let vp = verify(&[Instruction::Exit]).expect("verify");
        attach_verified(TP, vp);
        {
            let guard = BPF_REGISTRY.slots[TP as usize].program.lock();
            assert!(guard.is_some(), "program should be attached");
        }
        detach(TP);
        {
            let guard = BPF_REGISTRY.slots[TP as usize].program.lock();
            assert!(guard.is_none(), "program should be gone after detach");
        }
    }

    #[test]
    fn test_attach_rejects_invalid_program() {
        assert_eq!(attach(TP, &[]), Err(BpfError::EmptyProgram));
    }

    #[test]
    fn test_read_output_initial_zero() {
        // Use a distinct tracepoint so parallel tests don't interfere.
        let tp = TracepointId::LockAcquire;
        reset_output(tp);
        assert_eq!(read_output(tp), 0);
    }

    #[test]
    fn test_reset_output() {
        clean_slot();
        // Attach a program returning 7
        let prog = alloc::vec![
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(0),
                src: Source::Imm(7),
            },
            Instruction::Exit,
        ];
        attach(TP, &prog).expect("attach");
        let ev = make_event(TP);
        run_attached(&ev);
        assert!(read_output(TP) >= 7, "output should be >= 7");
        reset_output(TP);
        assert_eq!(read_output(TP), 0, "output should be zero after reset");
        detach(TP);
    }

    #[test]
    fn test_bpf_error_display_nonempty() {
        let variants: &[BpfError] = &[
            BpfError::EmptyProgram,
            BpfError::ProgramTooLong { len: 1, max: 0 },
            BpfError::MissingExit,
            BpfError::InvalidRegister { pc: 0, reg: 11 },
            BpfError::WriteToFramePointer { pc: 0 },
            BpfError::ScratchOutOfBounds {
                pc: 0,
                offset: 0,
                max: 0,
            },
            BpfError::JumpOutOfBounds { pc: 0, target: 999 },
            BpfError::BackwardJump { pc: 0, off: -1 },
            BpfError::DivisionByZero { pc: 0 },
            BpfError::HelperNotAllowed { pc: 0, helper: 1 },
            BpfError::InvalidOpcode { word: 0xAB << 56 },
            BpfError::ImmediateTooWide { imm: i64::MAX },
            BpfError::FuelExhausted,
            BpfError::MapFull,
            BpfError::InternalCorrupt,
        ];
        for e in variants {
            let s = alloc::format!("{}", e);
            assert!(!s.is_empty(), "Display for {:?} was empty", e);
        }
    }

    #[test]
    fn test_run_attached_increments_runs() {
        clean_slot();
        let prog = alloc::vec![
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(0),
                src: Source::Imm(7),
            },
            Instruction::Exit,
        ];
        attach(TP, &prog).expect("attach");
        let ev = make_event(TP);
        run_attached(&ev);
        let stats = read_stats(TP);
        assert_eq!(stats.runs, 1);
        assert_eq!(stats.output, 7);
        assert_eq!(stats.errors, 0);
        detach(TP);
    }

    #[test]
    fn test_run_attached_counts_errors() {
        clean_slot();
        // Build a 2-instruction program, then force fuel=1 so it exhausts
        let insns = alloc::vec![
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(0),
                src: Source::Imm(1),
            },
            Instruction::Exit,
        ];
        let vp = VerifiedProgram::with_fuel(&insns, 1).expect("verify");
        attach_verified(TP, vp);
        let ev = make_event(TP);
        run_attached(&ev);
        let stats = read_stats(TP);
        assert_eq!(stats.runs, 1);
        assert_eq!(stats.errors, 1);
        detach(TP);
    }

    #[test]
    fn test_context_from_trace_event_sets_fields() {
        let ev = TraceEvent {
            timestamp: 42,
            cpu_id: 2,
            event: EventType::PageAlloc {
                page_addr: 0x1000,
                count: 4,
            },
        };
        let ctx = BpfContext::from_trace_event(&ev);
        // PageAlloc = TracepointId::PageAlloc = 6
        assert_eq!(ctx.get(CtxField::TracepointId), 6);
        assert_eq!(ctx.get(CtxField::Timestamp), 42);
        assert_eq!(ctx.get(CtxField::CpuId), 2);
        assert_eq!(ctx.get(CtxField::Arg0), 0x1000);
        assert_eq!(ctx.get(CtxField::Arg1), 4);
    }
}
