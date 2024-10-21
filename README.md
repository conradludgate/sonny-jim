# sonny-jim

**son**ny-**j**im is a memory-efficient and simple JSON parsing library.

## Usage

Currently, one API is exposed:

```rust
let mut arena = Arena::new(str);
let value = parse(&mut arena)?;
```

This is similar to `serde_json::Value`:

```rust
let value: serde_json::Value = serde_json::from_str(str)?;
```

## Details

Sometimes you have to work with dynamic JSON objects in a read-only fashion.
If these JSON objects are user-provided, that leaves room for malicious use.

`serde_json` struggles in this area in several ways:
1. recursive - `serde_json` uses recursion while parsing, causing stack overflow risks
    * `serde_json` avoids this by setting a recursion depth limit
2. memory usage - `serde_json::Value` constructs many `String`s, `Vec`s, `Map`s, etc.
   These often contain very few values indivually, but amplify the memory usage quite a lot,
   and this can lead to fragmentation issues at well
3. lack of async - the parsing of JSON cannot be paused and resumed. Parsing the Kubernetes OpenAPI
   specification to a `serde_json::Value` takes 9ms on my M2 Max. This will absolutely block an async runtime thread
   and cause latency issues.

`sonny-jim` solves these in the following ways:
1. iterative - `sonny-jim` uses a `Vec` as a lightweight stack, rather than using recursion.
2. arenas - `sonny-jim` allocates any objects into an arena, for very compact representations.
   any strings and values are backed directly by the original JSON input string.
3. periodic yielding - because of `sonny-jim`'s iterative implementation, yielding periodically is trivial.
