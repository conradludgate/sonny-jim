## parse

* `small` is a small json object. 182 bytes.
* `kube` is a large real-world json object. 3.4MB

* `serde` parses a `&str` into a `serde_json::Value`.
* `serde_raw` _validates_ a `&str` as a `&serde_json::RawValue`.
* `simd_json_borrowed` parses a `&str` (allocated into a `&mut Vec<u8>`) as a `simd_json::BorrowedValue`.
* `sonny_jim` parses a `&str` as a `sonny_jim::Value`, with allocations in a `sonny_jim::Arena`.

### Apple M2 Max - MacOS 15.0.1

```
Timer precision: 41 ns
parse                     fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ kube                                 │               │               │               │         │
│  ├─ serde               9.298 ms      │ 14.49 ms      │ 9.619 ms      │ 9.713 ms      │ 400     │ 2000
│  │                      alloc:        │               │               │               │         │
│  │                        101400      │ 101400        │ 101400        │ 101146        │         │
│  │                        14.09 MB    │ 14.09 MB      │ 14.09 MB      │ 14.05 MB      │         │
│  │                      dealloc:      │               │               │               │         │
│  │                        101400      │ 101400        │ 101400        │ 101146        │         │
│  │                        14.29 MB    │ 14.29 MB      │ 14.29 MB      │ 14.25 MB      │         │
│  │                      grow:         │               │               │               │         │
│  │                        1165        │ 1165          │ 1165          │ 1162          │         │
│  │                        201.8 KB    │ 201.8 KB      │ 201.8 KB      │ 201.3 KB      │         │
│  ├─ serde_raw           2.145 ms      │ 4.717 ms      │ 2.185 ms      │ 2.205 ms      │ 400     │ 2000
│  │                      alloc:        │               │               │               │         │
│  │                        1           │ 1             │ 1             │ 1             │         │
│  │                        8 B         │ 8 B           │ 8 B           │ 8 B           │         │
│  │                      dealloc:      │               │               │               │         │
│  │                        1           │ 1             │ 1             │ 1             │         │
│  │                        8 B         │ 8 B           │ 8 B           │ 8 B           │         │
│  ├─ simd_json_borrowed  4.704 ms      │ 8.57 ms       │ 4.811 ms      │ 4.901 ms      │ 400     │ 2000
│  │                      alloc:        │               │               │               │         │
│  │                        45891       │ 45891         │ 45891         │ 45891         │         │
│  │                        19.07 MB    │ 19.07 MB      │ 19.07 MB      │ 19.07 MB      │         │
│  │                      dealloc:      │               │               │               │         │
│  │                        45891       │ 45891         │ 45891         │ 45891         │         │
│  │                        24.96 MB    │ 24.96 MB      │ 24.96 MB      │ 24.96 MB      │         │
│  │                      grow:         │               │               │               │         │
│  │                        2           │ 2             │ 2             │ 2             │         │
│  │                        5.889 MB    │ 5.889 MB      │ 5.889 MB      │ 5.889 MB      │         │
│  ╰─ sonny_jim           4.255 ms      │ 4.765 ms      │ 4.326 ms      │ 4.354 ms      │ 400     │ 2000
│                         alloc:        │               │               │               │         │
│                           16          │ 16            │ 16            │ 16            │         │
│                           74.13 KB    │ 74.13 KB      │ 74.13 KB      │ 74.13 KB      │         │
│                         dealloc:      │               │               │               │         │
│                           16          │ 16            │ 16            │ 16            │         │
│                           2.47 MB     │ 2.47 MB       │ 2.47 MB       │ 2.47 MB       │         │
│                         grow:         │               │               │               │         │
│                           45          │ 45            │ 45            │ 45            │         │
│                           2.395 MB    │ 2.395 MB      │ 2.395 MB      │ 2.395 MB      │         │
╰─ small                                │               │               │               │         │
   ├─ serde               710.1 ns      │ 857 ns        │ 721.8 ns      │ 724.1 ns      │ 4000    │ 2000000
   │                      alloc:        │               │               │               │         │
   │                        11          │ 11            │ 11            │ 11            │         │
   │                        1.442 KB    │ 1.442 KB      │ 1.442 KB      │ 1.442 KB      │         │
   │                      dealloc:      │               │               │               │         │
   │                        11          │ 11            │ 11            │ 11            │         │
   │                        1.45 KB     │ 1.45 KB       │ 1.45 KB       │ 1.45 KB       │         │
   │                      grow:         │               │               │               │         │
   │                        1           │ 1             │ 1             │ 1             │         │
   │                        8 B         │ 8 B           │ 8 B           │ 8 B           │         │
   ├─ serde_raw           218.7 ns      │ 308.5 ns      │ 220.1 ns      │ 223.1 ns      │ 4000    │ 2000000
   │                      alloc:        │               │               │               │         │
   │                        1           │ 1             │ 1             │ 1             │         │
   │                        8 B         │ 8 B           │ 8 B           │ 8 B           │         │
   │                      dealloc:      │               │               │               │         │
   │                        1           │ 1             │ 1             │ 1             │         │
   │                        8 B         │ 8 B           │ 8 B           │ 8 B           │         │
   ├─ simd_json_borrowed  855.4 ns      │ 1.349 µs      │ 871.5 ns      │ 872.4 ns      │ 4000    │ 2000000
   │                      alloc:        │               │               │               │         │
   │                        12          │ 12            │ 12            │ 12            │         │
   │                        1.646 KB    │ 1.646 KB      │ 1.646 KB      │ 1.646 KB      │         │
   │                      dealloc:      │               │               │               │         │
   │                        12          │ 12            │ 12            │ 12            │         │
   │                        2.73 KB     │ 2.73 KB       │ 2.73 KB       │ 2.73 KB       │         │
   │                      grow:         │               │               │               │         │
   │                        4           │ 4             │ 4             │ 4             │         │
   │                        1.084 KB    │ 1.084 KB      │ 1.084 KB      │ 1.084 KB      │         │
   ╰─ sonny_jim           758.5 ns      │ 952.6 ns      │ 767.4 ns      │ 770.3 ns      │ 4000    │ 2000000
                          alloc:        │               │               │               │         │
                            8           │ 8             │ 8             │ 8             │         │
                            484 B       │ 484 B         │ 484 B         │ 484 B         │         │
                          dealloc:      │               │               │               │         │
                            8           │ 8             │ 8             │ 8             │         │
                            716 B       │ 716 B         │ 716 B         │ 716 B         │         │
                          grow:         │               │               │               │         │
                            3           │ 3             │ 3             │ 3             │         │
                            232 B       │ 232 B         │ 232 B         │ 232 B         │         │
```
