#![no_main]

use libfuzzer_sys::fuzz_target;
use sonny_jim::{Arena, parse};

fuzz_target!(|data: &str| {
    _ = parse(&mut Arena::new(data));
});
