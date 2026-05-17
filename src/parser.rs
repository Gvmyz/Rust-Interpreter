use crate::
    {ast::{Atom, Decl, Def, Elem, Expr, IncludeDef, NominalType, Program, Spec, TypeDef, TypeExpr, TypeRhs, ValueDef}, 
    error::{PResult, ParseError}, 
    lexer::{Token, TokenKind}
};


// "id=\x:T.x
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    // Associated functions
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<TokenKind> {
        self.tokens.get(self.pos).map(|t| t.kind.clone())
    } 

    fn next(&mut self) {
        self.pos += 1;
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.peek() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_token(&mut self, kind: TokenKind) -> PResult<()> {
        match self.peek() {
            Some(token_kind) => {
                if token_kind == kind {
                    self.next();
                    Ok(())
                } else { // todo: Check error sending
                    Err(crate::error::ParseError::Msg(
                        format!("Got wrong token: {:?} instead of expected {:?}",token_kind, kind)
                    ))
                }
            },
            None => Err(crate::error::ParseError::Msg("Got no token of expected {kind:?}".to_string()))
        }
    }

    fn expect_value_name(&mut self) ->PResult<String> {
        match self.peek() {
            Some(TokenKind::Valuename(s)) => {
                self.next();
                Ok(s)
            }, 
            Some(other) => Err(ParseError::Msg(format!("expected value name, got {other:?}"))),
            None => Err(ParseError::Msg("expected value name, got EOF".into())),
        }
    }

    fn expect_type_name(&mut self) ->PResult<String> {
        match self.peek() {
            Some(TokenKind::Typename(s)) => {
                self.next();
                Ok(s)
            }, 
            Some(other) => Err(ParseError::Msg(format!("expected type name, got {other:?}"))),
            None => Err(ParseError::Msg("expected type name, got EOF".into())),
        }
    }

    pub fn run_parsing(&mut self) -> PResult<Program> {
        let mut defs: Vec<Def> = Vec::new();

        
        if self.peek().is_none() {
            return Ok(Program { defs });
        }

        defs.push(self.parse_def()?);

        while matches!(self.peek(), Some(TokenKind::Comma)) {
            self.next();
            defs.push(self.parse_def()?);
        }
        
        Ok(Program { defs })
    }

    pub fn parse_program(&mut self) -> PResult<Program> {
        let prog = self.run_parsing()?;
        self.expect_eof()?;
        Ok(prog)
    }

    pub fn parse_expr_entry(&mut self) -> PResult<Expr> {
        let expr = self.parse_vexp()?;
        self.expect_eof()?;
        Ok(expr)
    }

    fn parse_def(&mut self) -> PResult<Def> {
        match self.peek() {
            Some(TokenKind::At) => {
                Ok(Def::Include(self.parse_include_def()?))
            }
            Some(TokenKind::Valuename(_)) => {
                Ok(Def::Value(self.parse_vdef()?))
            },
            Some(TokenKind::Typename(_)) => {
                Ok(Def::Type(self.parse_tdef()?))
            },
            Some(other) => Err(ParseError::Msg(format!("Error while parsing def, got: {other:?}"))),
            None => Err(ParseError::Msg("Got nothing while parsing def".into()))
        }
    }

    fn take_include_path_token(&mut self) -> PResult<String> {
        match self.peek() {
            Some(TokenKind::Valuename(s)) => { self.next(); Ok(s) }
            Some(TokenKind::Typename(s)) => { self.next(); Ok(s) }
            Some(TokenKind::Dot) => { self.next(); Ok(".".into()) }
            _ => Err(ParseError::Msg("expected include path segment".into())),
        }
    }

    fn parse_include_def(&mut self) -> PResult<IncludeDef> {
        self.expect_token(TokenKind::At)?;

        let mut file_name = String::new();

        // Collect tokens until ',' or EOF
        while let Some(k) = self.peek() {
            if matches!(k, TokenKind::Comma) {
                break;
            }
            // stop if something that clearly can't be part of a path appears
            // (optional: keep only what you want)
            let seg = self.take_include_path_token()?;
            file_name.push_str(&seg);
        }

        if file_name.is_empty() {
            return Err(ParseError::Msg("empty include path after '@'".into()));
        }

        Ok(IncludeDef { file_name })
    }

    fn parse_vdef(&mut self) -> PResult<ValueDef> {
        let name = self.expect_value_name()?;
        
        // Consume "="
        self.expect_token(TokenKind::Equal)?;

        let expr = self.parse_vexp()?;

        Ok(ValueDef { name, expr: Box::new(expr) })
    }

    fn parse_tdef(&mut self) -> PResult<TypeDef> {
        let name = self.expect_type_name()?;

        // Consume ":"
        self.expect_token(TokenKind::Equal)?;

        let rhs = match self.peek() {
            Some(TokenKind::LBrace) | Some(TokenKind::LBracket) => {
                TypeRhs::Nominal(self.parse_tnom()?)
            },
            _ => TypeRhs::Structural(self.parse_texp()?)
        };

        Ok(TypeDef { name, rhs })
    }


    fn parse_texp(&mut self) -> PResult<TypeExpr> {
        let left = self.parse_tval()?;

        if self.eat(TokenKind::Arrow) {
            let right = self.parse_texp()?;
            return Ok(TypeExpr::Arrow(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_tval(&mut self) -> PResult<TypeExpr> {
        match self.peek() {
            Some(TokenKind::Typename(_)) => {
                Ok(TypeExpr::Named(self.expect_type_name()?))
            },
            Some(TokenKind::LParen) => {
                self.expect_token(TokenKind::LParen)?;
                let first = self.parse_texp()?;
                // Tuple type only if there is a comma (len >= 2)
                if self.eat(TokenKind::Comma) {
                    let mut items = vec![first];
                    items.push(self.parse_texp()?);

                    while self.eat(TokenKind::Comma) {
                        items.push(self.parse_texp()?);
                    }

                    self.expect_token(TokenKind::RParen)?;
                    Ok(TypeExpr::Tuple(items))
                } else {
                    self.expect_token(TokenKind::RParen)?;
                    Ok(first) // parentheses
                }
            },
            Some(other) => Err(ParseError::Msg(format!("expected type value, got {other:?}"))),
        None => Err(ParseError::Msg("expected type value, got EOF".into())),
        }
    }

    fn parse_tnom(&mut self) -> PResult<NominalType> {
        match self.peek() {
            Some(TokenKind::LBrace) => {
                self.expect_token(TokenKind::LBrace)?;
                let first = self.parse_decl()?;
                let mut decls = vec![first];
                while self.eat(TokenKind::Comma) {
                    decls.push(self.parse_decl()?);
                }
                self.expect_token(TokenKind::RBrace)?;
                Ok(NominalType::StructType(decls))
            },
            Some(TokenKind::LBracket) => {
                self.expect_token(TokenKind::LBracket)?;
                let first = self.parse_elem()?;
                let mut elems = vec![first];
                while self.eat(TokenKind::Comma) {
                    elems.push(self.parse_elem()?);
                }
                self.expect_token(TokenKind::RBracket)?;
                Ok(NominalType::UnionType(elems))
            },
            Some(other) => Err(ParseError::Msg(format!("expected nominal type, got {other:?}"))),
            None => Err(ParseError::Msg("expected nominal type, got EOF".into())),
        }
    }

    fn parse_decl(&mut self) -> PResult<Decl> {
        let name = self.expect_value_name()?;
        self.expect_token(TokenKind::Colon)?;
        let ty = self.parse_texp()?;

        Ok(Decl { name, ty })
    }

    fn parse_elem(&mut self) -> PResult<Elem> {
        let name = self.expect_value_name()?;
        let elem_type = if self.eat(TokenKind::Colon) {
            Some(self.parse_texp()?)
        } else {
            None
        };
        
        Ok(Elem { name, elem_type })
    }


    fn starts_fval(k: &TokenKind) -> bool {
        matches!(k, 
            TokenKind::Backslash
            | TokenKind::LParen
            | TokenKind::Valuename(_)
            | TokenKind::Typename(_)
        )
    }

    fn parse_vexp(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_fval()?;

        while let Some(k) = self.peek() {
            if !Self::starts_fval(&k) {
                break;
            }
            let arg = self.parse_fval()?;
            expr = Expr::Application { fun: Box::new(expr), arg: Box::new(arg) }
        }

        Ok(expr)
    }


    fn parse_fval(&mut self) -> PResult<Expr> {
        match self.peek() {
            Some(TokenKind::Backslash) => self.parse_lambda(),
            _ => self.parse_val_expr()
        }
    }

    fn parse_lambda(&mut self) -> PResult<Expr> {
        self.expect_token(TokenKind::Backslash)?;
        let param = self.parse_decl()?;
        self.expect_token(TokenKind::Dot)?;
        let expr = self.parse_vexp()?;
        Ok(Expr::Lambda { param, body: Box::new(expr) })
    }

    fn parse_val_expr(&mut self) -> PResult<Expr> {
        Ok(Expr::Atom(self.parse_val()?))
    }

    fn parse_val(&mut self) -> PResult<Atom> {
        let mut atom = match self.peek() {
            Some(TokenKind::Valuename(_)) => Atom::Var(self.expect_value_name()?),
            Some(TokenKind::LParen) => self.parse_tuple_or_paren()?,
            Some(TokenKind::Typename(_)) => {
                let ty_name = self.expect_type_name()?;

                let spec = self.parse_spec()?; // expects '[' or '{'
                Atom::Typed { ty_name, spec }
            },
            Some(other) => return Err(ParseError::Msg(format!("expected value, got {other:?}"))),
            None => return Err(ParseError::Msg("expected value, got EOF".into())),
        };
        
        loop {
            match self.peek() {
                Some(TokenKind::Dot) => {
                    self.expect_token(TokenKind::Dot)?;
                    let field = self.expect_value_name()?;
                    atom = Atom::Access { base: Box::new(atom), field }
                },
                Some(TokenKind::LBracket) => {
                    atom = self.parse_case_postfix(atom)?;
                },
                _ => break,
            }
        }
        Ok(atom)
    }

    fn parse_tuple_or_paren(&mut self) -> PResult<Atom> {
        self.expect_token(TokenKind::LParen)?;
        let first = self.parse_vexp()?;

        // If there is a comma, it's a tuple of length >= 2
        if self.eat(TokenKind::Comma) {
            let mut items = vec![first];
            items.push(self.parse_vexp()?);

            while self.eat(TokenKind::Comma) {
                items.push(self.parse_vexp()?);
            }

            self.expect_token(TokenKind::RParen)?;
            Ok(Atom::Tuple(items))
        } else {
            // No comma => parentheses
            self.expect_token(TokenKind::RParen)?;
            Ok(Atom::Paren(Box::new(first)))
        }
    }

    fn parse_case_postfix(&mut self, scrutinee: Atom) -> PResult<Atom> {
        self.expect_token(TokenKind::LBracket)?;

        let mut branches = Vec::new();
        branches.push(self.parse_vdef()?);
        while self.eat(TokenKind::Comma) {
            branches.push(self.parse_vdef()?);
        }

        let default = if self.eat(TokenKind::Pipe) {
            Some(Box::new(self.parse_vexp()?))
        } else {
            None
        };

        self.expect_token(TokenKind::RBracket)?;
        
        Ok(Atom::Case { 
            scrutinee: Box::new(scrutinee), 
            branches, 
            default 
        })
    }

    fn peek_next(&mut self) -> Option<TokenKind> {
        self.tokens.get(self.pos +1).map(|t| t.kind.clone())
    }

    fn parse_spec(&mut self) -> PResult<Spec> {
        match self.peek() {
            Some(TokenKind::LBracket) => {
                self.expect_token(TokenKind::LBracket)?;

                // Decide between vdef vs vname by looking ahead for '='
                // As value starting both Unions
                let spec = match (self.peek(), self.peek_next()) {
                    (Some(TokenKind::Valuename(_)), Some(TokenKind::Equal)) => {
                        let vdef = self.parse_vdef()?;
                        Spec::UnionField(vdef)
                    },
                    (Some(TokenKind::Valuename(_)), _) => {
                        let label = self.expect_value_name()?;
                        Spec::UnionLabel(label)
                    },
                    (Some(other), _) => return Err(ParseError::Msg(format!("expected union label/field, got {other:?}"))),
                    (None, _) => return Err(ParseError::Msg("expected union label/field, got EOF".into())),
                };

                self.expect_token(TokenKind::RBracket)?;
                Ok(spec)

            }, 
            Some(TokenKind::LBrace) => {

                self.expect_token(TokenKind::LBrace)?;

                let mut fields = Vec::new();

                if matches!(self.peek(), Some(TokenKind::Valuename(_))) {
                    fields.push(self.parse_vdef()?);
                    while self.eat(TokenKind::Comma) {
                        fields.push(self.parse_vdef()?);
                    }
                }

                self.expect_token(TokenKind::RBrace)?;
                Ok(Spec::StructFields(fields))
            },
            Some(other) => Err(ParseError::Msg(format!("expected spec '[' or '{{', got {other:?}"))),
            None => Err(ParseError::Msg("expected spec, got EOF".into())),
        }
    }

    fn expect_eof(&self) -> PResult<()> {
        if self.peek().is_none() {
            Ok(())
        } else {
            Err(ParseError::Msg(format!("unexpected trailing tokens: {:?}", self.peek())))
        }
    }
}