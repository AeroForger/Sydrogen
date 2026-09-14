use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    types, AbiParam, FuncRef, GlobalValue, InstBuilder, MachMemFlags, StackSlotData, StackSlotKind,
    UserFuncName, Value,
};
use cranelift_codegen::settings;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::ast::*;
use crate::errors::{ForgeError, ForgeResult};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

pub const RUNTIME_SUPPORT_C: &str = r#"
#include <stdio.h>

int furnace_print_f64(const char *format, double value) {
    return printf(format, value);
}
"#;

/// Module-level identifiers for external C libraries and global format strings.
pub struct RuntimeSymbols {
    pub printf_id: FuncId,
    pub puts_id: FuncId,
    pub fputs_id: FuncId,
    pub stdout_id: DataId,
    pub scanf_id: FuncId,
    pub pow_id: FuncId,
    pub exit_id: FuncId,
    pub malloc_id: FuncId,
    pub realloc_id: FuncId,
    pub free_id: FuncId,
    pub memmove_id: FuncId,
    pub int_fmt_id: DataId,
    pub float_fmt_id: DataId,
    pub int_inline_fmt_id: DataId,
    pub float_inline_fmt_id: DataId,
    pub int_scanf_id: DataId,
    pub float_scanf_id: DataId,
    pub str_scanf_id: DataId,
    pub oob_fmt_id: DataId,
    pub true_str_id: DataId,
    pub false_str_id: DataId,
}

/// Function-local Cranelift handles (GlobalValues and FuncRefs) bound to a specific function's IR context.
pub struct RuntimeRefs {
    pub printf: FuncRef,
    pub puts: FuncRef,
    pub fputs: FuncRef,
    pub stdout: GlobalValue,
    pub scanf: FuncRef,
    pub pow: FuncRef,
    pub exit: FuncRef,
    pub malloc: FuncRef,
    pub realloc: FuncRef,
    pub free: FuncRef,
    pub memmove: FuncRef,
    pub int_fmt: GlobalValue,
    pub float_fmt: GlobalValue,
    pub int_inline_fmt: GlobalValue,
    pub float_inline_fmt: GlobalValue,
    pub int_scanf: GlobalValue,
    pub float_scanf: GlobalValue,
    pub str_scanf: GlobalValue,
    pub oob_fmt: GlobalValue,
    pub true_str: GlobalValue,
    pub false_str: GlobalValue,
}

/// Owns module-level compilation state and external dependencies.
pub struct CodeGenContext {
    pub module: ObjectModule,
    pub ptr_type: types::Type,
    pub runtime_symbols: RuntimeSymbols,
    pub string_pool: HashMap<String, DataId>,
}

impl CodeGenContext {
    pub fn new() -> ForgeResult<Self> {
        let flag_builder = settings::builder();
        let isa_builder = cranelift_native::builder()
            .map_err(|e| ForgeError::codegen(format!("Native ISA error: {}", e)))?;
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .map_err(|e| ForgeError::codegen(format!("ISA finish error: {}", e)))?;

        let builder =
            ObjectBuilder::new(isa, "furnace.o", cranelift_module::default_libcall_names())
                .map_err(|e| ForgeError::codegen(format!("ObjectBuilder error: {}", e)))?;
        let mut module = ObjectModule::new(builder);

        let ptr_type = module.target_config().pointer_type();

        // Declare external C functions
        let mut printf_sig = module.make_signature();
        printf_sig.params.push(AbiParam::new(ptr_type));
        printf_sig.params.push(AbiParam::new(types::F64));
        printf_sig.returns.push(AbiParam::new(types::I32));
        let printf_id = module
            .declare_function("furnace_print_f64", Linkage::Import, &printf_sig)
            .map_err(|e| ForgeError::codegen(format!("declare printf: {}", e)))?;

        let mut puts_sig = module.make_signature();
        puts_sig.params.push(AbiParam::new(ptr_type));
        puts_sig.returns.push(AbiParam::new(types::I32));
        let puts_id = module
            .declare_function("puts", Linkage::Import, &puts_sig)
            .map_err(|e| ForgeError::codegen(format!("declare puts: {}", e)))?;

        let mut fputs_sig = module.make_signature();
        fputs_sig.params.push(AbiParam::new(ptr_type));
        fputs_sig.params.push(AbiParam::new(ptr_type));
        fputs_sig.returns.push(AbiParam::new(types::I32));
        let fputs_id = module
            .declare_function("fputs", Linkage::Import, &fputs_sig)
            .map_err(|e| ForgeError::codegen(format!("declare fputs: {}", e)))?;

        let stdout_id = module
            .declare_data("stdout", Linkage::Import, false, false)
            .map_err(|e| ForgeError::codegen(format!("declare stdout: {}", e)))?;

        let mut scanf_sig = module.make_signature();
        scanf_sig.params.push(AbiParam::new(ptr_type));
        scanf_sig.params.push(AbiParam::new(ptr_type));
        scanf_sig.returns.push(AbiParam::new(types::I32));
        let scanf_id = module
            .declare_function("scanf", Linkage::Import, &scanf_sig)
            .map_err(|e| ForgeError::codegen(format!("declare scanf: {}", e)))?;

        let mut pow_sig = module.make_signature();
        pow_sig.params.push(AbiParam::new(types::F64));
        pow_sig.params.push(AbiParam::new(types::F64));
        pow_sig.returns.push(AbiParam::new(types::F64));
        let pow_id = module
            .declare_function("pow", Linkage::Import, &pow_sig)
            .map_err(|e| ForgeError::codegen(format!("declare pow: {}", e)))?;

        let mut exit_sig = module.make_signature();
        exit_sig.params.push(AbiParam::new(types::I32));
        let exit_id = module
            .declare_function("exit", Linkage::Import, &exit_sig)
            .map_err(|e| ForgeError::codegen(format!("declare exit: {}", e)))?;

        let mut malloc_sig = module.make_signature();
        malloc_sig.params.push(AbiParam::new(ptr_type));
        malloc_sig.returns.push(AbiParam::new(ptr_type));
        let malloc_id = module
            .declare_function("malloc", Linkage::Import, &malloc_sig)
            .map_err(|e| ForgeError::codegen(format!("declare malloc: {}", e)))?;

        let mut realloc_sig = module.make_signature();
        realloc_sig.params.push(AbiParam::new(ptr_type));
        realloc_sig.params.push(AbiParam::new(ptr_type));
        realloc_sig.returns.push(AbiParam::new(ptr_type));
        let realloc_id = module
            .declare_function("realloc", Linkage::Import, &realloc_sig)
            .map_err(|e| ForgeError::codegen(format!("declare realloc: {}", e)))?;

        let mut free_sig = module.make_signature();
        free_sig.params.push(AbiParam::new(ptr_type));
        let free_id = module
            .declare_function("free", Linkage::Import, &free_sig)
            .map_err(|e| ForgeError::codegen(format!("declare free: {}", e)))?;

        let mut memmove_sig = module.make_signature();
        memmove_sig.params.push(AbiParam::new(ptr_type));
        memmove_sig.params.push(AbiParam::new(ptr_type));
        memmove_sig.params.push(AbiParam::new(ptr_type));
        memmove_sig.returns.push(AbiParam::new(ptr_type));
        let memmove_id = module
            .declare_function("memmove", Linkage::Import, &memmove_sig)
            .map_err(|e| ForgeError::codegen(format!("declare memmove: {}", e)))?;

        // Define global format strings
        let int_fmt_id = Self::define_string_in_module(&mut module, "int_fmt", b"%.0f\n\0")?;
        let float_fmt_id = Self::define_string_in_module(&mut module, "float_fmt", b"%f\n\0")?;
        let int_inline_fmt_id =
            Self::define_string_in_module(&mut module, "int_inline_fmt", b"%.0f\0")?;
        let float_inline_fmt_id =
            Self::define_string_in_module(&mut module, "float_inline_fmt", b"%f\0")?;
        let int_scanf_id = Self::define_string_in_module(&mut module, "int_scanf", b"%d\0")?;
        let float_scanf_id = Self::define_string_in_module(&mut module, "float_scanf", b"%lf\0")?;
        let str_scanf_id = Self::define_string_in_module(&mut module, "str_scanf", b"%255s\0")?;
        let oob_fmt_id =
            Self::define_string_in_module(&mut module, "oob_fmt", b"Index out of bounds\0")?;
        let true_str_id = Self::define_string_in_module(&mut module, "true_str", b"true\0")?;
        let false_str_id = Self::define_string_in_module(&mut module, "false_str", b"false\0")?;

        let runtime_symbols = RuntimeSymbols {
            printf_id,
            puts_id,
            fputs_id,
            stdout_id,
            scanf_id,
            pow_id,
            exit_id,
            malloc_id,
            realloc_id,
            free_id,
            memmove_id,
            int_fmt_id,
            float_fmt_id,
            int_inline_fmt_id,
            float_inline_fmt_id,
            int_scanf_id,
            float_scanf_id,
            str_scanf_id,
            oob_fmt_id,
            true_str_id,
            false_str_id,
        };

        Ok(Self {
            module,
            ptr_type,
            runtime_symbols,
            string_pool: HashMap::new(),
        })
    }

    pub fn define_string(&mut self, name: &str, bytes: &[u8]) -> ForgeResult<DataId> {
        if let Some(id) = self.string_pool.get(name) {
            return Ok(*id);
        }

        let id = Self::define_string_in_module(&mut self.module, name, bytes)?;
        self.string_pool.insert(name.to_string(), id);
        Ok(id)
    }

    fn define_string_in_module(
        module: &mut ObjectModule,
        name: &str,
        bytes: &[u8],
    ) -> ForgeResult<DataId> {
        let mut data_ctx = DataDescription::new();
        data_ctx.define(bytes.to_vec().into_boxed_slice());
        let id = module
            .declare_data(name, Linkage::Export, false, false)
            .map_err(|e| ForgeError::codegen(format!("declare_data: {}", e)))?;
        module
            .define_data(id, &data_ctx)
            .map_err(|e| ForgeError::codegen(format!("define_data: {}", e)))?;
        Ok(id)
    }

    pub fn declare_runtime_in_func(
        &mut self,
        func: &mut cranelift_codegen::ir::Function,
    ) -> RuntimeRefs {
        let int_fmt = self
            .module
            .declare_data_in_func(self.runtime_symbols.int_fmt_id, func);
        let float_fmt = self
            .module
            .declare_data_in_func(self.runtime_symbols.float_fmt_id, func);
        let int_inline_fmt = self
            .module
            .declare_data_in_func(self.runtime_symbols.int_inline_fmt_id, func);
        let float_inline_fmt = self
            .module
            .declare_data_in_func(self.runtime_symbols.float_inline_fmt_id, func);
        let int_scanf = self
            .module
            .declare_data_in_func(self.runtime_symbols.int_scanf_id, func);
        let float_scanf = self
            .module
            .declare_data_in_func(self.runtime_symbols.float_scanf_id, func);
        let str_scanf = self
            .module
            .declare_data_in_func(self.runtime_symbols.str_scanf_id, func);
        let oob_fmt = self
            .module
            .declare_data_in_func(self.runtime_symbols.oob_fmt_id, func);
        let true_str = self
            .module
            .declare_data_in_func(self.runtime_symbols.true_str_id, func);
        let false_str = self
            .module
            .declare_data_in_func(self.runtime_symbols.false_str_id, func);
        let printf = self
            .module
            .declare_func_in_func(self.runtime_symbols.printf_id, func);
        let puts = self
            .module
            .declare_func_in_func(self.runtime_symbols.puts_id, func);
        let fputs = self
            .module
            .declare_func_in_func(self.runtime_symbols.fputs_id, func);
        let stdout = self
            .module
            .declare_data_in_func(self.runtime_symbols.stdout_id, func);
        let scanf = self
            .module
            .declare_func_in_func(self.runtime_symbols.scanf_id, func);
        let pow = self
            .module
            .declare_func_in_func(self.runtime_symbols.pow_id, func);
        let exit = self
            .module
            .declare_func_in_func(self.runtime_symbols.exit_id, func);
        let malloc = self
            .module
            .declare_func_in_func(self.runtime_symbols.malloc_id, func);
        let realloc = self
            .module
            .declare_func_in_func(self.runtime_symbols.realloc_id, func);
        let free = self
            .module
            .declare_func_in_func(self.runtime_symbols.free_id, func);
        let memmove = self
            .module
            .declare_func_in_func(self.runtime_symbols.memmove_id, func);

        RuntimeRefs {
            printf,
            puts,
            fputs,
            stdout,
            scanf,
            pow,
            exit,
            malloc,
            realloc,
            free,
            memmove,
            int_fmt,
            float_fmt,
            int_inline_fmt,
            float_inline_fmt,
            int_scanf,
            float_scanf,
            str_scanf,
            oob_fmt,
            true_str,
            false_str,
        }
    }
}

#[derive(Debug, Clone)]
pub enum VarInfo {
    Primitive(types::Type),
    Array { element_type: Subtype, size: usize },
    Tuple { fields: Vec<(Subtype, String)> },
    List { element_type: Subtype },
}

/// Owns the active function's IR construction and function-local compilation state.
pub struct FunctionCompiler<'a, 'ctx> {
    pub ctx: &'a mut CodeGenContext,
    pub builder: FunctionBuilder<'ctx>,
    pub runtime: RuntimeRefs,
    pub string_map: HashMap<String, GlobalValue>,
    pub func_ids: HashMap<String, FuncId>,
    pub func_return_kinds: HashMap<String, RetKind>,
    pub current_ret_kind: RetKind,
    pub var_map: HashMap<String, Variable>,
    pub var_info: HashMap<String, VarInfo>,
    pub float_vars: HashSet<String>,
    pub string_vars: HashSet<String>,
    pub bool_vars: HashSet<String>,
    pub break_targets: Vec<cranelift_codegen::ir::Block>,
    pub continue_targets: Vec<cranelift_codegen::ir::Block>,
    pub terminated: bool,
}

impl<'a, 'ctx> FunctionCompiler<'a, 'ctx> {
    pub fn new(
        ctx: &'a mut CodeGenContext,
        builder: FunctionBuilder<'ctx>,
        runtime: RuntimeRefs,
        string_map: HashMap<String, GlobalValue>,
        func_ids: HashMap<String, FuncId>,
        func_return_kinds: HashMap<String, RetKind>,
        current_ret_kind: RetKind,
    ) -> Self {
        Self {
            ctx,
            builder,
            runtime,
            string_map,
            func_ids,
            func_return_kinds,
            current_ret_kind,
            var_map: HashMap::new(),
            var_info: HashMap::new(),
            float_vars: HashSet::new(),
            string_vars: HashSet::new(),
            bool_vars: HashSet::new(),
            break_targets: Vec::new(),
            continue_targets: Vec::new(),
            terminated: false,
        }
    }

    fn compile_block(&mut self, body: &[Statement]) -> ForgeResult<bool> {
        let previous = self.terminated;
        self.terminated = false;
        for stmt in body {
            if self.terminated {
                break;
            }
            self.compile_statement(stmt)?;
        }
        let terminated = self.terminated;
        self.terminated = previous;
        Ok(terminated)
    }

    fn jump_and_terminate(&mut self, target: cranelift_codegen::ir::Block) {
        self.builder.ins().jump(target, &[]);
        let dead_block = self.builder.create_block();
        self.builder.switch_to_block(dead_block);
        self.emit_default_return();
        self.builder.seal_block(dead_block);
        self.terminated = true;
    }

    fn emit_default_return(&mut self) {
        if self.current_ret_kind.is_void() {
            self.builder.ins().return_(&[]);
            return;
        }
        let value = match &self.current_ret_kind {
            RetKind::Float => self.builder.ins().f64const(0.0),
            RetKind::Weld
            | RetKind::Ore(_)
            | RetKind::OreTuple(_)
            | RetKind::Materials(_, _)
            | RetKind::Generic
            | RetKind::Function
            | RetKind::Dynamic => self.builder.ins().iconst(self.ctx.ptr_type, 0),
            _ => self.builder.ins().iconst(types::I32, 0),
        };
        self.builder.ins().return_(&[value]);
    }

    fn is_float_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Number(n) => n.is_float,
            Expr::Identifier(name) => self.float_vars.contains(name),
            Expr::MemberAccess { object, member } => {
                if let Expr::Identifier(name) = &**object {
                    if let Some(VarInfo::Tuple { fields }) = self.var_info.get(name) {
                        if let Some((Subtype::Float, _)) =
                            fields.iter().find(|(_, f_name)| f_name == member)
                        {
                            return true;
                        }
                    }
                }
                false
            }
            Expr::IndexAccess { object, .. } => {
                if let Expr::Identifier(name) = &**object {
                    if let Some(VarInfo::Array {
                        element_type: Subtype::Float,
                        ..
                    }) = self.var_info.get(name)
                    {
                        return true;
                    }
                    if let Some(VarInfo::List {
                        element_type: Subtype::Float,
                        ..
                    }) = self.var_info.get(name)
                    {
                        return true;
                    }
                }
                false
            }
            Expr::BinaryOp { lhs, rhs, .. } => self.is_float_expr(lhs) || self.is_float_expr(rhs),
            Expr::UnaryOp { operand, .. } => self.is_float_expr(operand),
            Expr::Input(InputNode {
                subtype: Some(Subtype::Float),
            }) => true,
            Expr::Call { callee, .. } => {
                matches!(self.func_return_kinds.get(callee), Some(RetKind::Float))
            }
            _ => false,
        }
    }

    fn is_string_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Str(_) => true,
            Expr::Identifier(name) => self.string_vars.contains(name),
            Expr::MemberAccess { object, member } => {
                if let Expr::Identifier(name) = &**object {
                    if let Some(VarInfo::Tuple { fields }) = self.var_info.get(name) {
                        if let Some((Subtype::Weld, _)) =
                            fields.iter().find(|(_, f_name)| f_name == member)
                        {
                            return true;
                        }
                    }
                }
                false
            }
            Expr::IndexAccess { object, .. } => {
                if let Expr::Identifier(name) = &**object {
                    if let Some(VarInfo::Array {
                        element_type: Subtype::Weld,
                        ..
                    }) = self.var_info.get(name)
                    {
                        return true;
                    }
                    if let Some(VarInfo::List {
                        element_type: Subtype::Weld,
                        ..
                    }) = self.var_info.get(name)
                    {
                        return true;
                    }
                }
                false
            }
            Expr::Call { callee, .. } => {
                matches!(self.func_return_kinds.get(callee), Some(RetKind::Weld))
            }
            _ => false,
        }
    }

    fn is_bool_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Bool(_) => true,
            Expr::Identifier(name) => self.bool_vars.contains(name),
            Expr::Call { callee, .. } => {
                matches!(self.func_return_kinds.get(callee), Some(RetKind::Bool))
            }
            _ => false,
        }
    }

    fn collection_element_type(&self, object: &Expr) -> types::Type {
        let subtype = match object {
            Expr::Identifier(name) => match self.var_info.get(name) {
                Some(VarInfo::Array { element_type, .. })
                | Some(VarInfo::List { element_type }) => Some(element_type),
                _ => None,
            },
            _ => None,
        };
        match subtype {
            Some(Subtype::Float) => types::F64,
            Some(Subtype::Weld) => self.ctx.ptr_type,
            _ => types::I32,
        }
    }

    fn emit_bounds_check(&mut self, index_val: Value, len_val: Value) -> ForgeResult<()> {
        let in_bounds = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedLessThan, index_val, len_val);
        let ok_block = self.builder.create_block();
        let err_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(in_bounds, ok_block, &[], err_block, &[]);

        self.builder.switch_to_block(err_block);
        self.builder.seal_block(err_block);
        let oob_msg = self
            .builder
            .ins()
            .symbol_value(self.ctx.ptr_type, self.runtime.oob_fmt);
        self.builder.ins().call(self.runtime.puts, &[oob_msg]);
        let one = self.builder.ins().iconst(types::I32, 1);
        self.builder.ins().call(self.runtime.exit, &[one]);
        self.emit_default_return();

        self.builder.switch_to_block(ok_block);
        self.builder.seal_block(ok_block);
        Ok(())
    }

    fn emit_increment(&mut self, var_name: &str, op: &IncrOp) -> ForgeResult<()> {
        let var = self
            .var_map
            .get(var_name)
            .copied()
            .ok_or_else(|| ForgeError::codegen(format!("Undefined variable: {}", var_name)))?;
        let current = self.builder.use_var(var);

        let is_float = self.float_vars.contains(var_name);
        let new_val = if is_float {
            let one = self.builder.ins().f64const(1.0);
            match op {
                IncrOp::Inc => self.builder.ins().fadd(current, one),
                IncrOp::Dec => self.builder.ins().fsub(current, one),
            }
        } else {
            let one = self.builder.ins().iconst(types::I32, 1);
            match op {
                IncrOp::Inc => self.builder.ins().iadd(current, one),
                IncrOp::Dec => self.builder.ins().isub(current, one),
            }
        };

        self.builder.def_var(var, new_val);
        Ok(())
    }

    fn compile_namespace_call(
        &mut self,
        namespace: &str,
        method: &str,
        _args: &[Expr],
    ) -> ForgeResult<Value> {
        match (namespace, method) {
            ("Program", "Stop") => {
                let zero = self.builder.ins().iconst(types::I32, 0);
                self.builder.ins().call(self.runtime.exit, &[zero]);
                if self.current_ret_kind.is_void() {
                    self.builder.ins().return_(&[]);
                } else {
                    self.builder.ins().return_(&[zero]);
                }
                self.terminated = true;
                let dead_block = self.builder.create_block();
                self.builder.switch_to_block(dead_block);
                self.emit_default_return();
                self.builder.seal_block(dead_block);
                Ok(zero)
            }
            _ => Err(ForgeError::codegen(format!(
                "Unsupported namespace call: {}.{}",
                namespace, method
            ))),
        }
    }

    fn compile_empty_list(&mut self) -> ForgeResult<Value> {
        let header_size = self.builder.ins().iconst(self.ctx.ptr_type, 24);
        let call_inst = self.builder.ins().call(self.runtime.malloc, &[header_size]);
        let list_ptr = self.builder.inst_results(call_inst)[0];

        let buf_size = self.builder.ins().iconst(self.ctx.ptr_type, 32); // initial cap 4 * 8
        let call_buf = self.builder.ins().call(self.runtime.malloc, &[buf_size]);
        let buf_ptr = self.builder.inst_results(call_buf)[0];

        let zero = self.builder.ins().iconst(self.ctx.ptr_type, 0);
        let four = self.builder.ins().iconst(self.ctx.ptr_type, 4);

        self.builder
            .ins()
            .store(MachMemFlags::new(), zero, list_ptr, 0);
        self.builder
            .ins()
            .store(MachMemFlags::new(), four, list_ptr, 8);
        self.builder
            .ins()
            .store(MachMemFlags::new(), buf_ptr, list_ptr, 16);

        Ok(list_ptr)
    }

    fn compile_assignment_target_ptr(&mut self, target: &AssignmentTarget) -> ForgeResult<Value> {
        match target {
            AssignmentTarget::Var(name) => {
                if let Some(var) = self.var_map.get(name).copied() {
                    Ok(self.builder.use_var(var))
                } else {
                    Err(ForgeError::codegen(format!("Undefined variable: {}", name)))
                }
            }
            AssignmentTarget::Member { object, member } => {
                let obj_ptr = self.compile_assignment_target_ptr(object)?;
                let info = self.get_target_info(object)?;
                if let VarInfo::Tuple { fields } = info {
                    let idx = fields
                        .iter()
                        .position(|(_, f_name)| f_name == member)
                        .ok_or_else(|| {
                            ForgeError::codegen(format!("Unknown tuple field: {}", member))
                        })?;
                    let offset = (idx * 8) as i32;
                    let off_val = self.builder.ins().iconst(self.ctx.ptr_type, offset as i64);
                    Ok(self.builder.ins().iadd(obj_ptr, off_val))
                } else {
                    Err(ForgeError::codegen(format!(
                        "Member target not supported on {:?}",
                        info
                    )))
                }
            }
            AssignmentTarget::Index { object, index } => {
                let obj_ptr = self.compile_assignment_target_ptr(object)?;
                let idx_val = self.compile_expr(index)?;
                let idx_val_ptr = self.builder.ins().sextend(self.ctx.ptr_type, idx_val);
                let info = self.get_target_info(object)?;
                match info {
                    VarInfo::Array { .. } => {
                        let len = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_ptr,
                            0,
                        );
                        self.emit_bounds_check(idx_val_ptr, len)?;
                        let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                        let byte_offset = self.builder.ins().imul(idx_val_ptr, eight);
                        let elem_addr = self.builder.ins().iadd(obj_ptr, byte_offset);
                        let sixteen = self.builder.ins().iconst(self.ctx.ptr_type, 16);
                        Ok(self.builder.ins().iadd(elem_addr, sixteen))
                    }
                    VarInfo::List { .. } => {
                        let len = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_ptr,
                            0,
                        );
                        self.emit_bounds_check(idx_val_ptr, len)?;
                        let buf_ptr = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_ptr,
                            16,
                        );
                        let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                        let byte_offset = self.builder.ins().imul(idx_val_ptr, eight);
                        Ok(self.builder.ins().iadd(buf_ptr, byte_offset))
                    }
                    _ => Err(ForgeError::codegen(
                        "Index assignment target must be Array or List",
                    )),
                }
            }
        }
    }

    fn get_target_info(&self, target: &AssignmentTarget) -> ForgeResult<VarInfo> {
        match target {
            AssignmentTarget::Var(name) => self.var_info.get(name).cloned().ok_or_else(|| {
                ForgeError::codegen(format!("Undefined variable info for: {}", name))
            }),
            _ => Ok(VarInfo::Primitive(self.ctx.ptr_type)),
        }
    }

    pub fn compile_statement(&mut self, stmt: &Statement) -> ForgeResult<()> {
        match stmt {
            Statement::VarDecl(v) => {
                let (ty, info) = match &v.type_decl {
                    TypeDecl::Number(Subtype::Float) => {
                        (types::F64, VarInfo::Primitive(types::F64))
                    }
                    TypeDecl::Number(_) => (types::I32, VarInfo::Primitive(types::I32)),
                    TypeDecl::Weld => (self.ctx.ptr_type, VarInfo::Primitive(self.ctx.ptr_type)),
                    TypeDecl::Bool => (types::I32, VarInfo::Primitive(types::I32)),
                    TypeDecl::Ore(size_opt) => {
                        let size = size_opt.unwrap_or(0) as usize;
                        (
                            self.ctx.ptr_type,
                            VarInfo::Array {
                                element_type: Subtype::Int,
                                size,
                            },
                        )
                    }
                    TypeDecl::OreTuple(fields) => (
                        self.ctx.ptr_type,
                        VarInfo::Tuple {
                            fields: fields.clone(),
                        },
                    ),
                    TypeDecl::Materials(elem_type, _) => (
                        self.ctx.ptr_type,
                        VarInfo::List {
                            element_type: elem_type.clone(),
                        },
                    ),
                };
                let var = self.builder.declare_var(ty);
                self.var_map.insert(v.name.clone(), var);
                self.var_info.insert(v.name.clone(), info);
                if ty == types::F64 {
                    self.float_vars.insert(v.name.clone());
                }
                if ty == self.ctx.ptr_type {
                    self.string_vars.insert(v.name.clone());
                }
                if matches!(&v.type_decl, TypeDecl::Bool) {
                    self.bool_vars.insert(v.name.clone());
                }

                if let Some(init) = &v.initializer {
                    let val = self.compile_expr(init)?;
                    self.builder.def_var(var, val);
                } else if matches!(&v.type_decl, TypeDecl::Materials(_, true)) {
                    let list_val = self.compile_empty_list()?;
                    self.builder.def_var(var, list_val);
                } else {
                    let zero = if ty == types::F64 {
                        self.builder.ins().f64const(0.0)
                    } else {
                        self.builder.ins().iconst(ty, 0).into()
                    };
                    self.builder.def_var(var, zero);
                }
            }
            Statement::Print(p) => {
                if let Expr::Str(parts) = &p.expr {
                    if parts
                        .iter()
                        .any(|part| matches!(part, StringPart::Interp(_)))
                    {
                        for part in parts {
                            match part {
                                StringPart::Literal(text) => {
                                    if let Some(gv) = self.string_map.get(text) {
                                        let value =
                                            self.builder.ins().symbol_value(self.ctx.ptr_type, *gv);
                                        let stdout_ptr = self
                                            .builder
                                            .ins()
                                            .symbol_value(self.ctx.ptr_type, self.runtime.stdout);
                                        let stdout = self.builder.ins().load(
                                            self.ctx.ptr_type,
                                            MachMemFlags::new(),
                                            stdout_ptr,
                                            0,
                                        );
                                        self.builder
                                            .ins()
                                            .call(self.runtime.fputs, &[value, stdout]);
                                    }
                                }
                                StringPart::Interp(name) => {
                                    let interp_expr = if name.contains('.') {
                                        let mut parts = name.split('.');
                                        let object_name = parts.next().unwrap().to_string();
                                        let member = parts.collect::<Vec<_>>().join(".");
                                        Expr::MemberAccess {
                                            object: Box::new(Expr::Identifier(object_name)),
                                            member,
                                        }
                                    } else {
                                        Expr::Identifier(name.clone())
                                    };
                                    let value = self.compile_expr(&interp_expr)?;
                                    if self.is_string_expr(&interp_expr) {
                                        let stdout_ptr = self
                                            .builder
                                            .ins()
                                            .symbol_value(self.ctx.ptr_type, self.runtime.stdout);
                                        let stdout = self.builder.ins().load(
                                            self.ctx.ptr_type,
                                            MachMemFlags::new(),
                                            stdout_ptr,
                                            0,
                                        );
                                        self.builder
                                            .ins()
                                            .call(self.runtime.fputs, &[value, stdout]);
                                    } else {
                                        let is_float = self.is_float_expr(&interp_expr);
                                        let fmt = if is_float {
                                            self.runtime.float_inline_fmt
                                        } else {
                                            self.runtime.int_inline_fmt
                                        };
                                        let fmt_value =
                                            self.builder.ins().symbol_value(self.ctx.ptr_type, fmt);
                                        let numeric_value = if is_float {
                                            value
                                        } else {
                                            self.builder.ins().fcvt_from_sint(types::F64, value)
                                        };
                                        self.builder
                                            .ins()
                                            .call(self.runtime.printf, &[fmt_value, numeric_value]);
                                    }
                                }
                            }
                        }
                        let newline = self
                            .string_map
                            .get("\n")
                            .map(|gv| self.builder.ins().symbol_value(self.ctx.ptr_type, *gv));
                        if let Some(value) = newline {
                            let stdout_ptr = self
                                .builder
                                .ins()
                                .symbol_value(self.ctx.ptr_type, self.runtime.stdout);
                            let stdout = self.builder.ins().load(
                                self.ctx.ptr_type,
                                MachMemFlags::new(),
                                stdout_ptr,
                                0,
                            );
                            self.builder
                                .ins()
                                .call(self.runtime.fputs, &[value, stdout]);
                        }
                        return Ok(());
                    }
                }
                let val = self.compile_expr(&p.expr)?;
                let is_string = self.is_string_expr(&p.expr);
                let is_float = self.is_float_expr(&p.expr);
                let is_bool = self.is_bool_expr(&p.expr);

                if is_string {
                    self.builder.ins().call(self.runtime.puts, &[val]);
                } else if is_bool {
                    let is_true = self.builder.ins().icmp_imm_u(IntCC::NotEqual, val, 0);
                    let true_str_val = self
                        .builder
                        .ins()
                        .symbol_value(self.ctx.ptr_type, self.runtime.true_str);
                    let false_str_val = self
                        .builder
                        .ins()
                        .symbol_value(self.ctx.ptr_type, self.runtime.false_str);

                    let merge_block = self.builder.create_block();
                    let true_block = self.builder.create_block();
                    let false_block = self.builder.create_block();

                    self.builder
                        .ins()
                        .brif(is_true, true_block, &[], false_block, &[]);

                    self.builder.switch_to_block(true_block);
                    self.builder.seal_block(true_block);
                    self.builder.ins().call(self.runtime.puts, &[true_str_val]);
                    self.builder.ins().jump(merge_block, &[]);

                    self.builder.switch_to_block(false_block);
                    self.builder.seal_block(false_block);
                    self.builder.ins().call(self.runtime.puts, &[false_str_val]);
                    self.builder.ins().jump(merge_block, &[]);

                    self.builder.switch_to_block(merge_block);
                    self.builder.seal_block(merge_block);
                } else {
                    let fmt_gv = if is_float {
                        self.runtime.float_fmt
                    } else {
                        self.runtime.int_fmt
                    };
                    let fmt_val = self.builder.ins().symbol_value(self.ctx.ptr_type, fmt_gv);
                    let numeric_val = if is_float {
                        val
                    } else {
                        self.builder.ins().fcvt_from_sint(types::F64, val)
                    };
                    self.builder
                        .ins()
                        .call(self.runtime.printf, &[fmt_val, numeric_val]);
                }
            }
            Statement::Assignment(a) => {
                let val = self.compile_expr(&a.value)?;
                match &a.target {
                    AssignmentTarget::Var(name) => {
                        if let Some(var) = self.var_map.get(name).copied() {
                            self.builder.def_var(var, val);
                        } else {
                            return Err(ForgeError::codegen(format!(
                                "Undefined variable: {}",
                                name
                            )));
                        }
                    }
                    AssignmentTarget::Member { object, member } => {
                        let target_ptr = self.compile_assignment_target_ptr(object)?;
                        let info = self.get_target_info(object)?;
                        if let VarInfo::Tuple { fields } = info {
                            let idx = fields
                                .iter()
                                .position(|(_, f_name)| f_name == member)
                                .ok_or_else(|| {
                                    ForgeError::codegen(format!("Unknown tuple field: {}", member))
                                })?;
                            let offset = (idx * 8) as i32;
                            self.builder
                                .ins()
                                .store(MachMemFlags::new(), val, target_ptr, offset);
                        } else {
                            return Err(ForgeError::codegen(
                                "Member assignment not supported on non-tuple",
                            ));
                        }
                    }
                    AssignmentTarget::Index { object, index } => {
                        let target_ptr = self.compile_assignment_target_ptr(object)?;
                        let idx_val = self.compile_expr(index)?;
                        let idx_val_ptr = self.builder.ins().sextend(self.ctx.ptr_type, idx_val);
                        let info = self.get_target_info(object)?;
                        match info {
                            VarInfo::Array { .. } => {
                                let len = self.builder.ins().load(
                                    self.ctx.ptr_type,
                                    MachMemFlags::new(),
                                    target_ptr,
                                    0,
                                );
                                self.emit_bounds_check(idx_val_ptr, len)?;
                                let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                                let byte_offset = self.builder.ins().imul(idx_val_ptr, eight);
                                let elem_addr = self.builder.ins().iadd(target_ptr, byte_offset);
                                self.builder
                                    .ins()
                                    .store(MachMemFlags::new(), val, elem_addr, 16);
                            }
                            VarInfo::List { .. } => {
                                let len = self.builder.ins().load(
                                    self.ctx.ptr_type,
                                    MachMemFlags::new(),
                                    target_ptr,
                                    0,
                                );
                                self.emit_bounds_check(idx_val_ptr, len)?;
                                let buf_ptr = self.builder.ins().load(
                                    self.ctx.ptr_type,
                                    MachMemFlags::new(),
                                    target_ptr,
                                    16,
                                );
                                let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                                let byte_offset = self.builder.ins().imul(idx_val_ptr, eight);
                                let elem_addr = self.builder.ins().iadd(buf_ptr, byte_offset);
                                self.builder
                                    .ins()
                                    .store(MachMemFlags::new(), val, elem_addr, 0);
                            }
                            _ => {
                                return Err(ForgeError::codegen(
                                    "Index assignment on non-collection",
                                ))
                            }
                        }
                    }
                }
            }
            Statement::If(if_node) => {
                let merge_block = self.builder.create_block();
                let is_top_level_if = self.break_targets.is_empty();
                if is_top_level_if {
                    self.break_targets.push(merge_block);
                }

                for (cond, body) in &if_node.branches {
                    let cond_val = self.compile_expr(cond)?;
                    let cond_bool = self.builder.ins().icmp_imm_u(IntCC::NotEqual, cond_val, 0);

                    let then_block = self.builder.create_block();
                    let next_cond_block = self.builder.create_block();

                    self.builder
                        .ins()
                        .brif(cond_bool, then_block, &[], next_cond_block, &[]);

                    self.builder.switch_to_block(then_block);
                    self.builder.seal_block(then_block);

                    let branch_terminated = self.compile_block(body)?;
                    if !branch_terminated {
                        self.builder.ins().jump(merge_block, &[]);
                    }

                    self.builder.switch_to_block(next_cond_block);
                    self.builder.seal_block(next_cond_block);
                }

                if let Some(else_body) = &if_node.else_body {
                    let else_terminated = self.compile_block(else_body)?;
                    if !else_terminated {
                        self.builder.ins().jump(merge_block, &[]);
                    }
                }
                if if_node.else_body.is_none() {
                    self.builder.ins().jump(merge_block, &[]);
                }

                if is_top_level_if {
                    self.break_targets.pop();
                }

                self.builder.switch_to_block(merge_block);
                self.builder.seal_block(merge_block);
            }
            Statement::Switch(switch_node) => {
                // Compile before any Case checks so calls and other side effects
                // in the selector occur exactly once.
                let switch_value = self.compile_expr(&switch_node.expression)?;
                let switch_is_float = self.is_float_expr(&switch_node.expression);
                let merge_block = self.builder.create_block();
                let is_top_level_switch = self.break_targets.is_empty();
                if is_top_level_switch {
                    self.break_targets.push(merge_block);
                }

                let cases: Vec<_> = switch_node
                    .branches
                    .iter()
                    .filter_map(|branch| match branch {
                        SwitchBranch::Case(case) => Some(case),
                        SwitchBranch::Base(_) => None,
                    })
                    .collect();
                let base_body = switch_node.branches.iter().find_map(|branch| match branch {
                    SwitchBranch::Base(body) => Some(body),
                    SwitchBranch::Case(_) => None,
                });
                let case_blocks: Vec<_> = (0..cases.len())
                    .map(|_| self.builder.create_block())
                    .collect();
                let base_block = base_body.map(|_| self.builder.create_block());
                let checks: Vec<_> = cases
                    .iter()
                    .enumerate()
                    .flat_map(|(case_index, case)| {
                        case.values.iter().map(move |value| (case_index, value))
                    })
                    .collect();

                for (index, (case_index, pattern)) in checks.iter().enumerate() {
                    let pattern_value = self.compile_expr(pattern)?;
                    let matches = if switch_is_float {
                        self.builder
                            .ins()
                            .fcmp(FloatCC::Equal, switch_value, pattern_value)
                    } else {
                        self.builder
                            .ins()
                            .icmp(IntCC::Equal, switch_value, pattern_value)
                    };
                    let no_match = if index + 1 < checks.len() {
                        self.builder.create_block()
                    } else {
                        base_block.unwrap_or(merge_block)
                    };
                    self.builder
                        .ins()
                        .brif(matches, case_blocks[*case_index], &[], no_match, &[]);
                    if index + 1 < checks.len() {
                        self.builder.switch_to_block(no_match);
                        self.builder.seal_block(no_match);
                    }
                }
                if checks.is_empty() {
                    self.builder
                        .ins()
                        .jump(base_block.unwrap_or(merge_block), &[]);
                }

                for (case, block) in cases.iter().zip(&case_blocks) {
                    self.builder.switch_to_block(*block);
                    self.builder.seal_block(*block);
                    let terminated = self.compile_block(&case.body)?;
                    if !terminated {
                        self.builder.ins().jump(merge_block, &[]);
                    }
                }
                if let (Some(body), Some(block)) = (base_body, base_block) {
                    self.builder.switch_to_block(block);
                    self.builder.seal_block(block);
                    let terminated = self.compile_block(body)?;
                    if !terminated {
                        self.builder.ins().jump(merge_block, &[]);
                    }
                }
                if is_top_level_switch {
                    self.break_targets.pop();
                }
                self.builder.switch_to_block(merge_block);
                self.builder.seal_block(merge_block);
            }
            Statement::While(while_node) => {
                let header_block = self.builder.create_block();
                let body_block = self.builder.create_block();
                let exit_block = self.builder.create_block();
                self.builder.ins().jump(header_block, &[]);

                self.builder.switch_to_block(header_block);
                let condition = self.compile_expr(&while_node.condition)?;
                let condition = self.builder.ins().icmp_imm_u(IntCC::NotEqual, condition, 0);
                self.builder
                    .ins()
                    .brif(condition, body_block, &[], exit_block, &[]);

                self.break_targets.push(exit_block);
                self.continue_targets.push(header_block);

                self.builder.switch_to_block(body_block);
                let body_terminated = self.compile_block(&while_node.body)?;
                if !body_terminated {
                    self.builder.ins().jump(header_block, &[]);
                }
                self.builder.seal_block(body_block);
                self.builder.seal_block(header_block);

                self.break_targets.pop();
                self.continue_targets.pop();

                self.builder.switch_to_block(exit_block);
                self.builder.seal_block(exit_block);
            }
            Statement::For(for_node) => {
                self.compile_statement(&Statement::VarDecl(for_node.init.clone()))?;

                let cond_block = self.builder.create_block();
                let body_block = self.builder.create_block();
                let incr_block = self.builder.create_block();
                let exit_block = self.builder.create_block();
                self.builder.ins().jump(cond_block, &[]);

                self.builder.switch_to_block(cond_block);
                let cond_val = self.compile_expr(&for_node.condition)?;
                let cond_bool = self.builder.ins().icmp_imm_u(IntCC::NotEqual, cond_val, 0);
                self.builder
                    .ins()
                    .brif(cond_bool, body_block, &[], exit_block, &[]);

                self.break_targets.push(exit_block);
                self.continue_targets.push(incr_block);

                self.builder.switch_to_block(body_block);
                let body_terminated = self.compile_block(&for_node.body)?;
                if !body_terminated {
                    self.builder.ins().jump(incr_block, &[]);
                }
                self.builder.seal_block(body_block);

                self.builder.switch_to_block(incr_block);
                self.builder.seal_block(incr_block);
                if !body_terminated {
                    self.emit_increment(&for_node.increment_var, &for_node.increment_op)?;
                    self.builder.ins().jump(cond_block, &[]);
                }

                self.builder.seal_block(cond_block);

                self.break_targets.pop();
                self.continue_targets.pop();

                self.builder.switch_to_block(exit_block);
                self.builder.seal_block(exit_block);
            }
            Statement::ForEach(foreach_node) => {
                let collection_var = self
                    .var_map
                    .get(&foreach_node.collection_name)
                    .copied()
                    .ok_or_else(|| {
                        ForgeError::codegen(format!(
                            "Undefined variable: {}",
                            foreach_node.collection_name
                        ))
                    })?;
                let collection_info = self
                    .var_info
                    .get(&foreach_node.collection_name)
                    .cloned()
                    .ok_or_else(|| {
                        ForgeError::codegen(format!(
                            "Undefined variable info for: {}",
                            foreach_node.collection_name
                        ))
                    })?;
                let is_list = match &collection_info {
                    VarInfo::Array { .. } => false,
                    VarInfo::List { .. } => true,
                    _ => {
                        return Err(ForgeError::codegen(format!(
                            "ForEach collection '{}' must be an Array or List",
                            foreach_node.collection_name
                        )))
                    }
                };
                let collection_ptr = self.builder.use_var(collection_var);
                let item_ty = match &foreach_node.item_type {
                    TypeDecl::Number(Subtype::Float) => types::F64,
                    TypeDecl::Number(_) | TypeDecl::Bool => types::I32,
                    TypeDecl::Weld => self.ctx.ptr_type,
                    _ => {
                        return Err(ForgeError::codegen(
                            "ForEach item type must match the collection element type",
                        ))
                    }
                };

                // The item belongs to the loop body, so preserve a same-named
                // outer variable and restore it after the loop.
                let old_var = self.var_map.get(&foreach_node.item_name).copied();
                let old_info = self.var_info.get(&foreach_node.item_name).cloned();
                let old_float = self.float_vars.contains(&foreach_node.item_name);
                let old_string = self.string_vars.contains(&foreach_node.item_name);
                let old_bool = self.bool_vars.contains(&foreach_node.item_name);

                let item_var = self.builder.declare_var(item_ty);
                self.var_map
                    .insert(foreach_node.item_name.clone(), item_var);
                self.var_info
                    .insert(foreach_node.item_name.clone(), VarInfo::Primitive(item_ty));
                self.float_vars.remove(&foreach_node.item_name);
                self.string_vars.remove(&foreach_node.item_name);
                self.bool_vars.remove(&foreach_node.item_name);
                if item_ty == types::F64 {
                    self.float_vars.insert(foreach_node.item_name.clone());
                } else if item_ty == self.ctx.ptr_type {
                    self.string_vars.insert(foreach_node.item_name.clone());
                } else if matches!(foreach_node.item_type, TypeDecl::Bool) {
                    self.bool_vars.insert(foreach_node.item_name.clone());
                }

                let index_var = self.builder.declare_var(self.ctx.ptr_type);
                let zero = self.builder.ins().iconst(self.ctx.ptr_type, 0);
                self.builder.def_var(index_var, zero);

                let cond_block = self.builder.create_block();
                let body_block = self.builder.create_block();
                let incr_block = self.builder.create_block();
                let exit_block = self.builder.create_block();
                self.builder.ins().jump(cond_block, &[]);

                self.builder.switch_to_block(cond_block);
                let index = self.builder.use_var(index_var);
                let len = self.builder.ins().load(
                    self.ctx.ptr_type,
                    MachMemFlags::new(),
                    collection_ptr,
                    0,
                );
                let has_item = self.builder.ins().icmp(IntCC::UnsignedLessThan, index, len);
                self.builder
                    .ins()
                    .brif(has_item, body_block, &[], exit_block, &[]);

                self.break_targets.push(exit_block);
                self.continue_targets.push(incr_block);

                self.builder.switch_to_block(body_block);
                let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                let byte_offset = self.builder.ins().imul(index, eight);
                let item_addr = if is_list {
                    let buffer = self.builder.ins().load(
                        self.ctx.ptr_type,
                        MachMemFlags::new(),
                        collection_ptr,
                        16,
                    );
                    self.builder.ins().iadd(buffer, byte_offset)
                } else {
                    let element = self.builder.ins().iadd(collection_ptr, byte_offset);
                    let header_size = self.builder.ins().iconst(self.ctx.ptr_type, 16);
                    self.builder.ins().iadd(element, header_size)
                };
                let item = self
                    .builder
                    .ins()
                    .load(item_ty, MachMemFlags::new(), item_addr, 0);
                self.builder.def_var(item_var, item);
                let body_terminated = self.compile_block(&foreach_node.body)?;
                if !body_terminated {
                    self.builder.ins().jump(incr_block, &[]);
                }
                self.builder.seal_block(body_block);

                self.builder.switch_to_block(incr_block);
                self.builder.seal_block(incr_block);
                let index = self.builder.use_var(index_var);
                let one = self.builder.ins().iconst(self.ctx.ptr_type, 1);
                let next_index = self.builder.ins().iadd(index, one);
                self.builder.def_var(index_var, next_index);
                self.builder.ins().jump(cond_block, &[]);
                self.builder.seal_block(cond_block);

                self.break_targets.pop();
                self.continue_targets.pop();

                match old_var {
                    Some(var) => {
                        self.var_map.insert(foreach_node.item_name.clone(), var);
                    }
                    None => {
                        self.var_map.remove(&foreach_node.item_name);
                    }
                }
                match old_info {
                    Some(info) => {
                        self.var_info.insert(foreach_node.item_name.clone(), info);
                    }
                    None => {
                        self.var_info.remove(&foreach_node.item_name);
                    }
                }
                self.float_vars.remove(&foreach_node.item_name);
                self.string_vars.remove(&foreach_node.item_name);
                self.bool_vars.remove(&foreach_node.item_name);
                if old_float {
                    self.float_vars.insert(foreach_node.item_name.clone());
                }
                if old_string {
                    self.string_vars.insert(foreach_node.item_name.clone());
                }
                if old_bool {
                    self.bool_vars.insert(foreach_node.item_name.clone());
                }

                self.builder.switch_to_block(exit_block);
                self.builder.seal_block(exit_block);
            }
            Statement::Stop => {
                let target = self
                    .break_targets
                    .last()
                    .copied()
                    .ok_or_else(|| ForgeError::codegen("Stop used outside of break target"))?;
                self.jump_and_terminate(target);
            }
            Statement::Skip => {
                let target = self
                    .continue_targets
                    .last()
                    .copied()
                    .ok_or_else(|| ForgeError::codegen("Skip used outside of loop"))?;
                self.jump_and_terminate(target);
            }
            Statement::ExprStmt(e) => {
                self.compile_expr(e)?;
            }
            Statement::Return(value) => {
                if let Some(expr) = value {
                    let val = self.compile_expr(expr)?;
                    self.builder.ins().return_(&[val]);
                } else if self.current_ret_kind.is_void() {
                    self.builder.ins().return_(&[]);
                } else {
                    return Err(ForgeError::codegen(
                        "Missing return value for non-void function",
                    ));
                }
                self.terminated = true;
            }
            Statement::DataDecl(_) | Statement::Use(_) | Statement::Using(_) => {
                // Ignore for now, validly parsed but no codegen action needed
            }
            Statement::ObjectDecl(obj) => {
                let mut fields = Vec::new();
                for (path, value) in &obj.inits {
                    let field_type = match value {
                        Expr::Number(n) if n.is_float => Subtype::Float,
                        Expr::Number(_) => Subtype::Int,
                        Expr::Bool(_) => Subtype::Int,
                        Expr::Str(_) => Subtype::Weld,
                        _ => Subtype::Generic,
                    };
                    let field_name = path.last().cloned().unwrap_or_default();
                    if !fields.iter().any(|(_, name)| name == &field_name) {
                        fields.push((field_type, field_name));
                    }
                }
                if fields.is_empty() {
                    return Ok(());
                }
                let var = self.builder.declare_var(self.ctx.ptr_type);
                let alloc_size = self
                    .builder
                    .ins()
                    .iconst(self.ctx.ptr_type, (fields.len() * 8) as i64);
                let obj_ptr = self.builder.ins().call(self.runtime.malloc, &[alloc_size]);
                let obj_ptr = self.builder.inst_results(obj_ptr)[0];
                self.var_map.insert(obj.name.clone(), var);
                self.var_info.insert(
                    obj.name.clone(),
                    VarInfo::Tuple {
                        fields: fields.clone(),
                    },
                );
                self.builder.def_var(var, obj_ptr);

                for (path, value) in &obj.inits {
                    let field_name = path.last().cloned().unwrap_or_default();
                    let index = fields
                        .iter()
                        .position(|(_, name)| name == &field_name)
                        .unwrap_or(0);
                    let val = self.compile_expr(value)?;
                    let offset = (index * 8) as i32;
                    self.builder
                        .ins()
                        .store(MachMemFlags::new(), val, obj_ptr, offset);
                }
            }
            _ => {
                return Err(ForgeError::codegen(format!(
                    "Unsupported statement: {:?}",
                    stmt
                )))
            }
        }
        Ok(())
    }

    pub fn compile_expr(&mut self, expr: &Expr) -> ForgeResult<Value> {
        match expr {
            Expr::Bool(b) => Ok(self
                .builder
                .ins()
                .iconst(types::I32, if *b { 1 } else { 0 })),
            Expr::Number(n) => {
                if n.is_float {
                    Ok(self.builder.ins().f64const(n.float_val))
                } else {
                    Ok(self.builder.ins().iconst(types::I32, n.int_val))
                }
            }
            Expr::Identifier(name) => {
                if name.contains('.') {
                    let mut parts = name.split('.');
                    let object_name = parts.next().unwrap();
                    let member = parts.collect::<Vec<_>>().join(".");
                    return self.compile_expr(&Expr::MemberAccess {
                        object: Box::new(Expr::Identifier(object_name.to_string())),
                        member,
                    });
                }
                if let Some(var) = self.var_map.get(name).copied() {
                    Ok(self.builder.use_var(var))
                } else if let Some(gv) = self.string_map.get(name).copied() {
                    Ok(self.builder.ins().symbol_value(self.ctx.ptr_type, gv))
                } else {
                    Err(ForgeError::codegen(format!("Undefined variable: {}", name)))
                }
            }
            Expr::ArrayLiteral(elements) => {
                let len = elements.len();
                let total_size = (16 + len * 8) as i64;
                let size_val = self.builder.ins().iconst(self.ctx.ptr_type, total_size);
                let call_inst = self.builder.ins().call(self.runtime.malloc, &[size_val]);
                let array_ptr = self.builder.inst_results(call_inst)[0];

                let len_val = self.builder.ins().iconst(self.ctx.ptr_type, len as i64);
                let elem_size_val = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                self.builder
                    .ins()
                    .store(MachMemFlags::new(), len_val, array_ptr, 0);
                self.builder
                    .ins()
                    .store(MachMemFlags::new(), elem_size_val, array_ptr, 8);

                for (i, elem) in elements.iter().enumerate() {
                    let val = self.compile_expr(elem)?;
                    let offset = (16 + i * 8) as i32;
                    self.builder
                        .ins()
                        .store(MachMemFlags::new(), val, array_ptr, offset);
                }

                Ok(array_ptr)
            }
            Expr::TupleLiteral(elements) => {
                let len = elements.len();
                let total_size = (len * 8) as i64;
                let size_val = self.builder.ins().iconst(self.ctx.ptr_type, total_size);
                let call_inst = self.builder.ins().call(self.runtime.malloc, &[size_val]);
                let tuple_ptr = self.builder.inst_results(call_inst)[0];

                for (i, elem) in elements.iter().enumerate() {
                    let val = self.compile_expr(elem)?;
                    let offset = (i * 8) as i32;
                    self.builder
                        .ins()
                        .store(MachMemFlags::new(), val, tuple_ptr, offset);
                }

                Ok(tuple_ptr)
            }
            Expr::ListLiteral(elements) => {
                let len = elements.len();
                let capacity = if len == 0 { 4 } else { len };

                let header_size = self.builder.ins().iconst(self.ctx.ptr_type, 24);
                let call_inst = self.builder.ins().call(self.runtime.malloc, &[header_size]);
                let list_ptr = self.builder.inst_results(call_inst)[0];

                let buf_size = self
                    .builder
                    .ins()
                    .iconst(self.ctx.ptr_type, (capacity * 8) as i64);
                let call_buf = self.builder.ins().call(self.runtime.malloc, &[buf_size]);
                let buf_ptr = self.builder.inst_results(call_buf)[0];

                let len_val = self.builder.ins().iconst(self.ctx.ptr_type, len as i64);
                let cap_val = self
                    .builder
                    .ins()
                    .iconst(self.ctx.ptr_type, capacity as i64);

                self.builder
                    .ins()
                    .store(MachMemFlags::new(), len_val, list_ptr, 0);
                self.builder
                    .ins()
                    .store(MachMemFlags::new(), cap_val, list_ptr, 8);
                self.builder
                    .ins()
                    .store(MachMemFlags::new(), buf_ptr, list_ptr, 16);

                for (i, elem) in elements.iter().enumerate() {
                    let val = self.compile_expr(elem)?;
                    let offset = (i * 8) as i32;
                    self.builder
                        .ins()
                        .store(MachMemFlags::new(), val, buf_ptr, offset);
                }

                Ok(list_ptr)
            }
            Expr::IndexAccess { object, index } => {
                let obj_val = self.compile_expr(object)?;
                let idx_val = self.compile_expr(index)?;
                let idx_val_ptr = self.builder.ins().sextend(self.ctx.ptr_type, idx_val);

                // Determine if Array or List
                let mut is_list = false;
                if let Expr::Identifier(name) = &**object {
                    if let Some(info) = self.var_info.get(name) {
                        if matches!(info, VarInfo::List { .. }) {
                            is_list = true;
                        }
                    }
                }

                let len =
                    self.builder
                        .ins()
                        .load(self.ctx.ptr_type, MachMemFlags::new(), obj_val, 0);
                self.emit_bounds_check(idx_val_ptr, len)?;

                let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                let byte_offset = self.builder.ins().imul(idx_val_ptr, eight);

                if is_list {
                    let buf_ptr = self.builder.ins().load(
                        self.ctx.ptr_type,
                        MachMemFlags::new(),
                        obj_val,
                        16,
                    );
                    let elem_addr = self.builder.ins().iadd(buf_ptr, byte_offset);
                    let element_type = self.collection_element_type(object);
                    Ok(self
                        .builder
                        .ins()
                        .load(element_type, MachMemFlags::new(), elem_addr, 0))
                } else {
                    let elem_addr = self.builder.ins().iadd(obj_val, byte_offset);
                    Ok(self
                        .builder
                        .ins()
                        .load(types::I32, MachMemFlags::new(), elem_addr, 16))
                }
            }
            Expr::MemberAccess { object, member } => {
                let obj_val = self.compile_expr(object)?;
                if member == "Length" || member == "Len" {
                    let len_val =
                        self.builder
                            .ins()
                            .load(types::I32, MachMemFlags::new(), obj_val, 0);
                    return Ok(len_val);
                }

                if let Expr::Identifier(name) = &**object {
                    if let Some(VarInfo::Tuple { fields }) = self.var_info.get(name).cloned() {
                        let idx = fields
                            .iter()
                            .position(|(_, f_name)| f_name == member)
                            .ok_or_else(|| {
                                ForgeError::codegen(format!("Unknown tuple field: {}", member))
                            })?;
                        let (field_type, _) = &fields[idx];
                        let offset = (idx * 8) as i32;
                        let val_type = match field_type {
                            Subtype::Float => types::F64,
                            Subtype::Weld => self.ctx.ptr_type,
                            _ => types::I32,
                        };
                        return Ok(self.builder.ins().load(
                            val_type,
                            MachMemFlags::new(),
                            obj_val,
                            offset,
                        ));
                    }
                }

                Err(ForgeError::codegen(format!(
                    "Member access not supported: {}.{}",
                    "obj", member
                )))
            }
            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                let obj_val = self.compile_expr(object)?;
                match method.as_str() {
                    "Length" | "Len" => {
                        let len_val =
                            self.builder
                                .ins()
                                .load(types::I32, MachMemFlags::new(), obj_val, 0);
                        Ok(len_val)
                    }
                    "Add" => {
                        if args.is_empty() {
                            return Err(ForgeError::codegen("Add expects 1 argument"));
                        }
                        let item_val = self.compile_expr(&args[0])?;
                        let len_val = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_val,
                            0,
                        );
                        let cap_val = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_val,
                            8,
                        );
                        let buf_val = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_val,
                            16,
                        );

                        let need_realloc = self.builder.ins().icmp(
                            IntCC::SignedGreaterThanOrEqual,
                            len_val,
                            cap_val,
                        );
                        let grow_block = self.builder.create_block();
                        let append_block = self.builder.create_block();

                        self.builder
                            .ins()
                            .brif(need_realloc, grow_block, &[], append_block, &[]);

                        self.builder.switch_to_block(grow_block);
                        self.builder.seal_block(grow_block);
                        let two = self.builder.ins().iconst(self.ctx.ptr_type, 2);
                        let new_cap = self.builder.ins().imul(cap_val, two);
                        let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                        let new_buf_size = self.builder.ins().imul(new_cap, eight);
                        let call_realloc = self
                            .builder
                            .ins()
                            .call(self.runtime.realloc, &[buf_val, new_buf_size]);
                        let new_buf = self.builder.inst_results(call_realloc)[0];
                        self.builder
                            .ins()
                            .store(MachMemFlags::new(), new_cap, obj_val, 8);
                        self.builder
                            .ins()
                            .store(MachMemFlags::new(), new_buf, obj_val, 16);
                        self.builder.ins().jump(append_block, &[]);

                        self.builder.switch_to_block(append_block);
                        self.builder.seal_block(append_block);

                        let current_buf = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_val,
                            16,
                        );
                        let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                        let offset = self.builder.ins().imul(len_val, eight);
                        let slot_addr = self.builder.ins().iadd(current_buf, offset);
                        self.builder
                            .ins()
                            .store(MachMemFlags::new(), item_val, slot_addr, 0);

                        let one = self.builder.ins().iconst(self.ctx.ptr_type, 1);
                        let new_len = self.builder.ins().iadd(len_val, one);
                        self.builder
                            .ins()
                            .store(MachMemFlags::new(), new_len, obj_val, 0);

                        let zero = self.builder.ins().iconst(types::I32, 0);
                        Ok(zero)
                    }
                    "Remove" | "RemoveAt" => {
                        if args.is_empty() {
                            return Err(ForgeError::codegen("Remove expects 1 argument"));
                        }
                        let idx_val = self.compile_expr(&args[0])?;
                        let idx_val_ptr = self.builder.ins().sextend(self.ctx.ptr_type, idx_val);

                        let len_val = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_val,
                            0,
                        );
                        let buf_val = self.builder.ins().load(
                            self.ctx.ptr_type,
                            MachMemFlags::new(),
                            obj_val,
                            16,
                        );
                        self.emit_bounds_check(idx_val_ptr, len_val)?;

                        let eight = self.builder.ins().iconst(self.ctx.ptr_type, 8);
                        let one = self.builder.ins().iconst(self.ctx.ptr_type, 1);

                        let dest_offset = self.builder.ins().imul(idx_val_ptr, eight);
                        let dest_addr = self.builder.ins().iadd(buf_val, dest_offset);

                        let next_idx = self.builder.ins().iadd(idx_val_ptr, one);
                        let src_offset = self.builder.ins().imul(next_idx, eight);
                        let src_addr = self.builder.ins().iadd(buf_val, src_offset);

                        let remaining_elements = self.builder.ins().isub(len_val, next_idx);
                        let bytes_to_move = self.builder.ins().imul(remaining_elements, eight);

                        self.builder
                            .ins()
                            .call(self.runtime.memmove, &[dest_addr, src_addr, bytes_to_move]);

                        let new_len = self.builder.ins().isub(len_val, one);
                        self.builder
                            .ins()
                            .store(MachMemFlags::new(), new_len, obj_val, 0);

                        let zero = self.builder.ins().iconst(types::I32, 0);
                        Ok(zero)
                    }
                    other => Err(ForgeError::codegen(format!(
                        "Unknown method call: {}",
                        other
                    ))),
                }
            }
            Expr::NamespaceCall {
                namespace,
                method,
                args,
            } => self.compile_namespace_call(namespace, method, args),
            Expr::Call { callee, args } => {
                let func_id = self.func_ids.get(callee).copied().ok_or_else(|| {
                    ForgeError::codegen(format!("Undefined function: {}", callee))
                })?;
                let func_ref = self
                    .ctx
                    .module
                    .declare_func_in_func(func_id, self.builder.func);
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(self.compile_expr(arg)?);
                }
                let call_inst = self.builder.ins().call(func_ref, &call_args);
                let ret_kind = self
                    .func_return_kinds
                    .get(callee)
                    .cloned()
                    .unwrap_or(RetKind::Void);
                if ret_kind.is_void() {
                    return Ok(self.builder.ins().iconst(types::I32, 0));
                }
                let results = self.builder.inst_results(call_inst);
                Ok(results[0])
            }
            Expr::BinaryOp { op, lhs, rhs } => {
                let l = self.compile_expr(lhs)?;
                let r = self.compile_expr(rhs)?;

                let is_float = self.is_float_expr(lhs) || self.is_float_expr(rhs);
                if is_float {
                    let l_f = if self.is_float_expr(lhs) {
                        l
                    } else {
                        self.builder.ins().fcvt_from_sint(types::F64, l)
                    };
                    let r_f = if self.is_float_expr(rhs) {
                        r
                    } else {
                        self.builder.ins().fcvt_from_sint(types::F64, r)
                    };
                    match op {
                        BinOp::Add => Ok(self.builder.ins().fadd(l_f, r_f)),
                        BinOp::Sub => Ok(self.builder.ins().fsub(l_f, r_f)),
                        BinOp::Mul => Ok(self.builder.ins().fmul(l_f, r_f)),
                        BinOp::Div => Ok(self.builder.ins().fdiv(l_f, r_f)),
                        BinOp::Pow => {
                            let pow_inst = self.builder.ins().call(self.runtime.pow, &[l_f, r_f]);
                            Ok(self.builder.inst_results(pow_inst)[0])
                        }
                        BinOp::Rem => Err(ForgeError::codegen(
                            "Modulo (%) is only supported for integer types",
                        )),
                        BinOp::Eq => Ok(self.builder.ins().fcmp(FloatCC::Equal, l_f, r_f)),
                        BinOp::Ne => Ok(self.builder.ins().fcmp(FloatCC::NotEqual, l_f, r_f)),
                        BinOp::Lt => Ok(self.builder.ins().fcmp(FloatCC::LessThan, l_f, r_f)),
                        BinOp::Gt => Ok(self.builder.ins().fcmp(FloatCC::GreaterThan, l_f, r_f)),
                        BinOp::Le => {
                            Ok(self.builder.ins().fcmp(FloatCC::LessThanOrEqual, l_f, r_f))
                        }
                        BinOp::Ge => {
                            Ok(self
                                .builder
                                .ins()
                                .fcmp(FloatCC::GreaterThanOrEqual, l_f, r_f))
                        }
                        BinOp::And | BinOp::Or | BinOp::Xor => Err(ForgeError::codegen(
                            "Boolean operators are not supported for Float values",
                        )),
                    }
                } else {
                    match op {
                        BinOp::Add => Ok(self.builder.ins().iadd(l, r)),
                        BinOp::Sub => Ok(self.builder.ins().isub(l, r)),
                        BinOp::Mul => Ok(self.builder.ins().imul(l, r)),
                        BinOp::Div => Ok(self.builder.ins().sdiv(l, r)),
                        BinOp::Rem => Ok(self.builder.ins().srem(l, r)),
                        BinOp::Pow => {
                            let l_f = self.builder.ins().fcvt_from_sint(types::F64, l);
                            let r_f = self.builder.ins().fcvt_from_sint(types::F64, r);
                            let pow_inst = self.builder.ins().call(self.runtime.pow, &[l_f, r_f]);
                            let res_f = self.builder.inst_results(pow_inst)[0];
                            Ok(self.builder.ins().fcvt_to_sint(types::I32, res_f))
                        }
                        BinOp::Eq => Ok(self.builder.ins().icmp(IntCC::Equal, l, r)),
                        BinOp::Ne => Ok(self.builder.ins().icmp(IntCC::NotEqual, l, r)),
                        BinOp::Lt => Ok(self.builder.ins().icmp(IntCC::SignedLessThan, l, r)),
                        BinOp::Gt => Ok(self.builder.ins().icmp(IntCC::SignedGreaterThan, l, r)),
                        BinOp::Le => {
                            Ok(self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, l, r))
                        }
                        BinOp::Ge => {
                            Ok(self
                                .builder
                                .ins()
                                .icmp(IntCC::SignedGreaterThanOrEqual, l, r))
                        }
                        BinOp::And => Ok(self.builder.ins().band(l, r)),
                        BinOp::Or => Ok(self.builder.ins().bor(l, r)),
                        BinOp::Xor => Ok(self.builder.ins().bxor(l, r)),
                    }
                }
            }
            Expr::UnaryOp { op, operand } => {
                let val = self.compile_expr(operand)?;
                match op {
                    UnOp::Plus => Ok(val),
                    UnOp::Neg => {
                        if self.is_float_expr(operand) {
                            Ok(self.builder.ins().fneg(val))
                        } else {
                            Ok(self.builder.ins().ineg(val))
                        }
                    }
                }
            }
            Expr::Input(input_node) => {
                let (fmt_gv, ty) = match input_node.subtype {
                    Some(Subtype::Int) => (self.runtime.int_scanf, types::I32),
                    Some(Subtype::Float) => (self.runtime.float_scanf, types::F64),
                    _ => {
                        // Input strings escape the current function, so allocate
                        // program-lifetime storage rather than returning a stack address.
                        let size = self.builder.ins().iconst(self.ctx.ptr_type, 256);
                        let allocation = self.builder.ins().call(self.runtime.malloc, &[size]);
                        let string_ptr = self.builder.inst_results(allocation)[0];
                        let fmt_val = self
                            .builder
                            .ins()
                            .symbol_value(self.ctx.ptr_type, self.runtime.str_scanf);
                        self.builder
                            .ins()
                            .call(self.runtime.scanf, &[fmt_val, string_ptr]);
                        return Ok(string_ptr);
                    }
                };

                let slot_data = StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 0);
                let slot = self.builder.create_sized_stack_slot(slot_data);

                let slot_ptr = self.builder.ins().stack_addr(self.ctx.ptr_type, slot, 0);
                let fmt_val = self.builder.ins().symbol_value(self.ctx.ptr_type, fmt_gv);

                self.builder
                    .ins()
                    .call(self.runtime.scanf, &[fmt_val, slot_ptr]);
                Ok(self
                    .builder
                    .ins()
                    .stack_load(self.ctx.ptr_type, ty, slot, 0))
            }
            Expr::Str(parts) => {
                let mut s = String::new();
                for part in parts {
                    if let StringPart::Literal(l) = part {
                        s.push_str(l);
                    }
                }
                if let Some(gv) = self.string_map.get(&s).copied() {
                    Ok(self.builder.ins().symbol_value(self.ctx.ptr_type, gv))
                } else {
                    Err(ForgeError::codegen(format!(
                        "Undefined string literal: {}",
                        s
                    )))
                }
            }
        }
    }
}

pub fn compile(program: &Program, obj_path: &std::path::Path, _link_math: bool) -> ForgeResult<()> {
    if crate::backend::program_uses_error_handling(program) {
        return Err(ForgeError::codegen(
            "typed error handling is not yet supported by the Cranelift backend; use --backend native",
        ));
    }
    let mut ctx = CodeGenContext::new()?;
    let ptr_type = ctx.ptr_type;
    let function_return_kinds: HashMap<String, RetKind> = program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::FunctionDecl(function) => {
                Some((function.name.clone(), function.ret_kind.clone()))
            }
            _ => None,
        })
        .collect();

    let mut function_ids: HashMap<String, FuncId> = HashMap::new();

    for stmt in &program.statements {
        let Statement::FunctionDecl(function) = stmt else {
            continue;
        };
        if function.name == "Main" {
            continue;
        }
        let mut sig = ctx.module.make_signature();
        for param in &function.params {
            let ty = match &param.type_decl {
                TypeDecl::Number(Subtype::Float) => types::F64,
                TypeDecl::Number(_) | TypeDecl::Bool => types::I32,
                TypeDecl::Weld => ptr_type,
                TypeDecl::Ore(_) | TypeDecl::OreTuple(_) | TypeDecl::Materials(_, _) => ptr_type,
            };
            sig.params.push(AbiParam::new(ty));
        }
        match &function.ret_kind {
            RetKind::Int | RetKind::Bool => sig.returns.push(AbiParam::new(types::I32)),
            RetKind::Nunction | RetKind::Void => {}
            RetKind::Float => sig.returns.push(AbiParam::new(types::F64)),
            RetKind::Weld
            | RetKind::Ore(_)
            | RetKind::OreTuple(_)
            | RetKind::Materials(_, _)
            | RetKind::Generic
            | RetKind::Function
            | RetKind::Dynamic => sig.returns.push(AbiParam::new(ptr_type)),
        }
        let func_id = ctx
            .module
            .declare_function(&function.name, Linkage::Local, &sig)
            .map_err(|e| ForgeError::codegen(format!("declare {}: {}", function.name, e)))?;
        function_ids.insert(function.name.clone(), func_id);
    }

    let mut main_sig = ctx.module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I32));
    let main_id = ctx
        .module
        .declare_function("main", Linkage::Export, &main_sig)
        .map_err(|e| ForgeError::codegen(format!("declare main: {}", e)))?;

    for stmt in &program.statements {
        let Statement::FunctionDecl(function) = stmt else {
            continue;
        };
        if function.name == "Main" {
            continue;
        }
        let func_id = *function_ids.get(&function.name).unwrap();
        let mut clif_ctx = ctx.module.make_context();
        let mut fn_builder_ctx = FunctionBuilderContext::new();
        let mut sig = ctx.module.make_signature();
        for param in &function.params {
            let ty = match &param.type_decl {
                TypeDecl::Number(Subtype::Float) => types::F64,
                TypeDecl::Number(_) | TypeDecl::Bool => types::I32,
                TypeDecl::Weld => ptr_type,
                TypeDecl::Ore(_) | TypeDecl::OreTuple(_) | TypeDecl::Materials(_, _) => ptr_type,
            };
            sig.params.push(AbiParam::new(ty));
        }
        match &function.ret_kind {
            RetKind::Int | RetKind::Bool => sig.returns.push(AbiParam::new(types::I32)),
            RetKind::Nunction | RetKind::Void => {}
            RetKind::Float => sig.returns.push(AbiParam::new(types::F64)),
            RetKind::Weld
            | RetKind::Ore(_)
            | RetKind::OreTuple(_)
            | RetKind::Materials(_, _)
            | RetKind::Generic
            | RetKind::Function
            | RetKind::Dynamic => sig.returns.push(AbiParam::new(ptr_type)),
        }
        clif_ctx.func.signature = sig.clone();
        clif_ctx.func.name = UserFuncName::user(0, func_id.as_u32());
        let runtime = ctx.declare_runtime_in_func(&mut clif_ctx.func);

        let mut string_map: HashMap<String, GlobalValue> = HashMap::new();
        for stmt in &function.body {
            collect_strings(stmt, &mut string_map, &mut ctx, &mut clif_ctx)?;
        }
        if !string_map.contains_key("\n") {
            let id = ctx.define_string("str_newline", b"\n\0")?;
            let gv = ctx.module.declare_data_in_func(id, &mut clif_ctx.func);
            string_map.insert("\n".to_string(), gv);
        }

        let mut builder = FunctionBuilder::new(&mut clif_ctx.func, &mut fn_builder_ctx);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut compiler = FunctionCompiler::new(
            &mut ctx,
            builder,
            runtime,
            string_map,
            function_ids.clone(),
            function_return_kinds.clone(),
            function.ret_kind.clone(),
        );

        for (index, param) in function.params.iter().enumerate() {
            let ty = match &param.type_decl {
                TypeDecl::Number(Subtype::Float) => types::F64,
                TypeDecl::Number(_) | TypeDecl::Bool => types::I32,
                TypeDecl::Weld => ptr_type,
                TypeDecl::Ore(_) | TypeDecl::OreTuple(_) | TypeDecl::Materials(_, _) => ptr_type,
            };
            let var = compiler.builder.declare_var(ty);
            let value = compiler.builder.block_params(entry)[index];
            compiler.var_map.insert(param.name.clone(), var);
            compiler.var_info.insert(
                param.name.clone(),
                match &param.type_decl {
                    TypeDecl::Number(Subtype::Float) => VarInfo::Primitive(types::F64),
                    TypeDecl::Number(_) => VarInfo::Primitive(types::I32),
                    TypeDecl::Bool => VarInfo::Primitive(types::I32),
                    TypeDecl::Weld => VarInfo::Primitive(ptr_type),
                    TypeDecl::Ore(size) => VarInfo::Array {
                        element_type: Subtype::Int,
                        size: size.unwrap_or(0) as usize,
                    },
                    TypeDecl::OreTuple(fields) => VarInfo::Tuple {
                        fields: fields.clone(),
                    },
                    TypeDecl::Materials(elem_type, _) => VarInfo::List {
                        element_type: elem_type.clone(),
                    },
                },
            );
            if matches!(&param.type_decl, TypeDecl::Weld) {
                compiler.string_vars.insert(param.name.clone());
            }
            if matches!(&param.type_decl, TypeDecl::Number(Subtype::Float)) {
                compiler.float_vars.insert(param.name.clone());
            }
            if matches!(&param.type_decl, TypeDecl::Bool) {
                compiler.bool_vars.insert(param.name.clone());
            }
            compiler.builder.def_var(var, value);
        }

        for stmt in &function.body {
            if compiler.terminated {
                break;
            }
            compiler.compile_statement(stmt)?;
        }

        if !compiler.terminated && !ends_with_explicit_return(&function.body) {
            if function.ret_kind.is_void() {
                compiler.builder.ins().return_(&[]);
            } else {
                let zero = match &function.ret_kind {
                    RetKind::Int | RetKind::Bool => compiler.builder.ins().iconst(types::I32, 0),
                    RetKind::Float => compiler.builder.ins().f64const(0.0),
                    _ => compiler.builder.ins().iconst(ptr_type, 0),
                };
                compiler.builder.ins().return_(&[zero]);
            }
        }

        ctx.module
            .define_function(func_id, &mut clif_ctx)
            .map_err(|e| ForgeError::codegen(format!("define {}: {}", function.name, e)))?;
    }

    let main_func = program
        .statements
        .iter()
        .find_map(|s| {
            if let Statement::FunctionDecl(f) = s {
                if f.name == "Main" {
                    return Some(f);
                }
            }
            None
        })
        .ok_or_else(|| ForgeError::codegen("No Main function found"))?;

    let mut clif_ctx = ctx.module.make_context();
    let mut fn_builder_ctx = FunctionBuilderContext::new();
    clif_ctx.func.signature = main_sig.clone();
    clif_ctx.func.name = UserFuncName::user(0, main_id.as_u32());
    let runtime = ctx.declare_runtime_in_func(&mut clif_ctx.func);

    let mut string_map: HashMap<String, GlobalValue> = HashMap::new();
    for stmt in &program.statements {
        if matches!(stmt, Statement::ObjectDecl(_)) {
            collect_strings(stmt, &mut string_map, &mut ctx, &mut clif_ctx)?;
        }
    }
    for stmt in &main_func.body {
        collect_strings(stmt, &mut string_map, &mut ctx, &mut clif_ctx)?;
    }
    if !string_map.contains_key("\n") {
        let id = ctx.define_string("str_newline", b"\n\0")?;
        let gv = ctx.module.declare_data_in_func(id, &mut clif_ctx.func);
        string_map.insert("\n".to_string(), gv);
    }

    {
        let mut builder = FunctionBuilder::new(&mut clif_ctx.func, &mut fn_builder_ctx);
        let block = builder.create_block();
        builder.switch_to_block(block);
        builder.seal_block(block);
        let mut compiler = FunctionCompiler::new(
            &mut ctx,
            builder,
            runtime,
            string_map,
            function_ids.clone(),
            function_return_kinds.clone(),
            RetKind::Int,
        );
        for stmt in &program.statements {
            if matches!(stmt, Statement::ObjectDecl(_)) {
                compiler.compile_statement(stmt)?;
            }
        }
        for stmt in &main_func.body {
            if compiler.terminated {
                break;
            }
            compiler.compile_statement(stmt)?;
        }
        if !compiler.terminated && !ends_with_explicit_return(&main_func.body) {
            let ret_value = compiler.builder.ins().iconst(types::I32, 0);
            compiler.builder.ins().return_(&[ret_value]);
        }
    }

    ctx.module
        .define_function(main_id, &mut clif_ctx)
        .map_err(|e| ForgeError::codegen(format!("define main: {}", e)))?;
    let product = ctx.module.finish();
    let bytes = product
        .emit()
        .map_err(|e| ForgeError::codegen(format!("emit object: {}", e)))?;
    std::fs::write(obj_path, bytes)
        .map_err(|e| ForgeError::codegen(format!("write object: {}", e)))?;

    Ok(())
}

fn ends_with_explicit_return(body: &[Statement]) -> bool {
    match body.last() {
        Some(Statement::Return(_)) => true,
        _ => false,
    }
}

fn collect_strings(
    stmt: &Statement,
    string_map: &mut HashMap<String, GlobalValue>,
    ctx: &mut CodeGenContext,
    clif_ctx: &mut cranelift_codegen::Context,
) -> ForgeResult<()> {
    match stmt {
        Statement::Print(p) => {
            collect_strings_in_expr(&p.expr, string_map, ctx, clif_ctx)?;
        }
        Statement::VarDecl(v) => {
            if let Some(init) = &v.initializer {
                collect_strings_in_expr(init, string_map, ctx, clif_ctx)?;
            }
        }
        Statement::Assignment(a) => {
            collect_strings_in_expr(&a.value, string_map, ctx, clif_ctx)?;
        }
        Statement::ObjectDecl(obj) => {
            for (_, value) in &obj.inits {
                collect_strings_in_expr(value, string_map, ctx, clif_ctx)?;
            }
        }
        Statement::If(if_node) => {
            for (_, body) in &if_node.branches {
                for s in body {
                    collect_strings(s, string_map, ctx, clif_ctx)?;
                }
            }
            if let Some(else_body) = &if_node.else_body {
                for s in else_body {
                    collect_strings(s, string_map, ctx, clif_ctx)?;
                }
            }
        }
        Statement::Switch(node) => {
            collect_strings_in_expr(&node.expression, string_map, ctx, clif_ctx)?;
            for branch in &node.branches {
                match branch {
                    SwitchBranch::Case(case) => {
                        for value in &case.values {
                            collect_strings_in_expr(value, string_map, ctx, clif_ctx)?;
                        }
                        collect_strings_in_body(&case.body, string_map, ctx, clif_ctx)?;
                    }
                    SwitchBranch::Base(body) => {
                        collect_strings_in_body(body, string_map, ctx, clif_ctx)?;
                    }
                }
            }
        }
        Statement::While(while_node) => {
            collect_strings_in_body(&while_node.body, string_map, ctx, clif_ctx)?;
        }
        Statement::For(for_node) => {
            if let Some(init) = &for_node.init.initializer {
                collect_strings_in_expr(init, string_map, ctx, clif_ctx)?;
            }
            collect_strings_in_expr(&for_node.condition, string_map, ctx, clif_ctx)?;
            collect_strings_in_body(&for_node.body, string_map, ctx, clif_ctx)?;
        }
        Statement::ForEach(foreach_node) => {
            collect_strings_in_body(&foreach_node.body, string_map, ctx, clif_ctx)?;
        }
        Statement::Return(Some(value)) | Statement::ExprStmt(value) => {
            collect_strings_in_expr(value, string_map, ctx, clif_ctx)?;
        }
        _ => {}
    }
    Ok(())
}

fn collect_strings_in_expr(
    expr: &Expr,
    string_map: &mut HashMap<String, GlobalValue>,
    ctx: &mut CodeGenContext,
    clif_ctx: &mut cranelift_codegen::Context,
) -> ForgeResult<()> {
    match expr {
        Expr::Str(parts) => {
            let mut s = String::new();
            for part in parts {
                if let StringPart::Literal(l) = part {
                    s.push_str(l);
                }
            }
            if !string_map.contains_key(&s) {
                let name = string_symbol_name(&s);
                let id = ctx.define_string(&name, format!("{}\0", s).as_bytes())?;
                let gv = ctx.module.declare_data_in_func(id, &mut clif_ctx.func);
                string_map.insert(s.clone(), gv);
            }
            for part in parts {
                if let StringPart::Literal(text) = part {
                    if !string_map.contains_key(text) {
                        let name = string_symbol_name(text);
                        let id = ctx.define_string(&name, format!("{}\0", text).as_bytes())?;
                        let gv = ctx.module.declare_data_in_func(id, &mut clif_ctx.func);
                        string_map.insert(text.clone(), gv);
                    }
                }
            }
        }
        Expr::ArrayLiteral(elements)
        | Expr::TupleLiteral(elements)
        | Expr::ListLiteral(elements) => {
            for elem in elements {
                collect_strings_in_expr(elem, string_map, ctx, clif_ctx)?;
            }
        }
        Expr::Call { args, .. } | Expr::NamespaceCall { args, .. } => {
            for arg in args {
                collect_strings_in_expr(arg, string_map, ctx, clif_ctx)?;
            }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_strings_in_expr(object, string_map, ctx, clif_ctx)?;
            for arg in args {
                collect_strings_in_expr(arg, string_map, ctx, clif_ctx)?;
            }
        }
        Expr::BinaryOp { lhs, rhs, .. } => {
            collect_strings_in_expr(lhs, string_map, ctx, clif_ctx)?;
            collect_strings_in_expr(rhs, string_map, ctx, clif_ctx)?;
        }
        Expr::UnaryOp { operand, .. } => {
            collect_strings_in_expr(operand, string_map, ctx, clif_ctx)?;
        }
        Expr::MemberAccess { object, .. } => {
            collect_strings_in_expr(object, string_map, ctx, clif_ctx)?;
        }
        Expr::IndexAccess { object, index } => {
            collect_strings_in_expr(object, string_map, ctx, clif_ctx)?;
            collect_strings_in_expr(index, string_map, ctx, clif_ctx)?;
        }
        _ => {}
    }
    Ok(())
}

fn string_symbol_name(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("str_lit_{:016x}", hasher.finish())
}

fn collect_strings_in_body(
    body: &[Statement],
    string_map: &mut HashMap<String, GlobalValue>,
    ctx: &mut CodeGenContext,
    clif_ctx: &mut cranelift_codegen::Context,
) -> ForgeResult<()> {
    for stmt in body {
        collect_strings(stmt, string_map, ctx, clif_ctx)?;
    }
    Ok(())
}
