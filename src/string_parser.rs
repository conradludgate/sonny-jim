use alloc::string::String;
use core::ops::{Index, Range};

use crate::{SrcSpan, StringKey};

pub(crate) struct Scratch<'a> {
    pub(crate) src: &'a str,
    pub(crate) scratch: String,
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

impl<'a> Scratch<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        Self {
            src,
            scratch: String::new(),
        }
    }

    pub(crate) fn parse_string_escapes(&mut self, span: SrcSpan) -> Result<StringKey, ()> {
        // check that this actually points to a string...
        let span = span.0;
        debug_assert!(span.start + 2 <= span.end);
        debug_assert_eq!(self.src.as_bytes()[span.start as usize], b'"');
        debug_assert_eq!(self.src.as_bytes()[span.end as usize - 1], b'"');

        let mut start = span.start as usize + 1;
        let end = span.end as usize - 1;

        let scratch_start = self.scratch.len();

        loop {
            let b = self.src.as_bytes();
            let Some(escape) = memchr::memchr(b'\\', &b[start..end]) else {
                break;
            };
            self.scratch.push_str(&self.src[start..start + escape]);

            start += escape;
            start += 1;
            let ctrl = b[start];
            start += 1;

            match ctrl {
                b'"' => self.scratch.push('"'),
                b'\\' => self.scratch.push('\\'),
                b'/' => self.scratch.push('/'),
                b'b' => self.scratch.push('\x08'),
                b'f' => self.scratch.push('\x0c'),
                b'n' => self.scratch.push('\n'),
                b'r' => self.scratch.push('\r'),
                b't' => self.scratch.push('\t'),
                b'u' => {
                    // TODO: is this even right???
                    // \u1234 -> U+1234
                    // TODO: maybe support utf16

                    let hex_bytes: [u8; 4] = *b[start..].first_chunk().ok_or(())?;
                    let mut code = [0; 2];
                    hex::decode_to_slice(hex_bytes, &mut code).map_err(|_| ())?;

                    if let Some(c) = char::from_u32(u16::from_be_bytes(code) as u32) {
                        self.scratch.push(c);
                    } else {
                        return Err(());
                    }

                    start += 4;
                }
                _ => return Err(()),
            }
        }

        let span = if scratch_start < self.scratch.len() {
            self.scratch.push_str(&self.src[start..end]);
            self.scratch.len() as u32..scratch_start as u32
        } else {
            start as u32..end as u32
        };

        Ok(StringKey(span))
    }

    pub(crate) fn truncate(&mut self, key: StringKey) {
        if key.0.start > key.0.end {
            self.scratch.truncate(key.0.end as usize);
        }
    }
}
