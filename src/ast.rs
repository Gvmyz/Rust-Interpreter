
// Create Structures EBNF


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub defs: Vec<Def>
}

/* ===== DEF ===== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Def {
    Include(IncludeDef),
    Type(TypeDef),
    Value(ValueDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeDef {
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueDef {
    pub name: String,
    pub expr: Box<Expr>,
}

/* ===== TypeDef ===== */

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    pub name: String,
    pub rhs: TypeRhs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRhs {
    Nominal(NominalType),
    Structural(TypeExpr)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NominalType {
    StructType(Vec<Decl>),
    UnionType(Vec<Elem>),
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    pub name: String,
    pub ty: TypeExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elem {
    pub name: String,
    pub elem_type: Option<TypeExpr>,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Named(String),
    Arrow(Box<TypeExpr>, Box<TypeExpr>),
    Tuple(Vec<TypeExpr>)
}



#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Lambda {
        param: Decl,
        body: Box<Expr>,
    },

    Application {
        fun: Box<Expr>,
        arg: Box<Expr>,
    },

    Atom(Atom)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    Var(String),

    // "(" <vexp> {"," <vexp>} ")"
    Tuple(Vec<Expr>),

    // "(" <vexp> {"," <vexp>} ")" when there is only ONE element inside paren
    Paren(Box<Expr>),

    // <val> "." <vname>
    Access {
        base: Box<Atom>,
        field: String,
    },

    // <val> "[" <vdef> {"," <vdef>} ["|" <vexp>] "]"
    // Note: inside case, branches are <vdef> (name = <vexp>)
    Case {
        scrutinee: Box<Atom>,
        branches: Vec<ValueDef>,
        default: Option<Box<Expr>>,
    },

    // <tname> <spec>
    // This is used to create/take value of type; spec disambiguates struct/union.
    Typed {
        ty_name: String,
        spec: Spec,
    },
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    // "[" (<vdef> | <vname>) "]"
    // - X[a]   (label-only)
    // - X[x=b] (label + payload)
    UnionLabel(String),
    UnionField(ValueDef),

    // "{" [<vdef> {"," <vdef>}] "}"
    // Struct{a=x, b=y}
    StructFields(Vec<ValueDef>),
}