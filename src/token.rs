use logos::{Lexer, Logos};
use memchr::memchr2;

use crate::LeafValue;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\r\n]+")] // Ignore this regex pattern between tokens
pub(crate) enum Token {
    #[token("{")]
    OpenObject,
    #[token("[")]
    OpenArray,

    #[token("}")]
    CloseObject,
    #[token("]")]
    CloseArray,

    #[token(":")]
    Colon,

    #[token(",")]
    Comma,

    #[token("false", |_| LeafValue::False)]
    #[token("true", |_| LeafValue::True)]
    #[token("null", |_| LeafValue::Null)]
    #[regex(r"[-\d][\deE+\-\.]*", |_| LeafValue::Number)]
    #[regex("\"", lex_string)]
    Leaf(LeafValue),
}

fn lex_string(lexer: &mut Lexer<Token>) -> Result<LeafValue, ()> {
    let s = lexer.remainder();

    let mut i = 0;
    loop {
        let Some(b) = s.as_bytes().get(i..) else {
            break Err(());
        };
        match memchr2(b'\\', b'"', b) {
            Some(j) => {
                if b[j] == b'\\' {
                    i += j + 2;
                } else {
                    i += j + 1;
                    lexer.bump(i);
                    break Ok(LeafValue::String);
                }
            }
            None => break Err(()),
        }
    }
}
