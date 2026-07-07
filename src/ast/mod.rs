/// Top-level output of the parser: an ordered list of top-level items.
#[derive(Debug, Default)]
pub struct Ast {
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Function(FunctionDef),
}

#[derive(Debug)]
pub struct FunctionDef {
    pub name: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug)]
pub enum Stmt {
    Expr(Expr),
}

#[derive(Debug)]
pub enum Expr {
    Integer(i64),
    Ident(String),
}

/// Ofan type representation (to be expanded with the type system).
#[derive(Debug)]
pub enum Type {
    Int,
    Float,
    Bool,
    Unit,
}
