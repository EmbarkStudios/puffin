use std::{
    hash::Hasher,
    io::{Read, Write},
};

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

    for (i, (id, location)) in message_ids.zip(locations).enumerate().take(100_000) {
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
        compressed.len(),
        uncompressed.len() as f32 / compressed.len() as f32
    )
}

pub fn compression_comparison(c: &mut Criterion) {
    let test_stream = create_test_stream();

    // zstd via `zstd` crate
    {
        let encoded = zstd::stream::encode_all(test_stream.bytes(), 3).unwrap();

        c.bench_function("zstd encode", |b| {
            b.iter(|| {
                zstd::stream::encode_all(test_stream.bytes(), 3).unwrap();
            })
        });
        c.bench_function("zstd decode", |b| {
            b.iter(|| {
                zstd::stream::decode_all(encoded.as_slice()).unwrap();
            })
        });

        // sanity & size check
        let decoded = zstd::stream::decode_all(encoded.as_slice()).unwrap();
        assert_eq!(decoded, test_stream.bytes());
        assert_debug_snapshot!("zstd encode", report_compression(&test_stream, &encoded));
    }
    // brotli via `brotli` crate
    {
        let lgwin = 0; // Let compressor decide.
        let level = 2; // compression level 0-11, higher means densor/slower
        let buffer_size = test_stream.len();

        // Allocate buffers only once.
        let mut encoded = Vec::with_capacity(test_stream.len());
        let mut decoded = Vec::with_capacity(test_stream.len());

        // Using with_params with default params was too slow to finish.
        //let mut params = brotli::enc::BrotliEncoderParams::default();

        c.bench_function("brotli encode", |b| {
            b.iter(|| {
                encoded.clear();
                brotli::CompressorWriter::new(&mut encoded, buffer_size, level, lgwin)
                    .write_all(test_stream.bytes())
                    .unwrap();
            })
        });

        // Use this encoded data from here on out.
        encoded.clear();
        brotli::CompressorWriter::new(&mut encoded, buffer_size, level, lgwin)
            .write_all(test_stream.bytes())
            .unwrap();

        c.bench_function("brotli decode", |b| {
            b.iter(|| {
                decoded.clear();
                brotli::Decompressor::new(std::io::Cursor::new(&encoded), decoded.capacity())
                    .read_to_end(&mut decoded)
                    .unwrap();
            })
        });

        // sanity & size check
        decoded.clear();
        brotli::Decompressor::new(std::io::Cursor::new(&encoded), decoded.capacity())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, test_stream.bytes());
        assert_debug_snapshot!("brotli encode", report_compression(&test_stream, &encoded));
    }
}
