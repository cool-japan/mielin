#![allow(dead_code)]
//! MielinBPF verifier — static analysis pass that validates a program before
//! allowing it to be executed by the interpreter.

#[cfg(not(feature = "bpf-maps"))]
use super::isa::HelperId;
use super::{
    isa::{
        AluOp, Instruction, Reg, Source, DEFAULT_FUEL, MAX_PROGRAM_LEN, MIN_FUEL, REG_FRAME,
        SCRATCH_BYTES,
    },
    BpfError,
};

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

// ---------------------------------------------------------------------------
// VerifiedProgram
// ---------------------------------------------------------------------------

/// A program that has passed the verifier.
///
/// Holds a private copy of the instruction sequence together with the computed
/// fuel budget.  The interpreter accepts only `VerifiedProgram` instances.
#[derive(Debug)]
pub struct VerifiedProgram {
    pub(super) instructions: Vec<Instruction>,
    pub(super) fuel: u64,
}

impl VerifiedProgram {
    /// Number of instructions.
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// `true` if the program contains no instructions.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    /// Initial fuel budget.
    pub fn fuel(&self) -> u64 {
        self.fuel
    }

    /// Slice view of the instruction sequence.
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    /// Build a `VerifiedProgram` with an explicit fuel override.
    /// Only available in test or when the `std` feature is enabled so
    /// unit tests can exercise `FuelExhausted`.
    #[cfg(any(test, feature = "std"))]
    pub fn with_fuel(prog: &[Instruction], fuel: u64) -> Result<Self, BpfError> {
        let mut vp = verify(prog)?;
        vp.fuel = fuel;
        Ok(vp)
    }
}

// ---------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------

/// Verify a slice of instructions and, on success, return a `VerifiedProgram`.
///
/// # Errors
///
/// Returns the first `BpfError` violation found during the linear scan.
pub fn verify(prog: &[Instruction]) -> Result<VerifiedProgram, BpfError> {
    // 1. Non-empty check
    if prog.is_empty() {
        return Err(BpfError::EmptyProgram);
    }

    // 2. Length limit
    if prog.len() > MAX_PROGRAM_LEN {
        return Err(BpfError::ProgramTooLong {
            len: prog.len(),
            max: MAX_PROGRAM_LEN,
        });
    }

    // 3. Must end with Exit
    match prog.last() {
        Some(Instruction::Exit) => {}
        _ => return Err(BpfError::MissingExit),
    }

    // 4. Per-instruction checks
    for (pc, insn) in prog.iter().enumerate() {
        match *insn {
            // ----------------------------------------------------------------
            // Alu
            // ----------------------------------------------------------------
            Instruction::Alu { op, dst, src } => {
                check_reg(pc, dst)?;
                reject_frame_write(pc, dst)?;
                check_src_reg(pc, src)?;
                // Static division-by-zero detection
                if matches!(op, AluOp::Div | AluOp::Mod) {
                    if let Source::Imm(0) = src {
                        return Err(BpfError::DivisionByZero { pc });
                    }
                }
            }

            // ----------------------------------------------------------------
            // LoadMem
            // ----------------------------------------------------------------
            Instruction::LoadMem { dst, offset } => {
                check_reg(pc, dst)?;
                reject_frame_write(pc, dst)?;
                check_scratch_bounds(pc, offset)?;
            }

            // ----------------------------------------------------------------
            // StoreMem
            // ----------------------------------------------------------------
            Instruction::StoreMem { src, offset } => {
                check_src_reg(pc, src)?;
                check_scratch_bounds(pc, offset)?;
            }

            // ----------------------------------------------------------------
            // LoadCtx
            // ----------------------------------------------------------------
            Instruction::LoadCtx { dst, field: _ } => {
                check_reg(pc, dst)?;
                reject_frame_write(pc, dst)?;
            }

            // ----------------------------------------------------------------
            // Jmp
            // ----------------------------------------------------------------
            Instruction::Jmp { off } => {
                check_jump(pc, off, prog.len())?;
            }

            // ----------------------------------------------------------------
            // JmpIf
            // ----------------------------------------------------------------
            Instruction::JmpIf {
                cc: _,
                dst,
                src,
                off,
            } => {
                check_reg(pc, dst)?;
                check_src_reg(pc, src)?;
                check_jump(pc, off, prog.len())?;
            }

            // ----------------------------------------------------------------
            // Call
            // ----------------------------------------------------------------
            Instruction::Call { helper } => {
                // When the bpf-maps feature is absent, only Nop is permitted.
                #[cfg(not(feature = "bpf-maps"))]
                if helper != HelperId::Nop {
                    return Err(BpfError::HelperNotAllowed {
                        pc,
                        helper: helper as u8,
                    });
                }
                // Suppress unused-variable warning when bpf-maps is active.
                let _ = helper;
            }

            // ----------------------------------------------------------------
            // Exit — always valid; already checked as last insn
            // ----------------------------------------------------------------
            Instruction::Exit => {}
        }
    }

    // 5. Compute fuel budget
    let fuel = (prog.len() as u64).clamp(MIN_FUEL, DEFAULT_FUEL);

    Ok(VerifiedProgram {
        instructions: prog.to_vec(),
        fuel,
    })
}

// ---------------------------------------------------------------------------
// Helper predicates
// ---------------------------------------------------------------------------

/// Reject invalid register indices.
#[inline]
fn check_reg(pc: usize, r: Reg) -> Result<(), BpfError> {
    if !r.is_valid() {
        Err(BpfError::InvalidRegister { pc, reg: r.0 })
    } else {
        Ok(())
    }
}

/// Reject writes to the frame-pointer register (R10).
#[inline]
fn reject_frame_write(pc: usize, dst: Reg) -> Result<(), BpfError> {
    if dst.0 == REG_FRAME {
        Err(BpfError::WriteToFramePointer { pc })
    } else {
        Ok(())
    }
}

/// Validate the register in a `Source::Reg` operand.
#[inline]
fn check_src_reg(pc: usize, src: Source) -> Result<(), BpfError> {
    if let Source::Reg(r) = src {
        check_reg(pc, r)?;
    }
    Ok(())
}

/// Validate a scratch-memory offset: must be 8-aligned and within bounds.
#[inline]
fn check_scratch_bounds(pc: usize, offset: u16) -> Result<(), BpfError> {
    let off = offset as usize;
    if !off.is_multiple_of(8) || off + 8 > SCRATCH_BYTES {
        Err(BpfError::ScratchOutOfBounds {
            pc,
            offset,
            max: SCRATCH_BYTES,
        })
    } else {
        Ok(())
    }
}

/// Validate a jump offset: forward-only, target within program bounds.
#[inline]
fn check_jump(pc: usize, off: i16, prog_len: usize) -> Result<(), BpfError> {
    if off <= 0 {
        return Err(BpfError::BackwardJump { pc, off });
    }
    let target = pc as i64 + 1 + off as i64;
    if target < 0 || target > prog_len as i64 {
        return Err(BpfError::JumpOutOfBounds { pc, target });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bpf::isa::{AluOp, CtxField, Instruction, Reg, Source, SCRATCH_BYTES};

    fn exit_prog() -> alloc::vec::Vec<Instruction> {
        alloc::vec![Instruction::Exit]
    }

    #[test]
    fn test_verify_minimal_exit() {
        let vp = verify(&exit_prog());
        assert!(vp.is_ok());
    }

    #[test]
    fn test_verify_mov_then_exit() {
        let prog = alloc::vec![
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(0),
                src: Source::Imm(42),
            },
            Instruction::Exit,
        ];
        let vp = verify(&prog).expect("verify");
        assert_eq!(vp.len(), 2);
        assert_eq!(vp.fuel(), 16); // max(2, 16) = 16 = MIN_FUEL
    }

    #[test]
    fn test_verify_forward_jump() {
        // pc=0: JmpIf(Eq, R1, Imm(0), off=1) → skips pc=1 if R1==0
        // pc=1: Alu Mov R0, 99
        // pc=2: Exit
        let prog = alloc::vec![
            Instruction::JmpIf {
                cc: crate::bpf::isa::Jcc::Eq,
                dst: Reg(1),
                src: Source::Imm(0),
                off: 1,
            },
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(0),
                src: Source::Imm(99),
            },
            Instruction::Exit,
        ];
        assert!(verify(&prog).is_ok());
    }

    #[test]
    fn test_verify_rejects_empty() {
        assert_eq!(verify(&[]).unwrap_err(), BpfError::EmptyProgram);
    }

    #[test]
    fn test_verify_rejects_missing_exit() {
        let prog = alloc::vec![Instruction::Alu {
            op: AluOp::Mov,
            dst: Reg(0),
            src: Source::Imm(1),
        }];
        assert_eq!(verify(&prog).unwrap_err(), BpfError::MissingExit);
    }

    #[test]
    fn test_verify_rejects_too_long() {
        let mut prog = alloc::vec![Instruction::Exit; MAX_PROGRAM_LEN + 1];
        // The last one must remain Exit to pass that check first; the length
        // check fires before per-instruction checks.
        prog[MAX_PROGRAM_LEN] = Instruction::Exit;
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::ProgramTooLong {
                len: MAX_PROGRAM_LEN + 1,
                max: MAX_PROGRAM_LEN,
            }
        );
    }

    #[test]
    fn test_verify_rejects_invalid_reg() {
        let prog = alloc::vec![
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(11), // invalid
                src: Source::Imm(0),
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::InvalidRegister { pc: 0, reg: 11 }
        );
    }

    #[test]
    fn test_verify_rejects_write_r10() {
        let prog = alloc::vec![
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(10), // frame pointer — read-only
                src: Source::Imm(0),
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::WriteToFramePointer { pc: 0 }
        );
    }

    #[test]
    fn test_verify_rejects_scratch_oob() {
        let prog = alloc::vec![
            Instruction::LoadMem {
                dst: Reg(0),
                offset: SCRATCH_BYTES as u16, // one past the end
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::ScratchOutOfBounds {
                pc: 0,
                offset: SCRATCH_BYTES as u16,
                max: SCRATCH_BYTES,
            }
        );
    }

    #[test]
    fn test_verify_rejects_unaligned_scratch() {
        let prog = alloc::vec![
            Instruction::LoadMem {
                dst: Reg(0),
                offset: 3
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::ScratchOutOfBounds {
                pc: 0,
                offset: 3,
                max: SCRATCH_BYTES
            }
        );
    }

    #[test]
    fn test_verify_rejects_backward_jump() {
        let prog = alloc::vec![
            Instruction::JmpIf {
                cc: crate::bpf::isa::Jcc::Eq,
                dst: Reg(0),
                src: Source::Imm(0),
                off: -1,
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::BackwardJump { pc: 0, off: -1 }
        );
    }

    #[test]
    fn test_verify_rejects_self_jump() {
        // off<=0 → BackwardJump per spec.
        let prog = alloc::vec![
            Instruction::JmpIf {
                cc: crate::bpf::isa::Jcc::Eq,
                dst: Reg(0),
                src: Source::Imm(0),
                off: 0,
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::BackwardJump { pc: 0, off: 0 }
        );
    }

    #[test]
    fn test_verify_rejects_jump_past_end() {
        // 3-instruction program; off=100 from pc=0 → target=101, out of bounds
        let prog = alloc::vec![
            Instruction::JmpIf {
                cc: crate::bpf::isa::Jcc::Eq,
                dst: Reg(0),
                src: Source::Imm(0),
                off: 100,
            },
            Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(0),
                src: Source::Imm(0),
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::JumpOutOfBounds { pc: 0, target: 101 }
        );
    }

    #[test]
    fn test_verify_rejects_div_by_zero() {
        let prog = alloc::vec![
            Instruction::Alu {
                op: AluOp::Div,
                dst: Reg(0),
                src: Source::Imm(0),
            },
            Instruction::Exit,
        ];
        assert_eq!(
            verify(&prog).unwrap_err(),
            BpfError::DivisionByZero { pc: 0 }
        );
    }

    #[test]
    fn test_verify_fuel_clamped() {
        // Single Exit → prog.len()=1, max(1,16)=16
        let vp = verify(&exit_prog()).expect("verify");
        assert_eq!(vp.fuel(), MIN_FUEL);
    }

    #[test]
    fn test_verify_fuel_for_long_program() {
        // 100-instruction program: 99 Mov insns + Exit
        let mut prog: alloc::vec::Vec<Instruction> = (0..99)
            .map(|_| Instruction::Alu {
                op: AluOp::Mov,
                dst: Reg(0),
                src: Source::Imm(0),
            })
            .collect();
        prog.push(Instruction::Exit);
        let vp = verify(&prog).expect("verify");
        assert_eq!(vp.fuel(), 100);
    }

    #[test]
    fn test_verify_loadctx_valid() {
        let prog = alloc::vec![
            Instruction::LoadCtx {
                dst: Reg(0),
                field: CtxField::Timestamp
            },
            Instruction::Exit,
        ];
        assert!(verify(&prog).is_ok());
    }

    #[test]
    fn test_verify_scratch_max_valid_offset() {
        // Last valid offset: SCRATCH_BYTES - 8
        let offset = (SCRATCH_BYTES - 8) as u16;
        let prog = alloc::vec![
            Instruction::LoadMem {
                dst: Reg(0),
                offset
            },
            Instruction::Exit,
        ];
        assert!(verify(&prog).is_ok(), "offset {} should be valid", offset);
    }
}
