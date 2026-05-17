use miette::Diagnostic;
use thiserror::Error;

use crate::{lexer::Span, types::Type};

#[derive(Error, Debug, Diagnostic)]
pub enum TiflError {
    #[error("not implemented")]
    #[diagnostic(code(tifl::not_implemented))]
    NotImplemented,

    #[error("lexer error:  {message}")]
    #[diagnostic(code(tifl::lex_error))]
    LexError { message: String, span: Span },
}

#[derive(Debug, Error, Diagnostic)]
pub enum ParseError {
    #[error("parse error: {0}")]
    Msg(String),
}

pub type PResult<T> = std::result::Result<T, ParseError>;


#[derive(Debug, Clone, thiserror::Error, Diagnostic)]
pub enum EvalError {
    #[error("unbound variable '{0}'")]
    UnboundVar(String),

    #[error("attempted to apply non-function value")]
    NotAFunction,

    #[error("tuple index out of bounds: .{index} on len {len}")]
    TupleIndexOob { index: usize, len: usize },

    #[error("field '{field}' not found on struct")]
    UnknownField { field: String },

    #[error("case on non-union value")]
    CaseOnNonUnion,

    #[error("missing case branch for label '{0}' and no default")]
    MissingCaseBranch(String),

    #[error("union ctor: label '{label}' expects payload")]
    MissingPayload { label: String },

    #[error("union ctor: label '{label}' expects no payload")]
    UnexpectedPayload { label: String },
    
    #[error("type error at runtime: expected {expected:?}, got {got:?}")]
    RuntimeTypeMismatch { expected: Type, got: Type },

    #[error("unknown type '{0}'")]
    UnknownType(String),

    #[error("typed ctor: '{0}' is not a struct or union type")]
    NotNominalType(String),

    #[error("struct ctor: unknown field '{field}' for '{ty}'")]
    UnknownStructInitField { ty: String, field: String },

    #[error("struct ctor: missing field '{field}' for '{ty}'")]
    MissingStructField { ty: String, field: String },

    #[error("struct ctor: duplicate field '{field}' for '{ty}'")]
    DuplicateStructField { ty: String, field: String },

    #[error("union ctor: unknown label '{label}' for '{ty}'")]
    UnknownUnionLabel { ty: String, label: String },

    #[error("case default used but scrutinee has payload and default is not a function")]
    DefaultNotAFunction,

    #[error("case branches disagree on result type: expected {expected:?}, got {got:?}")]
    CaseResultMismatch { expected: Type, got: Type },

    #[error("case branch '{label}' not in union '{ty}'")]
    CaseUnknownLabel { ty: String, label: String },

    #[error("duplicate case branch for label '{0}'")]
    DuplicateCaseBranch(String),

    #[error("non-exhaustive case on '{ty}', missing: {missing:?}")]
    NonExhaustiveCase { ty: String, missing: Vec<String> },

    #[error("case branch for label '{label}' must be a function taking {expected_arg:?}")]
    CaseBranchMustBeFunction { label: String, expected_arg: Type },

    #[error("default clause is not compatible with remaining labels")]
    BadDefaultClause,
}



#[derive(Debug, Error, Diagnostic)]
pub enum EnvBuildError {
    #[error("duplicate type definition '{name}' at program scope")]
    #[diagnostic(code(tifl::env::duplicate_type))]
    DuplicateType { name: String },

    #[error("duplicate value definition '{name}' at program scope")]
    #[diagnostic(code(tifl::env::duplicate_value))]
    DuplicateValue { name: String },
}


#[derive(Debug, thiserror::Error, Diagnostic)]
pub enum TypeLowerError {
    #[error("Unknown type name: {0}")]
    UnknownType(String),

    #[error("duplicate field '{field}' in struct type '{ty}'")]
    DuplicateStructField {ty: String, field: String},

    #[error("duplicate vairant '{variant}' in union type '{ty}'")]
    DuplicateUnionVariant { ty: String, variant: String },

    #[error("unguarded recursive union type '{ty}' (no recursion-free alternative)")]
    UnguardedRecursiveUnion { ty: String },
}