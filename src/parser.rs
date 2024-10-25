use alloc::vec::Vec;
use core::ops::RangeFrom;

use logos::Lexer;

use crate::{
    token::Token, Arena, ContextItem, Error, Keys, LeafValue, SrcSpan, StackItem, StackItemKind,
    StringKey, Value, ValueInner, Values,
};

pub(crate) struct Parser<'a, 's> {
    pub(crate) arena: &'a mut Arena<'s>,
    pub(crate) lexer: Lexer<'s, Token>,

    /// tracks which object or array we are in
    pub(crate) stack: Vec<StackItem>,
    /// values used by the current/parent objects or arrays.
    pub(crate) value_stack: Vec<Value>,
    /// keys used by the current/parent objects
    pub(crate) key_stack: Vec<StringKey>,
}

pub(crate) enum PollParse {
    Ready(Value),
    Pending(ContextItem),
}

impl Parser<'_, '_> {
    #[cold]
    fn early_eof(&mut self, context: ContextItem) -> Error {
        let src = self.arena.scratch.src;
        Error {
            token: None,
            span: SrcSpan(src.len() as u32..src.len() as u32),
            stack: core::mem::take(&mut self.stack),
            context,
        }
    }

    #[cold]
    fn parse_error(&mut self, context: ContextItem, token: Token, span: SrcSpan) -> Error {
        Error {
            token: Some(token),
            span,
            stack: core::mem::take(&mut self.stack),
            context,
        }
    }

    #[cold]
    fn token_error(&mut self, context: ContextItem, span: SrcSpan) -> Error {
        Error {
            token: None,
            span,
            stack: core::mem::take(&mut self.stack),
            context,
        }
    }

    #[inline]
    pub(crate) fn step(&mut self, mut context: ContextItem) -> Result<PollParse, Error> {
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
                let span = SrcSpan((span.start as u32)..(span.end as u32));
                return Err(self.token_error(context, span));
            }
            None => match context {
                ContextItem::Value { span, value } if stack.is_empty() => {
                    return Ok(PollParse::Ready(Value { span, inner: value }))
                }
                context => return Err(self.early_eof(context)),
            },
        };

        let span = lexer.span();
        let span = SrcSpan((span.start as u32)..(span.end as u32));

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
                        value: ValueInner {
                            keys: Keys(u32::MAX..u32::MAX),
                            vals: Values(u32::MAX..value.to_repr()),
                        },
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
                        span: span.0.start..,
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
                        span: span.0.start..,
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
                        let span = SrcSpan(start..span.0.end);

                        match context {
                            ContextItem::WaitingKey if value_stack.len() == vindex as usize => {
                                context = ContextItem::Value {
                                    span,
                                    value: ValueInner {
                                        keys: Keys(0..0),
                                        vals: Values(0..0),
                                    },
                                };
                            }
                            ContextItem::Value { span, value: inner } => {
                                value_stack.push(Value {
                                    span: span.clone(),
                                    inner,
                                });

                                let vi = arena.values.len();
                                arena.values.extend(value_stack.drain(vindex as usize..));
                                let vj = arena.values.len();

                                let ki = arena.keys.len();
                                arena.keys.extend(key_stack.drain(kindex as usize..));
                                let kj = arena.keys.len();

                                context = ContextItem::Value {
                                    span,
                                    value: ValueInner {
                                        keys: Keys(ki as u32..kj as u32),
                                        vals: Values(vi as u32..vj as u32),
                                    },
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
                        let span = SrcSpan(start..span.0.end);

                        match context {
                            ContextItem::WaitingValue if value_stack.len() == vindex as usize => {
                                context = ContextItem::Value {
                                    span,
                                    value: ValueInner {
                                        keys: Keys(u32::MAX..u32::MAX),
                                        vals: Values(0..0),
                                    },
                                };
                            }
                            ContextItem::Value { span, value: inner } => {
                                value_stack.push(Value {
                                    span: span.clone(),
                                    inner,
                                });

                                let vi = arena.values.len();
                                arena.values.extend(value_stack.drain(vindex as usize..));
                                let vj = arena.values.len();

                                context = ContextItem::Value {
                                    span,
                                    value: ValueInner {
                                        keys: Keys(u32::MAX..u32::MAX),
                                        vals: Values(vi as u32..vj as u32),
                                    },
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
                    value_stack.push(Value { span, inner: value });
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

    pub(crate) fn step_while(
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
