#![no_std]

#[macro_use(vec)]
extern crate alloc;

#[cfg(test)]
#[macro_use(dbg)]
extern crate std;

use alloc::vec::Vec;
use core::ops::{Range, RangeFrom};

use logos::Logos;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\r\n]+")] // Ignore this regex pattern between tokens
enum Token {
    #[token("{")]
    BraceOpen,

    #[token("}")]
    BraceClose,

    #[token("[")]
    BracketOpen,

    #[token("]")]
    BracketClose,

    #[token(":")]
    Colon,

    #[token(",")]
    Comma,

    #[token("false", |_| LeafValue::Bool(false))]
    #[token("true", |_| LeafValue::Bool(true))]
    #[token("null", |_| LeafValue::Null)]
    #[regex(r"-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?", |_| LeafValue::Number)]
    #[regex(r#""([^"\\]|\\["\\/bnfrt]|\\u[a-fA-F0-9]{4})*""#, |_| LeafValue::String)]
    Leaf(LeafValue),
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum LeafValue {
    Bool(bool),
    Null,
    Number,
    String,
}

#[derive(Debug, PartialEq)]
enum StackItem {
    Object { span: RangeFrom<u32>, index: u32 },
    Array { span: RangeFrom<u32>, index: u32 },
}

#[derive(Debug, PartialEq)]
enum ContextItem {
    Key { span: Range<u32> },
    Value { span: Range<u32> },
    WaitingValue,
    WaitingKey,
}

#[derive(Debug)]
pub struct Error {
    token: Token,
    span: Range<u32>,
    stack: Vec<StackItem>,
    context: ContextItem,
}

pub fn parse(s: &str) -> Result<(), Error> {
    let mut lexer = Token::lexer(s);

    let mut stack = vec![];
    let mut context = ContextItem::WaitingValue;

    loop {
        let Some(token) = lexer.next() else { break };
        let token = token.unwrap();
        let span = lexer.span();
        let span = (span.start as u32)..(span.end as u32);

        macro_rules! bail {
            ($context:expr) => {
                return Err(Error {
                    token,
                    span,
                    stack,
                    context: $context,
                })
            };
        }

        match token {
            Token::Leaf(value) => match context {
                ContextItem::WaitingValue => context = ContextItem::Value { span },
                ContextItem::WaitingKey if value == LeafValue::String => {
                    context = ContextItem::Key { span }
                }
                context => bail!(context),
            },
            // starting a new object, which can either be in a value or a key position
            Token::BraceOpen => match context {
                ContextItem::WaitingValue => {
                    stack.push(StackItem::Object {
                        span: span.start..,
                        index: 0,
                    });
                    context = ContextItem::WaitingKey;
                }
                context => bail!(context),
            },
            // starting a new array, which can either be in a value or a key position
            Token::BracketOpen => match context {
                ContextItem::WaitingValue => {
                    stack.push(StackItem::Array {
                        span: span.start..,
                        index: 0,
                    });
                    context = ContextItem::WaitingValue;
                }
                context => bail!(context),
            },

            // closing the current object
            Token::BraceClose => {
                let (object_start, index) = match stack.pop() {
                    Some(StackItem::Object { span, index }) => (span.start, index),
                    Some(v) => {
                        stack.push(v);
                        bail!(context);
                    }
                    None => bail!(context),
                };

                match context {
                    ContextItem::WaitingKey if index == 0 => {
                        context = ContextItem::Value {
                            span: object_start..span.end,
                        };
                    }
                    ContextItem::Value { .. } => {
                        context = ContextItem::Value {
                            span: object_start..span.end,
                        };
                    }
                    context => bail!(context),
                }
            }
            Token::BracketClose => {
                let (array_start, index) = match stack.pop() {
                    Some(StackItem::Array { span, index }) => (span.start, index),
                    Some(v) => {
                        stack.push(v);
                        bail!(context);
                    }
                    None => bail!(context),
                };

                match context {
                    ContextItem::WaitingValue if index == 0 => {
                        context = ContextItem::Value {
                            span: array_start..span.end,
                        };
                    }
                    ContextItem::Value { .. } => {
                        context = ContextItem::Value {
                            span: array_start..span.end,
                        };
                    }
                    context => bail!(context),
                }
            }
            // commas may only follow key items
            Token::Colon => match context {
                ContextItem::Key { .. } => context = ContextItem::WaitingValue,
                context => bail!(context),
            },
            // commas may only follow value items if we are in an object or array
            Token::Comma => match context {
                ContextItem::Value { .. } if !stack.is_empty() => match stack.last_mut().unwrap() {
                    StackItem::Array { index, .. } => {
                        *index += 1;
                        context = ContextItem::WaitingValue;
                    }
                    StackItem::Object { index, .. } => {
                        *index += 1;
                        context = ContextItem::WaitingKey;
                    }
                },
                context => bail!(context),
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use core::hint::black_box;
    use std::time::Instant;

    #[test]
    fn test() {
        let src = include_str!("../testdata/kubernetes-oapi.json");

        let start = Instant::now();
        for _ in 0..1000 {
            black_box(crate::parse(black_box(src))).unwrap();
        }
        dbg!(start.elapsed() / 1000);
    }

    #[test]
    fn test2() {
        let src = include_str!("../testdata/kubernetes-oapi.json");

        let start = Instant::now();
        for _ in 0..1000 {
            black_box(serde_json::from_str::<&serde_json::value::RawValue>(
                black_box(src),
            ))
            .unwrap();
        }
        dbg!(start.elapsed() / 1000);
    }
}
