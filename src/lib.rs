#![no_std]
#![forbid(unsafe_code)]

#[macro_use(vec)]
extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::vec::Vec;
use core::hash::BuildHasher;
use core::ops::{Index, Range, RangeFrom};
use core::task::Poll;
use foldhash::quality::RandomState;
use hashbrown::hash_table::Entry;
use hashbrown::HashTable;
use parser::{Parser, PollParse};
use string_parser::Scratch;
use token::Token;

use logos::Logos;

mod fmt;
mod parser;
mod string_parser;
mod token;

#[derive(Debug, Clone)]
pub struct SrcSpan(Range<u32>);

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum LeafValue {
    False = 0,
    True = 1,
    Null = 2,
    Number = 3,
    String = 4,
}

impl LeafValue {
    fn from_repr(x: u8) -> Self {
        match x {
            0 => LeafValue::False,
            1 => LeafValue::True,
            2 => LeafValue::Null,
            3 => LeafValue::Number,
            4 => LeafValue::String,
            _ => unreachable!(),
        }
    }

    fn to_repr(self) -> u8 {
        self as u8
    }
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
    Key { span: SrcSpan, key: StringKey },
    WaitingValue,
    Value { span: SrcSpan, value: ValueInner },
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Error {
    token: Option<Token>,
    span: SrcSpan,
    stack: Vec<StackItem>,
    context: ContextItem,
}

#[derive(Clone)]
pub struct Value {
    pub span: SrcSpan,
    inner: ValueInner,
}

impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Value")
            .field("span", &self.span)
            .field("kind", &self.kind())
            .finish()
    }
}

impl Value {
    pub fn kind(&self) -> ValueKind {
        self.inner.kind()
    }
}

// compact repr for ValueKind.
// if keys.start == u32::MAX then this is not an object.
// if vals.start == u32::MAX then this is not an array either.
// if this is not an object or an array, then it's a leaf which is encoded into vals.end.
#[derive(Clone)]
struct ValueInner {
    keys: Keys,
    vals: Values,
}

impl core::fmt::Debug for ValueInner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.kind().fmt(f)
    }
}

impl ValueInner {
    fn kind(&self) -> ValueKind {
        if self.keys.0.start < u32::MAX {
            ValueKind::Object(Object {
                keys: self.keys.clone(),
                values: self.vals.clone(),
            })
        } else if self.vals.0.start < u32::MAX {
            ValueKind::Array(Array {
                values: self.vals.clone(),
            })
        } else {
            ValueKind::Leaf(LeafValue::from_repr(self.vals.0.end as u8))
        }
    }
}

#[derive(Debug, Clone)]
pub enum ValueKind {
    Leaf(LeafValue),
    Object(Object),
    Array(Array),
}

#[derive(Debug, Clone)]
pub struct Object {
    pub keys: Keys,
    pub values: Values,
}

#[derive(Debug, Clone)]
pub struct Array {
    pub values: Values,
}

#[derive(Debug, Clone)]
pub struct Values(Range<u32>);

#[derive(Debug, Clone)]
pub struct Keys(Range<u32>);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StringKey(Range<u32>);

pub struct Arena<'a> {
    scratch: Scratch<'a>,
    hasher: RandomState,
    table: HashTable<StringKey>,
    keys: Vec<StringKey>,
    values: Vec<Value>,
}

impl Index<&SrcSpan> for Arena<'_> {
    type Output = str;

    fn index(&self, index: &SrcSpan) -> &Self::Output {
        &self.scratch.src[index.0.start as usize..index.0.end as usize]
    }
}

impl<'a> Index<&StringKey> for Arena<'a> {
    type Output = str;

    fn index(&self, index: &StringKey) -> &Self::Output {
        &self.scratch[index]
    }
}

impl<'a> Index<&Values> for Arena<'a> {
    type Output = [Value];

    fn index(&self, index: &Values) -> &Self::Output {
        &self.values[index.0.start as usize..index.0.end as usize]
    }
}

impl<'a> Index<&Keys> for Arena<'a> {
    type Output = [StringKey];

    fn index(&self, index: &Keys) -> &Self::Output {
        &self.keys[index.0.start as usize..index.0.end as usize]
    }
}

impl<'a> Arena<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            scratch: Scratch::new(src),
            hasher: RandomState::default(),
            table: HashTable::new(),
            keys: Vec::new(),
            values: Vec::new(),
        }
    }

    fn intern_string(&mut self, span: SrcSpan) -> Result<StringKey, ()> {
        let key = self.scratch.parse_string_escapes(span)?;
        let str = &self.scratch[&key];

        let hash = self.hasher.hash_one(str);
        match self.table.entry(
            hash,
            |key| &self.scratch[key] == str,
            |key| self.hasher.hash_one(&self.scratch[key]),
        ) {
            Entry::Occupied(occupied_entry) => {
                self.scratch.truncate(key);
                Ok(occupied_entry.get().clone())
            }
            Entry::Vacant(vacant_entry) => Ok(vacant_entry.insert(key).get().clone()),
        }
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
    use crate::Arena;

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
