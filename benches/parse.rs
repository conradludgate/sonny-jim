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
