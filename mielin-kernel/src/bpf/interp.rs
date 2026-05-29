#![allow(dead_code)]
//! MielinBPF interpreter — executes a `VerifiedProgram` against a `BpfContext`.

use super::{
    isa::{
        AluOp, CtxField, Instruction, Jcc, Source, NUM_REGS, REG_FRAME,
        REG_RETURN, SCRATCH_BYTES, SCRATCH_WORDS,
    },
    verifier::VerifiedProgram,
    BpfError,
};

// ---------------------------------------------------------------------------
// BpfContext
// ---------------------------------------------------------------------------

/// A snapshot of kernel-event fields exposed to a BPF program.
#[derive(Debug, Clone, Copy)]
pub struct BpfContext {
    fields: [u64; CtxField::COUNT],
}

impl BpfContext {
    /// Return an all-zero context.
    pub const fn new() -> Self {
        Self {
            fields: [0u64; CtxField::COUNT],
        }
    }

    /// Read a context field.
    #[inline]
    pub fn get(&self, f: CtxField) -> u64 {
        self.fields[f as usize]
    }

    /// Write a context field and return `&mut self` for chaining.
    #[inline]
    pub fn set(&mut self, f: CtxField, v: u64) -> &mut Self {
        self.fields[f as usize] = v;
        self
    }

    /// Build a `BpfContext` from a kernel `TraceEvent`.
    ///
    /// * `TracepointId` — the `u8` discriminant of the tracepoint
    /// * `Timestamp`    — the event timestamp
    /// * `CpuId`        — the cpu that emitted the event (0 if not available)
    /// * `Arg0/1/2`  — the first three meaningful payload values of the
    ///   specific `EventType` variant
    pub fn from_trace_event(ev: &crate::observability::TraceEvent) -> Self {
        use crate::observability::EventType;

        let mut ctx = Self::new();

        // Numeric tracepoint id
        ctx.fields[CtxField::TracepointId as usize] = ev.event.tracepoint_id() as u64;
        ctx.fields[CtxField::Timestamp    as usize] = ev.timestamp;
        ctx.fields[CtxField::CpuId        as usize] = ev.cpu_id as u64;

        // Map per-variant fields to Arg0/Arg1/Arg2
        match ev.event {
            EventType::TaskSpawn { task_id, priority } => {
                ctx.fields[CtxField::Arg0 as usize] = task_id as u64;
                ctx.fields[CtxField::Arg1 as usize] = priority as u64;
            }
            EventType::TaskTerminate { task_id } => {
                ctx.fields[CtxField::Arg0 as usize] = task_id as u64;
            }
            EventType::TaskSchedule { task_id, cpu_id } => {
                ctx.fields[CtxField::Arg0 as usize] = task_id as u64;
                ctx.fields[CtxField::Arg1 as usize] = cpu_id as u64;
            }
            EventType::TaskYield { task_id } => {
                ctx.fields[CtxField::Arg0 as usize] = task_id as u64;
            }
            EventType::TaskBlock { task_id, reason } => {
                ctx.fields[CtxField::Arg0 as usize] = task_id as u64;
                ctx.fields[CtxField::Arg1 as usize] = reason;
            }
            EventType::TaskWake { task_id } => {
                ctx.fields[CtxField::Arg0 as usize] = task_id as u64;
            }
            EventType::PageAlloc { page_addr, count } => {
                ctx.fields[CtxField::Arg0 as usize] = page_addr as u64;
                ctx.fields[CtxField::Arg1 as usize] = count as u64;
            }
            EventType::PageFree { page_addr, count } => {
                ctx.fields[CtxField::Arg0 as usize] = page_addr as u64;
                ctx.fields[CtxField::Arg1 as usize] = count as u64;
            }
            EventType::InterruptEntry { irq, cpu_id } => {
                ctx.fields[CtxField::Arg0 as usize] = irq as u64;
                ctx.fields[CtxField::Arg1 as usize] = cpu_id as u64;
            }
            EventType::InterruptExit { irq, duration_ns } => {
                ctx.fields[CtxField::Arg0 as usize] = irq as u64;
                ctx.fields[CtxField::Arg1 as usize] = duration_ns;
            }
            EventType::IpiSent { target_cpu, ipi_type } => {
                ctx.fields[CtxField::Arg0 as usize] = target_cpu as u64;
                ctx.fields[CtxField::Arg1 as usize] = ipi_type as u64;
            }
            EventType::IpiReceived { cpu_id, ipi_type } => {
                ctx.fields[CtxField::Arg0 as usize] = cpu_id as u64;
                ctx.fields[CtxField::Arg1 as usize] = ipi_type as u64;
            }
            EventType::TimerTick { jiffies } => {
                ctx.fields[CtxField::Arg0 as usize] = jiffies;
            }
            EventType::ContextSwitch { from_task, to_task } => {
                ctx.fields[CtxField::Arg0 as usize] = from_task as u64;
                ctx.fields[CtxField::Arg1 as usize] = to_task as u64;
            }
            EventType::SyscallEntry { syscall_nr, task_id } => {
                ctx.fields[CtxField::Arg0 as usize] = syscall_nr as u64;
                ctx.fields[CtxField::Arg1 as usize] = task_id as u64;
            }
            EventType::SyscallExit { syscall_nr, result } => {
                ctx.fields[CtxField::Arg0 as usize] = syscall_nr as u64;
                ctx.fields[CtxField::Arg1 as usize] = result as u64;
            }
            EventType::PageFault { fault_addr, flags } => {
                ctx.fields[CtxField::Arg0 as usize] = fault_addr as u64;
                ctx.fields[CtxField::Arg1 as usize] = flags as u64;
            }
            EventType::TlbFlush { cpu_id, addr } => {
                ctx.fields[CtxField::Arg0 as usize] = cpu_id as u64;
                ctx.fields[CtxField::Arg1 as usize] = addr as u64;
            }
            EventType::LockAcquire { lock_addr, task_id } => {
                ctx.fields[CtxField::Arg0 as usize] = lock_addr as u64;
                ctx.fields[CtxField::Arg1 as usize] = task_id as u64;
            }
            EventType::LockRelease { lock_addr, task_id } => {
                ctx.fields[CtxField::Arg0 as usize] = lock_addr as u64;
                ctx.fields[CtxField::Arg1 as usize] = task_id as u64;
            }
        }

        ctx
    }
}

impl Default for BpfContext {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Vm
// ---------------------------------------------------------------------------

/// A MielinBPF virtual machine with registers and a scratch-memory bank.
pub struct Vm {
    regs:    [u64; NUM_REGS],
    scratch: [u64; SCRATCH_WORDS],
}

impl Vm {
    /// Construct a new, zeroed VM.
    pub const fn new() -> Self {
        Self {
            regs:    [0u64; NUM_REGS],
            scratch: [0u64; SCRATCH_WORDS],
        }
    }

    /// Execute a verified program against `ctx`, returning the value in R0.
    ///
    /// # Errors
    ///
    /// * `FuelExhausted` — the program consumed all allocated fuel
    /// * `InternalCorrupt` — a defensive index-out-of-bounds check failed
    ///
    /// All other errors were ruled out by the verifier.
    pub fn run(
        &mut self,
        prog: &VerifiedProgram,
        ctx: &BpfContext,
    ) -> Result<u64, BpfError> {
        // Reset state
        self.regs    = [0u64; NUM_REGS];
        self.scratch = [0u64; SCRATCH_WORDS];
        // R10 = top of scratch (frame pointer, read-only by verifier rules)
        self.regs[REG_FRAME as usize] = SCRATCH_BYTES as u64;

        let insns = prog.instructions();
        let mut pc: usize = 0;
        let mut fuel: u64 = prog.fuel();

        loop {
            // Implicit program termination
            if pc >= insns.len() {
                return Ok(self.regs[REG_RETURN as usize]);
            }

            // Fuel check
            if fuel == 0 {
                return Err(BpfError::FuelExhausted);
            }
            fuel -= 1;

            let insn = insns
                .get(pc)
                .copied()
                .ok_or(BpfError::InternalCorrupt)?;
            pc += 1;

            match insn {
                // ------------------------------------------------------------
                // Alu
                // ------------------------------------------------------------
                Instruction::Alu { op, dst, src } => {
                    let rhs = self.resolve_src(src);
                    let lhs = *self
                        .regs
                        .get(dst.idx())
                        .ok_or(BpfError::InternalCorrupt)?;

                    let result = match op {
                        AluOp::Add  => lhs.wrapping_add(rhs),
                        AluOp::Sub  => lhs.wrapping_sub(rhs),
                        AluOp::Mul  => lhs.wrapping_mul(rhs),
                        AluOp::Div  => {
                            if rhs == 0 { 0 } else { lhs / rhs }
                        }
                        AluOp::Mod  => {
                            if rhs == 0 { 0 } else { lhs % rhs }
                        }
                        AluOp::And  => lhs & rhs,
                        AluOp::Or   => lhs | rhs,
                        AluOp::Xor  => lhs ^ rhs,
                        AluOp::Lsh  => lhs.wrapping_shl((rhs & 63) as u32),
                        AluOp::Rsh  => lhs.wrapping_shr((rhs & 63) as u32),
                        AluOp::Arsh => {
                            let shift = (rhs & 63) as u32;
                            ((lhs as i64).wrapping_shr(shift)) as u64
                        }
                        AluOp::Mov  => rhs,
                    };

                    *self
                        .regs
                        .get_mut(dst.idx())
                        .ok_or(BpfError::InternalCorrupt)? = result;
                }

                // ------------------------------------------------------------
                // LoadMem
                // ------------------------------------------------------------
                Instruction::LoadMem { dst, offset } => {
                    let idx = offset as usize / 8;
                    let val = *self
                        .scratch
                        .get(idx)
                        .ok_or(BpfError::InternalCorrupt)?;
                    *self
                        .regs
                        .get_mut(dst.idx())
                        .ok_or(BpfError::InternalCorrupt)? = val;
                }

                // ------------------------------------------------------------
                // StoreMem
                // ------------------------------------------------------------
                Instruction::StoreMem { src, offset } => {
                    let val = self.resolve_src(src);
                    let idx = offset as usize / 8;
                    *self
                        .scratch
                        .get_mut(idx)
                        .ok_or(BpfError::InternalCorrupt)? = val;
                }

                // ------------------------------------------------------------
                // LoadCtx
                // ------------------------------------------------------------
                Instruction::LoadCtx { dst, field } => {
                    let val = ctx.get(field);
                    *self
                        .regs
                        .get_mut(dst.idx())
                        .ok_or(BpfError::InternalCorrupt)? = val;
                }

                // ------------------------------------------------------------
                // Jmp
                // ------------------------------------------------------------
                Instruction::Jmp { off } => {
                    // The verifier guarantees off > 0 and target in bounds.
                    pc = (pc as i64 + off as i64) as usize;
                }

                // ------------------------------------------------------------
                // JmpIf
                // ------------------------------------------------------------
                Instruction::JmpIf { cc, dst, src, off } => {
                    let a = *self
                        .regs
                        .get(dst.idx())
                        .ok_or(BpfError::InternalCorrupt)?;
                    let b = self.resolve_src(src);

                    let taken = match cc {
                        Jcc::Eq  => a == b,
                        Jcc::Ne  => a != b,
                        Jcc::Gt  => a > b,
                        Jcc::Ge  => a >= b,
                        Jcc::Lt  => a < b,
                        Jcc::Le  => a <= b,
                        Jcc::Sgt => (a as i64) > (b as i64),
                        Jcc::Sge => (a as i64) >= (b as i64),
                        Jcc::Slt => (a as i64) < (b as i64),
                        Jcc::Sle => (a as i64) <= (b as i64),
                    };

                    if taken {
                        pc = (pc as i64 + off as i64) as usize;
                    }
                }

                // ------------------------------------------------------------
                // Call — Nop is the only verified helper without bpf-maps
                // ------------------------------------------------------------
                Instruction::Call { helper: _ } => {
                    // Nop: no-operation; map helpers not yet implemented.
                }

                // ------------------------------------------------------------
                // Exit
                // ------------------------------------------------------------
                Instruction::Exit => {
                    return Ok(self.regs[REG_RETURN as usize]);
                }
            }
        }
    }

    /// Resolve a `Source` to its concrete `u64` value.
    #[inline]
    fn resolve_src(&self, src: Source) -> u64 {
        match src {
            Source::Reg(r) => {
                // Defensive: verifier ensures r is valid; fall back to 0 on
                // corrupt data rather than panicking.
                self.regs.get(r.idx()).copied().unwrap_or(0)
            }
            Source::Imm(v) => v as u64,
        }
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bpf::{
        isa::{AluOp, CtxField, HelperId, Instruction, Jcc, Reg, Source},
        verifier::{verify, VerifiedProgram},
    };
    use crate::observability::{EventType, TraceEvent};

    // -----------------------------------------------------------------------
    // Tiny helpers to build programs quickly
    // -----------------------------------------------------------------------

    fn mov_imm(dst: u8, v: i64) -> Instruction {
        Instruction::Alu {
            op:  AluOp::Mov,
            dst: Reg(dst),
            src: Source::Imm(v),
        }
    }

    fn mov_reg(dst: u8, src: u8) -> Instruction {
        Instruction::Alu {
            op:  AluOp::Mov,
            dst: Reg(dst),
            src: Source::Reg(Reg(src)),
        }
    }

    fn run_prog(insns: alloc::vec::Vec<Instruction>) -> Result<u64, BpfError> {
        let vp = verify(&insns).expect("verify");
        let ctx = BpfContext::new();
        Vm::new().run(&vp, &ctx)
    }

    fn run_prog_ctx(
        insns: alloc::vec::Vec<Instruction>,
        ctx: &BpfContext,
    ) -> Result<u64, BpfError> {
        let vp = verify(&insns).expect("verify");
        Vm::new().run(&vp, ctx)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_exec_mov_imm_42() {
        let result = run_prog(alloc::vec![mov_imm(0, 42), Instruction::Exit]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_add() {
        let result = run_prog(alloc::vec![
            mov_imm(1, 10),
            mov_imm(2, 32),
            Instruction::Alu {
                op:  AluOp::Add,
                dst: Reg(0),
                src: Source::Reg(Reg(1)),
            },
            Instruction::Alu {
                op:  AluOp::Add,
                dst: Reg(0),
                src: Source::Reg(Reg(2)),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_sub() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 100),
            Instruction::Alu {
                op:  AluOp::Sub,
                dst: Reg(0),
                src: Source::Imm(58),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_mul() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 6),
            Instruction::Alu {
                op:  AluOp::Mul,
                dst: Reg(0),
                src: Source::Imm(7),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_div_normal() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 84),
            Instruction::Alu {
                op:  AluOp::Div,
                dst: Reg(0),
                src: Source::Imm(2),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_div_by_zero_yields_zero() {
        // Runtime div-by-zero: divisor in a register set to 0 at runtime.
        // The verifier only catches *immediate* zeros, so Imm(1) passes
        // verification but we can then use a register-sourced zero at runtime.
        let result = run_prog(alloc::vec![
            mov_imm(0, 100),
            mov_imm(1, 0),   // R1 = 0 (runtime zero, not caught by verifier)
            Instruction::Alu {
                op:  AluOp::Div,
                dst: Reg(0),
                src: Source::Reg(Reg(1)),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn test_exec_mod() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 45),
            Instruction::Alu {
                op:  AluOp::Mod,
                dst: Reg(0),
                src: Source::Imm(3),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn test_exec_and() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 0xFF),
            Instruction::Alu {
                op:  AluOp::And,
                dst: Reg(0),
                src: Source::Imm(0x42),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0x42));
    }

    #[test]
    fn test_exec_or() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 0x40),
            Instruction::Alu {
                op:  AluOp::Or,
                dst: Reg(0),
                src: Source::Imm(0x02),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0x42));
    }

    #[test]
    fn test_exec_xor() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 0x47),
            Instruction::Alu {
                op:  AluOp::Xor,
                dst: Reg(0),
                src: Source::Imm(0x05),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0x42));
    }

    #[test]
    fn test_exec_lsh() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 1),
            Instruction::Alu {
                op:  AluOp::Lsh,
                dst: Reg(0),
                src: Source::Imm(6),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(64));
    }

    #[test]
    fn test_exec_rsh() {
        let result = run_prog(alloc::vec![
            mov_imm(0, 84),
            Instruction::Alu {
                op:  AluOp::Rsh,
                dst: Reg(0),
                src: Source::Imm(1),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_jmp_forward() {
        // Unconditional Jmp(off=1) from pc=0 skips pc=1 (Mov R0,99)
        // so R0 remains 0.
        let result = run_prog(alloc::vec![
            Instruction::Jmp { off: 1 },
            mov_imm(0, 99),          // skipped
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn test_exec_jmpif_taken() {
        // R1 = 42; JmpIf(Eq, R1, 42) → taken, skip bad assignment
        let result = run_prog(alloc::vec![
            mov_imm(1, 42),
            Instruction::JmpIf {
                cc:  Jcc::Eq,
                dst: Reg(1),
                src: Source::Imm(42),
                off: 1,
            },
            mov_imm(0, 99),          // skipped
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn test_exec_jmpif_not_taken() {
        // R1 = 42; JmpIf(Eq, R1, 99) → not taken, falls through to Mov R0, 7
        let result = run_prog(alloc::vec![
            mov_imm(1, 42),
            Instruction::JmpIf {
                cc:  Jcc::Eq,
                dst: Reg(1),
                src: Source::Imm(99),
                off: 1,
            },
            mov_imm(0, 7),           // executed
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(7));
    }

    #[test]
    fn test_exec_loadctx_arg0() {
        let mut ctx = BpfContext::new();
        ctx.set(CtxField::Arg0, 77);
        let result = run_prog_ctx(
            alloc::vec![
                Instruction::LoadCtx { dst: Reg(0), field: CtxField::Arg0 },
                Instruction::Exit,
            ],
            &ctx,
        );
        assert_eq!(result, Ok(77));
    }

    #[test]
    fn test_exec_store_load_scratch() {
        // Store 42 into scratch[0], then load it into R0.
        let result = run_prog(alloc::vec![
            mov_imm(1, 42),
            Instruction::StoreMem {
                src:    Source::Reg(Reg(1)),
                offset: 0,
            },
            Instruction::LoadMem { dst: Reg(0), offset: 0 },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_fuel_exhausted() {
        // Build a valid 2-instruction program, then override fuel to 1 so
        // the loop exhausts it immediately.
        let insns = alloc::vec![mov_imm(0, 1), Instruction::Exit];
        let vp = VerifiedProgram::with_fuel(&insns, 1).expect("verify");
        let ctx = BpfContext::new();
        let result = Vm::new().run(&vp, &ctx);
        assert_eq!(result, Err(BpfError::FuelExhausted));
    }

    #[test]
    fn test_exec_arsh() {
        // -84 >> 1 = -42 (arithmetic right shift)
        let result = run_prog(alloc::vec![
            mov_imm(0, -84_i32 as i64),
            Instruction::Alu {
                op:  AluOp::Arsh,
                dst: Reg(0),
                src: Source::Imm(1),
            },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(-42_i64 as u64));
    }

    #[test]
    fn test_exec_mov_reg() {
        let result = run_prog(alloc::vec![
            mov_imm(1, 42),
            mov_reg(0, 1),
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_context_from_trace_event_sets_fields() {
        let ev = TraceEvent {
            timestamp: 9999,
            cpu_id:    3,
            event:     EventType::TaskSpawn { task_id: 77, priority: 5 },
        };
        let ctx = BpfContext::from_trace_event(&ev);
        assert_eq!(ctx.get(CtxField::TracepointId), 0); // TaskSpawn = 0
        assert_eq!(ctx.get(CtxField::Timestamp),    9999);
        assert_eq!(ctx.get(CtxField::CpuId),        3);
        assert_eq!(ctx.get(CtxField::Arg0),         77);
        assert_eq!(ctx.get(CtxField::Arg1),         5);
    }

    #[test]
    fn test_exec_call_nop() {
        // Call{Nop} must be a no-op and not crash.
        let result = run_prog(alloc::vec![
            mov_imm(0, 42),
            Instruction::Call { helper: HelperId::Nop },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn test_exec_store_load_scratch_word_boundary() {
        // Write to scratch[7] (offset=56), read back.
        let result = run_prog(alloc::vec![
            mov_imm(1, 0xDEAD_BEEF),
            Instruction::StoreMem {
                src:    Source::Reg(Reg(1)),
                offset: 56,
            },
            Instruction::LoadMem { dst: Reg(0), offset: 56 },
            Instruction::Exit,
        ]);
        assert_eq!(result, Ok(0xDEAD_BEEF));
    }
}
