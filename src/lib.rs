pub const VERSION: &str = "Alpha-7";

pub mod ast;
pub mod backend;
pub mod codegen;
pub mod errors;
pub mod imports;
pub mod ir;
pub mod lowering;
pub mod parser;
pub mod project;
pub mod semantic;
