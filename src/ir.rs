pub type VReg = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Reg(VReg),
    Const(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintKind {
    Int,
    Bool,
    Str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Int,
    Float,
    Str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrInst {
    Const {
        dest: VReg,
        value: i64,
    },
    ConstStr {
        dest: VReg,
        value: String,
    },
    Input {
        dest: VReg,
        kind: InputKind,
    },
    Copy {
        dest: VReg,
        src: Operand,
    },
    BinOp {
        dest: VReg,
        op: IrBinOp,
        lhs: Operand,
        rhs: Operand,
    },
    Call {
        dest: VReg,
        func: String,
        args: Vec<Operand>,
    },
    Param {
        dest: VReg,
        index: usize,
    },
    LoadVar {
        dest: VReg,
        var_name: String,
    },
    StoreVar {
        var_name: String,
        src: Operand,
    },
    Print {
        src: Operand,
        kind: PrintKind,
    },
    PrintInline {
        src: Operand,
        kind: PrintKind,
    },
    PrintConstStr {
        str_val: String,
    },
    SysExit {
        status: Operand,
    },
    SetError {
        code: Operand,
        message: Operand,
        type_name: Operand,
    },
    ClearError,
    GetErrorCode {
        dest: VReg,
    },
    GetErrorMessage {
        dest: VReg,
    },
    GetErrorTypeName {
        dest: VReg,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrTerminator {
    Ret(Option<Operand>),
    Jump(String),
    Branch {
        cond: Operand,
        true_label: String,
        false_label: String,
    },
    PropagateError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub label: String,
    pub insts: Vec<IrInst>,
    pub terminator: IrTerminator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrFunction {
    pub name: String,
    pub param_names: Vec<String>,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
}
