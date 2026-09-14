use rayon::prelude::*;
use std::collections::HashMap;

use crate::ast::{
    AssignmentTarget, BinOp, Expr, ForEachNode, ForNode, FunctionDecl, Param, Program, RetKind,
    Statement, Subtype, TypeDecl,
};
use crate::errors::{ForgeError, ForgeResult};

#[derive(Debug, Clone, Copy)]
struct ValidationContext<'a> {
    current_function: Option<&'a str>,
    in_loop_or_if: bool,
    in_loop: bool,
}

#[derive(Clone)]
struct Scope {
    var_types: HashMap<String, TypeDecl>,
    object_types: HashMap<String, String>,
    catch_bindings: std::collections::HashSet<String>,
}

impl Scope {
    fn new() -> Self {
        Self {
            var_types: HashMap::new(),
            object_types: HashMap::new(),
            catch_bindings: std::collections::HashSet::new(),
        }
    }

    fn from_params(params: &[Param]) -> Self {
        let var_types = params
            .iter()
            .map(|p| (p.name.clone(), p.type_decl.clone()))
            .collect();
        Self {
            var_types,
            object_types: HashMap::new(),
            catch_bindings: std::collections::HashSet::new(),
        }
    }

    fn get(&self, name: &str) -> Option<&TypeDecl> {
        self.var_types.get(name)
    }

    fn contains_key(&self, name: &str) -> bool {
        self.var_types.contains_key(name)
    }
}

fn collect_var_types(statements: &[Statement], scope: &mut Scope) {
    for stmt in statements {
        match stmt {
            Statement::VarDecl(decl) => {
                scope
                    .var_types
                    .insert(decl.name.clone(), decl.type_decl.clone());
            }
            Statement::ObjectDecl(object) => {
                scope
                    .object_types
                    .insert(object.name.clone(), object.type_name.clone());
            }
            Statement::If(node) => {
                for (_, body) in &node.branches {
                    collect_var_types(body, scope);
                }
                if let Some(else_body) = &node.else_body {
                    collect_var_types(else_body, scope);
                }
            }
            Statement::While(node) => {
                collect_var_types(&node.body, scope);
            }
            Statement::For(node) => {
                collect_var_types(&node.body, scope);
            }
            Statement::ForEach(node) => {
                collect_var_types(&node.body, scope);
            }
            Statement::Do(node) => {
                collect_var_types(&node.body, scope);
                for catch in &node.catches {
                    scope.catch_bindings.insert(catch.binding.clone());
                    collect_var_types(&catch.body, scope);
                }
                if let Some(body) = &node.finally_body {
                    collect_var_types(body, scope);
                }
            }
            Statement::FunctionDecl(func) => {
                collect_var_types(&func.body, scope);
            }
            _ => {}
        }
    }
}

fn expr_type_name(expr: &Expr) -> &'static str {
    match expr {
        Expr::Bool(_) => "Bool",
        Expr::Number(n) if n.is_float => "Float",
        Expr::Number(_) => "Int",
        Expr::Str(_) => "Weld",
        Expr::ArrayLiteral(_) => "Array",
        Expr::TupleLiteral(_) => "Tuple",
        Expr::ListLiteral(_) => "List",
        Expr::Input(_) => "Input",
        _ => "value",
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ExprType {
    Number(Subtype),
    Weld,
    Bool,
    Array {
        element_type: Subtype,
    },
    Tuple {
        fields: Vec<(Subtype, String)>,
    },
    /// `None` represents an empty list literal whose element type must be
    /// supplied by its surrounding declaration, assignment, parameter, or
    /// return type. It is deliberately distinct from `Generic`: a declared
    /// generic list is not implicitly usable as every concrete list type.
    List {
        element_type: Option<Subtype>,
    },
    DataObject {
        type_name: String,
    },
    Unknown,
}

impl ExprType {
    fn from_subtype(subtype: &Subtype) -> Self {
        match subtype {
            Subtype::Int | Subtype::Float | Subtype::Generic => Self::Number(subtype.clone()),
            Subtype::Weld => Self::Weld,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Number(Subtype::Int) => "Int",
            Self::Number(Subtype::Float) => "Float",
            Self::Number(Subtype::Generic) => "Generic",
            Self::Number(Subtype::Weld) | Self::Weld => "Weld",
            Self::Bool => "Bool",
            Self::Array { .. } => "Array",
            Self::Tuple { .. } => "Tuple",
            Self::List { .. } => "List",
            Self::DataObject { .. } => "Data",
            Self::Unknown => "value",
        }
    }
}

fn type_decl_to_expr_type(type_decl: &TypeDecl) -> ExprType {
    match type_decl {
        TypeDecl::Number(subtype) => ExprType::Number(subtype.clone()),
        TypeDecl::Weld => ExprType::Weld,
        TypeDecl::Bool => ExprType::Bool,
        // Ore currently has no element subtype in its declaration syntax. Its
        // existing backend layout is integer elements, so retain that contract
        // until the language gains typed-array syntax.
        TypeDecl::Ore(_) => ExprType::Array {
            element_type: Subtype::Int,
        },
        TypeDecl::OreTuple(fields) => ExprType::Tuple {
            fields: fields.clone(),
        },
        TypeDecl::Materials(element_type, _) => ExprType::List {
            element_type: Some(element_type.clone()),
        },
    }
}

fn ret_kind_to_expr_type(ret_kind: &RetKind) -> ExprType {
    match ret_kind {
        RetKind::Int => ExprType::Number(Subtype::Int),
        RetKind::Float => ExprType::Number(Subtype::Float),
        RetKind::Weld => ExprType::Weld,
        RetKind::Bool => ExprType::Bool,
        RetKind::Ore(_) => ExprType::Array {
            element_type: Subtype::Int,
        },
        RetKind::OreTuple(fields) => ExprType::Tuple {
            fields: fields.clone(),
        },
        RetKind::Materials(element_type, _) => ExprType::List {
            element_type: Some(element_type.clone()),
        },
        RetKind::Generic | RetKind::Function | RetKind::Dynamic => ExprType::Unknown,
        RetKind::Nunction | RetKind::Void => ExprType::Unknown,
    }
}

fn expr_type(expr: &Expr, scope: &Scope, program: &Program) -> ExprType {
    match expr {
        Expr::Number(number) => ExprType::Number(if number.is_float {
            Subtype::Float
        } else {
            Subtype::Int
        }),
        Expr::Str(_) => ExprType::Weld,
        Expr::Bool(_) => ExprType::Bool,
        Expr::Identifier(name) => scope
            .get(name)
            .map(type_decl_to_expr_type)
            .or_else(|| {
                scope
                    .object_types
                    .get(name)
                    .cloned()
                    .or_else(|| {
                        program
                            .statements
                            .iter()
                            .find_map(|statement| match statement {
                                Statement::ObjectDecl(object) if object.name == *name => {
                                    Some(object.type_name.clone())
                                }
                                _ => None,
                            })
                    })
                    .filter(|type_name| data_decl(program, type_name).is_some())
                    .map(|type_name| ExprType::DataObject { type_name })
            })
            .unwrap_or(ExprType::Unknown),
        Expr::Input(input) => match input.subtype.as_ref() {
            Some(Subtype::Weld) | None => ExprType::Weld,
            Some(subtype) => ExprType::Number(subtype.clone()),
        },
        Expr::Call { callee, .. } => program
            .statements
            .iter()
            .find_map(|statement| match statement {
                Statement::FunctionDecl(function) if function.name == *callee => {
                    Some(ret_kind_to_expr_type(&function.ret_kind))
                }
                _ => None,
            })
            .unwrap_or(ExprType::Unknown),
        Expr::BinaryOp { op, lhs, rhs } => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
                match (
                    expr_type(lhs, scope, program),
                    expr_type(rhs, scope, program),
                ) {
                    (ExprType::Number(Subtype::Float), _)
                    | (_, ExprType::Number(Subtype::Float)) => ExprType::Number(Subtype::Float),
                    (ExprType::Number(Subtype::Int), ExprType::Number(Subtype::Int)) => {
                        ExprType::Number(Subtype::Int)
                    }
                    _ => ExprType::Unknown,
                }
            }
            BinOp::Rem => ExprType::Number(Subtype::Int),
            BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::Le
            | BinOp::Ge
            | BinOp::And
            | BinOp::Or
            | BinOp::Xor => ExprType::Bool,
        },
        Expr::UnaryOp { operand, .. } => expr_type(operand, scope, program),
        Expr::ArrayLiteral(_) => ExprType::Array {
            element_type: Subtype::Int,
        },
        Expr::TupleLiteral(elements) => {
            let fields = match elements
                .iter()
                .map(|element| subtype_from_expr_type(&expr_type(element, scope, program)))
                .collect::<Option<Vec<_>>>()
            {
                Some(fields) => fields,
                None => return ExprType::Unknown,
            };
            ExprType::Tuple { fields }
        }
        Expr::ListLiteral(elements) => {
            let mut element_types = elements.iter().map(|element| {
                subtype_from_expr_type(&expr_type(element, scope, program))
                    .map(|(subtype, _)| subtype)
            });
            let Some(first) = element_types.next() else {
                return ExprType::List { element_type: None };
            };
            let Some(element_type) = first else {
                return ExprType::Unknown;
            };
            if element_types.all(|ty| ty == Some(element_type.clone())) {
                ExprType::List {
                    element_type: Some(element_type),
                }
            } else {
                ExprType::Unknown
            }
        }
        Expr::IndexAccess { object, .. } => match expr_type(object, scope, program) {
            ExprType::Array { element_type } => ExprType::from_subtype(&element_type),
            ExprType::List {
                element_type: Some(element_type),
            } => ExprType::from_subtype(&element_type),
            _ => ExprType::Unknown,
        },
        Expr::MemberAccess { object, member } => match expr_type(object, scope, program) {
            ExprType::Array { .. } | ExprType::List { .. }
                if member == "Length" || member == "Len" =>
            {
                ExprType::Number(Subtype::Int)
            }
            ExprType::Tuple { fields } => fields
                .iter()
                .find(|(_, field_name)| field_name == member)
                .map(|(field_type, _)| ExprType::from_subtype(field_type))
                .unwrap_or(ExprType::Unknown),
            ExprType::DataObject { type_name } => data_member_type(program, &type_name, member)
                .map(type_decl_to_expr_type)
                .unwrap_or(ExprType::Unknown),
            _ => ExprType::Unknown,
        },
        Expr::MethodCall { object, method, .. } => match expr_type(object, scope, program) {
            ExprType::Array { .. } | ExprType::List { .. }
                if method == "Length" || method == "Len" =>
            {
                ExprType::Number(Subtype::Int)
            }
            ExprType::List { .. }
                if method == "Add" || method == "Remove" || method == "RemoveAt" =>
            {
                ExprType::Number(Subtype::Int)
            }
            _ => ExprType::Unknown,
        },
        Expr::NamespaceCall { .. } => ExprType::Unknown,
    }
}

fn subtype_from_expr_type(expr_type: &ExprType) -> Option<(Subtype, String)> {
    match expr_type {
        ExprType::Number(subtype) => Some((subtype.clone(), String::new())),
        ExprType::Weld => Some((Subtype::Weld, String::new())),
        _ => None,
    }
}

fn subtype_compatible(actual: &Subtype, expected: &Subtype) -> bool {
    actual == expected || (*expected == Subtype::Generic && *actual == Subtype::Int)
}

fn data_decl<'a>(program: &'a Program, type_name: &str) -> Option<&'a crate::ast::DataDecl> {
    program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::DataDecl(data) if data.name == type_name => Some(data),
            _ => None,
        })
}

fn data_member_type<'a>(
    program: &'a Program,
    type_name: &str,
    member: &str,
) -> Option<&'a TypeDecl> {
    data_decl(program, type_name)?
        .members
        .iter()
        .find(|field| field.name == member)
        .map(|field| &field.type_decl)
}

fn types_compatible(actual: &ExprType, expected: &TypeDecl) -> bool {
    match (actual, expected) {
        (ExprType::Number(actual), TypeDecl::Number(expected)) => {
            subtype_compatible(actual, expected)
        }
        (ExprType::Weld, TypeDecl::Weld)
        | (ExprType::Bool, TypeDecl::Bool)
        | (ExprType::Array { .. }, TypeDecl::Ore(_)) => true,
        (
            ExprType::Tuple {
                fields: actual_fields,
            },
            TypeDecl::OreTuple(expected_fields),
        ) => {
            actual_fields.len() == expected_fields.len()
                && actual_fields.iter().zip(expected_fields).all(
                    |((actual_type, actual_name), (expected_type, expected_name))| {
                        subtype_compatible(actual_type, expected_type)
                            && (actual_name.is_empty() || actual_name == expected_name)
                    },
                )
        }
        (
            ExprType::List {
                element_type: Some(element_type),
            },
            TypeDecl::Materials(expected_type, _),
        ) => subtype_compatible(element_type, expected_type),
        // An empty literal is contextually typed by the declared list type.
        (ExprType::List { element_type: None }, TypeDecl::Materials(_, _)) => true,
        _ => false,
    }
}

fn type_name(type_decl: &TypeDecl) -> &'static str {
    match type_decl {
        TypeDecl::Number(Subtype::Int) => "Int",
        TypeDecl::Number(Subtype::Float) => "Float",
        TypeDecl::Number(Subtype::Generic) => "Generic",
        TypeDecl::Number(Subtype::Weld) => "Weld",
        TypeDecl::Weld => "Weld",
        TypeDecl::Bool => "Bool",
        TypeDecl::Ore(_) => "Array",
        TypeDecl::OreTuple(_) => "Tuple",
        TypeDecl::Materials(_, _) => "List",
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum NumericType {
    Int,
    Float,
}

fn numeric_type(expr: &Expr, scope: &Scope, program: &Program) -> Option<NumericType> {
    match expr {
        Expr::Number(n) => Some(if n.is_float {
            NumericType::Float
        } else {
            NumericType::Int
        }),
        Expr::Identifier(name) => match scope.get(name) {
            Some(TypeDecl::Number(Subtype::Int)) => Some(NumericType::Int),
            Some(TypeDecl::Number(Subtype::Float)) => Some(NumericType::Float),
            _ => None,
        },
        Expr::Input(input) => match input.subtype {
            Some(Subtype::Int) => Some(NumericType::Int),
            Some(Subtype::Float) => Some(NumericType::Float),
            _ => None,
        },
        Expr::Call { callee, .. } => {
            let ret = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::FunctionDecl(f) if f.name == *callee => Some(&f.ret_kind),
                    _ => None,
                });
            match ret {
                Some(RetKind::Int) => Some(NumericType::Int),
                Some(RetKind::Float) => Some(NumericType::Float),
                _ => None,
            }
        }
        Expr::BinaryOp { op, lhs, rhs }
            if matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow
            ) =>
        {
            let lhs_type = numeric_type(lhs, scope, program)?;
            let rhs_type = numeric_type(rhs, scope, program)?;
            Some(
                if lhs_type == NumericType::Float || rhs_type == NumericType::Float {
                    NumericType::Float
                } else {
                    NumericType::Int
                },
            )
        }
        Expr::BinaryOp {
            op: BinOp::Rem,
            lhs,
            rhs,
        } => {
            if numeric_type(lhs, scope, program) == Some(NumericType::Int)
                && numeric_type(rhs, scope, program) == Some(NumericType::Int)
            {
                Some(NumericType::Int)
            } else {
                None
            }
        }
        Expr::UnaryOp { operand, .. } => numeric_type(operand, scope, program),
        Expr::IndexAccess { .. } | Expr::MemberAccess { .. } | Expr::MethodCall { .. } => {
            match expr_type(expr, scope, program) {
                ExprType::Number(Subtype::Int) => Some(NumericType::Int),
                ExprType::Number(Subtype::Float) => Some(NumericType::Float),
                _ => None,
            }
        }
        _ => None,
    }
}

pub fn analyze(program: &Program) -> ForgeResult<()> {
    // Validate loose top-level statements
    let top_level_ctx = ValidationContext {
        current_function: None,
        in_loop_or_if: false,
        in_loop: false,
    };
    let empty_scope = Scope::new();
    for statement in &program.statements {
        if !matches!(statement, Statement::FunctionDecl(_)) {
            validate_statement(statement, &empty_scope, program, top_level_ctx)?;
        }
    }

    let functions: Vec<&FunctionDecl> = program
        .statements
        .par_iter()
        .filter_map(|statement| {
            if let Statement::FunctionDecl(function) = statement {
                Some(function)
            } else {
                None
            }
        })
        .collect();

    functions
        .par_iter()
        .try_for_each(|function| validate_function(function, program))
}

fn validate_function(function: &FunctionDecl, program: &Program) -> ForgeResult<()> {
    if function.name == "Main" && !function.params.is_empty() {
        return Err(ForgeError::parse("Main cannot have parameters"));
    }

    let mut scope = Scope::from_params(&function.params);
    collect_var_types(&function.body, &mut scope);

    let context = ValidationContext {
        current_function: Some(function.name.as_str()),
        in_loop_or_if: false,
        in_loop: false,
    };
    validate_statements(&function.body, &scope, program, context)
}

fn validate_statements(
    statements: &[Statement],
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    for statement in statements {
        validate_statement(statement, scope, program, context)?;
    }
    Ok(())
}

fn validate_statement(
    statement: &Statement,
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    match statement {
        Statement::VarDecl(decl) => {
            validate_var_decl(decl, scope, program, context)?;
        }
        Statement::ObjectDecl(object) => {
            validate_object_decl(object, scope, program, context)?;
        }
        Statement::Assignment(assignment) => {
            validate_assignment_target(&assignment.target, scope, program, context)?;
            validate_expr(&assignment.value, scope, program, context)?;
            validate_assignment_type(&assignment, scope, program)?;
        }
        Statement::If(node) => {
            let branch_ctx = ValidationContext {
                in_loop_or_if: true,
                ..context
            };
            for (condition, body) in &node.branches {
                validate_expr(condition, scope, program, context)?;
                validate_statements(body, scope, program, branch_ctx)?;
            }
            if let Some(body) = &node.else_body {
                validate_statements(body, scope, program, branch_ctx)?;
            }
        }
        Statement::While(node) => {
            let loop_ctx = ValidationContext {
                in_loop_or_if: true,
                in_loop: true,
                ..context
            };
            validate_expr(&node.condition, scope, program, context)?;
            validate_statements(&node.body, scope, program, loop_ctx)?;
        }
        Statement::For(node) => {
            validate_for_stmt(node, scope, program, context)?;
        }
        Statement::ForEach(node) => {
            validate_foreach_stmt(node, scope, program, context)?;
        }
        Statement::Do(node) => {
            if node.catches.is_empty() && node.finally_body.is_none() {
                return Err(ForgeError::parse(
                    "Do block must be followed by at least one Catch or Finally",
                ));
            }
            validate_statements(&node.body, scope, program, context)?;
            let mut caught = std::collections::HashSet::new();
            let mut generic_seen = false;
            for catch in &node.catches {
                if !catch.error_type.is_builtin() {
                    return Err(ForgeError::parse(format!(
                        "invalid error type '{}'",
                        catch.error_type.name()
                    )));
                }
                if generic_seen {
                    return Err(ForgeError::parse(format!(
                        "Catch {} is unreachable because Catch Error appears first",
                        catch.error_type.name()
                    )));
                }
                if !caught.insert(catch.error_type.clone()) {
                    return Err(ForgeError::parse(format!(
                        "duplicate Catch for {}",
                        catch.error_type.name()
                    )));
                }
                generic_seen = catch.error_type == crate::ast::ErrorType::Error;
                let mut catch_scope = scope.clone();
                catch_scope
                    .var_types
                    .insert(catch.binding.clone(), TypeDecl::Weld);
                validate_statements(&catch.body, &catch_scope, program, context)?;
            }
            if let Some(body) = &node.finally_body {
                validate_statements(body, scope, program, context)?;
            }
        }
        Statement::Throw(error) => {
            if !error.error_type.is_builtin() {
                return Err(ForgeError::parse(format!(
                    "invalid error type '{}'",
                    error.error_type.name()
                )));
            }
            validate_expr(&error.message, scope, program, context)?;
            if expr_type(&error.message, scope, program) != ExprType::Weld {
                return Err(ForgeError::parse(format!(
                    "Throw {} requires a Weld message",
                    error.error_type.name()
                )));
            }
        }
        Statement::Stop => {
            if context.current_function == Some("Main") {
                return Err(ForgeError::parse("Stop cannot be used in Main"));
            }
            if !context.in_loop_or_if {
                return Err(ForgeError::parse(
                    "Stop can only be used inside a loop or If statement",
                ));
            }
        }
        Statement::Skip => {
            if !context.in_loop {
                return Err(ForgeError::parse("Skip can only be used inside a loop"));
            }
        }
        Statement::Use(import) => {
            let path = import.path.join(".");
            return Err(ForgeError::parse(format!(
                "unresolved import reached semantic analysis: {}",
                path
            )));
        }
        Statement::Using(import) => {
            return Err(ForgeError::parse(format!(
                "unresolved import reached semantic analysis: {}: {}",
                import.path.join("."),
                import.symbol
            )));
        }
        Statement::Print(print) => validate_expr(&print.expr, scope, program, context)?,
        Statement::Return(value) => {
            if let Some(value) = value {
                validate_expr(value, scope, program, context)?;
            }
            let function_name = context
                .current_function
                .ok_or_else(|| ForgeError::parse("Return can only be used inside a function"))?;
            let function = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::FunctionDecl(function) if function.name == function_name => {
                        Some(function)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    ForgeError::parse(format!("Undefined function: {}", function_name))
                })?;
            match (&function.ret_kind, value) {
                (ret_kind, None) if !ret_kind.is_void() => {
                    return Err(ForgeError::parse(format!(
                        "Function {} must return a value",
                        crate::imports::display_symbol_name(function_name)
                    )));
                }
                (ret_kind, Some(_)) if ret_kind.is_void() => {
                    return Err(ForgeError::parse(format!(
                        "Void function {} cannot return a value",
                        crate::imports::display_symbol_name(function_name)
                    )));
                }
                (ret_kind, Some(value)) => {
                    let expected = match ret_kind {
                        RetKind::Int => Some(TypeDecl::Number(Subtype::Int)),
                        RetKind::Float => Some(TypeDecl::Number(Subtype::Float)),
                        RetKind::Weld => Some(TypeDecl::Weld),
                        RetKind::Bool => Some(TypeDecl::Bool),
                        RetKind::Ore(size) => Some(TypeDecl::Ore(*size)),
                        RetKind::OreTuple(fields) => Some(TypeDecl::OreTuple(fields.clone())),
                        RetKind::Materials(element, has_new) => {
                            Some(TypeDecl::Materials(element.clone(), *has_new))
                        }
                        RetKind::Generic | RetKind::Function | RetKind::Dynamic => None,
                        RetKind::Nunction | RetKind::Void => unreachable!(),
                    };
                    if let Some(expected) = expected {
                        let actual = expr_type(value, scope, program);
                        if !types_compatible(&actual, &expected) {
                            return Err(ForgeError::parse(format!(
                                "Return type mismatch in {}: expected {}, got {}",
                                crate::imports::display_symbol_name(function_name),
                                type_name(&expected),
                                expr_type(value, scope, program).name()
                            )));
                        }
                    }
                }
                _ => {}
            }
        }
        Statement::ExprStmt(expr) => validate_expr(expr, scope, program, context)?,
        _ => {}
    }
    Ok(())
}

fn validate_for_stmt(
    node: &ForNode,
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    let loop_ctx = ValidationContext {
        in_loop_or_if: true,
        in_loop: true,
        ..context
    };
    let init_scope = scope.clone();
    validate_var_decl(&node.init, &init_scope, program, context)?;

    let mut loop_scope = init_scope;
    loop_scope
        .var_types
        .insert(node.init.name.clone(), node.init.type_decl.clone());
    validate_expr(&node.condition, &loop_scope, program, context)?;
    validate_statements(&node.body, &loop_scope, program, loop_ctx)?;

    match loop_scope.get(&node.increment_var) {
        None => Err(ForgeError::parse(format!(
            "Undefined variable: {}",
            node.increment_var
        ))),
        Some(TypeDecl::Number(Subtype::Int | Subtype::Float)) => Ok(()),
        Some(_) => Err(ForgeError::parse(format!(
            "For loop increment variable '{}' must be numeric",
            node.increment_var
        ))),
    }
}

fn validate_foreach_stmt(
    node: &ForEachNode,
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    let collection_type = scope.get(&node.collection_name).ok_or_else(|| {
        ForgeError::parse(format!("Undefined variable: {}", node.collection_name))
    })?;
    let element_type = match type_decl_to_expr_type(collection_type) {
        ExprType::Array { element_type } => ExprType::from_subtype(&element_type),
        ExprType::List {
            element_type: Some(element_type),
        } => ExprType::from_subtype(&element_type),
        other => {
            return Err(ForgeError::parse(format!(
                "ForEach collection '{}' must be an Array or List, got {}",
                node.collection_name,
                other.name()
            )))
        }
    };
    if !types_compatible(&element_type, &node.item_type) {
        return Err(ForgeError::parse(format!(
            "ForEach item '{}' has type {}, but collection '{}' contains {}",
            node.item_name,
            type_name(&node.item_type),
            node.collection_name,
            element_type.name()
        )));
    }

    let mut loop_scope = scope.clone();
    loop_scope
        .var_types
        .insert(node.item_name.clone(), node.item_type.clone());
    let loop_ctx = ValidationContext {
        in_loop_or_if: true,
        in_loop: true,
        ..context
    };
    validate_statements(&node.body, &loop_scope, program, loop_ctx)
}

fn validate_var_decl(
    decl: &crate::ast::VarDecl,
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    if let TypeDecl::Ore(_) = &decl.type_decl {
        if let Some(Expr::ArrayLiteral(elements)) = &decl.initializer {
            for element in elements {
                if !expr_matches_subtype(element, &Subtype::Int, scope, program) {
                    return Err(ForgeError::parse(
                        "Type mismatch in array initializer: expected element of type Int",
                    ));
                }
            }
        }
    }

    match &decl.type_decl {
        TypeDecl::Bool => {
            if let Some(init) = &decl.initializer {
                if expr_type(init, scope, program) != ExprType::Bool {
                    return Err(ForgeError::parse(format!(
                        "Type error: cannot assign {} to Bool variable '{}'",
                        expr_type(init, scope, program).name(),
                        decl.name
                    )));
                }
            }
        }
        TypeDecl::Ore(Some(expected_size)) => {
            if let Some(init) = &decl.initializer {
                if let Expr::ArrayLiteral(elements) = init {
                    if elements.len() as i64 != *expected_size {
                        return Err(ForgeError::parse(format!(
                            "Array size mismatch: expected {} elements, got {}",
                            expected_size,
                            elements.len()
                        )));
                    }
                }
            }
        }
        TypeDecl::OreTuple(fields) => {
            if let Some(init) = &decl.initializer {
                if let Expr::TupleLiteral(elements) = init {
                    if elements.len() != fields.len() {
                        return Err(ForgeError::parse(format!(
                            "Tuple field count mismatch: expected {} fields, got {}",
                            fields.len(),
                            elements.len()
                        )));
                    }
                    for (elem, (field_type, field_name)) in elements.iter().zip(fields.iter()) {
                        if !expr_matches_subtype(elem, field_type, scope, program) {
                            return Err(ForgeError::parse(format!(
                                "Type mismatch for tuple field '{}': expected {:?}",
                                field_name, field_type
                            )));
                        }
                    }
                }
            }
        }
        TypeDecl::Materials(elem_type, has_new) => {
            if *has_new && decl.initializer.is_some() {
                return Err(ForgeError::parse(
                    "Empty list declared with 'new' cannot have an initializer",
                ));
            }
            if !has_new {
                if let Some(Expr::ListLiteral(_)) = &decl.initializer {
                    if !types_compatible(
                        &expr_type(decl.initializer.as_ref().unwrap(), scope, program),
                        &decl.type_decl,
                    ) {
                        return Err(ForgeError::parse(format!(
                            "Type mismatch in list initializer: expected element of type {:?}",
                            elem_type
                        )));
                    }
                }
            }
        }
        _ => {}
    }

    if let Some(initializer) = &decl.initializer {
        validate_expr(initializer, scope, program, context)?;
        if !types_compatible(&expr_type(initializer, scope, program), &decl.type_decl) {
            return Err(ForgeError::parse(format!(
                "Type mismatch: cannot assign {} to {} variable '{}'",
                expr_type(initializer, scope, program).name(),
                type_name(&decl.type_decl),
                decl.name
            )));
        }
    }
    Ok(())
}

fn validate_object_decl(
    object: &crate::ast::ObjectDecl,
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    let data = data_decl(program, &object.type_name)
        .ok_or_else(|| ForgeError::parse(format!("Unknown Data type: {}", object.type_name)))?;
    for (path, value) in &object.inits {
        validate_expr(value, scope, program, context)?;
        let member = path
            .last()
            .ok_or_else(|| ForgeError::parse("Missing Data member"))?;
        let expected = data
            .members
            .iter()
            .find(|field| field.name == *member)
            .ok_or_else(|| ForgeError::parse(format!("Unknown Data field: {}", member)))?;
        if !types_compatible(&expr_type(value, scope, program), &expected.type_decl) {
            return Err(ForgeError::parse(format!(
                "Type mismatch for Data field '{}': expected {}, got {}",
                member,
                type_name(&expected.type_decl),
                expr_type(value, scope, program).name()
            )));
        }
    }
    Ok(())
}

fn expr_matches_subtype(expr: &Expr, expected: &Subtype, scope: &Scope, program: &Program) -> bool {
    subtype_from_expr_type(&expr_type(expr, scope, program))
        .is_some_and(|(actual, _)| subtype_compatible(&actual, expected))
}

fn validate_assignment_target(
    target: &AssignmentTarget,
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    match target {
        AssignmentTarget::Var(_) => Ok(()),
        AssignmentTarget::Member { object, .. } => {
            validate_assignment_target(object, scope, program, context)
        }
        AssignmentTarget::Index { object, index } => {
            validate_assignment_target(object, scope, program, context)?;
            validate_expr(index, scope, program, context)?;
            if !matches!(
                expr_type(index, scope, program),
                ExprType::Number(Subtype::Int)
            ) {
                return Err(ForgeError::parse(format!(
                    "Index must be Int, got {}",
                    expr_type(index, scope, program).name()
                )));
            }
            Ok(())
        }
    }
}

fn validate_assignment_type(
    assignment: &crate::ast::AssignmentNode,
    scope: &Scope,
    program: &Program,
) -> ForgeResult<()> {
    let target_type = assignment_target_type(&assignment.target, scope)?;
    let value_type = expr_type(&assignment.value, scope, program);

    if !expr_types_compatible(&value_type, &target_type) {
        if let AssignmentTarget::Var(name) = &assignment.target {
            if matches!(target_type, ExprType::Bool) {
                return Err(ForgeError::parse(format!(
                    "Type error: cannot assign {} to Bool variable '{}'",
                    value_type.name(),
                    name
                )));
            }
        }
        return Err(ForgeError::parse(format!(
            "Type mismatch: cannot assign {} to {}",
            value_type.name(),
            target_type.name()
        )));
    }
    Ok(())
}

fn assignment_target_type(target: &AssignmentTarget, scope: &Scope) -> ForgeResult<ExprType> {
    match target {
        AssignmentTarget::Var(name) => scope
            .get(name)
            .map(type_decl_to_expr_type)
            .ok_or_else(|| ForgeError::parse(format!("Undefined variable: {}", name))),
        AssignmentTarget::Member { object, member } => match assignment_target_type(object, scope)?
        {
            ExprType::Tuple { fields } => fields
                .iter()
                .find(|(_, field_name)| field_name == member)
                .map(|(field_type, _)| ExprType::from_subtype(field_type))
                .ok_or_else(|| ForgeError::parse(format!("Unknown tuple field: {}", member))),
            ExprType::Unknown => Ok(ExprType::Unknown),
            other => Err(ForgeError::parse(format!(
                "Member assignment requires a tuple, got {}",
                other.name()
            ))),
        },
        AssignmentTarget::Index { object, .. } => match assignment_target_type(object, scope)? {
            ExprType::Array { element_type } => Ok(ExprType::from_subtype(&element_type)),
            ExprType::List {
                element_type: Some(element_type),
            } => Ok(ExprType::from_subtype(&element_type)),
            ExprType::List { element_type: None } => Err(ForgeError::parse(
                "Index assignment requires a list with a known element type",
            )),
            ExprType::Unknown => Ok(ExprType::Unknown),
            other => Err(ForgeError::parse(format!(
                "Index assignment requires an Array or List, got {}",
                other.name()
            ))),
        },
    }
}

fn expr_types_compatible(actual: &ExprType, expected: &ExprType) -> bool {
    match (actual, expected) {
        (ExprType::Number(actual), ExprType::Number(expected)) => {
            subtype_compatible(actual, expected)
        }
        (ExprType::Weld, ExprType::Weld) | (ExprType::Bool, ExprType::Bool) => true,
        (ExprType::Array { .. }, ExprType::Array { .. }) => true,
        (
            ExprType::Tuple {
                fields: actual_fields,
            },
            ExprType::Tuple {
                fields: expected_fields,
            },
        ) => {
            actual_fields.len() == expected_fields.len()
                && actual_fields.iter().zip(expected_fields).all(
                    |((actual_type, actual_name), (expected_type, expected_name))| {
                        subtype_compatible(actual_type, expected_type)
                            && (actual_name.is_empty() || actual_name == expected_name)
                    },
                )
        }
        (
            ExprType::List {
                element_type: Some(actual_type),
            },
            ExprType::List {
                element_type: Some(expected_type),
            },
        ) => subtype_compatible(actual_type, expected_type),
        (ExprType::List { element_type: None }, ExprType::List { .. }) => true,
        _ => false,
    }
}

fn validate_expr(
    expr: &Expr,
    scope: &Scope,
    program: &Program,
    context: ValidationContext,
) -> ForgeResult<()> {
    match expr {
        Expr::Call { callee, args } => {
            let function = program
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::FunctionDecl(function) if function.name == *callee => Some(function),
                    _ => None,
                })
                .ok_or_else(|| ForgeError::parse(format!("Undefined function: {}", callee)))?;
            if function.params.len() != args.len() {
                return Err(ForgeError::parse(format!(
                    "Function {} expects {} arguments, got {}",
                    crate::imports::display_symbol_name(callee),
                    function.params.len(),
                    args.len()
                )));
            }
            for (index, (arg, param)) in args.iter().zip(&function.params).enumerate() {
                validate_expr(arg, scope, program, context)?;
                let actual = expr_type(arg, scope, program);
                if !types_compatible(&actual, &param.type_decl) {
                    return Err(ForgeError::parse(format!(
                        "Argument {} of {}: expected {}, got {}",
                        index + 1,
                        crate::imports::display_symbol_name(callee),
                        type_name(&param.type_decl),
                        expr_type(arg, scope, program).name()
                    )));
                }
            }
        }
        Expr::NamespaceCall {
            namespace,
            method,
            args,
        } => {
            if namespace == "Program" {
                if context.current_function.is_none() {
                    return Err(ForgeError::parse(
                        "Program.Stop() can only be used inside a function scope",
                    ));
                }
                if method == "Stop" {
                    if !args.is_empty() {
                        return Err(ForgeError::parse("Program.Stop() expects 0 arguments"));
                    }
                } else {
                    return Err(ForgeError::parse(format!(
                        "Unknown method {} in Program namespace",
                        method
                    )));
                }
            } else {
                return Err(ForgeError::parse(format!(
                    "Unknown namespace: {}",
                    namespace
                )));
            }
            for arg in args {
                validate_expr(arg, scope, program, context)?;
            }
        }
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            validate_expr(object, scope, program, context)?;
            for arg in args {
                validate_expr(arg, scope, program, context)?;
            }
            validate_method_call(object, method, args, scope, program)?;
        }
        Expr::IndexAccess { object, index } => {
            validate_expr(object, scope, program, context)?;
            validate_expr(index, scope, program, context)?;
            match expr_type(object, scope, program) {
                ExprType::Array { .. } | ExprType::List { .. } => {}
                ExprType::Unknown => {
                    return Err(ForgeError::parse("Unknown index access receiver"))
                }
                other => {
                    return Err(ForgeError::parse(format!(
                        "Index access requires an Array or List, got {}",
                        other.name()
                    )))
                }
            }
            if !matches!(
                expr_type(index, scope, program),
                ExprType::Number(Subtype::Int)
            ) {
                return Err(ForgeError::parse(format!(
                    "Index must be Int, got {}",
                    expr_type(index, scope, program).name()
                )));
            }
        }
        Expr::ArrayLiteral(elements) => {
            for elem in elements {
                validate_expr(elem, scope, program, context)?;
                if !expr_matches_subtype(elem, &Subtype::Int, scope, program) {
                    return Err(ForgeError::parse(
                        "Type mismatch in array literal: expected element of type Int",
                    ));
                }
            }
        }
        Expr::ListLiteral(elements) => {
            for elem in elements {
                validate_expr(elem, scope, program, context)?;
            }
            if !elements.is_empty() && matches!(expr_type(expr, scope, program), ExprType::Unknown)
            {
                return Err(ForgeError::parse(
                    "List literal elements must have one consistent resolved type",
                ));
            }
        }
        Expr::TupleLiteral(elements) => {
            for elem in elements {
                validate_expr(elem, scope, program, context)?;
            }
        }
        Expr::BinaryOp { lhs, rhs, op } => {
            validate_expr(lhs, scope, program, context)?;
            validate_expr(rhs, scope, program, context)?;

            if matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::Pow
            ) && numeric_type(expr, scope, program).is_none()
            {
                let invalid_operand = if numeric_type(lhs, scope, program).is_none() {
                    lhs
                } else {
                    rhs
                };
                if matches!(op, BinOp::Rem) {
                    if expr_type(invalid_operand, scope, program).name() == "Float" {
                        return Err(ForgeError::parse(
                            "Modulo (%) is only supported for integer types",
                        ));
                    }
                    return Err(ForgeError::parse(format!(
                        "Modulo operator (%) requires integer operands, got {}",
                        expr_type(invalid_operand, scope, program).name()
                    )));
                }
                return Err(ForgeError::parse(format!(
                    "Invalid operand type for arithmetic operation: expected Int or Float, got {}",
                    expr_type(invalid_operand, scope, program).name()
                )));
            }
        }
        Expr::UnaryOp { operand, .. } => {
            validate_expr(operand, scope, program, context)?;
            if numeric_type(operand, scope, program).is_none() {
                return Err(ForgeError::parse(format!(
                    "Invalid operand type for unary operation: expected Int or Float, got {}",
                    expr_type_name(operand)
                )));
            }
        }
        Expr::MemberAccess { object, member } => {
            validate_expr(object, scope, program, context)?;
            validate_member_access(object, member, scope, program)?;
        }
        Expr::Identifier(name) if name.contains('.') && !scope.contains_key(name) => {
            return Err(ForgeError::parse(format!(
                "shared member access is not allowed: {}",
                name
            )));
        }
        Expr::Identifier(name)
            if scope.catch_bindings.contains(name) && !scope.contains_key(name) =>
        {
            return Err(ForgeError::parse(format!("Undefined variable: {}", name)));
        }
        _ => {}
    }
    Ok(())
}

fn validate_member_access(
    object: &Expr,
    member: &str,
    scope: &Scope,
    program: &Program,
) -> ForgeResult<()> {
    match expr_type(object, scope, program) {
        ExprType::Array { .. } | ExprType::List { .. } if member == "Length" || member == "Len" => {
            Ok(())
        }
        ExprType::Tuple { fields } => {
            if fields.iter().any(|(_, field_name)| field_name == member) {
                Ok(())
            } else {
                Err(ForgeError::parse(format!(
                    "Unknown tuple field: {}",
                    member
                )))
            }
        }
        ExprType::DataObject { type_name } => {
            if data_member_type(program, &type_name, member).is_some() {
                Ok(())
            } else {
                Err(ForgeError::parse(format!("Unknown Data field: {}", member)))
            }
        }
        ExprType::Unknown => Err(ForgeError::parse("Unknown member access receiver")),
        other => Err(ForgeError::parse(format!(
            "Member access requires a tuple, Array, or List, got {}",
            other.name()
        ))),
    }
}

fn validate_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    scope: &Scope,
    program: &Program,
) -> ForgeResult<()> {
    let receiver = expr_type(object, scope, program);
    match receiver {
        ExprType::List {
            element_type: Some(element_type),
        } => match method {
            "Add" => {
                if args.len() != 1 {
                    return Err(ForgeError::parse("Add expects 1 argument"));
                }
                if !expr_matches_subtype(&args[0], &element_type, scope, program) {
                    return Err(ForgeError::parse(format!(
                        "Type mismatch: Add expects element of type {:?}",
                        element_type
                    )));
                }
                Ok(())
            }
            "Remove" | "RemoveAt" => {
                if args.len() != 1 {
                    return Err(ForgeError::parse(format!("{} expects 1 argument", method)));
                }
                if !matches!(
                    expr_type(&args[0], scope, program),
                    ExprType::Number(Subtype::Int)
                ) {
                    return Err(ForgeError::parse(format!(
                        "{} index must be Int, got {}",
                        method,
                        expr_type(&args[0], scope, program).name()
                    )));
                }
                Ok(())
            }
            "Length" | "Len" => {
                if args.is_empty() {
                    Ok(())
                } else {
                    Err(ForgeError::parse(format!("{} expects 0 arguments", method)))
                }
            }
            other => Err(ForgeError::parse(format!("Unknown method: {}", other))),
        },
        ExprType::List { element_type: None } => match method {
            "Length" | "Len" if args.is_empty() => Ok(()),
            "Length" | "Len" => Err(ForgeError::parse(format!("{} expects 0 arguments", method))),
            _ => Err(ForgeError::parse(
                "Method call requires a list with a known element type",
            )),
        },
        ExprType::Array { .. } => match method {
            "Length" | "Len" if args.is_empty() => Ok(()),
            "Length" | "Len" => Err(ForgeError::parse(format!("{} expects 0 arguments", method))),
            _ => Err(ForgeError::parse(format!(
                "Method {} requires a Materials receiver",
                method
            ))),
        },
        ExprType::Unknown => Err(ForgeError::parse("Unknown method call receiver")),
        _ => Err(ForgeError::parse(format!(
            "Method {} requires an Array or Materials receiver",
            method
        ))),
    }
}
