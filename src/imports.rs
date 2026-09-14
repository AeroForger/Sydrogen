//! Module discovery and `Use` / `Using` resolution.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::ast::{AssignmentTarget, Expr, Modifier, Program, Statement, UseNode, UsingNode};
use crate::errors::{ForgeError, ForgeResult};
use crate::project::ProjectSource;

#[derive(Debug, Clone)]
pub struct ModuleSource {
    pub name: String,
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub program: Program,
}

#[derive(Debug, Clone)]
pub struct ModuleTable {
    modules: Vec<ModuleSource>,
    by_name: BTreeMap<String, usize>,
}

impl ModuleTable {
    pub fn from_project(sources: &[ProjectSource]) -> ForgeResult<Self> {
        let modules = sources
            .iter()
            .map(|source| {
                Ok(ModuleSource {
                    name: module_name(&source.path)?,
                    path: source.path.clone(),
                    relative_path: source.relative_path.clone(),
                    program: source.program.clone(),
                })
            })
            .collect::<ForgeResult<Vec<_>>>()?;
        Self::new(modules)
    }

    fn new(modules: Vec<ModuleSource>) -> ForgeResult<Self> {
        let mut by_name = BTreeMap::new();
        for (index, module) in modules.iter().enumerate() {
            if let Some(previous) = by_name.insert(module.name.clone(), index) {
                return Err(ForgeError::parse(format!(
                    "duplicate module name '{}': '{}' and '{}'",
                    module.name,
                    modules[previous].path.display(),
                    module.path.display()
                )));
            }
        }
        Ok(Self { modules, by_name })
    }

    pub fn modules(&self) -> &[ModuleSource] {
        &self.modules
    }

    pub fn get(&self, name: &str) -> Option<&ModuleSource> {
        self.by_name.get(name).map(|index| &self.modules[*index])
    }
}

#[derive(Debug, Clone, Copy)]
enum ResolutionMode {
    Project,
    Standalone,
}

#[derive(Debug, Clone)]
struct ModuleSymbol {
    mangled_name: String,
    is_public: bool,
    kind: SymbolKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolKind {
    Function,
    Data,
}

#[derive(Debug, Clone, Default)]
struct ImportScope {
    namespaces: HashMap<String, usize>,
    selective: HashMap<String, (usize, String)>,
}

#[derive(Debug, Clone)]
struct ImportSite {
    path: Vec<String>,
    symbol: Option<String>,
    line: usize,
    column: usize,
}

pub fn resolve_project(sources: &[ProjectSource]) -> ForgeResult<Program> {
    let table = ModuleTable::from_project(sources)?;
    resolve_table(table, ResolutionMode::Project)
}

pub fn load_standalone(entry: &Path) -> ForgeResult<Program> {
    let entry = std::fs::canonicalize(entry).map_err(|error| {
        ForgeError::parse(format!("cannot read '{}': {}", entry.display(), error))
    })?;
    let mut modules = vec![load_module(&entry, &entry)?];
    let mut loaded_paths = HashMap::from([(entry.clone(), 0usize)]);
    let mut index = 0;
    while index < modules.len() {
        let imports = top_level_imports(&modules[index].program);
        for import in imports {
            let candidate = standalone_import_path(&modules[index].path, &import.path);
            let canonical = std::fs::canonicalize(&candidate).map_err(|error| {
                import_error(
                    &modules[index],
                    &import,
                    format!(
                        "module '{}' not found (expected '{}': {})",
                        import.path.join("."),
                        candidate.display(),
                        error
                    ),
                )
            })?;
            if !loaded_paths.contains_key(&canonical) {
                let next = modules.len();
                modules.push(load_module(&canonical, &entry)?);
                loaded_paths.insert(canonical, next);
            }
        }
        index += 1;
    }
    resolve_table(ModuleTable::new(modules)?, ResolutionMode::Standalone)
}

pub fn module_name(path: &Path) -> ForgeResult<String> {
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ForgeError::parse(format!("invalid module path '{}'", path.display())))?;
    Ok(name.to_string())
}

fn load_module(path: &Path, entry: &Path) -> ForgeResult<ModuleSource> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        ForgeError::parse(format!(
            "cannot read source '{}': {}",
            path.display(),
            error
        ))
    })?;
    let program = crate::parser::parse_program(&source)
        .map_err(|error| ForgeError::parse(format!("{}: {}", path.display(), error)))?;
    let root = entry.parent().unwrap_or_else(|| Path::new("."));
    Ok(ModuleSource {
        name: module_name(path)?,
        path: path.to_path_buf(),
        relative_path: path.strip_prefix(root).unwrap_or(path).to_path_buf(),
        program,
    })
}

fn standalone_import_path(importer: &Path, module_path: &[String]) -> PathBuf {
    let mut path = importer
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    for component in module_path {
        path.push(component);
    }
    path.set_extension("anvil");
    path
}

fn resolve_table(table: ModuleTable, mode: ResolutionMode) -> ForgeResult<Program> {
    let symbols = build_module_symbols(&table)?;
    let mut scopes = vec![ImportScope::default(); table.modules.len()];
    let mut graph = vec![Vec::new(); table.modules.len()];

    for (module_index, module) in table.modules.iter().enumerate() {
        let local_names: BTreeSet<&str> =
            symbols[module_index].keys().map(String::as_str).collect();
        for import in top_level_imports(&module.program) {
            let target = resolve_module_index(&table, module_index, &import.path, mode)
                .ok_or_else(|| {
                    import_error(
                        module,
                        &import,
                        format!("module '{}' not found", import.path.join(".")),
                    )
                })?;
            if !graph[module_index].contains(&target) {
                graph[module_index].push(target);
            }

            if let Some(symbol_name) = &import.symbol {
                let symbol = symbols[target].get(symbol_name).ok_or_else(|| {
                    import_error(
                        module,
                        &import,
                        format!(
                            "symbol '{}' not found in module '{}'",
                            symbol_name,
                            import.path.join(".")
                        ),
                    )
                })?;
                if !symbol.is_public {
                    return Err(import_error(
                        module,
                        &import,
                        format!(
                            "symbol '{}' in module '{}' is private",
                            symbol_name,
                            import.path.join(".")
                        ),
                    ));
                }
                if local_names.contains(symbol_name.as_str()) {
                    return Err(import_error(
                        module,
                        &import,
                        format!(
                            "imported symbol '{}' conflicts with a declaration in module '{}'",
                            symbol_name, module.name
                        ),
                    ));
                }
                if let Some(existing) = scopes[module_index].selective.get(symbol_name) {
                    if existing != &(target, symbol_name.clone()) {
                        return Err(import_error(
                            module,
                            &import,
                            format!(
                                "imported symbol '{}' conflicts with another symbol in this scope",
                                symbol_name
                            ),
                        ));
                    }
                } else {
                    scopes[module_index]
                        .selective
                        .insert(symbol_name.clone(), (target, symbol_name.clone()));
                }
            } else {
                let full_name = import.path.join(".");
                insert_namespace(
                    module,
                    &import,
                    &mut scopes[module_index],
                    full_name,
                    target,
                )?;
            }
        }
    }

    let order = dependency_order(&table, &graph)?;
    let mut statements = Vec::new();
    for module_index in order {
        let module = &table.modules[module_index];
        for mut statement in module.program.statements.clone() {
            if matches!(statement, Statement::Use(_) | Statement::Using(_)) {
                continue;
            }
            rewrite_statement(
                &mut statement,
                module_index,
                module,
                &table,
                &symbols,
                &scopes[module_index],
                false,
            )?;
            match &mut statement {
                Statement::FunctionDecl(function) => {
                    if let Some(symbol) = symbols[module_index].get(&function.name) {
                        function.name.clone_from(&symbol.mangled_name);
                    }
                }
                Statement::DataDecl(data) => {
                    if let Some(symbol) = symbols[module_index].get(&data.name) {
                        data.name.clone_from(&symbol.mangled_name);
                    }
                }
                _ => {}
            }
            statements.push(statement);
        }
    }
    Ok(Program { statements })
}

fn build_module_symbols(table: &ModuleTable) -> ForgeResult<Vec<HashMap<String, ModuleSymbol>>> {
    let mut result = Vec::with_capacity(table.modules.len());
    let mut main_module: Option<usize> = None;
    for (module_index, module) in table.modules.iter().enumerate() {
        let mut symbols = HashMap::new();
        for statement in &module.program.statements {
            let (name, modifier, kind) = match statement {
                Statement::FunctionDecl(function) => {
                    (&function.name, &function.modifier, SymbolKind::Function)
                }
                Statement::DataDecl(data) => (&data.name, &data.modifier, SymbolKind::Data),
                _ => continue,
            };
            if symbols.contains_key(name) {
                return Err(ForgeError::parse(format!(
                    "{}: duplicate symbol '{}' in module '{}'",
                    module.path.display(),
                    name,
                    module.name
                )));
            }
            if kind == SymbolKind::Function && name == "Main" {
                if let Some(previous) = main_module {
                    return Err(ForgeError::parse(format!(
                        "multiple Main functions in modules '{}' and '{}'",
                        table.modules[previous].name, module.name
                    )));
                }
                main_module = Some(module_index);
            }
            symbols.insert(
                name.clone(),
                ModuleSymbol {
                    mangled_name: if kind == SymbolKind::Function && name == "Main" {
                        "Main".into()
                    } else {
                        mangle_symbol(&module.name, name)
                    },
                    is_public: matches!(modifier, Some(Modifier::Open)),
                    kind,
                },
            );
        }
        result.push(symbols);
    }
    Ok(result)
}

fn mangle_symbol(module: &str, symbol: &str) -> String {
    format!("__forge_m{}_{}_{}", module.len(), module, symbol)
}

pub(crate) fn display_symbol_name(name: &str) -> &str {
    let Some(encoded) = name.strip_prefix("__forge_m") else {
        return name;
    };
    let Some((module_length, remainder)) = encoded.split_once('_') else {
        return name;
    };
    let Ok(module_length) = module_length.parse::<usize>() else {
        return name;
    };
    remainder
        .get(module_length..)
        .and_then(|suffix| suffix.strip_prefix('_'))
        .filter(|symbol| !symbol.is_empty())
        .unwrap_or(name)
}

fn insert_namespace(
    module: &ModuleSource,
    import: &ImportSite,
    scope: &mut ImportScope,
    name: String,
    target: usize,
) -> ForgeResult<()> {
    if let Some(existing) = scope.namespaces.get(&name) {
        if *existing != target {
            return Err(import_error(
                module,
                import,
                format!("module namespace '{}' conflicts in this scope", name),
            ));
        }
    } else {
        scope.namespaces.insert(name, target);
    }
    Ok(())
}

fn resolve_module_index(
    table: &ModuleTable,
    importer: usize,
    path: &[String],
    mode: ResolutionMode,
) -> Option<usize> {
    match mode {
        ResolutionMode::Project => {
            let name = path.last()?;
            let index = *table.by_name.get(name)?;
            if path.len() == 1 || relative_module_path_matches(&table.modules[index], path) {
                Some(index)
            } else {
                None
            }
        }
        ResolutionMode::Standalone => {
            let candidate = standalone_import_path(&table.modules[importer].path, path);
            let canonical = std::fs::canonicalize(candidate).ok()?;
            table
                .modules
                .iter()
                .position(|module| module.path == canonical)
        }
    }
}

fn relative_module_path_matches(module: &ModuleSource, path: &[String]) -> bool {
    let mut components: Vec<String> = module
        .relative_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    let Some(last) = components.last_mut() else {
        return false;
    };
    *last = Path::new(last)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    components.ends_with(path)
}

fn top_level_imports(program: &Program) -> Vec<ImportSite> {
    program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Use(import) => Some(use_site(import)),
            Statement::Using(import) => Some(using_site(import)),
            _ => None,
        })
        .collect()
}

fn use_site(import: &UseNode) -> ImportSite {
    ImportSite {
        path: import.path.clone(),
        symbol: None,
        line: import.line,
        column: import.column,
    }
}

fn using_site(import: &UsingNode) -> ImportSite {
    ImportSite {
        path: import.path.clone(),
        symbol: Some(import.symbol.clone()),
        line: import.line,
        column: import.column,
    }
}

fn import_error(
    module: &ModuleSource,
    import: &ImportSite,
    message: impl Into<String>,
) -> ForgeError {
    ForgeError::parse(format!(
        "{}:{}:{}: {}",
        module.path.display(),
        import.line,
        import.column,
        message.into()
    ))
}

fn dependency_order(table: &ModuleTable, graph: &[Vec<usize>]) -> ForgeResult<Vec<usize>> {
    fn visit(
        node: usize,
        table: &ModuleTable,
        graph: &[Vec<usize>],
        states: &mut [u8],
        stack: &mut Vec<usize>,
        order: &mut Vec<usize>,
    ) -> ForgeResult<()> {
        states[node] = 1;
        stack.push(node);
        for &dependency in &graph[node] {
            if states[dependency] == 1 {
                let start = stack.iter().position(|entry| *entry == dependency).unwrap();
                let mut cycle: Vec<String> = stack[start..]
                    .iter()
                    .map(|index| table.modules[*index].name.clone())
                    .collect();
                cycle.push(table.modules[dependency].name.clone());
                return Err(ForgeError::parse(format!(
                    "circular import detected: {}",
                    cycle.join(" -> ")
                )));
            }
            if states[dependency] == 0 {
                visit(dependency, table, graph, states, stack, order)?;
            }
        }
        stack.pop();
        states[node] = 2;
        order.push(node);
        Ok(())
    }

    let mut states = vec![0; table.modules.len()];
    let mut stack = Vec::new();
    let mut order = Vec::with_capacity(table.modules.len());
    for node in 0..table.modules.len() {
        if states[node] == 0 {
            visit(node, table, graph, &mut states, &mut stack, &mut order)?;
        }
    }
    Ok(order)
}

fn rewrite_statement(
    statement: &mut Statement,
    module_index: usize,
    module: &ModuleSource,
    table: &ModuleTable,
    symbols: &[HashMap<String, ModuleSymbol>],
    scope: &ImportScope,
    nested: bool,
) -> ForgeResult<()> {
    match statement {
        Statement::Use(import) => {
            if nested {
                return Err(import_error(
                    module,
                    &use_site(import),
                    "Use must be top-level",
                ));
            }
        }
        Statement::Using(import) => {
            if nested {
                return Err(import_error(
                    module,
                    &using_site(import),
                    "Using must be top-level",
                ));
            }
        }
        Statement::VarDecl(declaration) => {
            if let Some(initializer) = &mut declaration.initializer {
                rewrite_expr(initializer, module_index, module, table, symbols, scope)?;
            }
        }
        Statement::DataDecl(_) | Statement::Input(_) | Statement::Stop | Statement::Skip => {}
        Statement::ObjectDecl(object) => {
            rewrite_data_type(
                &mut object.type_name,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
            for (_, value) in &mut object.inits {
                rewrite_expr(value, module_index, module, table, symbols, scope)?;
            }
        }
        Statement::FunctionDecl(function) => {
            rewrite_body(
                &mut function.body,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
        }
        Statement::Print(print) => {
            rewrite_expr(&mut print.expr, module_index, module, table, symbols, scope)?;
        }
        Statement::If(node) => {
            for (condition, body) in &mut node.branches {
                rewrite_expr(condition, module_index, module, table, symbols, scope)?;
                rewrite_body(body, module_index, module, table, symbols, scope)?;
            }
            if let Some(body) = &mut node.else_body {
                rewrite_body(body, module_index, module, table, symbols, scope)?;
            }
        }
        Statement::While(node) => {
            rewrite_expr(
                &mut node.condition,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
            rewrite_body(&mut node.body, module_index, module, table, symbols, scope)?;
        }
        Statement::For(node) => {
            if let Some(initializer) = &mut node.init.initializer {
                rewrite_expr(initializer, module_index, module, table, symbols, scope)?;
            }
            rewrite_expr(
                &mut node.condition,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
            rewrite_body(&mut node.body, module_index, module, table, symbols, scope)?;
        }
        Statement::ForEach(node) => {
            rewrite_body(&mut node.body, module_index, module, table, symbols, scope)?
        }
        Statement::Switch(node) => {
            rewrite_expr(
                &mut node.expression,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
            for branch in &mut node.branches {
                match branch {
                    crate::ast::SwitchBranch::Case(case) => {
                        for value in &mut case.values {
                            rewrite_expr(value, module_index, module, table, symbols, scope)?;
                        }
                        rewrite_body(&mut case.body, module_index, module, table, symbols, scope)?;
                    }
                    crate::ast::SwitchBranch::Base(body) => {
                        rewrite_body(body, module_index, module, table, symbols, scope)?;
                    }
                }
            }
        }
        Statement::Do(node) => {
            rewrite_body(&mut node.body, module_index, module, table, symbols, scope)?;
            for catch in &mut node.catches {
                rewrite_body(&mut catch.body, module_index, module, table, symbols, scope)?;
            }
            if let Some(body) = &mut node.finally_body {
                rewrite_body(body, module_index, module, table, symbols, scope)?;
            }
        }
        Statement::Throw(error) => {
            rewrite_expr(
                &mut error.message,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
        }
        Statement::Return(value) => {
            if let Some(value) = value {
                rewrite_expr(value, module_index, module, table, symbols, scope)?;
            }
        }
        Statement::Assignment(assignment) => {
            rewrite_assignment_target(
                &mut assignment.target,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
            rewrite_expr(
                &mut assignment.value,
                module_index,
                module,
                table,
                symbols,
                scope,
            )?;
        }
        Statement::ExprStmt(expression) => {
            rewrite_expr(expression, module_index, module, table, symbols, scope)?;
        }
    }
    Ok(())
}

fn rewrite_body(
    body: &mut [Statement],
    module_index: usize,
    module: &ModuleSource,
    table: &ModuleTable,
    symbols: &[HashMap<String, ModuleSymbol>],
    scope: &ImportScope,
) -> ForgeResult<()> {
    for statement in body {
        rewrite_statement(statement, module_index, module, table, symbols, scope, true)?;
    }
    Ok(())
}

fn rewrite_assignment_target(
    target: &mut AssignmentTarget,
    module_index: usize,
    module: &ModuleSource,
    table: &ModuleTable,
    symbols: &[HashMap<String, ModuleSymbol>],
    scope: &ImportScope,
) -> ForgeResult<()> {
    match target {
        AssignmentTarget::Var(_) => Ok(()),
        AssignmentTarget::Member { object, .. } => {
            rewrite_assignment_target(object, module_index, module, table, symbols, scope)
        }
        AssignmentTarget::Index { object, index } => {
            rewrite_assignment_target(object, module_index, module, table, symbols, scope)?;
            rewrite_expr(index, module_index, module, table, symbols, scope)
        }
    }
}

fn rewrite_expr(
    expression: &mut Expr,
    module_index: usize,
    module: &ModuleSource,
    table: &ModuleTable,
    symbols: &[HashMap<String, ModuleSymbol>],
    scope: &ImportScope,
) -> ForgeResult<()> {
    match expression {
        Expr::Call { callee, args } => {
            for argument in args.iter_mut() {
                rewrite_expr(argument, module_index, module, table, symbols, scope)?;
            }
            if let Some(symbol) = symbols[module_index].get(callee) {
                require_symbol_kind(module, callee, symbol, SymbolKind::Function)?;
                callee.clone_from(&symbol.mangled_name);
            } else if let Some((target, original)) = scope.selective.get(callee) {
                let symbol = &symbols[*target][original];
                require_symbol_kind(module, original, symbol, SymbolKind::Function)?;
                callee.clone_from(&symbol.mangled_name);
            }
        }
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            for argument in args.iter_mut() {
                rewrite_expr(argument, module_index, module, table, symbols, scope)?;
            }
            if let Some(path) = expression_path(object) {
                if let Some(target) = scope.namespaces.get(&path.join(".")) {
                    let symbol = symbols[*target].get(method).ok_or_else(|| {
                        ForgeError::parse(format!(
                            "{}: symbol '{}' not found in module '{}'",
                            module.path.display(),
                            method,
                            table.modules[*target].name
                        ))
                    })?;
                    if !symbol.is_public {
                        return Err(ForgeError::parse(format!(
                            "{}: symbol '{}' in module '{}' is private",
                            module.path.display(),
                            method,
                            table.modules[*target].name
                        )));
                    }
                    require_symbol_kind(module, method, symbol, SymbolKind::Function)?;
                    *expression = Expr::Call {
                        callee: symbol.mangled_name.clone(),
                        args: std::mem::take(args),
                    };
                    return Ok(());
                }
            }
            rewrite_expr(object, module_index, module, table, symbols, scope)?;
        }
        Expr::NamespaceCall { args, .. } => {
            for argument in args {
                rewrite_expr(argument, module_index, module, table, symbols, scope)?;
            }
        }
        Expr::MemberAccess { object, .. } => {
            rewrite_expr(object, module_index, module, table, symbols, scope)?;
        }
        Expr::IndexAccess { object, index } => {
            rewrite_expr(object, module_index, module, table, symbols, scope)?;
            rewrite_expr(index, module_index, module, table, symbols, scope)?;
        }
        Expr::BinaryOp { lhs, rhs, .. } => {
            rewrite_expr(lhs, module_index, module, table, symbols, scope)?;
            rewrite_expr(rhs, module_index, module, table, symbols, scope)?;
        }
        Expr::UnaryOp { operand, .. } => {
            rewrite_expr(operand, module_index, module, table, symbols, scope)?;
        }
        Expr::ArrayLiteral(elements)
        | Expr::TupleLiteral(elements)
        | Expr::ListLiteral(elements) => {
            for element in elements {
                rewrite_expr(element, module_index, module, table, symbols, scope)?;
            }
        }
        Expr::Number(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Identifier(_) | Expr::Input(_) => {}
    }
    Ok(())
}

fn rewrite_data_type(
    type_name: &mut String,
    module_index: usize,
    module: &ModuleSource,
    table: &ModuleTable,
    symbols: &[HashMap<String, ModuleSymbol>],
    scope: &ImportScope,
) -> ForgeResult<()> {
    if let Some(symbol) = symbols[module_index].get(type_name) {
        require_symbol_kind(module, type_name, symbol, SymbolKind::Data)?;
        type_name.clone_from(&symbol.mangled_name);
    } else if let Some((target, original)) = scope.selective.get(type_name) {
        let symbol = &symbols[*target][original];
        require_symbol_kind(module, original, symbol, SymbolKind::Data)?;
        type_name.clone_from(&symbol.mangled_name);
    } else if let Some(target) = scope.namespaces.get(type_name) {
        return Err(ForgeError::parse(format!(
            "{}: module '{}' cannot be used as a Data type",
            module.path.display(),
            table.modules[*target].name
        )));
    }
    Ok(())
}

fn require_symbol_kind(
    module: &ModuleSource,
    name: &str,
    symbol: &ModuleSymbol,
    expected: SymbolKind,
) -> ForgeResult<()> {
    if symbol.kind == expected {
        return Ok(());
    }
    let expected_name = match expected {
        SymbolKind::Function => "callable",
        SymbolKind::Data => "a Data declaration",
    };
    Err(ForgeError::parse(format!(
        "{}: symbol '{}' is not {}",
        module.path.display(),
        name,
        expected_name
    )))
}

fn expression_path(expression: &Expr) -> Option<Vec<String>> {
    match expression {
        Expr::Identifier(name) => Some(vec![name.clone()]),
        Expr::MemberAccess { object, member } => {
            let mut path = expression_path(object)?;
            path.push(member.clone());
            Some(path)
        }
        _ => None,
    }
}
