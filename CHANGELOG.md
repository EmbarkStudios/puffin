# Changelog

All notable changes to `puffin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## Unreleased
* In-memory compression of frames to use up less RAM. Enabled by the new feature "packing".
* Remove support for legacy `.puffin` files in order to remove `lz4_flex` dependency.


## 0.11.0 - 2021-11-12
* Introduce `StreamInfoRef` to avoid memory allocations.
* Remove deprecated macros `profile_function_data` and `profile_scope_data`.


## 0.10.1 - 2021-11-02
* `now_ns` now returns nanoseconds since unix epoch.
* Make scope merging deterministic.


## 0.10.0 - 2021-10-12
* Rewrite scope merging.
* Implement `Hash` on `ThreadInfo`.


## 0.9.0 - 2021-09-20
* API change: split out new `FrameView` and `GlobalFrameView` from `GlobalProfiler`.


## 0.8.1 - 2021-09-07
* Remove profile scopes in serialization to avoid deadlock in `puffin_viewer`.


## 0.8.0 - 2021-09-06
* Switch from lz4 to zstd compression for 50% file size and bandwidth reduction.


## 0.7.0 - 2021-08-23
* Speed up `GlobalProfiler::new_frame`.
* New `serialization` feature flag enables exporting and importing `.puffin` files. This replaces the old `with_serde` feature flag.
* Add `GlobalProfiler::add_sink` for installing callbacks that are called each frame.


## 0.6.0 - 2021-07-05
* Handle Windows, which uses backslash (`\`) as path separator.


## 0.5.2
* Add opt-in `serde` support.


## 0.5.1
* Remove stderr warning about empty frames.


## 0.5.0
* `GlobalProfiler` now store recent history and the slowest frames.
