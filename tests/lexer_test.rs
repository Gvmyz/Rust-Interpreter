use tifl_interpreter::lexer::{Lexer, Token, TokenKind, Span};
use tifl_interpreter::error::TiflError;

fn lex_kinds_ok(src: &str) -> Vec<TokenKind> {
    Lexer::new(src)
        .map(|r| r.expect("lexer returned Err"))
        .map(|t| t.kind)
        .collect()
}

fn assert_lex_kinds(src: &str, expected: Vec<TokenKind>) {
    let got = lex_kinds_ok(src);
    assert_eq!(
        got, expected,
        "\nSRC:\n{src}\n\nGOT:\n{got:#?}\n\nEXPECTED:\n{expected:#?}\n"
    );
}

fn tn(s: &str) -> TokenKind { TokenKind::Typename(s.to_string()) }
fn vn(s: &str) -> TokenKind { TokenKind::Valuename(s.to_string()) }


fn assert_some_spans(src: &str, anchors: &[(usize, TokenKind, Span)]) {
    let toks: Vec<Token> = Lexer::new(src).map(|r| r.expect("lexer Err")).collect();
    for (idx, kind, span) in anchors {
        assert_eq!(&toks[*idx].kind, kind, "token[{idx}] kind mismatch");
        assert_eq!(&toks[*idx].span, span, "token[{idx}] span mismatch");
    }
}

//
// 1) FULL-SURFACE SMOKE TEST:
// punctuation + arrow + whitespace skipping + comments skipping
//
#[test]
fn lex_surface_tokens_whitespace_and_comments() {
    use TokenKind::*;
    let src = r#"
        // comment should be skipped
        A -> B  =  { x : C , y : (D -> E) }  // trailing comment
        ()[]{}:,.|\  .
    "#;

    assert_lex_kinds(
        src,
        vec![
            tn("A"), Arrow, tn("B"), Equal,
            LBrace,
                vn("x"), Colon, tn("C"), Comma,
                vn("y"), Colon, LParen, tn("D"), Arrow, tn("E"), RParen,
            RBrace,
            // the last line:
            LParen, RParen,
            LBracket, RBracket,
            LBrace, RBrace,
            Colon, Comma, Dot, Pipe, Backslash, Dot,
        ],
    );

    // Checks Arrow spans are length 2 and that we didn't accidentally include whitespace.
    assert_some_spans(
        "@p, A->B",
        &[
            (0, TokenKind::At, Span { start: 0, end: 1 }),
            (1, vn("p"), Span { start: 1, end: 2 }),
            (2, TokenKind::Comma, Span { start: 2, end: 3 }),
            (3, tn("A"), Span { start: 4, end: 5 }),
            (4, TokenKind::Arrow, Span { start: 5, end: 7 }),
            (5, tn("B"), Span { start: 7, end: 8 }),
        ],
    );
}

//
// 2) BIG “PROGRAM-FRAGMENT” TEST (covers most grammar constructs):
// - type defs (struct, union, tuple, arrows)
// - value defs (lambda, application)
// - struct construction + dot access
// - union construction
// - case with default (|)
// - numeric access after dot
// - symbolic value names (+, <) and quoted literals ("", "xx", 'a')
// Notes: Your lexer currently returns quoted literals INCLUDING quotes as Valuename.
//
#[test]
fn lex_big_program_fragment_including_quotes() {
    use TokenKind::*;
    let src = r#"
        // types
        T = { x:A, y:(B->C) },
        U = [nil, elem:(T,U)],
        Pair = (A,B),
        F = A->B->C,

        // values + literals
        empty = "", s = "xx", c = 'a',
        id = \x:T.x,
        add = \x:Int.\y:Int.+ x y,

        // struct + dot access
        st = T{x=a,y=b}, st.x,

        // union construction + case/default
        u = U[elem=(st,u)],
        u[x=\z:A.z, nil=empty | s],

        // tuple and numeric access
        (a,b).1
    "#;

    assert_lex_kinds(
        src,
        vec![
            // T = { x:A, y:(B->C) },
            tn("T"), Equal, LBrace,
                vn("x"), Colon, tn("A"), Comma,
                vn("y"), Colon, LParen, tn("B"), Arrow, tn("C"), RParen,
            RBrace, Comma,

            // U = [nil, elem:(T,U)],
            tn("U"), Equal, LBracket,
                vn("nil"), Comma,
                vn("elem"), Colon, LParen, tn("T"), Comma, tn("U"), RParen,
            RBracket, Comma,

            // Pair = (A,B),
            tn("Pair"), Equal, LParen, tn("A"), Comma, tn("B"), RParen, Comma,

            // F = A->B->C,
            tn("F"), Equal, tn("A"), Arrow, tn("B"), Arrow, tn("C"), Comma,

            // empty = "", s="xx", c='a',
            vn("empty"), Equal, vn(r#""""#), Comma,
            vn("s"), Equal, vn(r#""xx""#), Comma,
            vn("c"), Equal, vn("'a'"), Comma,

            // id = \x:T.x,
            vn("id"), Equal, Backslash, vn("x"), Colon, tn("T"), Dot, vn("x"), Comma,

            // add = \x:Int.\y:Int.+ x y,
            vn("add"), Equal,
                Backslash, vn("x"), Colon, tn("Int"), Dot,
                Backslash, vn("y"), Colon, tn("Int"), Dot,
                vn("+"), vn("x"), vn("y"),
            Comma,

            // st = T{x=a,y=b}, st.x,
            vn("st"), Equal, tn("T"), LBrace,
                vn("x"), Equal, vn("a"), Comma,
                vn("y"), Equal, vn("b"),
            RBrace, Comma,
            vn("st"), Dot, vn("x"), Comma,

            // u = U[elem=(st,u)],
            vn("u"), Equal, tn("U"), LBracket,
                vn("elem"), Equal, LParen, vn("st"), Comma, vn("u"), RParen,
            RBracket, Comma,

            // u[x=\z:A.z, nil=empty | s],
            vn("u"), LBracket,
                vn("x"), Equal, Backslash, vn("z"), Colon, tn("A"), Dot, vn("z"), Comma,
                vn("nil"), Equal, vn("empty"),
                Pipe,
                vn("s"),
            RBracket, Comma,

            // (a,b).1
            LParen, vn("a"), Comma, vn("b"), RParen, Dot, vn("1"),
        ],
    );
}

//
// 3) QUOTE ERROR TEST (unterminated)
// This is the most important negative test now that you return LexError.
//
#[test]
fn lex_unterminated_quote_errors() {
    let mut it = Lexer::new(r#""xx"#);
    let err = it.next().expect("should yield one item").expect_err("expected Err");
    match err {
        TiflError::LexError { message, span } => {
            assert!(message.contains("unterminated quoted literal"));
            assert_eq!(span.start, 0);
            assert!(span.end >= 1);
        }
        other => panic!("expected LexError, got {other:?}"),
    }
}


#[test]
fn lex_symbolic_value_names() {

    let src = r#"< = \x:Int.\y:Int.x"#;
    let toks: Vec<_> = Lexer::new(src).map(|r| r.unwrap().kind).collect();

    assert!(matches!(&toks[0], TokenKind::Valuename(s) if s == "<"));
    assert!(matches!(&toks[1], TokenKind::Equal));
}