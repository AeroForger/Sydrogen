pub mod encoder;

use std::collections::HashMap;

use crate::backend::x86_64::encoder::{Encoder, Reg, SYSV_ARG_REGS};
use crate::errors::{ForgeError, ForgeResult};
use crate::ir::{InputKind, IrBinOp, IrInst, IrProgram, IrTerminator, Operand, PrintKind, VReg};

pub struct NativeX86Backend {
    pub encoder: Encoder,
    pub func_offsets: HashMap<String, usize>,
    user_code_vaddr: u64,
}

#[derive(Clone, Copy)]
enum VRegLocation {
    Reg(Reg),
    Spill(i32),
}

const ALLOCATABLE_REGS: [Reg; 5] = [Reg::RBX, Reg::R12, Reg::R13, Reg::R14, Reg::R15];
const CALLEE_SAVE_SLOTS: [i32; 5] = [8, 16, 24, 32, 40];

fn operand_vregs(op: &Operand, uses: &mut Vec<VReg>) {
    if let Operand::Reg(vreg) = op {
        uses.push(*vreg);
    }
}

fn inst_vregs(inst: &IrInst, uses: &mut Vec<VReg>, defs: &mut Vec<VReg>) {
    match inst {
        IrInst::Const { dest, .. }
        | IrInst::ConstStr { dest, .. }
        | IrInst::Input { dest, .. }
        | IrInst::GetErrorCode { dest }
        | IrInst::GetErrorMessage { dest }
        | IrInst::GetErrorTypeName { dest }
        | IrInst::Param { dest, .. }
        | IrInst::LoadVar { dest, .. } => defs.push(*dest),
        IrInst::Copy { dest, src } => {
            defs.push(*dest);
            operand_vregs(src, uses);
        }
        IrInst::BinOp { dest, lhs, rhs, .. } => {
            defs.push(*dest);
            operand_vregs(lhs, uses);
            operand_vregs(rhs, uses);
        }
        IrInst::Call { dest, args, .. } => {
            defs.push(*dest);
            for arg in args {
                operand_vregs(arg, uses);
            }
        }
        IrInst::StoreVar { src, .. }
        | IrInst::Print { src, .. }
        | IrInst::PrintInline { src, .. }
        | IrInst::SysExit { status: src } => operand_vregs(src, uses),
        IrInst::SetError {
            code,
            message,
            type_name,
        } => {
            operand_vregs(code, uses);
            operand_vregs(message, uses);
            operand_vregs(type_name, uses);
        }
        IrInst::PrintConstStr { .. } | IrInst::ClearError => {}
    }
}

fn allocate_vregs(func: &crate::ir::IrFunction) -> (HashMap<VReg, VRegLocation>, usize) {
    let mut ranges: HashMap<VReg, (usize, usize)> = HashMap::new();
    let mut position = 0;
    for block in &func.blocks {
        for inst in &block.insts {
            let mut uses = Vec::new();
            let mut defs = Vec::new();
            inst_vregs(inst, &mut uses, &mut defs);
            for vreg in uses.into_iter().chain(defs) {
                ranges
                    .entry(vreg)
                    .and_modify(|range| range.1 = position)
                    .or_insert((position, position));
            }
            position += 1;
        }
        if let IrTerminator::Ret(Some(op)) | IrTerminator::Branch { cond: op, .. } =
            &block.terminator
        {
            let mut uses = Vec::new();
            operand_vregs(op, &mut uses);
            for vreg in uses {
                ranges
                    .entry(vreg)
                    .and_modify(|range| range.1 = position)
                    .or_insert((position, position));
            }
        }
        position += 1;
    }

    let mut intervals: Vec<(VReg, usize, usize)> = ranges
        .into_iter()
        .map(|(vreg, (start, end))| (vreg, start, end))
        .collect();
    intervals.sort_by_key(|(_, start, _)| *start);

    let mut active: Vec<(VReg, usize, Reg)> = Vec::new();
    let mut locations = HashMap::new();
    let mut spill_count: usize = 0;
    for (vreg, start, end) in intervals {
        active.retain(|(_, active_end, _)| *active_end >= start);
        let used: Vec<Reg> = active.iter().map(|(_, _, reg)| *reg).collect();
        if let Some(reg) = ALLOCATABLE_REGS
            .iter()
            .copied()
            .find(|reg| !used.contains(reg))
        {
            locations.insert(vreg, VRegLocation::Reg(reg));
            active.push((vreg, end, reg));
        } else if let Some((spill_vreg, spill_end, spill_reg)) = active
            .iter()
            .copied()
            .min_by_key(|(_, active_end, _)| *active_end)
        {
            if spill_end > end {
                let offset = 48 + (spill_count as i32) * 8;
                spill_count += 1;
                locations.insert(spill_vreg, VRegLocation::Spill(offset));
                locations.insert(vreg, VRegLocation::Reg(spill_reg));
                active.retain(|(active_vreg, _, _)| *active_vreg != spill_vreg);
                active.push((vreg, end, spill_reg));
            } else {
                let offset = 48 + (spill_count as i32) * 8;
                spill_count += 1;
                locations.insert(vreg, VRegLocation::Spill(offset));
            }
        }
    }
    (locations, spill_count)
}

impl NativeX86Backend {
    pub fn new(user_code_vaddr: u64) -> Self {
        Self {
            encoder: Encoder::new(),
            func_offsets: HashMap::new(),
            user_code_vaddr,
        }
    }

    pub fn compile_program(
        mut self,
        ir: &IrProgram,
    ) -> ForgeResult<(Vec<u8>, HashMap<String, usize>)> {
        let mut pending_calls: Vec<(usize, String)> = Vec::new();
        let mut pending_helper_calls: Vec<(usize, HelperKind)> = Vec::new();
        let mut string_literal_patches: Vec<(usize, usize)> = Vec::new(); // (patch_pos of imm64, string_id)
        let mut string_literals: Vec<String> = Vec::new();
        let mut string_id_map: HashMap<String, usize> = HashMap::new();

        for func in &ir.functions {
            let func_start = self.encoder.len();
            self.func_offsets.insert(func.name.clone(), func_start);

            let (vreg_locations, spill_count) = allocate_vregs(func);
            let mut var_offsets: HashMap<String, i32> = HashMap::new();
            let mut current_offset = 48 + (spill_count as i32) * 8;

            for block in &func.blocks {
                for inst in &block.insts {
                    match inst {
                        IrInst::LoadVar { var_name, .. } | IrInst::StoreVar { var_name, .. } => {
                            if !var_offsets.contains_key(var_name) {
                                var_offsets.insert(var_name.clone(), current_offset);
                                current_offset += 8;
                            }
                        }
                        _ => {}
                    }
                }
            }

            let raw_stack_size = (current_offset - 8) as u32;
            let stack_frame_size = (raw_stack_size + 15) & !15;

            // Prologue
            self.encoder.push_rbp();
            self.encoder.mov_rbp_rsp();
            if stack_frame_size > 0 {
                self.encoder.sub_rsp_imm32(stack_frame_size);
            }
            for (reg, slot) in ALLOCATABLE_REGS.iter().zip(CALLEE_SAVE_SLOTS) {
                self.encoder.store_rbp_disp32(slot, *reg);
            }

            let mut block_offsets: HashMap<String, usize> = HashMap::new();
            let mut pending_jumps: Vec<(usize, String)> = Vec::new();

            for block in &func.blocks {
                let block_start = self.encoder.len();
                block_offsets.insert(block.label.clone(), block_start);

                for inst in &block.insts {
                    match inst {
                        IrInst::Param { dest, index } => {
                            if *index >= 6 {
                                return Err(ForgeError::codegen(format!(
                                    "Parameter index {} exceeds supported 6 SysV registers",
                                    index
                                )));
                            }
                            let src_reg = SYSV_ARG_REGS[*index];
                            self.store_vreg(*dest, src_reg, &vreg_locations);
                        }
                        IrInst::Const { dest, value } => {
                            self.emit_const_vreg(*dest, *value, &vreg_locations);
                        }
                        IrInst::ConstStr { dest, value } => {
                            let str_id = get_or_insert_string(
                                value,
                                &mut string_literals,
                                &mut string_id_map,
                            );
                            let patch_pos = emit_mov_rdi_string_ptr(&mut self.encoder);
                            string_literal_patches.push((patch_pos, str_id));
                            self.store_vreg(*dest, Reg::RDI, &vreg_locations);
                        }
                        IrInst::Input { dest, kind } => match kind {
                            InputKind::Int => {
                                let patch = self.encoder.call_rel32(0);
                                pending_helper_calls.push((patch, HelperKind::InputInt));
                                self.store_vreg(*dest, Reg::RAX, &vreg_locations);
                            }
                            InputKind::Float | InputKind::Str => {
                                return Err(ForgeError::codegen(
                                    "Native backend currently supports Input(Int) only",
                                ));
                            }
                        },
                        IrInst::Copy { dest, src } => {
                            self.copy_operand_to_vreg(*dest, src, &vreg_locations);
                        }
                        IrInst::StoreVar { var_name, src } => {
                            let disp = *var_offsets.get(var_name).unwrap();
                            self.store_operand_to_stack(src, disp, &vreg_locations);
                        }
                        IrInst::LoadVar { dest, var_name } => {
                            let src_disp = *var_offsets.get(var_name).unwrap();
                            match vreg_locations.get(dest).unwrap() {
                                VRegLocation::Reg(reg) => {
                                    self.encoder.load_rbp_disp32(*reg, src_disp)
                                }
                                VRegLocation::Spill(dest_disp) => {
                                    self.encoder.load_rbp_disp32(Reg::R10, src_disp);
                                    self.encoder.store_rbp_disp32(*dest_disp, Reg::R10);
                                }
                            }
                        }
                        IrInst::BinOp { dest, op, lhs, rhs } => match op {
                            IrBinOp::Add => {
                                self.emit_alu(*dest, *op, lhs, rhs, &vreg_locations);
                            }
                            IrBinOp::Sub => {
                                self.emit_alu(*dest, *op, lhs, rhs, &vreg_locations);
                            }
                            IrBinOp::Mul => {
                                self.emit_alu(*dest, *op, lhs, rhs, &vreg_locations);
                            }
                            IrBinOp::Div => {
                                self.load_operand(Reg::RAX, lhs, &vreg_locations);
                                self.encoder.cqo();
                                self.load_operand(Reg::R10, rhs, &vreg_locations);
                                self.encoder.idiv_reg(Reg::R10);
                                self.store_vreg(*dest, Reg::RAX, &vreg_locations);
                            }
                            IrBinOp::Rem => {
                                self.load_operand(Reg::RAX, lhs, &vreg_locations);
                                self.encoder.cqo();
                                self.load_operand(Reg::R10, rhs, &vreg_locations);
                                self.encoder.idiv_reg(Reg::R10);
                                self.store_vreg(*dest, Reg::RDX, &vreg_locations);
                            }
                            IrBinOp::Pow => {
                                self.load_operand(Reg::RDI, lhs, &vreg_locations);
                                self.load_operand(Reg::RSI, rhs, &vreg_locations);
                                let patch = self.encoder.call_rel32(0);
                                pending_helper_calls.push((patch, HelperKind::PowInt));
                                self.store_vreg(*dest, Reg::RAX, &vreg_locations);
                            }
                            IrBinOp::And => {
                                self.emit_alu(*dest, *op, lhs, rhs, &vreg_locations);
                            }
                            IrBinOp::Or => {
                                self.emit_alu(*dest, *op, lhs, rhs, &vreg_locations);
                            }
                            IrBinOp::Xor => {
                                self.emit_alu(*dest, *op, lhs, rhs, &vreg_locations);
                            }
                            IrBinOp::Eq
                            | IrBinOp::Ne
                            | IrBinOp::Lt
                            | IrBinOp::Gt
                            | IrBinOp::Le
                            | IrBinOp::Ge => {
                                self.emit_compare(*dest, *op, lhs, rhs, &vreg_locations);
                            }
                        },
                        IrInst::Call { dest, func, args } => {
                            if args.len() > 6 {
                                return Err(ForgeError::codegen(format!(
                                    "Function call '{}' exceeds maximum 6 SysV argument registers",
                                    func
                                )));
                            }
                            for (i, arg) in args.iter().enumerate() {
                                let arg_reg = SYSV_ARG_REGS[i];
                                self.load_operand(arg_reg, arg, &vreg_locations);
                            }
                            let patch_pos = self.encoder.call_rel32(0);
                            pending_calls.push((patch_pos, func.clone()));
                            self.store_vreg(*dest, Reg::RAX, &vreg_locations);
                        }
                        IrInst::Print { src, kind } => match kind {
                            PrintKind::Int => {
                                self.load_operand(Reg::RDI, src, &vreg_locations);
                                let patch = self.encoder.call_rel32(0);
                                pending_helper_calls.push((patch, HelperKind::PrintInt));
                            }
                            PrintKind::Str => {
                                self.load_operand(Reg::RDI, src, &vreg_locations);
                                let patch = self.encoder.call_rel32(0);
                                pending_helper_calls.push((patch, HelperKind::PrintStr));
                                let newline_id = get_or_insert_string(
                                    "\n\0",
                                    &mut string_literals,
                                    &mut string_id_map,
                                );
                                let newline_patch = emit_mov_rdi_string_ptr(&mut self.encoder);
                                string_literal_patches.push((newline_patch, newline_id));
                                let newline_call = self.encoder.call_rel32(0);
                                pending_helper_calls.push((newline_call, HelperKind::PrintStr));
                            }
                            PrintKind::Bool => {
                                self.load_operand(Reg::RDI, src, &vreg_locations);
                                self.encoder.mov_reg_imm64(Reg::R10, 0);
                                self.encoder.cmp_reg_reg(Reg::RDI, Reg::R10);
                                let patch_jne = self.encoder.jne_rel32(0);

                                // False case
                                let str_id_false = get_or_insert_string(
                                    "false\n\0",
                                    &mut string_literals,
                                    &mut string_id_map,
                                );
                                let patch_pos_false = emit_mov_rdi_string_ptr(&mut self.encoder);
                                string_literal_patches.push((patch_pos_false, str_id_false));
                                self.encoder.mov_reg_imm64(Reg::RSI, 6);
                                let patch_print_false = self.encoder.call_rel32(0);
                                pending_helper_calls
                                    .push((patch_print_false, HelperKind::PrintStr));

                                let patch_jmp_end = self.encoder.jmp_rel32(0);

                                // True case
                                let true_offset = self.encoder.len();
                                self.encoder.patch_i32_le(
                                    patch_jne,
                                    (true_offset as i64 - (patch_jne + 4) as i64) as i32,
                                );

                                let str_id_true = get_or_insert_string(
                                    "true\n\0",
                                    &mut string_literals,
                                    &mut string_id_map,
                                );
                                let patch_pos_true = emit_mov_rdi_string_ptr(&mut self.encoder);
                                string_literal_patches.push((patch_pos_true, str_id_true));
                                self.encoder.mov_reg_imm64(Reg::RSI, 5);
                                let patch_print_true = self.encoder.call_rel32(0);
                                pending_helper_calls.push((patch_print_true, HelperKind::PrintStr));

                                let end_offset = self.encoder.len();
                                self.encoder.patch_i32_le(
                                    patch_jmp_end,
                                    (end_offset as i64 - (patch_jmp_end + 4) as i64) as i32,
                                );
                            }
                        },
                        IrInst::PrintInline { src, kind } => match kind {
                            PrintKind::Str => {
                                self.load_operand(Reg::RDI, src, &vreg_locations);
                                let patch = self.encoder.call_rel32(0);
                                pending_helper_calls.push((patch, HelperKind::PrintStr));
                            }
                            PrintKind::Int => {
                                self.load_operand(Reg::RDI, src, &vreg_locations);
                                self.encoder.mov_reg_imm64(Reg::RSI, 0);
                                let patch = self.encoder.call_rel32(0);
                                pending_helper_calls.push((patch, HelperKind::PrintIntInline));
                            }
                            PrintKind::Bool => {
                                self.load_operand(Reg::RDI, src, &vreg_locations);
                                self.encoder.mov_reg_imm64(Reg::R10, 0);
                                self.encoder.cmp_reg_reg(Reg::RDI, Reg::R10);
                                let patch_jne = self.encoder.jne_rel32(0);
                                let false_id = get_or_insert_string(
                                    "false\0",
                                    &mut string_literals,
                                    &mut string_id_map,
                                );
                                let false_patch = emit_mov_rdi_string_ptr(&mut self.encoder);
                                string_literal_patches.push((false_patch, false_id));
                                let false_call = self.encoder.call_rel32(0);
                                pending_helper_calls.push((false_call, HelperKind::PrintStr));
                                let end_jump = self.encoder.jmp_rel32(0);
                                let true_offset = self.encoder.len();
                                self.encoder.patch_i32_le(
                                    patch_jne,
                                    (true_offset as i64 - (patch_jne + 4) as i64) as i32,
                                );
                                let true_id = get_or_insert_string(
                                    "true\0",
                                    &mut string_literals,
                                    &mut string_id_map,
                                );
                                let true_patch = emit_mov_rdi_string_ptr(&mut self.encoder);
                                string_literal_patches.push((true_patch, true_id));
                                let true_call = self.encoder.call_rel32(0);
                                pending_helper_calls.push((true_call, HelperKind::PrintStr));
                                let end_offset = self.encoder.len();
                                self.encoder.patch_i32_le(
                                    end_jump,
                                    (end_offset as i64 - (end_jump + 4) as i64) as i32,
                                );
                            }
                        },
                        IrInst::PrintConstStr { str_val } => {
                            let terminated = if str_val.ends_with('\0') {
                                str_val.clone()
                            } else {
                                format!("{}\0", str_val)
                            };
                            let str_id = get_or_insert_string(
                                &terminated,
                                &mut string_literals,
                                &mut string_id_map,
                            );
                            let patch_pos = emit_mov_rdi_string_ptr(&mut self.encoder);
                            string_literal_patches.push((patch_pos, str_id));
                            self.encoder.mov_reg_imm64(Reg::RSI, str_val.len() as i64);
                            let patch_print = self.encoder.call_rel32(0);
                            pending_helper_calls.push((patch_print, HelperKind::PrintStr));
                        }
                        IrInst::SysExit { status } => {
                            self.load_operand(Reg::RDI, status, &vreg_locations);
                            self.encoder.mov_reg_imm64(Reg::RAX, 60);
                            self.encoder.syscall();
                        }
                        IrInst::SetError {
                            code,
                            message,
                            type_name,
                        } => {
                            self.load_operand(Reg::RDX, code, &vreg_locations);
                            self.load_operand(Reg::R8, message, &vreg_locations);
                            self.load_operand(Reg::R9, type_name, &vreg_locations);
                        }
                        IrInst::ClearError => {
                            self.encoder.mov_reg_imm64(Reg::RDX, 0);
                            self.encoder.mov_reg_imm64(Reg::R8, 0);
                            self.encoder.mov_reg_imm64(Reg::R9, 0);
                        }
                        IrInst::GetErrorCode { dest } => {
                            self.store_vreg(*dest, Reg::RDX, &vreg_locations);
                        }
                        IrInst::GetErrorMessage { dest } => {
                            self.store_vreg(*dest, Reg::R8, &vreg_locations);
                        }
                        IrInst::GetErrorTypeName { dest } => {
                            self.store_vreg(*dest, Reg::R9, &vreg_locations);
                        }
                    }
                }

                // Block Terminator
                match &block.terminator {
                    IrTerminator::Ret(opt_op) => {
                        if let Some(op) = opt_op {
                            self.load_operand(Reg::RAX, op, &vreg_locations);
                        } else {
                            self.encoder.mov_reg_imm64(Reg::RAX, 0);
                        }
                        self.encoder.mov_reg_imm64(Reg::RDX, 0);
                        self.encoder.mov_reg_imm64(Reg::R8, 0);
                        self.encoder.mov_reg_imm64(Reg::R9, 0);
                        self.restore_callee_saved();
                        self.encoder.leave();
                        self.encoder.ret();
                    }
                    IrTerminator::Jump(target) => {
                        let patch_pos = self.encoder.jmp_rel32(0);
                        pending_jumps.push((patch_pos, target.clone()));
                    }
                    IrTerminator::Branch {
                        cond,
                        true_label,
                        false_label,
                    } => {
                        self.load_operand(Reg::R10, cond, &vreg_locations);
                        self.encoder.mov_reg_imm64(Reg::R11, 0);
                        self.encoder.cmp_reg_reg(Reg::R10, Reg::R11);
                        let patch_jne = self.encoder.jne_rel32(0);
                        pending_jumps.push((patch_jne, true_label.clone()));
                        let patch_jmp = self.encoder.jmp_rel32(0);
                        pending_jumps.push((patch_jmp, false_label.clone()));
                    }
                    IrTerminator::PropagateError => {
                        self.restore_callee_saved();
                        self.encoder.leave();
                        self.encoder.ret();
                    }
                }
            }

            for (patch_pos, target_label) in pending_jumps {
                let target_offset = *block_offsets.get(&target_label).ok_or_else(|| {
                    ForgeError::codegen(format!("Undefined jump target block: {}", target_label))
                })?;
                let rel_offset = (target_offset as i64) - ((patch_pos + 4) as i64);
                self.encoder.patch_i32_le(patch_pos, rel_offset as i32);
            }
        }

        // Patch function call offsets
        for (patch_pos, target_func) in pending_calls {
            let target_offset = *self.func_offsets.get(&target_func).ok_or_else(|| {
                ForgeError::codegen(format!("Undefined target function call: {}", target_func))
            })?;
            let rel_offset = (target_offset as i64) - ((patch_pos + 4) as i64);
            self.encoder.patch_i32_le(patch_pos, rel_offset as i32);
        }

        // Emit runtime helpers
        let helper_print_str_offset = self.encoder.emit_print_str_helper();
        let helper_print_int_offset = self.encoder.emit_print_int_helper();
        let helper_print_int_inline_offset = self.encoder.emit_print_int_inline_helper();
        let helper_pow_int_offset = self.encoder.emit_pow_int_helper();
        let helper_input_int_offset = self.encoder.emit_input_int_helper();

        // Shared language-level uncaught-error formatter. The ELF entry stub
        // calls this with R9 = type name and R8 = message.
        let uncaught_error_offset = self.encoder.len();
        self.func_offsets.insert(
            "__sydrogen_uncaught_error".to_string(),
            uncaught_error_offset,
        );
        self.encoder.push_rbp();
        self.encoder.mov_rbp_rsp();
        self.encoder.mov_reg_reg(Reg::RDI, Reg::R9);
        let type_call = self.encoder.call_rel32(0);
        pending_helper_calls.push((type_call, HelperKind::PrintStr));
        let separator_id = get_or_insert_string(": \0", &mut string_literals, &mut string_id_map);
        let separator_patch = emit_mov_rdi_string_ptr(&mut self.encoder);
        string_literal_patches.push((separator_patch, separator_id));
        let separator_call = self.encoder.call_rel32(0);
        pending_helper_calls.push((separator_call, HelperKind::PrintStr));
        self.encoder.mov_reg_reg(Reg::RDI, Reg::R8);
        let message_call = self.encoder.call_rel32(0);
        pending_helper_calls.push((message_call, HelperKind::PrintStr));
        let newline_id = get_or_insert_string("\n\0", &mut string_literals, &mut string_id_map);
        let newline_patch = emit_mov_rdi_string_ptr(&mut self.encoder);
        string_literal_patches.push((newline_patch, newline_id));
        let newline_call = self.encoder.call_rel32(0);
        pending_helper_calls.push((newline_call, HelperKind::PrintStr));
        self.encoder.mov_reg_imm64(Reg::RAX, 1);
        self.encoder.leave();
        self.encoder.ret();

        for (patch_pos, helper_kind) in pending_helper_calls {
            let target_offset = match helper_kind {
                HelperKind::PrintStr => helper_print_str_offset,
                HelperKind::PrintInt => helper_print_int_offset,
                HelperKind::PrintIntInline => helper_print_int_inline_offset,
                HelperKind::PowInt => helper_pow_int_offset,
                HelperKind::InputInt => helper_input_int_offset,
            };
            let rel_offset = (target_offset as i64) - ((patch_pos + 4) as i64);
            self.encoder.patch_i32_le(patch_pos, rel_offset as i32);
        }

        // Emit static string payload and patch pointers
        let mut str_vaddrs = Vec::new();
        for s in &string_literals {
            let str_code_offset = self.encoder.len();
            let str_vaddr = self.user_code_vaddr + str_code_offset as u64;
            str_vaddrs.push(str_vaddr);
            self.encoder.emit_bytes(s.as_bytes());
        }

        for (patch_pos, str_id) in string_literal_patches {
            let vaddr = str_vaddrs[str_id];
            let bytes = vaddr.to_le_bytes();
            self.encoder.code[patch_pos..patch_pos + 8].copy_from_slice(&bytes);
        }

        Ok((self.encoder.code, self.func_offsets))
    }

    fn load_operand(
        &mut self,
        dst: Reg,
        op: &Operand,
        vreg_locations: &HashMap<VReg, VRegLocation>,
    ) {
        match op {
            Operand::Const(val) => {
                self.encoder.mov_reg_imm64(dst, *val);
            }
            Operand::Reg(vreg) => match vreg_locations.get(vreg).unwrap() {
                VRegLocation::Reg(src) if *src == dst => {}
                VRegLocation::Reg(src) => self.encoder.mov_reg_reg(dst, *src),
                VRegLocation::Spill(disp) => self.encoder.load_rbp_disp32(dst, *disp),
            },
        }
    }

    fn emit_const_vreg(
        &mut self,
        vreg: VReg,
        value: i64,
        vreg_locations: &HashMap<VReg, VRegLocation>,
    ) {
        match vreg_locations.get(&vreg).unwrap() {
            VRegLocation::Reg(reg) => self.encoder.mov_reg_imm64(*reg, value),
            VRegLocation::Spill(disp) => {
                self.encoder.mov_reg_imm64(Reg::R10, value);
                self.encoder.store_rbp_disp32(*disp, Reg::R10);
            }
        }
    }

    fn emit_alu(
        &mut self,
        dest: VReg,
        op: IrBinOp,
        lhs: &Operand,
        rhs: &Operand,
        vreg_locations: &HashMap<VReg, VRegLocation>,
    ) {
        if let VRegLocation::Reg(dest_reg) = vreg_locations.get(&dest).unwrap() {
            self.load_operand(*dest_reg, lhs, vreg_locations);
            self.load_operand(Reg::R10, rhs, vreg_locations);
            match op {
                IrBinOp::Add => self.encoder.add_reg_reg(*dest_reg, Reg::R10),
                IrBinOp::Sub => self.encoder.sub_reg_reg(*dest_reg, Reg::R10),
                IrBinOp::Mul => self.encoder.imul_reg_reg(*dest_reg, Reg::R10),
                IrBinOp::And => self.encoder.and_reg_reg(*dest_reg, Reg::R10),
                IrBinOp::Or => self.encoder.or_reg_reg(*dest_reg, Reg::R10),
                IrBinOp::Xor => self.encoder.xor_reg_reg(*dest_reg, Reg::R10),
                _ => unreachable!(),
            }
        } else {
            self.load_operand(Reg::R10, lhs, vreg_locations);
            self.load_operand(Reg::R11, rhs, vreg_locations);
            match op {
                IrBinOp::Add => self.encoder.add_reg_reg(Reg::R10, Reg::R11),
                IrBinOp::Sub => self.encoder.sub_reg_reg(Reg::R10, Reg::R11),
                IrBinOp::Mul => self.encoder.imul_reg_reg(Reg::R10, Reg::R11),
                IrBinOp::And => self.encoder.and_reg_reg(Reg::R10, Reg::R11),
                IrBinOp::Or => self.encoder.or_reg_reg(Reg::R10, Reg::R11),
                IrBinOp::Xor => self.encoder.xor_reg_reg(Reg::R10, Reg::R11),
                _ => unreachable!(),
            }
            self.store_vreg(dest, Reg::R10, vreg_locations);
        }
    }

    fn emit_compare(
        &mut self,
        dest: VReg,
        op: IrBinOp,
        lhs: &Operand,
        rhs: &Operand,
        vreg_locations: &HashMap<VReg, VRegLocation>,
    ) {
        let dest_reg = match vreg_locations.get(&dest).unwrap() {
            VRegLocation::Reg(reg) => Some(*reg),
            VRegLocation::Spill(_) => None,
        };
        let lhs_reg = dest_reg.unwrap_or(Reg::R10);
        self.load_operand(lhs_reg, lhs, vreg_locations);
        self.load_operand(Reg::R11, rhs, vreg_locations);
        self.encoder.cmp_reg_reg(lhs_reg, Reg::R11);
        let result_reg = dest_reg.unwrap_or(Reg::R10);
        match op {
            IrBinOp::Eq => self.encoder.sete_movzx(result_reg),
            IrBinOp::Ne => self.encoder.setne_movzx(result_reg),
            IrBinOp::Lt => self.encoder.setl_movzx(result_reg),
            IrBinOp::Ge => self.encoder.setge_movzx(result_reg),
            IrBinOp::Le => self.encoder.setle_movzx(result_reg),
            IrBinOp::Gt => self.encoder.setg_movzx(result_reg),
            _ => unreachable!(),
        }
        if dest_reg.is_none() {
            self.store_vreg(dest, result_reg, vreg_locations);
        }
    }

    fn copy_operand_to_vreg(
        &mut self,
        dest: VReg,
        src: &Operand,
        vreg_locations: &HashMap<VReg, VRegLocation>,
    ) {
        match vreg_locations.get(&dest).unwrap() {
            VRegLocation::Reg(reg) => self.load_operand(*reg, src, vreg_locations),
            VRegLocation::Spill(disp) => {
                self.load_operand(Reg::R10, src, vreg_locations);
                self.encoder.store_rbp_disp32(*disp, Reg::R10);
            }
        }
    }

    fn store_operand_to_stack(
        &mut self,
        src: &Operand,
        disp: i32,
        vreg_locations: &HashMap<VReg, VRegLocation>,
    ) {
        match src {
            Operand::Const(value) => {
                self.encoder.mov_reg_imm64(Reg::R10, *value);
                self.encoder.store_rbp_disp32(disp, Reg::R10);
            }
            Operand::Reg(vreg) => match vreg_locations.get(vreg).unwrap() {
                VRegLocation::Reg(reg) => self.encoder.store_rbp_disp32(disp, *reg),
                VRegLocation::Spill(src_disp) => {
                    self.encoder.load_rbp_disp32(Reg::R10, *src_disp);
                    self.encoder.store_rbp_disp32(disp, Reg::R10);
                }
            },
        }
    }

    fn store_vreg(&mut self, vreg: VReg, src: Reg, vreg_locations: &HashMap<VReg, VRegLocation>) {
        match vreg_locations.get(&vreg).unwrap() {
            VRegLocation::Reg(dst) if *dst == src => {}
            VRegLocation::Reg(dst) => self.encoder.mov_reg_reg(*dst, src),
            VRegLocation::Spill(disp) => self.encoder.store_rbp_disp32(*disp, src),
        }
    }

    fn restore_callee_saved(&mut self) {
        for (reg, slot) in ALLOCATABLE_REGS.iter().zip(CALLEE_SAVE_SLOTS) {
            self.encoder.load_rbp_disp32(*reg, slot);
        }
    }
}

enum HelperKind {
    PrintStr,
    PrintInt,
    PrintIntInline,
    PowInt,
    InputInt,
}

fn get_or_insert_string(
    val: &str,
    string_literals: &mut Vec<String>,
    string_id_map: &mut HashMap<String, usize>,
) -> usize {
    if let Some(&id) = string_id_map.get(val) {
        id
    } else {
        let id = string_literals.len();
        string_literals.push(val.to_string());
        string_id_map.insert(val.to_string(), id);
        id
    }
}

fn emit_mov_rdi_string_ptr(encoder: &mut Encoder) -> usize {
    let rex = 0x48 | Reg::RDI.is_ext();
    let opcode = 0xB8 | Reg::RDI.code();
    encoder.emit_byte(rex);
    encoder.emit_byte(opcode);
    let patch_pos = encoder.len();
    encoder.emit_u64_le(0); // placeholder
    patch_pos
}
