use std::collections::{HashMap, HashSet};

use crate::ast::{
    AssignmentTarget, BinOp, DoNode, ErrorType, Expr, ForNode, FunctionDecl, IncrOp, Program,
    RetKind, Statement, Subtype, SwitchBranch, SwitchNode, TypeDecl, UnOp, WhileNode,
};
use crate::errors::{ForgeError, ForgeResult};
use crate::ir::{
    BasicBlock, InputKind, IrBinOp, IrFunction, IrInst, IrProgram, IrTerminator, Operand,
    PrintKind, VReg,
};

#[derive(Clone)]
struct LoopCtx {
    continue_label: String,
    exit_label: String,
}

struct Lowerer {
    vreg_counter: usize,
    block_counter: usize,
    current_label: String,
    current_insts: Vec<IrInst>,
    blocks: Vec<BasicBlock>,
    terminated: bool,
    known_functions: HashSet<String>,
    function_ret_kinds: HashMap<String, RetKind>,
    break_stack: Vec<LoopCtx>,
    var_types: HashMap<String, TypeDecl>,
    var_storage: HashMap<String, String>,
    storage_counter: usize,
    error_targets: Vec<String>,
    function_error_label: String,
    finally_return_targets: Vec<String>,
    current_ret_kind: RetKind,
}

impl Lowerer {
    fn new(
        known_functions: HashSet<String>,
        function_ret_kinds: HashMap<String, RetKind>,
        function_error_label: String,
        current_ret_kind: RetKind,
    ) -> Self {
        Self {
            vreg_counter: 0,
            block_counter: 0,
            current_label: String::new(),
            current_insts: Vec::new(),
            blocks: Vec::new(),
            terminated: false,
            known_functions,
            function_ret_kinds,
            break_stack: Vec::new(),
            var_types: HashMap::new(),
            var_storage: HashMap::new(),
            storage_counter: 0,
            error_targets: Vec::new(),
            function_error_label,
            finally_return_targets: Vec::new(),
            current_ret_kind,
        }
    }

    fn alloc_vreg(&mut self) -> VReg {
        let v = self.vreg_counter;
        self.vreg_counter += 1;
        v
    }

    fn alloc_label(&mut self, prefix: &str) -> String {
        let id = self.block_counter;
        self.block_counter += 1;
        format!("{}_{}", prefix, id)
    }

    fn start_block(&mut self, label: String) {
        self.current_label = label;
        self.current_insts = Vec::new();
        self.terminated = false;
    }

    fn finish_block(&mut self, terminator: IrTerminator) {
        if self.terminated {
            return;
        }
        let block = BasicBlock {
            label: std::mem::take(&mut self.current_label),
            insts: std::mem::take(&mut self.current_insts),
            terminator,
        };
        self.blocks.push(block);
        self.terminated = true;
    }

    fn emit(&mut self, inst: IrInst) {
        if !self.terminated {
            self.current_insts.push(inst);
        }
    }

    fn storage_name(&self, name: &str) -> String {
        self.var_storage
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    fn error_target(&self) -> String {
        self.error_targets
            .last()
            .cloned()
            .unwrap_or_else(|| self.function_error_label.clone())
    }

    fn branch_on_error(&mut self) {
        let status = self.alloc_vreg();
        self.emit(IrInst::GetErrorCode { dest: status });
        let continuation = self.alloc_label("error_ok");
        self.finish_block(IrTerminator::Branch {
            cond: Operand::Reg(status),
            true_label: self.error_target(),
            false_label: continuation.clone(),
        });
        self.start_block(continuation);
    }
}

pub fn lower(program: &Program) -> ForgeResult<IrProgram> {
    let mut known_functions = HashSet::new();
    let mut function_ret_kinds = HashMap::new();
    let mut top_level_stmts = Vec::new();

    for stmt in &program.statements {
        match stmt {
            Statement::FunctionDecl(f) => {
                known_functions.insert(f.name.clone());
                function_ret_kinds.insert(f.name.clone(), f.ret_kind.clone());
            }
            loose_stmt => {
                top_level_stmts.push(loose_stmt.clone());
            }
        }
    }

    if !known_functions.contains("Main") {
        return Err(ForgeError::codegen("No Main function found"));
    }

    let mut ir_functions = Vec::new();
    for stmt in &program.statements {
        if let Statement::FunctionDecl(f) = stmt {
            let ir_func =
                lower_function(f, &known_functions, &function_ret_kinds, &top_level_stmts)?;
            ir_functions.push(ir_func);
        }
    }

    Ok(IrProgram {
        functions: ir_functions,
    })
}

fn lower_function(
    f: &FunctionDecl,
    known_funcs: &HashSet<String>,
    function_ret_kinds: &HashMap<String, RetKind>,
    top_level_stmts: &[Statement],
) -> ForgeResult<IrFunction> {
    if f.params.len() > 6 {
        return Err(ForgeError::codegen(format!(
            "Function '{}' has {} parameters. Native backend supports at most 6 integer parameters.",
            f.name,
            f.params.len()
        )));
    }

    for param in &f.params {
        match &param.type_decl {
            TypeDecl::Number(Subtype::Int) | TypeDecl::Bool | TypeDecl::Weld => {}
            td => {
                return Err(ForgeError::codegen(format!(
                    "Parameter '{}' in function '{}' has unsupported type {:?}. Native backend supports Int, Bool, Weld.",
                    param.name, f.name, td
                )));
            }
        }
    }

    match &f.ret_kind {
        RetKind::Int | RetKind::Bool | RetKind::Weld | RetKind::Nunction | RetKind::Void => {}
        rk => {
            return Err(ForgeError::codegen(format!(
                "Function '{}' has unsupported return kind {:?}. Native backend supports Int/Bool/Weld/Void.",
                f.name, rk
            )));
        }
    }

    let error_label = format!("fn_{}_propagate_error", f.name);
    let mut lowerer = Lowerer::new(
        known_funcs.clone(),
        function_ret_kinds.clone(),
        error_label.clone(),
        f.ret_kind.clone(),
    );
    let entry_label = lowerer.alloc_label(&format!("fn_{}_entry", f.name));
    lowerer.start_block(entry_label);

    let mut param_names = Vec::new();
    for (idx, param) in f.params.iter().enumerate() {
        param_names.push(param.name.clone());
        lowerer
            .var_types
            .insert(param.name.clone(), param.type_decl.clone());
        let dest = lowerer.alloc_vreg();
        lowerer.emit(IrInst::Param { dest, index: idx });
        lowerer.emit(IrInst::StoreVar {
            var_name: param.name.clone(),
            src: Operand::Reg(dest),
        });
    }

    if f.name == "Main" && !top_level_stmts.is_empty() {
        lower_statements(top_level_stmts, &mut lowerer)?;
    }

    lower_statements(&f.body, &mut lowerer)?;

    if !lowerer.terminated {
        let default_ret = if f.ret_kind.is_void() {
            IrTerminator::Ret(None)
        } else {
            IrTerminator::Ret(Some(Operand::Const(0)))
        };
        lowerer.finish_block(default_ret);
    }

    lowerer.start_block(error_label);
    lowerer.finish_block(IrTerminator::PropagateError);

    Ok(IrFunction {
        name: f.name.clone(),
        param_names,
        blocks: lowerer.blocks,
    })
}

fn lower_statements(stmts: &[Statement], lowerer: &mut Lowerer) -> ForgeResult<()> {
    for stmt in stmts {
        if lowerer.terminated {
            break;
        }
        lower_statement(stmt, lowerer)?;
    }
    Ok(())
}

fn lower_statement(stmt: &Statement, lowerer: &mut Lowerer) -> ForgeResult<()> {
    match stmt {
        Statement::VarDecl(decl) => {
            match &decl.type_decl {
                TypeDecl::Number(Subtype::Int) | TypeDecl::Bool | TypeDecl::Weld => {}
                td => {
                    return Err(ForgeError::codegen(format!(
                        "Variable '{}' has unsupported type {:?}. Native backend supports Int, Bool, Weld.",
                        decl.name, td
                    )));
                }
            }
            let src = if let Some(init) = &decl.initializer {
                lower_expr(init, lowerer)?
            } else {
                Operand::Const(0)
            };
            lowerer
                .var_types
                .insert(decl.name.clone(), decl.type_decl.clone());
            let storage_name = if lowerer.var_storage.contains_key(&decl.name) {
                let storage_name = format!("{}#{}", decl.name, lowerer.storage_counter);
                lowerer.storage_counter += 1;
                storage_name
            } else {
                decl.name.clone()
            };
            lowerer
                .var_storage
                .insert(decl.name.clone(), storage_name.clone());
            lowerer.emit(IrInst::StoreVar {
                var_name: storage_name,
                src,
            });
        }
        Statement::Assignment(assign) => {
            let var_name = match &assign.target {
                AssignmentTarget::Var(name) => name.clone(),
                _ => {
                    return Err(ForgeError::codegen(
                        "Native backend only supports variable assignment targets",
                    ));
                }
            };
            let src = lower_expr(&assign.value, lowerer)?;
            let storage_name = lowerer.storage_name(&var_name);
            lowerer.emit(IrInst::StoreVar {
                var_name: storage_name,
                src,
            });
        }
        Statement::Return(opt_expr) => {
            let ret_op = if let Some(expr) = opt_expr {
                Some(lower_expr(expr, lowerer)?)
            } else {
                None
            };
            if let Some(target) = lowerer.finally_return_targets.last().cloned() {
                lowerer.emit(IrInst::StoreVar {
                    var_name: "__sydrogen_return_value".to_string(),
                    src: ret_op.unwrap_or(Operand::Const(0)),
                });
                lowerer.finish_block(IrTerminator::Jump(target));
            } else {
                lowerer.finish_block(IrTerminator::Ret(ret_op));
            }
        }
        Statement::Print(print_node) => {
            if let Expr::Str(parts) = &print_node.expr {
                if parts
                    .iter()
                    .any(|part| matches!(part, crate::ast::StringPart::Interp(_)))
                {
                    for part in parts {
                        match part {
                            crate::ast::StringPart::Literal(text) if !text.is_empty() => {
                                lowerer.emit(IrInst::PrintConstStr {
                                    str_val: text.clone(),
                                });
                            }
                            crate::ast::StringPart::Interp(name) => {
                                let expr = if name.contains('.') {
                                    let mut path = name.split('.');
                                    let object = path.next().unwrap().to_string();
                                    Expr::MemberAccess {
                                        object: Box::new(Expr::Identifier(object)),
                                        member: path.collect::<Vec<_>>().join("."),
                                    }
                                } else {
                                    Expr::Identifier(name.clone())
                                };
                                let is_string = is_string_expr(&expr, lowerer);
                                let is_bool = is_bool_expr(&expr, lowerer);
                                let src = lower_expr(&expr, lowerer)?;
                                let kind = if is_string {
                                    PrintKind::Str
                                } else if is_bool {
                                    PrintKind::Bool
                                } else {
                                    PrintKind::Int
                                };
                                lowerer.emit(IrInst::PrintInline { src, kind });
                            }
                            crate::ast::StringPart::Literal(_) => {}
                        }
                    }
                    lowerer.emit(IrInst::PrintConstStr {
                        str_val: "\n".to_string(),
                    });
                } else {
                    let text = parts
                        .iter()
                        .filter_map(|part| match part {
                            crate::ast::StringPart::Literal(text) => Some(text.as_str()),
                            crate::ast::StringPart::Interp(_) => None,
                        })
                        .collect::<String>();
                    lowerer.emit(IrInst::PrintConstStr {
                        str_val: format!("{}\n", text),
                    });
                }
            } else {
                let is_string = is_string_expr(&print_node.expr, lowerer);
                let is_bool = is_bool_expr(&print_node.expr, lowerer);
                let src = lower_expr(&print_node.expr, lowerer)?;
                let kind = if is_string {
                    PrintKind::Str
                } else if is_bool {
                    PrintKind::Bool
                } else {
                    PrintKind::Int
                };
                lowerer.emit(IrInst::Print { src, kind });
            }
        }
        Statement::If(if_node) => {
            let join_label = lowerer.alloc_label("if_join");
            let is_top_level_if = lowerer.break_stack.is_empty();
            if is_top_level_if {
                lowerer.break_stack.push(LoopCtx {
                    continue_label: String::new(),
                    exit_label: join_label.clone(),
                });
            }

            let mut cond_labels = Vec::new();
            let mut branch_labels = Vec::new();
            for i in 0..if_node.branches.len() {
                cond_labels.push(if i == 0 {
                    lowerer.current_label.clone()
                } else {
                    lowerer.alloc_label(&format!("if_cond_{}", i))
                });
                branch_labels.push(lowerer.alloc_label(&format!("if_branch_{}", i)));
            }

            let else_label = if if_node.else_body.is_some() {
                lowerer.alloc_label("if_else")
            } else {
                join_label.clone()
            };

            for i in 0..if_node.branches.len() {
                if i > 0 {
                    lowerer.start_block(cond_labels[i].clone());
                }

                let cond_op = lower_expr(&if_node.branches[i].0, lowerer)?;
                let false_target = if i + 1 < if_node.branches.len() {
                    cond_labels[i + 1].clone()
                } else {
                    else_label.clone()
                };

                lowerer.finish_block(IrTerminator::Branch {
                    cond: cond_op,
                    true_label: branch_labels[i].clone(),
                    false_label: false_target,
                });

                lowerer.start_block(branch_labels[i].clone());
                lower_statements(&if_node.branches[i].1, lowerer)?;
                if !lowerer.terminated {
                    lowerer.finish_block(IrTerminator::Jump(join_label.clone()));
                }
            }

            if let Some(else_body) = &if_node.else_body {
                lowerer.start_block(else_label);
                lower_statements(else_body, lowerer)?;
                if !lowerer.terminated {
                    lowerer.finish_block(IrTerminator::Jump(join_label.clone()));
                }
            }

            if is_top_level_if {
                lowerer.break_stack.pop();
            }
            lowerer.start_block(join_label);
        }
        Statement::While(while_node) => {
            lower_while_stmt(while_node, lowerer)?;
        }
        Statement::For(for_node) => {
            lower_for_stmt(for_node, lowerer)?;
        }
        Statement::ForEach(_) => {
            return Err(ForgeError::codegen("ForEach requires the typed backend"));
        }
        Statement::Switch(node) => lower_switch_statement(node, lowerer)?,
        Statement::Do(node) => lower_do_statement(node, lowerer)?,
        Statement::Throw(error) => {
            if !error.error_type.is_builtin() {
                return Err(ForgeError::codegen(format!(
                    "invalid error type '{}'",
                    error.error_type.name()
                )));
            }
            let message = lower_expr(&error.message, lowerer)?;
            let type_name = error_type_operand(&error.error_type, lowerer);
            lowerer.emit(IrInst::SetError {
                code: Operand::Const(error.error_type.code()),
                message,
                type_name,
            });
            lowerer.finish_block(IrTerminator::Jump(lowerer.error_target()));
        }
        Statement::Stop => {
            let break_ctx = lowerer.break_stack.last().cloned().ok_or_else(|| {
                ForgeError::codegen("Stop can only be used inside a loop or If statement")
            })?;
            lowerer.finish_block(IrTerminator::Jump(break_ctx.exit_label));
        }
        Statement::Skip => {
            let loop_ctx = lowerer
                .break_stack
                .last()
                .cloned()
                .ok_or_else(|| ForgeError::codegen("Skip can only be used inside a loop"))?;
            if loop_ctx.continue_label.is_empty() {
                return Err(ForgeError::codegen("Skip can only be used inside a loop"));
            }
            lowerer.finish_block(IrTerminator::Jump(loop_ctx.continue_label));
        }
        Statement::ExprStmt(expr) => {
            if let Expr::NamespaceCall {
                namespace,
                method,
                args,
            } = expr
            {
                if namespace == "Program" && method == "Stop" && args.is_empty() {
                    lowerer.emit(IrInst::SysExit {
                        status: Operand::Const(0),
                    });
                    return Ok(());
                }
            }
            lower_expr(expr, lowerer)?;
        }
        Statement::Input(_) => {
            return Err(ForgeError::codegen(
                "Input statements are not supported; use Input(...) as an expression",
            ));
        }
        Statement::Use(_) | Statement::Using(_) => {
            return Err(ForgeError::codegen(
                "unresolved import reached native backend",
            ));
        }
        Statement::DataDecl(_) => {
            return Err(ForgeError::codegen(
                "Data declarations are not yet supported in native backend",
            ));
        }
        Statement::ObjectDecl(_) => {
            return Err(ForgeError::codegen(
                "Object declarations are not yet supported in native backend",
            ));
        }
        Statement::FunctionDecl(_) => {
            return Err(ForgeError::codegen(
                "Nested function declarations are not supported",
            ));
        }
    }
    Ok(())
}

fn lower_switch_statement(node: &SwitchNode, lowerer: &mut Lowerer) -> ForgeResult<()> {
    let switch_value = lower_expr(&node.expression, lowerer)?;
    let join_label = lowerer.alloc_label("switch_end");
    let is_top_level_switch = lowerer.break_stack.is_empty();
    if is_top_level_switch {
        lowerer.break_stack.push(LoopCtx {
            continue_label: String::new(),
            exit_label: join_label.clone(),
        });
    }

    let cases: Vec<_> = node
        .branches
        .iter()
        .filter_map(|branch| match branch {
            SwitchBranch::Case(case) => Some(case),
            SwitchBranch::Base(_) => None,
        })
        .collect();
    let base_body = node.branches.iter().find_map(|branch| match branch {
        SwitchBranch::Base(body) => Some(body),
        SwitchBranch::Case(_) => None,
    });
    let case_labels: Vec<_> = (0..cases.len())
        .map(|index| lowerer.alloc_label(&format!("switch_case_{}", index)))
        .collect();
    let base_label = base_body.map(|_| lowerer.alloc_label("switch_base"));
    let checks: Vec<_> = cases
        .iter()
        .enumerate()
        .flat_map(|(case_index, case)| case.values.iter().map(move |value| (case_index, value)))
        .collect();
    let check_labels: Vec<_> = (0..checks.len())
        .map(|index| {
            if index == 0 {
                lowerer.current_label.clone()
            } else {
                lowerer.alloc_label(&format!("switch_check_{}", index))
            }
        })
        .collect();

    if checks.is_empty() {
        lowerer.finish_block(IrTerminator::Jump(
            base_label.clone().unwrap_or_else(|| join_label.clone()),
        ));
    } else {
        for (index, (case_index, case_value)) in checks.iter().enumerate() {
            if index > 0 {
                lowerer.start_block(check_labels[index].clone());
            }
            let pattern = lower_expr(case_value, lowerer)?;
            let matches = lowerer.alloc_vreg();
            lowerer.emit(IrInst::BinOp {
                dest: matches,
                op: IrBinOp::Eq,
                lhs: switch_value.clone(),
                rhs: pattern,
            });
            let no_match = if index + 1 < checks.len() {
                check_labels[index + 1].clone()
            } else {
                base_label.clone().unwrap_or_else(|| join_label.clone())
            };
            lowerer.finish_block(IrTerminator::Branch {
                cond: Operand::Reg(matches),
                true_label: case_labels[*case_index].clone(),
                false_label: no_match,
            });
        }
    }

    for (index, case) in cases.iter().enumerate() {
        lowerer.start_block(case_labels[index].clone());
        lower_statements(&case.body, lowerer)?;
        if !lowerer.terminated {
            lowerer.finish_block(IrTerminator::Jump(join_label.clone()));
        }
    }
    if let (Some(label), Some(body)) = (base_label, base_body) {
        lowerer.start_block(label);
        lower_statements(body, lowerer)?;
        if !lowerer.terminated {
            lowerer.finish_block(IrTerminator::Jump(join_label.clone()));
        }
    }

    if is_top_level_switch {
        lowerer.break_stack.pop();
    }
    lowerer.start_block(join_label);
    Ok(())
}

fn lower_while_stmt(node: &WhileNode, lowerer: &mut Lowerer) -> ForgeResult<()> {
    let cond_label = lowerer.alloc_label("while_cond");
    let body_label = lowerer.alloc_label("while_body");
    let exit_label = lowerer.alloc_label("while_exit");

    lowerer.break_stack.push(LoopCtx {
        continue_label: cond_label.clone(),
        exit_label: exit_label.clone(),
    });

    lowerer.finish_block(IrTerminator::Jump(cond_label.clone()));

    lowerer.start_block(cond_label.clone());
    let cond_op = lower_expr(&node.condition, lowerer)?;
    lowerer.finish_block(IrTerminator::Branch {
        cond: cond_op,
        true_label: body_label.clone(),
        false_label: exit_label.clone(),
    });

    lowerer.start_block(body_label);
    lower_statements(&node.body, lowerer)?;
    if !lowerer.terminated {
        lowerer.finish_block(IrTerminator::Jump(cond_label));
    }

    lowerer.break_stack.pop();
    lowerer.start_block(exit_label);
    Ok(())
}

fn lower_do_statement(node: &DoNode, lowerer: &mut Lowerer) -> ForgeResult<()> {
    if node.catches.is_empty() && node.finally_body.is_none() {
        return Err(ForgeError::codegen(
            "Do block must be followed by at least one Catch or Finally",
        ));
    }
    if let Some(catch) = node
        .catches
        .iter()
        .find(|catch| !catch.error_type.is_builtin())
    {
        return Err(ForgeError::codegen(format!(
            "invalid error type '{}'",
            catch.error_type.name()
        )));
    }
    let dispatch_label = lowerer.alloc_label("error_dispatch");
    let completion_label = lowerer.alloc_label("do_complete");
    let finally_label = node
        .finally_body
        .as_ref()
        .map(|_| lowerer.alloc_label("finally"));
    let finally_success_label = node
        .finally_body
        .as_ref()
        .map(|_| lowerer.alloc_label("finally_success"));
    let finally_error_label = node
        .finally_body
        .as_ref()
        .map(|_| lowerer.alloc_label("finally_error"));
    let finally_return_label = node
        .finally_body
        .as_ref()
        .map(|_| lowerer.alloc_label("finally_return"));
    let control_storage = format!("__sydrogen_finally_control#{}", lowerer.storage_counter);
    lowerer.storage_counter += 1;
    let outer_error_target = lowerer.error_target();
    let outer_return_target = lowerer.finally_return_targets.last().cloned();
    let propagation_target = finally_error_label
        .clone()
        .unwrap_or_else(|| outer_error_target.clone());

    if let Some(return_label) = &finally_return_label {
        lowerer.finally_return_targets.push(return_label.clone());
    }

    lowerer.error_targets.push(dispatch_label.clone());
    lower_statements(&node.body, lowerer)?;
    lowerer.error_targets.pop();
    if !lowerer.terminated {
        lowerer.finish_block(IrTerminator::Jump(
            finally_success_label
                .clone()
                .unwrap_or_else(|| completion_label.clone()),
        ));
    }

    lowerer.start_block(dispatch_label);
    let error_code = lowerer.alloc_vreg();
    let error_message = lowerer.alloc_vreg();
    lowerer.emit(IrInst::GetErrorCode { dest: error_code });
    lowerer.emit(IrInst::GetErrorMessage {
        dest: error_message,
    });

    let handler_labels: Vec<String> = (0..node.catches.len())
        .map(|index| lowerer.alloc_label(&format!("catch_{}", index)))
        .collect();
    let specific_count = node
        .catches
        .iter()
        .take_while(|catch| catch.error_type != ErrorType::Error)
        .count();
    let mut next_test = lowerer.current_label.clone();
    for index in 0..specific_count {
        if index > 0 {
            lowerer.start_block(next_test.clone());
        }
        let is_match = lowerer.alloc_vreg();
        lowerer.emit(IrInst::BinOp {
            dest: is_match,
            op: IrBinOp::Eq,
            lhs: Operand::Reg(error_code),
            rhs: Operand::Const(node.catches[index].error_type.code()),
        });
        next_test = lowerer.alloc_label("catch_next");
        let false_label = if index + 1 < specific_count {
            next_test.clone()
        } else if node.catches.len() > specific_count {
            handler_labels[specific_count].clone()
        } else {
            propagation_target.clone()
        };
        lowerer.finish_block(IrTerminator::Branch {
            cond: Operand::Reg(is_match),
            true_label: handler_labels[index].clone(),
            false_label,
        });
    }
    if specific_count == 0 {
        let target = handler_labels
            .first()
            .cloned()
            .unwrap_or_else(|| propagation_target.clone());
        lowerer.finish_block(IrTerminator::Jump(target));
    }

    for (index, catch) in node.catches.iter().enumerate() {
        lowerer.start_block(handler_labels[index].clone());
        let old_type = lowerer
            .var_types
            .insert(catch.binding.clone(), TypeDecl::Weld);
        let old_storage = lowerer.var_storage.get(&catch.binding).cloned();
        let storage = format!("{}#catch{}", catch.binding, lowerer.storage_counter);
        lowerer.storage_counter += 1;
        lowerer
            .var_storage
            .insert(catch.binding.clone(), storage.clone());
        lowerer.emit(IrInst::StoreVar {
            var_name: storage,
            src: Operand::Reg(error_message),
        });
        lowerer.emit(IrInst::ClearError);
        lowerer.error_targets.push(propagation_target.clone());
        lower_statements(&catch.body, lowerer)?;
        lowerer.error_targets.pop();
        if !lowerer.terminated {
            lowerer.finish_block(IrTerminator::Jump(
                finally_success_label
                    .clone()
                    .unwrap_or_else(|| completion_label.clone()),
            ));
        }
        match old_type {
            Some(value) => {
                lowerer.var_types.insert(catch.binding.clone(), value);
            }
            None => {
                lowerer.var_types.remove(&catch.binding);
            }
        }
        match old_storage {
            Some(value) => {
                lowerer.var_storage.insert(catch.binding.clone(), value);
            }
            None => {
                lowerer.var_storage.remove(&catch.binding);
            }
        }
    }

    if finally_return_label.is_some() {
        lowerer.finally_return_targets.pop();
    }

    if let Some(success_label) = &finally_success_label {
        lowerer.start_block(success_label.clone());
        lowerer.emit(IrInst::StoreVar {
            var_name: control_storage.clone(),
            src: Operand::Const(0),
        });
        lowerer.emit(IrInst::ClearError);
        lowerer.finish_block(IrTerminator::Jump(finally_label.clone().unwrap()));
    }

    if let Some(error_label) = &finally_error_label {
        lowerer.start_block(error_label.clone());
        lowerer.emit(IrInst::StoreVar {
            var_name: control_storage.clone(),
            src: Operand::Const(0),
        });
        lowerer.finish_block(IrTerminator::Jump(finally_label.clone().unwrap()));
    }

    if let Some(return_label) = &finally_return_label {
        lowerer.start_block(return_label.clone());
        lowerer.emit(IrInst::StoreVar {
            var_name: control_storage.clone(),
            src: Operand::Const(1),
        });
        lowerer.emit(IrInst::ClearError);
        lowerer.finish_block(IrTerminator::Jump(finally_label.clone().unwrap()));
    }

    if let (Some(finally_label), Some(finally_body)) = (&finally_label, &node.finally_body) {
        lowerer.start_block(finally_label.clone());
        let pending_code = lowerer.alloc_vreg();
        let pending_message = lowerer.alloc_vreg();
        let pending_type_name = lowerer.alloc_vreg();
        lowerer.emit(IrInst::GetErrorCode { dest: pending_code });
        lowerer.emit(IrInst::GetErrorMessage {
            dest: pending_message,
        });
        lowerer.emit(IrInst::GetErrorTypeName {
            dest: pending_type_name,
        });
        lowerer.emit(IrInst::ClearError);
        lowerer.error_targets.push(outer_error_target.clone());
        lower_statements(finally_body, lowerer)?;
        lowerer.error_targets.pop();
        if !lowerer.terminated {
            let restore_label = lowerer.alloc_label("finally_rethrow");
            let control_label = lowerer.alloc_label("finally_control");
            lowerer.finish_block(IrTerminator::Branch {
                cond: Operand::Reg(pending_code),
                true_label: restore_label.clone(),
                false_label: control_label.clone(),
            });
            lowerer.start_block(restore_label);
            lowerer.emit(IrInst::SetError {
                code: Operand::Reg(pending_code),
                message: Operand::Reg(pending_message),
                type_name: Operand::Reg(pending_type_name),
            });
            lowerer.finish_block(IrTerminator::Jump(outer_error_target));

            lowerer.start_block(control_label);
            let control = lowerer.alloc_vreg();
            lowerer.emit(IrInst::LoadVar {
                dest: control,
                var_name: control_storage,
            });
            let return_continuation = lowerer.alloc_label("finally_return_continue");
            lowerer.finish_block(IrTerminator::Branch {
                cond: Operand::Reg(control),
                true_label: return_continuation.clone(),
                false_label: completion_label.clone(),
            });
            lowerer.start_block(return_continuation);
            if let Some(outer_target) = outer_return_target {
                lowerer.finish_block(IrTerminator::Jump(outer_target));
            } else {
                let value = lowerer.alloc_vreg();
                lowerer.emit(IrInst::LoadVar {
                    dest: value,
                    var_name: "__sydrogen_return_value".to_string(),
                });
                let result = if lowerer.current_ret_kind.is_void() {
                    None
                } else {
                    Some(Operand::Reg(value))
                };
                lowerer.finish_block(IrTerminator::Ret(result));
            }
        }
    }

    lowerer.start_block(completion_label);
    Ok(())
}

fn lower_for_stmt(node: &ForNode, lowerer: &mut Lowerer) -> ForgeResult<()> {
    let old_type = lowerer.var_types.get(&node.init.name).cloned();
    let old_storage = lowerer.var_storage.get(&node.init.name).cloned();
    if old_storage.is_some() {
        let storage_name = format!("{}#{}", node.init.name, lowerer.storage_counter);
        lowerer.storage_counter += 1;
        lowerer
            .var_types
            .insert(node.init.name.clone(), node.init.type_decl.clone());
        lowerer
            .var_storage
            .insert(node.init.name.clone(), storage_name.clone());
        lowerer.emit(IrInst::StoreVar {
            var_name: storage_name.clone(),
            src: Operand::Const(0),
        });
        let src = if let Some(init) = &node.init.initializer {
            lower_expr(init, lowerer)?
        } else {
            Operand::Const(0)
        };
        lowerer.emit(IrInst::StoreVar {
            var_name: storage_name,
            src,
        });
    } else {
        lower_statement(&Statement::VarDecl(node.init.clone()), lowerer)?;
    }

    let cond_label = lowerer.alloc_label("for_cond");
    let body_label = lowerer.alloc_label("for_body");
    let incr_label = lowerer.alloc_label("for_incr");
    let exit_label = lowerer.alloc_label("for_exit");

    lowerer.break_stack.push(LoopCtx {
        continue_label: incr_label.clone(),
        exit_label: exit_label.clone(),
    });

    lowerer.finish_block(IrTerminator::Jump(cond_label.clone()));

    lowerer.start_block(cond_label.clone());
    let cond_op = lower_expr(&node.condition, lowerer)?;
    lowerer.finish_block(IrTerminator::Branch {
        cond: cond_op,
        true_label: body_label.clone(),
        false_label: exit_label.clone(),
    });

    lowerer.start_block(body_label);
    lower_statements(&node.body, lowerer)?;
    if !lowerer.terminated {
        lowerer.finish_block(IrTerminator::Jump(incr_label.clone()));
    }

    lowerer.start_block(incr_label);
    let increment_storage = lowerer.storage_name(&node.increment_var);
    let var_vreg = lowerer.alloc_vreg();
    lowerer.emit(IrInst::LoadVar {
        dest: var_vreg,
        var_name: increment_storage.clone(),
    });

    let new_var_vreg = lowerer.alloc_vreg();
    let op = match node.increment_op {
        IncrOp::Inc => IrBinOp::Add,
        IncrOp::Dec => IrBinOp::Sub,
    };
    lowerer.emit(IrInst::BinOp {
        dest: new_var_vreg,
        op,
        lhs: Operand::Reg(var_vreg),
        rhs: Operand::Const(1),
    });
    lowerer.emit(IrInst::StoreVar {
        var_name: increment_storage,
        src: Operand::Reg(new_var_vreg),
    });
    lowerer.finish_block(IrTerminator::Jump(cond_label));

    lowerer.break_stack.pop();
    match old_type {
        Some(type_decl) => {
            lowerer.var_types.insert(node.init.name.clone(), type_decl);
        }
        None => {
            lowerer.var_types.remove(&node.init.name);
        }
    }
    match old_storage {
        Some(storage_name) => {
            lowerer
                .var_storage
                .insert(node.init.name.clone(), storage_name);
        }
        None => {
            lowerer.var_storage.remove(&node.init.name);
        }
    }
    lowerer.start_block(exit_label);
    Ok(())
}

fn lower_expr(expr: &Expr, lowerer: &mut Lowerer) -> ForgeResult<Operand> {
    match expr {
        Expr::Number(num) => {
            if num.is_float {
                return Err(ForgeError::codegen(
                    "Float numbers are not supported in native backend",
                ));
            }
            Ok(Operand::Const(num.int_val))
        }
        Expr::Input(input) => {
            let kind = match input.subtype {
                Some(Subtype::Int) | None => InputKind::Int,
                Some(Subtype::Float) => InputKind::Float,
                Some(Subtype::Weld) | Some(Subtype::Generic) => InputKind::Str,
            };
            let dest = lowerer.alloc_vreg();
            lowerer.emit(IrInst::Input { dest, kind });
            Ok(Operand::Reg(dest))
        }
        Expr::Bool(val) => Ok(Operand::Const(if *val { 1 } else { 0 })),
        Expr::Str(parts) => {
            let mut text = String::new();
            for part in parts {
                if let crate::ast::StringPart::Literal(l) = part {
                    text.push_str(l);
                }
            }
            let dest = lowerer.alloc_vreg();
            lowerer.emit(IrInst::ConstStr {
                dest,
                value: format!("{}\0", text),
            });
            Ok(Operand::Reg(dest))
        }
        Expr::Identifier(name) => {
            if !lowerer.var_types.contains_key(name) {
                return Err(ForgeError::codegen(format!("Undefined variable: {}", name)));
            }
            let dest = lowerer.alloc_vreg();
            lowerer.emit(IrInst::LoadVar {
                dest,
                var_name: lowerer.storage_name(name),
            });
            Ok(Operand::Reg(dest))
        }
        Expr::BinaryOp { op, lhs, rhs } => {
            let lhs_op = lower_expr(lhs, lowerer)?;
            let rhs_op = lower_expr(rhs, lowerer)?;
            let ir_op = match op {
                BinOp::Add => IrBinOp::Add,
                BinOp::Sub => IrBinOp::Sub,
                BinOp::Mul => IrBinOp::Mul,
                BinOp::Div => IrBinOp::Div,
                BinOp::Rem => IrBinOp::Rem,
                BinOp::Pow => IrBinOp::Pow,
                BinOp::And => IrBinOp::And,
                BinOp::Or => IrBinOp::Or,
                BinOp::Xor => IrBinOp::Xor,
                BinOp::Eq => IrBinOp::Eq,
                BinOp::Ne => IrBinOp::Ne,
                BinOp::Lt => IrBinOp::Lt,
                BinOp::Gt => IrBinOp::Gt,
                BinOp::Le => IrBinOp::Le,
                BinOp::Ge => IrBinOp::Ge,
            };
            if matches!(ir_op, IrBinOp::Div | IrBinOp::Rem) {
                let division_label = lowerer.alloc_label("division_ok");
                let error_label = lowerer.alloc_label("division_by_zero");
                lowerer.finish_block(IrTerminator::Branch {
                    cond: rhs_op.clone(),
                    true_label: division_label.clone(),
                    false_label: error_label.clone(),
                });
                lowerer.start_block(error_label);
                let message = lower_expr(
                    &Expr::Str(vec![crate::ast::StringPart::Literal(
                        "division by zero".to_string(),
                    )]),
                    lowerer,
                )?;
                let type_name = error_type_operand(&ErrorType::DivisionByZero, lowerer);
                lowerer.emit(IrInst::SetError {
                    code: Operand::Const(ErrorType::DivisionByZero.code()),
                    message,
                    type_name,
                });
                lowerer.finish_block(IrTerminator::Jump(lowerer.error_target()));
                lowerer.start_block(division_label);
            }
            let dest = lowerer.alloc_vreg();
            lowerer.emit(IrInst::BinOp {
                dest,
                op: ir_op,
                lhs: lhs_op,
                rhs: rhs_op,
            });
            Ok(Operand::Reg(dest))
        }
        Expr::UnaryOp { op, operand } => {
            let inner_op = lower_expr(operand, lowerer)?;
            match op {
                UnOp::Plus => Ok(inner_op),
                UnOp::Neg => {
                    let dest = lowerer.alloc_vreg();
                    lowerer.emit(IrInst::BinOp {
                        dest,
                        op: IrBinOp::Sub,
                        lhs: Operand::Const(0),
                        rhs: inner_op,
                    });
                    Ok(Operand::Reg(dest))
                }
            }
        }
        Expr::Call { callee, args } => {
            if args.len() > 6 {
                return Err(ForgeError::codegen(format!(
                    "Function call '{}' has {} arguments. Native backend supports at most 6 integer arguments.",
                    callee,
                    args.len()
                )));
            }
            if !lowerer.known_functions.contains(callee) {
                return Err(ForgeError::codegen(format!(
                    "Call to undefined function '{}'",
                    callee
                )));
            }
            let mut arg_ops = Vec::new();
            for arg in args {
                arg_ops.push(lower_expr(arg, lowerer)?);
            }
            let dest = lowerer.alloc_vreg();
            lowerer.emit(IrInst::Call {
                dest,
                func: callee.clone(),
                args: arg_ops,
            });
            lowerer.branch_on_error();
            Ok(Operand::Reg(dest))
        }
        _ => Err(ForgeError::codegen(format!(
            "Expression {:?} is not supported in native backend",
            expr
        ))),
    }
}

fn error_type_operand(error_type: &ErrorType, lowerer: &mut Lowerer) -> Operand {
    let dest = lowerer.alloc_vreg();
    lowerer.emit(IrInst::ConstStr {
        dest,
        value: format!("{}\0", error_type.name()),
    });
    Operand::Reg(dest)
}

fn is_bool_expr(expr: &Expr, lowerer: &Lowerer) -> bool {
    match expr {
        Expr::Bool(_) => true,
        Expr::Identifier(name) => matches!(lowerer.var_types.get(name), Some(TypeDecl::Bool)),
        Expr::Call { callee, .. } => {
            matches!(lowerer.function_ret_kinds.get(callee), Some(RetKind::Bool))
        }
        Expr::BinaryOp { op, .. } => matches!(
            op,
            BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or
                | BinOp::Xor
        ),
        _ => false,
    }
}

fn is_string_expr(expr: &Expr, lowerer: &Lowerer) -> bool {
    match expr {
        Expr::Str(_) => true,
        Expr::Identifier(name) => matches!(lowerer.var_types.get(name), Some(TypeDecl::Weld)),
        Expr::Call { callee, .. } => {
            matches!(lowerer.function_ret_kinds.get(callee), Some(RetKind::Weld))
        }
        _ => false,
    }
}
