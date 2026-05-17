
use std::fmt::{Debug};
use std::fmt;

use crate::error::TiflError;

type LexResult<T> = std::result::Result<T, TiflError>;

/*
todo: 
- Manage both type of quotes `"` and `'`
*/

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Comma,
    At,
    Equal,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Arrow,
    LParen,
    RParen,
    Backslash,
    Dot,
    Pipe,

    Typename(String),
    Valuename(String),
}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0 }
    }

    /* Accessing byte values */ 

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_next_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let res = self.peek_byte()?;
        self.pos += 1;
        Some(res)
    }


    /* Returning  */

    fn single(&mut self, kind: TokenKind) -> Token {
        let start = self.pos;
        self.pos += 1;
        Token {
            kind,
            span: Span::new(start, self.pos)
        }
    }

    fn double(&mut self, kind: TokenKind) -> Token {
        let start = self.pos;
        self.pos += 2;
        Token {
            kind,
            span: Span::new(start, self.pos)
        }
    }


    /* Helpers */

    fn is_delimiter(byte: u8) -> bool {
        matches!(
            byte,
            b'@' | b',' | b'=' | b':' | b'.' | b'\\' | b'(' | b')' | b'{' | b'}' | b'[' | b']'
                | b'|' | b'\'' | b'"' | b' ' | b'\t' | b'\r' | b'\n' 
        )
    }

    fn extract_name(&mut self) -> Token {
        let start = self.pos;

        while let Some(b) = self.peek_byte() {
            if b == b'-' && self.peek_next_byte() == Some(b'>') {
                break;
            }
            if Self::is_delimiter(b) {
                break
            }
            self.pos += 1;
        }

        let end = self.pos;
        let text = &self.src[start..end];

        let kind = match text.as_bytes().first() {
            Some(b) if (b'A'..b'Z').contains(b) => TokenKind::Typename(text.to_string()),
            _ => TokenKind::Valuename(text.to_string())
        };

        Token { 
            kind, 
            span: Span::new(start, self.pos), 
        }
    }

    // todo: Separate ' and "
    fn lex_quotes(&mut self, quote: u8) -> LexResult<Token> {

        let start = self.pos;

        // Consume Quote
        self.bump();

        while let Some(b) = self.peek_byte() {
            self.pos += 1;
            if b == quote {
                let end = self.pos;

                let text = &self.src[start..end];
                
                return Ok(Token {
                    kind: TokenKind::Valuename(text.to_string()),
                    span: Span::new(start, end),
                });
            }

            // todo: Erroring out on \n for now
            if b == b'\n' {
                return Err(TiflError::LexError { 
                    message: "unterminated quoted literal".to_string(),
                    span: Span::new(start, self.pos),
                });
            }
        }

        return Err(TiflError::LexError { 
            message: "unterminated quoted literal".to_string(),
            span: Span::new(start, self.pos),
        });
        
    }

    fn skip_whitespaces_and_comments(&mut self) {
        loop {
            while matches!(self.peek_byte(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
                self.pos += 1;
            }

            if self.peek_byte() == Some(b'/') && self.peek_next_byte() == Some(b'/') {
                self.pos += 2;
                while let Some(b) = self.peek_byte() {
                    self.pos += 1;
                    if b == b'\n' {
                        break;
                    }
                }
                // In case there is whitespace after the comment
                continue; 
            }

            break;
        }
    }
}



impl<'a> Iterator for Lexer<'a> {
    type Item = LexResult<Token>;

    fn next(&mut self) -> Option<Self::Item> {


        self.skip_whitespaces_and_comments();

        let start = self.pos;

        let b = match self.peek_byte() {
            Some(b) => b,

            // Possibly use TokenKind::EOF and a done variable
            None => return None
        };

        // Manage "->" 
        // todo: Take care of case ValueName("-") at beginning of src
        if b == b'-' && self.peek_next_byte() == Some(b'>') {
            return Some(Ok(self.double(TokenKind::Arrow)))
        }

        let token = match b {
            b'@' => Ok(self.single(TokenKind::At)),
            b',' => Ok(self.single(TokenKind::Comma)),
            b'=' => Ok(self.single(TokenKind::Equal)),
            b':' => Ok(self.single(TokenKind::Colon)),
            b'.' => Ok(self.single(TokenKind::Dot)),
            b'\\' => Ok(self.single(TokenKind::Backslash)),
            b'(' => Ok(self.single(TokenKind::LParen)),
            b')' => Ok(self.single(TokenKind::RParen)),
            b'{' => Ok(self.single(TokenKind::LBrace)),
            b'}' => Ok(self.single(TokenKind::RBrace)),
            b'[' => Ok(self.single(TokenKind::LBracket)),
            b']' => Ok(self.single(TokenKind::RBracket)),
            b'|' => Ok(self.single(TokenKind::Pipe)),
            b'\'' | b'"' => self.lex_quotes(b),
            _ => {
                if Self::is_delimiter(b) {
                    Err(TiflError::LexError {
                        message: format!("unexpected character '{}'", b as char),
                        span: Span::new(start, start + 1),
                    })
                } else {
                    Ok(self.extract_name())
                }
            }
        };

        Some(token)
    }
}