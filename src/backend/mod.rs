pub mod elf;
pub mod pe;
pub mod x86_64;

use crate::ast::Program;
use crate::ast::{Expr, RetKind, Statement, Subtype, TypeDecl};
use crate::errors::{ForgeError, ForgeResult};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FALLBACK_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Linux,
    Windows,
}

impl Target {
    pub fn executable_extension(self) -> Option<&'static str> {
        match self {
            Self::Linux => None,
            Self::Windows => Some("exe"),
        }
    }

    fn user_code_vaddr(self) -> u64 {
        match self {
            Self::Linux => 0x400000 + 128 + elf::STARTUP_STUB_SIZE as u64,
            Self::Windows => pe::IMAGE_BASE + pe::SECTION_RVA as u64 + pe::STARTUP_STUB_SIZE as u64,
        }
    }
}

pub fn compile_native(program: &Program, target: Target) -> ForgeResult<Vec<u8>> {
    let needs_typed_backend = requires_typed_backend(program);
    if needs_typed_backend && program_uses_error_handling(program) {
        return Err(ForgeError::codegen(
            "native typed error handling cannot be combined with floats, collections, Data, objects, or typed input yet",
        ));
    }
    if needs_typed_backend {
        return match target {
            Target::Linux => compile_typed_elf(program),
            Target::Windows => Err(ForgeError::codegen(
                "Windows native compilation does not yet support floats, collections, Data, objects, or typed input",
            )),
        };
    }
    if target == Target::Windows {
        validate_windows_program(program)?;
    }
    let ir = crate::lowering::lower(program)?;
    let backend = x86_64::NativeX86Backend::new(target.user_code_vaddr());
    let (user_code, func_offsets) = backend.compile_program(&ir)?;
    match target {
        Target::Linux => elf::generate_elf64_executable(&user_code, &func_offsets),
        Target::Windows => pe::generate_pe32_plus_executable(&user_code, &func_offsets),
    }
}

pub fn program_uses_error_handling(program: &Program) -> bool {
    fn statements_use_errors(statements: &[Statement]) -> bool {
        statements.iter().any(|statement| match statement {
            Statement::Do(_) | Statement::Throw(_) => true,
            Statement::FunctionDecl(function) => statements_use_errors(&function.body),
            Statement::If(node) => {
                node.branches
                    .iter()
                    .any(|(_, body)| statements_use_errors(body))
                    || node
                        .else_body
                        .as_ref()
                        .is_some_and(|body| statements_use_errors(body))
            }
            Statement::While(node) => statements_use_errors(&node.body),
            Statement::For(node) => statements_use_errors(&node.body),
            Statement::ForEach(node) => statements_use_errors(&node.body),
            Statement::Switch(node) => node.branches.iter().any(|branch| match branch {
                crate::ast::SwitchBranch::Case(case) => statements_use_errors(&case.body),
                crate::ast::SwitchBranch::Base(body) => statements_use_errors(body),
            }),
            _ => false,
        })
    }
    statements_use_errors(&program.statements)
}

pub fn compile_to_elf(program: &Program) -> ForgeResult<Vec<u8>> {
    compile_native(program, Target::Linux)
}

pub fn compile_to_pe(program: &Program) -> ForgeResult<Vec<u8>> {
    compile_native(program, Target::Windows)
}

fn validate_windows_program(program: &Program) -> ForgeResult<()> {
    if program.statements.iter().any(windows_unsupported_statement) {
        return Err(ForgeError::codegen(
            "Windows native compilation does not yet support Print, Input, or Program.Stop() because those operations currently use the Linux syscall runtime",
        ));
    }
    Ok(())
}

fn windows_unsupported_statement(statement: &Statement) -> bool {
    match statement {
        Statement::Print(_) | Statement::Input(_) | Statement::Stop => true,
        Statement::VarDecl(decl) => decl
            .initializer
            .as_ref()
            .is_some_and(windows_unsupported_expr),
        Statement::FunctionDecl(function) => {
            function.body.iter().any(windows_unsupported_statement)
        }
        Statement::If(node) => {
            node.branches.iter().any(|(condition, body)| {
                windows_unsupported_expr(condition)
                    || body.iter().any(windows_unsupported_statement)
            }) || node
                .else_body
                .as_ref()
                .is_some_and(|body| body.iter().any(windows_unsupported_statement))
        }
        Statement::While(node) => {
            windows_unsupported_expr(&node.condition)
                || node.body.iter().any(windows_unsupported_statement)
        }
        Statement::For(node) => {
            node.init
                .initializer
                .as_ref()
                .is_some_and(windows_unsupported_expr)
                || windows_unsupported_expr(&node.condition)
                || node.body.iter().any(windows_unsupported_statement)
        }
        Statement::ForEach(node) => node.body.iter().any(windows_unsupported_statement),
        Statement::Switch(node) => {
            windows_unsupported_expr(&node.expression)
                || node.branches.iter().any(|branch| match branch {
                    crate::ast::SwitchBranch::Case(case) => {
                        case.values.iter().any(windows_unsupported_expr)
                            || case.body.iter().any(windows_unsupported_statement)
                    }
                    crate::ast::SwitchBranch::Base(body) => {
                        body.iter().any(windows_unsupported_statement)
                    }
                })
        }
        Statement::Do(node) => {
            node.body.iter().any(windows_unsupported_statement)
                || node
                    .catches
                    .iter()
                    .any(|catch| catch.body.iter().any(windows_unsupported_statement))
                || node
                    .finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(windows_unsupported_statement))
        }
        Statement::Throw(error) => windows_unsupported_expr(&error.message),
        Statement::Return(value) => value.as_ref().is_some_and(windows_unsupported_expr),
        Statement::Assignment(assignment) => windows_unsupported_expr(&assignment.value),
        Statement::ExprStmt(expr) => windows_unsupported_expr(expr),
        Statement::DataDecl(_)
        | Statement::ObjectDecl(_)
        | Statement::Skip
        | Statement::Use(_)
        | Statement::Using(_) => false,
    }
}

fn windows_unsupported_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Input(_) => true,
        Expr::MemberAccess { object, .. } => windows_unsupported_expr(object),
        Expr::IndexAccess { object, index } => {
            windows_unsupported_expr(object) || windows_unsupported_expr(index)
        }
        Expr::BinaryOp { lhs, rhs, .. } => {
            windows_unsupported_expr(lhs) || windows_unsupported_expr(rhs)
        }
        Expr::UnaryOp { operand, .. } => windows_unsupported_expr(operand),
        Expr::Call { args, .. } | Expr::NamespaceCall { args, .. } => {
            args.iter().any(windows_unsupported_expr)
        }
        Expr::MethodCall { object, args, .. } => {
            windows_unsupported_expr(object) || args.iter().any(windows_unsupported_expr)
        }
        Expr::ArrayLiteral(values) | Expr::TupleLiteral(values) | Expr::ListLiteral(values) => {
            values.iter().any(windows_unsupported_expr)
        }
        Expr::Number(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Identifier(_) => false,
    }
}

fn compile_typed_elf(program: &Program) -> ForgeResult<Vec<u8>> {
    let id = FALLBACK_ID.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "furnace_native_typed_{}_{}",
        std::process::id(),
        id
    ));
    std::fs::create_dir_all(&directory).map_err(|error| {
        ForgeError::codegen(format!("create native build directory: {}", error))
    })?;
    let object = directory.join("program.o");
    let executable = directory.join("program");
    let runtime_source = directory.join("runtime.c");

    let result = (|| {
        crate::codegen::compile(program, &object, true)?;
        std::fs::write(&runtime_source, crate::codegen::RUNTIME_SUPPORT_C)
            .map_err(|error| ForgeError::codegen(format!("write runtime support: {}", error)))?;
        let output = Command::new("cc")
            .arg(&object)
            .arg(&runtime_source)
            .arg("-o")
            .arg(&executable)
            .arg("-lm")
            .output()
            .map_err(|error| ForgeError::codegen(format!("invoke native linker: {}", error)))?;
        if !output.status.success() {
            return Err(ForgeError::codegen(format!(
                "native linker failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        std::fs::read(&executable)
            .map_err(|error| ForgeError::codegen(format!("read native executable: {}", error)))
    })();

    let _ = std::fs::remove_file(&object);
    let _ = std::fs::remove_file(&runtime_source);
    let _ = std::fs::remove_file(&executable);
    let _ = std::fs::remove_dir(&directory);
    result
}

fn requires_typed_backend(program: &Program) -> bool {
    program
        .statements
        .iter()
        .any(statement_requires_typed_backend)
}

fn type_requires_typed_backend(ty: &TypeDecl) -> bool {
    matches!(
        ty,
        TypeDecl::Number(Subtype::Float)
            | TypeDecl::Ore(_)
            | TypeDecl::OreTuple(_)
            | TypeDecl::Materials(_, _)
    )
}

fn return_requires_typed_backend(ret: &RetKind) -> bool {
    matches!(
        ret,
        RetKind::Float | RetKind::Ore(_) | RetKind::OreTuple(_) | RetKind::Materials(_, _)
    )
}

fn statement_requires_typed_backend(statement: &Statement) -> bool {
    match statement {
        Statement::DataDecl(_) | Statement::ObjectDecl(_) => true,
        Statement::VarDecl(decl) => {
            type_requires_typed_backend(&decl.type_decl)
                || decl
                    .initializer
                    .as_ref()
                    .is_some_and(expr_requires_typed_backend)
        }
        Statement::FunctionDecl(function) => {
            return_requires_typed_backend(&function.ret_kind)
                || function
                    .params
                    .iter()
                    .any(|param| type_requires_typed_backend(&param.type_decl))
                || function.body.iter().any(statement_requires_typed_backend)
        }
        Statement::Print(print) => expr_requires_typed_backend(&print.expr),
        Statement::Input(_) => true,
        Statement::If(node) => {
            node.branches.iter().any(|(condition, body)| {
                expr_requires_typed_backend(condition)
                    || body.iter().any(statement_requires_typed_backend)
            }) || node
                .else_body
                .as_ref()
                .is_some_and(|body| body.iter().any(statement_requires_typed_backend))
        }
        Statement::While(node) => {
            expr_requires_typed_backend(&node.condition)
                || node.body.iter().any(statement_requires_typed_backend)
        }
        Statement::For(node) => {
            type_requires_typed_backend(&node.init.type_decl)
                || node
                    .init
                    .initializer
                    .as_ref()
                    .is_some_and(expr_requires_typed_backend)
                || expr_requires_typed_backend(&node.condition)
                || node.body.iter().any(statement_requires_typed_backend)
        }
        // Collection storage and element types are implemented by the typed
        // backend. The native CLI transparently routes this statement there.
        Statement::ForEach(_) => true,
        Statement::Switch(node) => {
            expr_requires_typed_backend(&node.expression)
                || node.branches.iter().any(|branch| match branch {
                    crate::ast::SwitchBranch::Case(case) => {
                        case.values.iter().any(expr_requires_typed_backend)
                            || case.body.iter().any(statement_requires_typed_backend)
                    }
                    crate::ast::SwitchBranch::Base(body) => {
                        body.iter().any(statement_requires_typed_backend)
                    }
                })
        }
        Statement::Do(node) => {
            node.body.iter().any(statement_requires_typed_backend)
                || node
                    .catches
                    .iter()
                    .any(|catch| catch.body.iter().any(statement_requires_typed_backend))
                || node
                    .finally_body
                    .as_ref()
                    .is_some_and(|body| body.iter().any(statement_requires_typed_backend))
        }
        Statement::Throw(error) => expr_requires_typed_backend(&error.message),
        Statement::Return(value) => value.as_ref().is_some_and(expr_requires_typed_backend),
        Statement::Assignment(assignment) => expr_requires_typed_backend(&assignment.value),
        Statement::ExprStmt(expr) => expr_requires_typed_backend(expr),
        Statement::Stop | Statement::Skip | Statement::Use(_) | Statement::Using(_) => false,
    }
}

fn expr_requires_typed_backend(expr: &Expr) -> bool {
    match expr {
        Expr::Number(number) => number.is_float,
        Expr::Input(_) => true,
        Expr::ArrayLiteral(_) | Expr::TupleLiteral(_) | Expr::ListLiteral(_) => true,
        Expr::MemberAccess { object, .. } => expr_requires_typed_backend(object),
        Expr::IndexAccess { object, index } => {
            expr_requires_typed_backend(object) || expr_requires_typed_backend(index)
        }
        Expr::BinaryOp { lhs, rhs, .. } => {
            expr_requires_typed_backend(lhs) || expr_requires_typed_backend(rhs)
        }
        Expr::UnaryOp { operand, .. } => expr_requires_typed_backend(operand),
        Expr::Call { args, .. } | Expr::NamespaceCall { args, .. } => {
            args.iter().any(expr_requires_typed_backend)
        }
        Expr::MethodCall { object, args, .. } => {
            expr_requires_typed_backend(object) || args.iter().any(expr_requires_typed_backend)
        }
        Expr::Str(_) | Expr::Bool(_) | Expr::Identifier(_) => false,
    }
}
