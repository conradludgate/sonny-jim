#![no_std]

#[macro_use(vec)]
extern crate alloc;

#[cfg(test)]
#[macro_use(dbg)]
extern crate std;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::hash::BuildHasher;
use core::ops::{Index, Range, RangeFrom};
use hashbrown::hash_table::Entry;
use hashbrown::HashTable;
use indexmap::IndexMap;
use thin_vec::ThinVec;

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

#[derive(Debug, Clone)]
pub enum Value {
    Bool(bool),
    Null,
    Number(Range<u32>),
    String(StringKey),
    Object(Box<IndexMap<StringKey, Value, foldhash::quality::RandomState>>),
    Array(ThinVec<Value>),
}

#[derive(Debug, Clone)]
pub struct StringKey(Range<u32>);

struct Scratch<'a> {
    src: &'a str,
    scratch: String,
}

pub struct Interner<'a> {
    scratch: Scratch<'a>,
    hasher: foldhash::quality::RandomState,
    table: HashTable<StringKey>,
}

impl<'a> Index<&StringKey> for Scratch<'a> {
    type Output = str;

    fn index(&self, index: &StringKey) -> &Self::Output {
        let Range { start, end } = index.0;
        if end < start {
            &self.scratch[end as usize..start as usize]
        } else {
            &self.src[start as usize..end as usize]
        }
    }
}

impl<'a> Index<&StringKey> for Interner<'a> {
    type Output = str;

    fn index(&self, index: &StringKey) -> &Self::Output {
        &self.scratch[index]
    }
}

impl<'a> Interner<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            scratch: Scratch {
                src,
                scratch: String::new(),
            },
            hasher: Default::default(),
            table: HashTable::new(),
        }
    }

    fn intern(&mut self, span: Range<u32>) -> StringKey {
        let Self {
            scratch,
            hasher,
            table,
        } = self;

        // check that this actually points to a string...
        debug_assert_eq!(scratch.src.as_bytes()[span.start as usize], b'"');
        debug_assert_eq!(scratch.src.as_bytes()[span.end as usize], b'"');
        debug_assert!(span.start + 2 <= span.end);

        let mut start = span.start as usize + 1;
        let end = span.end as usize - 1;

        let scratch_start = scratch.scratch.len();

        loop {
            let b = scratch.src.as_bytes();
            let Some(escape) = memchr::memchr(b'\\', &b[start..end]) else {
                break;
            };
            scratch
                .scratch
                .push_str(&scratch.src[start..start + escape]);

            start += 1;

            match b[start] {
                b'"' => scratch.scratch.push('"'),
                b'\\' => scratch.scratch.push('\\'),
                b'/' => scratch.scratch.push('/'),
                b'b' => scratch.scratch.push('\x08'),
                b'f' => scratch.scratch.push('\x0c'),
                b'n' => scratch.scratch.push('\n'),
                b'r' => scratch.scratch.push('\r'),
                b't' => scratch.scratch.push('\t'),
                b'u' => {
                    // TODO: is this even right???
                    // \u1234 -> U+1234
                    // TODO: maybe support utf16

                    let hex_bytes: [u8; 4] = *b
                        .first_chunk()
                        .expect("logos should have validated that 4 hex bytes follow the \\u");
                    let mut code = [0; 2];
                    hex::decode_to_slice(hex_bytes, &mut code)
                        .expect("should have validated the hex already");

                    if let Some(c) = char::from_u32(u16::from_be_bytes(code) as u32) {
                        scratch.scratch.push(c);
                    } else {
                        todo!("error")
                    }
                }
                _ => unreachable!("escape character has been validated by logos"),
            }
        }

        let span;
        let str;
        if scratch_start < scratch.scratch.len() {
            scratch.scratch.push_str(&scratch.src[start..end]);
            span = scratch.scratch.len() as u32..scratch_start as u32;
            str = &scratch.scratch[scratch_start..];
        } else {
            span = start as u32..end as u32;
            str = &scratch.src[start..end];
        };

        let hash = hasher.hash_one(str);
        match table.entry(
            hash,
            |key| &scratch[key] == str,
            |key| hasher.hash_one(&scratch[key]),
        ) {
            Entry::Occupied(occupied_entry) => {
                scratch.scratch.truncate(scratch_start);
                occupied_entry.get().clone()
            }
            Entry::Vacant(vacant_entry) => vacant_entry.insert(StringKey(span)).get().clone(),
        }
    }
}

pub fn parse(i: Interner<'_>) -> Result<Value, Error> {
    let mut lexer = Token::lexer(i.scratch.src);

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
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use core::hint::black_box;
    use std::time::Instant;

    use crate::Interner;

    #[test]
    fn bench_this() {
        let src = include_str!("../testdata/kubernetes-oapi.json");

        let start = Instant::now();
        for _ in 0..1000 {
            black_box(crate::parse(black_box(Interner::new(src)))).unwrap();
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
        crate::parse(Interner::new(&input)).unwrap();
    }
}
