use std::hash::Hasher;

use criterion::Criterion;
use puffin::Stream;

use insta::assert_debug_snapshot;

pub fn create_test_stream() -> Stream {
    let mut stream = Stream::default();

    // Cycle through a bunch of message ids & location names so it's *absolutely* trivially compressible,
    // but also has plenty of opportunity to so compress.
    let id_strings = [
        "my_function",
        "something_with_a_really_long_name",
        "foo",
        "bar",
        "hello",
    ];
    let message_ids =
        std::iter::repeat((0..157).map(|i| format!("{}_{i}", id_strings[i % id_strings.len()])))
            .flatten();
    let location_strings = [
        "foobar.rs",
        "wumpf.rs",
        "mod.rs",
        "lib.rs",
        "compression.rs",
        "very_ominous_name_for_a_location.rs",
    ];
    let locations = std::iter::repeat(
        (0..173).map(|i| format!("{}:{i}", location_strings[i % location_strings.len()])),
    )
    .flatten();

    for (i, (id, location)) in message_ids.zip(locations).enumerate().take(1_000_000) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write_usize(i);
        let data = format!("{}", hasher.finish());

        let start_offset = stream.begin_scope((i * 2) as _, &id, &location, &data);
        stream.end_scope(start_offset, (i * 2 + 1) as _);
    }

    stream
}

fn report_compression(uncompressed: &Stream, compressed: &[u8]) -> String {
    format!(
        "{:?}bytes - ratio {:.2}",
        uncompressed.len(),
        uncompressed.len() as f32 / compressed.len() as f32
    )
}

pub fn compression_comparison(c: &mut Criterion) {
    let test_stream = create_test_stream();

    // zstd
    {
        c.bench_function("zstd encode", |b| {
            b.iter(|| {
                zstd::stream::encode_all(test_stream.bytes(), 3).unwrap();
            })
        });
        c.bench_function("zstd decode", |b| {
            let encoded = zstd::stream::encode_all(test_stream.bytes(), 3).unwrap();
            b.iter(|| {
                zstd::stream::decode_all(encoded.as_slice()).unwrap();
            })
        });

        // sanity & size check
        let encoded = zstd::stream::encode_all(test_stream.bytes(), 3).unwrap();
        let decoded = zstd::stream::decode_all(encoded.as_slice()).unwrap();
        assert_eq!(decoded, test_stream.bytes());
        assert_debug_snapshot!("zstd encode", report_compression(&test_stream, &encoded));
    }
}
