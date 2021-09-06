# Changelog

All notable changes to `puffin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## Unreleased


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
