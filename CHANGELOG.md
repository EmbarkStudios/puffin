<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->
# Changelog

All notable changes to `puffin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## [Unreleased] - ReleaseDate
## [0.16.0] - 2023-05-24

- [PR#136](https://github.com/EmbarkStudios/puffin/pull/136) Upgrade `ruzstd` to 0.4
- [PR#135](https://github.com/EmbarkStudios/puffin/pull/135) Allow picking `lz4_flex` and `zstd` compression by feature flag.

## [0.15.0] - 2023-04-24

- [PR#118](https://github.com/EmbarkStudios/puffin/pull/118) Updated `zstd` `0.12.3`

## [0.14.3] - 2023-02-09
- [PR#117](https://github.com/EmbarkStudios/puffin/pull/117) Add function `latest_frames` to retrieve latest _n_ captured frames.

## [0.14.2] - 2023-01-30

- [PR#123](https://github.com/EmbarkStudios/puffin/pull/123) Fix `puffin` build for non-web wasm enviroments. 

## [0.14.1] - 2022-12-13

- [PR#112](https://github.com/EmbarkStudios/puffin/pull/112) You can now compile and run `puffin` on the web if you enable the "web" feature.

## [0.14.0] - 2022-11-07

### Fixed
* [PR#102](https://github.com/EmbarkStudios/puffin/pull/102) Add a runtime setting on frameview to pack or not

## [0.13.3] - 2022-05-04
### Changed
* [PR#83](https://github.com/EmbarkStudios/puffin/pull/83) Add a runtime setting on frameview to pack or not

## [0.13.2] - 2022-05-04
### Changed
* [PR#76](https://github.com/EmbarkStudios/puffin/pull/76) updated `zstd` to `0.11.1`

## [0.13.0] - 2022-02-07
### Fixed
* Fix compilation for `wasm32-unknown-unknown`.

### Changed
* Upgrade `ztd` v0.9 -> v0.10
* [PR#64](https://github.com/EmbarkStudios/puffin/pull/64) updated dependencies and cleaned up crate metadata.

## [0.12.1] - 2021-11-16
### Fixed
* Make `parking_lot` an optional dependency.

## [0.12.0] - 2021-11-16
### Fixed
* In-memory compression of frames to use up less RAM. Enabled by the new feature "packing".

### Changed
* Remove support for legacy `.puffin` files in order to remove `lz4_flex` dependency.

## [0.11.0] - 2021-11-12
### Changed
* Introduce `StreamInfoRef` to avoid memory allocations.
* Remove deprecated macros `profile_function_data` and `profile_scope_data`.

## [0.10.1] - 2021-11-02
### Fixed
* `now_ns` now returns nanoseconds since unix epoch.
* Make scope merging deterministic.

## [0.10.0] - 2021-10-12
### Changed
* Rewrite scope merging.
* Implement `Hash` on `ThreadInfo`.

## [0.9.0] - 2021-09-20
### Changed
* API change: split out new `FrameView` and `GlobalFrameView` from `GlobalProfiler`.

## [0.8.1] - 2021-09-07
### Fixed
* Remove profile scopes in serialization to avoid deadlock in `puffin_viewer`.

## [0.8.0] - 2021-09-06
### Changed
* Switch from lz4 to zstd compression for 50% file size and bandwidth reduction.

## [0.7.0] - 2021-08-23
### Changed
* Speed up `GlobalProfiler::new_frame`.
* New `serialization` feature flag enables exporting and importing `.puffin` files. This replaces the old `with_serde` feature flag.

### Added
* Add `GlobalProfiler::add_sink` for installing callbacks that are called each frame.

## [0.6.0] - 2021-07-05
### Fixed
* Handle Windows, which uses backslash (`\`) as path separator.

## [0.5.2] - 2021-14-27
### Changed
* Add opt-in `serde` support.

## [0.5.1] - 2021-05-27
### Fixed
* Remove stderr warning about empty frames.

## [0.5.0] - 2021-05-27
### Changed
* `GlobalProfiler` now store recent history and the slowest frames.

<!-- next-url -->
[Unreleased]: https://github.com/EmbarkStudios/puffin/compare/0.16.0...HEAD
[0.16.0]: https://github.com/EmbarkStudios/puffin/compare/0.15.0...0.16.0
[0.15.0]: https://github.com/EmbarkStudios/puffin/compare/0.14.3...0.15.0
[0.14.3]: https://github.com/EmbarkStudios/puffin/compare/0.14.2...0.14.3
[0.14.2]: https://github.com/EmbarkStudios/puffin/compare/0.14.1...0.14.2
[0.14.1]: https://github.com/EmbarkStudios/puffin/compare/0.14.0...0.14.1
[0.14.0]: https://github.com/EmbarkStudios/puffin/compare/0.13.2...0.14.0
[0.13.2]: https://github.com/EmbarkStudios/puffin/compare/0.13.0...0.13.2
[0.13.0]: https://github.com/EmbarkStudios/puffin/compare/0.12.1...0.13.0
[0.12.1]: https://github.com/EmbarkStudios/puffin/compare/0.12.0...0.12.1
[0.12.0]: https://github.com/EmbarkStudios/puffin/compare/0.11.0...0.12.0
[0.11.0]: https://github.com/EmbarkStudios/puffin/compare/0.10.1...0.11.0
[0.10.1]: https://github.com/EmbarkStudios/puffin/compare/0.10.0...0.10.1
[0.10.0]: https://github.com/EmbarkStudios/puffin/compare/0.9.0...0.10.0
[0.9.0]: https://github.com/EmbarkStudios/puffin/compare/0.8.1...0.9.0
[0.8.1]: https://github.com/EmbarkStudios/puffin/compare/0.8.0...0.8.1
[0.8.0]: https://github.com/EmbarkStudios/puffin/compare/0.7.0...0.8.0
[0.7.0]: https://github.com/EmbarkStudios/puffin/compare/0.6.0...0.7.0
[0.6.0]: https://github.com/EmbarkStudios/puffin/compare/0.5.1...0.6.0
[0.5.2]: https://github.com/EmbarkStudios/puffin/compare/0.5.1...0.5.2
[0.5.1]: https://github.com/EmbarkStudios/puffin/compare/0.5.0...0.5.1
[0.5.0]: https://github.com/EmbarkStudios/puffin/releases/tag/0.5.0
