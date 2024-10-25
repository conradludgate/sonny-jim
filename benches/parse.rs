use divan::AllocProfiler;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

#[divan::bench_group(sample_count = 400, sample_size = 5)]
mod kube {
    use divan::{black_box, black_box_drop};
    use sonny_jim::Arena;

    const KUBE: &str = include_str!("../testdata/kubernetes-oapi.json");

    #[divan::bench]
    fn sonny_jim() {
        black_box_drop(sonny_jim::parse(black_box(&mut Arena::new(KUBE))));
    }

    #[divan::bench]
    fn serde_raw() {
        black_box_drop(serde_json::from_str::<&serde_json::value::RawValue>(
            black_box(KUBE),
        ));
    }

    #[divan::bench]
    fn serde() {
        black_box_drop(serde_json::from_str::<serde_json::value::Value>(black_box(
            KUBE,
        )));
    }

    #[divan::bench]
    fn simd_json_borrowed() {
        let mut d = black_box(KUBE).as_bytes().to_vec();
        let v: simd_json::BorrowedValue = simd_json::to_borrowed_value(&mut d).unwrap();
        black_box_drop(v);
    }
}

#[divan::bench_group(sample_count = 4000, sample_size = 500)]
mod small {
    use divan::{black_box, black_box_drop};
    use sonny_jim::Arena;

    const SMALL: &str = include_str!("../testdata/small.json");

    #[divan::bench]
    fn sonny_jim() {
        black_box_drop(sonny_jim::parse(black_box(&mut Arena::new(SMALL))));
    }

    #[divan::bench]
    fn serde_raw() {
        black_box_drop(serde_json::from_str::<&serde_json::value::RawValue>(
            black_box(SMALL),
        ));
    }

    #[divan::bench]
    fn serde() {
        black_box_drop(serde_json::from_str::<serde_json::value::Value>(black_box(
            SMALL,
        )));
    }

    #[divan::bench]
    fn simd_json_borrowed() {
        let mut d = black_box(SMALL).as_bytes().to_vec();
        let v: simd_json::BorrowedValue = simd_json::to_borrowed_value(&mut d).unwrap();
        black_box_drop(v);
    }
}

#[divan::bench_group(sample_count = 100, sample_size = 5)]
mod massive_stack {
    use divan::{black_box, black_box_drop, Bencher};
    use sonny_jim::Arena;

    const SIZE: usize = 500_000;

    #[divan::bench]
    fn sonny_jim(b: Bencher) {
        let input = {
            let first_half = "[".repeat(SIZE);
            let second_half = "]".repeat(SIZE);
            std::format!("{first_half}{second_half}")
        };

        b.with_inputs(|| &input).bench_local_values(|s| {
            black_box_drop(sonny_jim::parse(black_box(&mut Arena::new(s))));
        });
    }

    #[divan::bench]
    fn serde_raw(b: Bencher) {
        let input = {
            let first_half = "[".repeat(SIZE);
            let second_half = "]".repeat(SIZE);
            std::format!("{first_half}{second_half}")
        };

        b.with_inputs(|| &input).bench_local_values(|s| {
            black_box_drop(serde_json::from_str::<&serde_json::value::RawValue>(
                black_box(s),
            ));
        });
    }

    #[divan::bench]
    fn serde() {
        // we cannot just parse such a large string with serde_json::Value.
        // so we have to be creative.

        let mut v = serde_json::Value::Array(black_box(vec![]));
        for _ in 1..SIZE {
            v = serde_json::Value::Array(vec![v]);
        }
        let mut v = black_box(v);

        // we also have to creative to not stack overflow on drop...
        loop {
            let serde_json::Value::Array(mut a) = v else { break };
            let Some(b) = a.pop() else { break };
            v = b;
        }
    }
}
