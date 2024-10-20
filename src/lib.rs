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
    #[token("{", |_| StackKind::Object)]
    #[token("[", |_| StackKind::Array)]
    Open(StackKind),

    #[token("}", |_| StackKind::Object)]
    #[token("]", |_| StackKind::Array)]
    Close(StackKind),

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
enum StackKind {
    Object,
    Array,
}

impl StackKind {
    fn start_context(self) -> ContextItem {
        match self {
            StackKind::Object => ContextItem::WaitingKey,
            StackKind::Array => ContextItem::WaitingValue,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum LeafValue {
    Bool(bool),
    Null,
    Number,
    String,
}

#[derive(Debug, PartialEq)]
struct StackItem {
    kind: StackKind,
    span: RangeFrom<u32>,
    index: u32,
}

#[derive(Debug, PartialEq)]
enum ContextItem {
    Key { span: Range<u32> },
    Value { span: Range<u32> },
    WaitingValue,
    WaitingKey,
}

#[derive(Debug)]
#[allow(dead_code)]
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
            // starting a new object or array, which can only be in a value position
            Token::Open(kind) => match context {
                ContextItem::WaitingValue => {
                    stack.push(StackItem {
                        kind,
                        span: span.start..,
                        index: 0,
                    });
                    context = kind.start_context();
                }
                context => bail!(context),
            },

            // closing the current object or array
            Token::Close(kind2) => {
                let (start, index) = match stack.pop() {
                    Some(StackItem { kind, span, index }) if kind == kind2 => (span.start, index),
                    Some(v) => {
                        stack.push(v);
                        bail!(context);
                    }
                    None => bail!(context),
                };
                let span = start..span.end;

                match context {
                    ContextItem::WaitingKey if kind2 == StackKind::Object && index == 0 => {
                        context = ContextItem::Value { span };
                    }
                    ContextItem::WaitingValue if kind2 == StackKind::Array && index == 0 => {
                        context = ContextItem::Value { span };
                    }
                    ContextItem::Value { .. } => {
                        context = ContextItem::Value { span };
                    }
                    context => bail!(context),
                }
            }

            // colons may only follow key items
            Token::Colon => match context {
                ContextItem::Key { .. } => context = ContextItem::WaitingValue,
                context => bail!(context),
            },
            // commas may only follow value items if we are in an object or array
            Token::Comma => match context {
                ContextItem::Value { .. } if !stack.is_empty() => {
                    let StackItem { kind, index, .. } = stack.last_mut().unwrap();
                    {
                        *index += 1;
                        context = kind.start_context();
                    }
                }
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
    fn bench_this() {
        let src = include_str!("../testdata/kubernetes-oapi.json");

        let start = Instant::now();
        for _ in 0..1000 {
            black_box(crate::parse(black_box(src))).unwrap();
        }
        dbg!(start.elapsed() / 1000);
    }

    #[test]
    fn bench_serde_raw() {
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

    #[test]
    fn bench_serde() {
        let src = include_str!("../testdata/kubernetes-oapi.json");

        let start = Instant::now();
        for _ in 0..1000 {
            black_box(serde_json::from_str::<serde_json::value::Value>(black_box(
                src,
            )))
            .unwrap();
        }
        dbg!(start.elapsed() / 1000);
    }

    #[test]
    fn massive_stack() {
        let cool_factor = 1_000_000;

        let first_half = "[".repeat(cool_factor);
        let second_half = "]".repeat(cool_factor);
        let input = std::format!("{first_half}{second_half}");
        crate::parse(&input).unwrap();
    }
}
