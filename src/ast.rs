// Sydrogen AST - Rust port of furnace/ast_nodes.py

#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    Open,
    Closed,
    Showcase,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Subtype {
    Int,
    Float,
    Generic,
    Weld,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeDecl {
    Number(Subtype),
    Weld,
    Bool,
    Ore(Option<i64>),
    OreTuple(Vec<(Subtype, String)>),
    Materials(Subtype, bool /* New */),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeCategory {
    Number,
}

impl TypeDecl {
    pub fn category(&self) -> Option<TypeCategory> {
        match self {
            TypeDecl::Number(_) => Some(TypeCategory::Number),
            _ => None,
        }
    }

    pub fn is_number(&self) -> bool {
        self.category() == Some(TypeCategory::Number)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RetKind {
    Int,
    Float,
    Generic,
    Weld,
    Bool,
    Ore(Option<i64>),
    OreTuple(Vec<(Subtype, String)>),
    Materials(Subtype, bool),
    Function, // lowercase "function" = dynamic
    Nunction, // void
    Dynamic,
    Void,
}

impl RetKind {
    pub fn is_void(&self) -> bool {
        matches!(self, RetKind::Nunction | RetKind::Void)
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, RetKind::Function | RetKind::Dynamic)
    }

    pub fn from_type_decl(type_decl: &TypeDecl) -> Self {
        match type_decl {
            TypeDecl::Number(subtype) => match subtype {
                Subtype::Int => RetKind::Int,
                Subtype::Float => RetKind::Float,
                Subtype::Generic => RetKind::Generic,
                Subtype::Weld => RetKind::Weld,
            },
            TypeDecl::Weld => RetKind::Weld,
            TypeDecl::Bool => RetKind::Bool,
            TypeDecl::Ore(size) => RetKind::Ore(*size),
            TypeDecl::OreTuple(fields) => RetKind::OreTuple(fields.clone()),
            TypeDecl::Materials(elem_type, has_new) => {
                RetKind::Materials(elem_type.clone(), *has_new)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    VarDecl(VarDecl),
    DataDecl(DataDecl),
    ObjectDecl(ObjectDecl),
    FunctionDecl(FunctionDecl),
    Print(PrintNode),
    Input(InputNode),
    If(IfNode),
    While(WhileNode),
    For(ForNode),
    ForEach(ForEachNode),
    Do(DoNode),
    Throw(ErrorValue),
    Return(Option<Expr>),
    Stop,
    Skip,
    Assignment(AssignmentNode),
    Use(UseNode),
    Using(UsingNode),
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ErrorType {
    Error,
    InvalidValue,
    DivisionByZero,
    Overflow,
    IO,
    Unknown(String),
}

impl ErrorType {
    pub fn name(&self) -> &str {
        match self {
            Self::Error => "Error",
            Self::InvalidValue => "Error.InvalidValue",
            Self::DivisionByZero => "Error.DivisionByZero",
            Self::Overflow => "Error.Overflow",
            Self::IO => "Error.IO",
            Self::Unknown(name) => name,
        }
    }

    pub fn code(&self) -> i64 {
        match self {
            Self::Error => 1,
            Self::InvalidValue => 2,
            Self::DivisionByZero => 3,
            Self::Overflow => 4,
            Self::IO => 5,
            Self::Unknown(_) => 0,
        }
    }

    pub fn is_builtin(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

#[derive(Debug, Clone)]
pub struct ErrorValue {
    pub error_type: ErrorType,
    pub message: Expr,
}

#[derive(Debug, Clone)]
pub struct CatchNode {
    pub error_type: ErrorType,
    pub binding: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct DoNode {
    pub body: Vec<Statement>,
    pub catches: Vec<CatchNode>,
    pub finally_body: Option<Vec<Statement>>,
}

/// Increment/decrement direction for For loop headers.
#[derive(Debug, Clone, PartialEq)]
pub enum IncrOp {
    Inc,
    Dec,
}

/// For loop: `For (init; condition; increment) { body }`
#[derive(Debug, Clone)]
pub struct ForNode {
    pub init: VarDecl,
    pub condition: Expr,
    pub increment_var: String,
    pub increment_op: IncrOp,
    pub body: Vec<Statement>,
}

/// Collection loop: `ForEach (type item in collection) { body }`
#[derive(Debug, Clone)]
pub struct ForEachNode {
    pub item_type: TypeDecl,
    pub item_name: String,
    pub collection_name: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub modifier: Option<Modifier>,
    pub type_decl: TypeDecl,
    pub name: String,
    pub initializer: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct DataDecl {
    pub modifier: Option<Modifier>,
    pub name: String,
    pub members: Vec<VarDecl>,
}

#[derive(Debug, Clone)]
pub struct ObjectDecl {
    pub type_name: String,
    pub name: String,
    pub inits: Vec<(Vec<String>, Expr)>, // member path + value
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub modifier: Option<Modifier>,
    pub ret_kind: RetKind,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub type_decl: TypeDecl,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PrintNode {
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Interp(String),
}

#[derive(Debug, Clone)]
pub struct InputNode {
    pub subtype: Option<Subtype>,
}

#[derive(Debug, Clone)]
pub struct IfNode {
    pub branches: Vec<(Expr, Vec<Statement>)>,
    pub else_body: Option<Vec<Statement>>,
}

#[derive(Debug, Clone)]
pub struct WhileNode {
    pub condition: Expr,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum AssignmentTarget {
    Var(String),
    Member {
        object: Box<AssignmentTarget>,
        member: String,
    },
    Index {
        object: Box<AssignmentTarget>,
        index: Expr,
    },
}

#[derive(Debug, Clone)]
pub struct AssignmentNode {
    pub target: AssignmentTarget,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseNode {
    pub path: Vec<String>,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsingNode {
    pub path: Vec<String>,
    pub symbol: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(NumberLiteral),
    Str(Vec<StringPart>),
    Bool(bool),
    Identifier(String),
    MemberAccess {
        object: Box<Expr>,
        member: String,
    },
    IndexAccess {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    BinaryOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    UnaryOp {
        op: UnOp,
        operand: Box<Expr>,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    NamespaceCall {
        namespace: String,
        method: String,
        args: Vec<Expr>,
    },
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    Input(InputNode),
    ArrayLiteral(Vec<Expr>),
    TupleLiteral(Vec<Expr>),
    ListLiteral(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NumberLiteral {
    pub int_val: i64,
    pub float_val: f64,
    pub is_float: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnOp {
    Plus,
    Neg,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub statements: Vec<Statement>,
}
