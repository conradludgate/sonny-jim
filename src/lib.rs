#![no_std]

#[macro_use(vec)]
extern crate alloc;

#[cfg(test)]
#[macro_use(dbg)]
extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use core::hash::BuildHasher;
use core::ops::{Index, Range, RangeFrom};
use core::task::Poll;
use foldhash::quality::RandomState;
use hashbrown::hash_table::Entry;
use hashbrown::HashTable;

use logos::{Lexer, Logos};

mod fmt;

#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\r\n]+")] // Ignore this regex pattern between tokens
enum Token {
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

    #[token("false", |_| LeafValue::Bool(false))]
    #[token("true", |_| LeafValue::Bool(true))]
    #[token("null", |_| LeafValue::Null)]
    #[regex(r"[-0-9][0-9eE+\-\.]*", |_| LeafValue::Number)]
    #[regex(r#""([^"\\]*(\\.)?)*""#, |_| LeafValue::String)]
    Leaf(LeafValue),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LeafValue {
    Bool(bool),
    Null,
    Number,
    String,
}

#[derive(Debug)]
struct StackItem {
    span: RangeFrom<u32>,
    kind: StackItemKind,
}

#[derive(Debug)]
enum StackItemKind {
    Array(u32),
    Object(u32, u32),
}

#[derive(Debug, Clone)]
enum ContextItem {
    WaitingKey,
    Key { span: Range<u32>, key: StringKey },
    WaitingValue,
    Value { span: Range<u32>, value: ValueKind },
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Error {
    token: Option<Token>,
    span: Range<u32>,
    stack: Vec<StackItem>,
    context: ContextItem,
}

#[derive(Debug, Clone)]
pub struct Value {
    pub span: Range<u32>,
    pub kind: ValueKind,
}

#[derive(Debug, Clone)]
pub enum ValueKind {
    Leaf(LeafValue),
    Object(Object),
    Array(Array),
}

#[derive(Debug, Clone)]
pub struct Object {
    keys: Range<u32>,
    values: Range<u32>,
}

#[derive(Debug, Clone)]
pub struct Array {
    values: Range<u32>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StringKey(Range<u32>);

struct Scratch<'a> {
    src: &'a str,
    scratch: String,
}

pub struct Arena<'a> {
    scratch: Scratch<'a>,
    hasher: RandomState,
    table: HashTable<StringKey>,
    keys: Vec<StringKey>,
    values: Vec<Value>,
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

impl<'a> Index<&StringKey> for Arena<'a> {
    type Output = str;

    fn index(&self, index: &StringKey) -> &Self::Output {
        &self.scratch[index]
    }
}

impl<'a> Arena<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            scratch: Scratch {
                src,
                scratch: String::new(),
            },
            hasher: RandomState::default(),
            table: HashTable::new(),
            keys: Vec::new(),
            values: Vec::new(),
        }
    }

    fn intern_string(&mut self, span: Range<u32>) -> Result<StringKey, ()> {
        let Self {
            scratch,
            hasher,
            table,
            ..
        } = self;

        // check that this actually points to a string...
        debug_assert!(span.start + 2 <= span.end);
        debug_assert_eq!(scratch.src.as_bytes()[span.start as usize], b'"');
        debug_assert_eq!(scratch.src.as_bytes()[span.end as usize - 1], b'"');

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

            start += escape;
            start += 1;
            let ctrl = b[start];
            start += 1;

            match ctrl {
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

                    let hex_bytes: [u8; 4] = *b[start..].first_chunk().ok_or(())?;
                    let mut code = [0; 2];
                    hex::decode_to_slice(hex_bytes, &mut code).map_err(|_| ())?;

                    if let Some(c) = char::from_u32(u16::from_be_bytes(code) as u32) {
                        scratch.scratch.push(c);
                    } else {
                        return Err(());
                    }

                    start += 4;
                }
                _ => return Err(()),
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
                Ok(occupied_entry.get().clone())
            }
            Entry::Vacant(vacant_entry) => Ok(vacant_entry.insert(StringKey(span)).get().clone()),
        }
    }
}

struct Parser<'a, 's> {
    arena: &'a mut Arena<'s>,
    lexer: Lexer<'s, Token>,

    /// tracks which object or array we are in
    stack: Vec<StackItem>,
    /// values used by the current/parent objects or arrays.
    value_stack: Vec<Value>,
    /// keys used by the current/parent objects
    key_stack: Vec<StringKey>,
}

enum PollParse {
    Ready(Value),
    Pending(ContextItem),
}

impl Parser<'_, '_> {
    #[cold]
    fn early_eof(&mut self, context: ContextItem) -> Error {
        let src = self.arena.scratch.src;
        Error {
            token: None,
            span: src.len() as u32..src.len() as u32,
            stack: core::mem::take(&mut self.stack),
            context,
        }
    }

    #[cold]
    fn parse_error(&mut self, context: ContextItem, token: Token, span: Range<u32>) -> Error {
        Error {
            token: Some(token),
            span,
            stack: core::mem::take(&mut self.stack),
            context,
        }
    }

    #[cold]
    fn token_error(&mut self, context: ContextItem, span: Range<u32>) -> Error {
        Error {
            token: None,
            span,
            stack: core::mem::take(&mut self.stack),
            context,
        }
    }

    #[inline]
    fn step(&mut self, mut context: ContextItem) -> Result<PollParse, Error> {
        let Self {
            arena,
            lexer,
            stack,
            value_stack,
            key_stack,
        } = self;

        let token = match lexer.next() {
            Some(Ok(token)) => token,
            Some(Err(())) => {
                let span = lexer.span();
                let span = (span.start as u32)..(span.end as u32);
                return Err(self.token_error(context, span));
            }
            None => match context {
                ContextItem::Value { span, value } if stack.is_empty() => {
                    return Ok(PollParse::Ready(Value { span, kind: value }))
                }
                context => return Err(self.early_eof(context)),
            },
        };

        let span = lexer.span();
        let span = (span.start as u32)..(span.end as u32);

        macro_rules! bail {
            ($context:expr) => {
                return Err(self.parse_error($context, token, span))
            };
        }

        match token {
            Token::Leaf(value) => match context {
                // in value position, a leaf value is always ok
                ContextItem::WaitingValue => {
                    context = ContextItem::Value {
                        span,
                        value: ValueKind::Leaf(value),
                    }
                }
                // in a key position, only string values are ok
                ContextItem::WaitingKey if value == LeafValue::String => {
                    context = ContextItem::Key {
                        key: match arena.intern_string(span.clone()) {
                            Ok(key) => key,
                            Err(()) => bail!(context),
                        },
                        span,
                    }
                }
                context => bail!(context),
            },
            // starting a new object, which can only be in a value position
            Token::OpenObject => match context {
                ContextItem::WaitingValue => {
                    stack.push(StackItem {
                        span: span.start..,
                        kind: StackItemKind::Object(
                            value_stack.len() as u32,
                            key_stack.len() as u32,
                        ),
                    });
                    context = ContextItem::WaitingKey;
                }
                context => bail!(context),
            },
            // starting a new array, which can only be in a value position
            Token::OpenArray => match context {
                ContextItem::WaitingValue => {
                    stack.push(StackItem {
                        span: span.start..,
                        kind: StackItemKind::Array(value_stack.len() as u32),
                    });
                    context = ContextItem::WaitingValue;
                }
                context => bail!(context),
            },

            // closing the current object
            // the stack must contain an object item
            // Closing an object can occur if:
            // * It immediatelly follows a `OpenObject` (eg `{}`)
            // * It immediatelly follows a value, (eg `{ "key": "value" }`)
            // We codify this as:
            // * Acceptable before a key position iff the object is empty
            // * Acceptable after a value positon
            Token::CloseObject => {
                match stack.pop() {
                    Some(StackItem {
                        kind: StackItemKind::Object(vindex, kindex),
                        span: RangeFrom { start },
                    }) => {
                        let span = start..span.end;

                        match context {
                            ContextItem::WaitingKey if value_stack.len() == vindex as usize => {
                                context = ContextItem::Value {
                                    span,
                                    value: ValueKind::Object(Object {
                                        keys: 0..0,
                                        values: 0..0,
                                    }),
                                };
                            }
                            ContextItem::Value { span, value: kind } => {
                                value_stack.push(Value {
                                    span: span.clone(),
                                    kind,
                                });

                                let vi = arena.values.len();
                                arena.values.extend(value_stack.drain(vindex as usize..));
                                let vj = arena.values.len();

                                let ki = arena.keys.len();
                                arena.keys.extend(key_stack.drain(kindex as usize..));
                                let kj = arena.keys.len();

                                context = ContextItem::Value {
                                    span,
                                    value: ValueKind::Object(Object {
                                        keys: ki as u32..kj as u32,
                                        values: vi as u32..vj as u32,
                                    }),
                                };
                            }
                            context => bail!(context),
                        }
                    }
                    Some(v) => {
                        stack.push(v);
                        bail!(context);
                    }
                    None => bail!(context),
                };
            }

            // closing the current array
            // the stack must contain an array item
            // Closing an array can occur if:
            // * It immediatelly follows a `OpenArray` (eg `[]`)
            // * It immediatelly follows a value, (eg `["value"]`)
            // We codify this as:
            // * Acceptable before a value position iff the array is empty
            // * Acceptable after a value positon
            Token::CloseArray => {
                match stack.pop() {
                    Some(StackItem {
                        kind: StackItemKind::Array(vindex),
                        span: RangeFrom { start },
                    }) => {
                        let span = start..span.end;

                        match context {
                            ContextItem::WaitingValue if value_stack.len() == vindex as usize => {
                                context = ContextItem::Value {
                                    span,
                                    value: ValueKind::Array(Array { values: 0..0 }),
                                };
                            }
                            ContextItem::Value { span, value: kind } => {
                                value_stack.push(Value {
                                    span: span.clone(),
                                    kind,
                                });

                                let vi = arena.values.len();
                                arena.values.extend(value_stack.drain(vindex as usize..));
                                let vj = arena.values.len();

                                context = ContextItem::Value {
                                    span,
                                    value: ValueKind::Array(Array {
                                        values: vi as u32..vj as u32,
                                    }),
                                };
                            }
                            context => bail!(context),
                        }
                    }
                    Some(v) => {
                        stack.push(v);
                        bail!(context);
                    }
                    None => bail!(context),
                };
            }

            // colons may only follow key items
            Token::Colon => match context {
                ContextItem::Key { key, span } if !stack.is_empty() => {
                    match &mut stack.last_mut().unwrap().kind {
                        StackItemKind::Object(_, _) => {
                            key_stack.push(key);
                            context = ContextItem::WaitingValue
                        }
                        _ => bail!(ContextItem::Key { key, span }),
                    }
                }
                context => bail!(context),
            },

            // commas may only follow value items if we are in an object or array
            Token::Comma => match context {
                ContextItem::Value { span, value } if !stack.is_empty() => {
                    value_stack.push(Value { span, kind: value });
                    match stack.last_mut().unwrap().kind {
                        StackItemKind::Object(_, _) => context = ContextItem::WaitingKey,
                        StackItemKind::Array(_) => context = ContextItem::WaitingValue,
                    }
                }
                context => bail!(context),
            },
        }

        Ok(PollParse::Pending(context))
    }

    fn step_while(
        &mut self,
        mut f: impl FnMut() -> bool,
        mut context: ContextItem,
    ) -> Result<PollParse, Error> {
        while f() {
            match self.step(context)? {
                PollParse::Ready(value) => return Ok(PollParse::Ready(value)),
                PollParse::Pending(c) => context = c,
            }
        }
        Ok(PollParse::Pending(context))
    }
}

pub fn parse(arena: &mut Arena<'_>) -> Result<Value, Error> {
    let lexer = Token::lexer(arena.scratch.src);

    let mut parser = Parser {
        arena,
        lexer,
        stack: vec![],
        value_stack: vec![],
        key_stack: vec![],
    };

    // what kind of token are we expecting.
    // to start, we expect a value item.
    let mut context = ContextItem::WaitingValue;

    loop {
        match parser.step(context)? {
            PollParse::Ready(value) => break Ok(value),
            PollParse::Pending(c) => context = c,
        }
    }
}

const YIELD_AFTER: usize = 4096;

pub async fn parse_async(arena: &mut Arena<'_>) -> Result<Value, Error> {
    let lexer = Token::lexer(arena.scratch.src);

    let mut parser = Parser {
        arena,
        lexer,
        stack: vec![],
        value_stack: vec![],
        key_stack: vec![],
    };

    // what kind of token are we expecting.
    // to start, we expect a value item.
    let mut context = ContextItem::WaitingValue;

    core::future::poll_fn(move |cx| {
        let mut i = 0..YIELD_AFTER;
        match parser.step_while(|| i.next().is_some(), context.clone())? {
            PollParse::Ready(value) => return Poll::Ready(Ok(value)),
            PollParse::Pending(c) => context = c,
        }

        cx.waker().wake_by_ref();
        Poll::Pending
    })
    .await
}

#[cfg(test)]
mod tests {
    use core::hint::black_box;
    use std::time::Instant;

    use crate::Arena;

    const KUBE: &str = include_str!("../testdata/kubernetes-oapi.json");

    #[test]
    fn bench_this() {
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(crate::parse(black_box(&mut Arena::new(KUBE)))).unwrap();
        }
        dbg!(start.elapsed() / 1000);
    }

    #[test]
    fn bench_serde_raw() {
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(serde_json::from_str::<&serde_json::value::RawValue>(
                black_box(KUBE),
            ))
            .unwrap();
        }
        dbg!(start.elapsed() / 1000);
    }

    #[test]
    fn bench_serde() {
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(serde_json::from_str::<serde_json::value::Value>(black_box(
                KUBE,
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

        crate::parse(&mut Arena::new(&input)).unwrap();
    }

    #[pollster::test]
    async fn non_blocking() {
        let cool_factor = 1_000_000;

        let first_half = "[".repeat(cool_factor);
        let second_half = "]".repeat(cool_factor);
        let input = std::format!("{first_half}{second_half}");

        crate::parse_async(&mut Arena::new(&input)).await.unwrap();
    }

    #[test]
    fn snapshot() {
        let data = r#"{
            "definitions": {
                "io.k8s.api.admissionregistration.v1.AuditAnnotation": {
                    "description": "AuditAnnotation describes how to produce an audit annotation for an API request.",
                    "properties": {
                        "key": {
                            "description": "key specifies the audit annotation key. The audit annotation keys of a ValidatingAdmissionPolicy must be unique. The key must be a qualified name ([A-Za-z0-9][-A-Za-z0-9_.]*) no more than 63 bytes in length.\n\nThe key is combined with the resource name of the ValidatingAdmissionPolicy to construct an audit annotation key: \"{ValidatingAdmissionPolicy name}/{key}\".\n\nIf an admission webhook uses the same resource name as this ValidatingAdmissionPolicy and the same audit annotation key, the annotation key will be identical. In this case, the first annotation written with the key will be included in the audit event and all subsequent annotations with the same key will be discarded.\n\nRequired.",
                            "type": "string"
                        },
                        "valueExpression": {
                            "description": "valueExpression represents the expression which is evaluated by CEL to produce an audit annotation value. The expression must evaluate to either a string or null value. If the expression evaluates to a string, the audit annotation is included with the string value. If the expression evaluates to null or empty string the audit annotation will be omitted. The valueExpression may be no longer than 5kb in length. If the result of the valueExpression is more than 10kb in length, it will be truncated to 10kb.\n\nIf multiple ValidatingAdmissionPolicyBinding resources match an API request, then the valueExpression will be evaluated for each binding. All unique values produced by the valueExpressions will be joined together in a comma-separated list.\n\nRequired.",
                            "type": "string"
                        }
                    },
                    "required": [
                        "key",
                        "valueExpression"
                    ],
                    "type": "object"
                }
            }
        }"#;

        let mut arena = Arena::new(data);
        let parsed = crate::parse(&mut arena).unwrap();
        insta::assert_debug_snapshot!((parsed, arena.scratch.scratch, arena.values, arena.keys));
    }
}
