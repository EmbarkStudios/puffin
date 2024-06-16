<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to `puffin_viewer` will be documented in this file.

<!-- next-header -->
## [Unreleased] - ReleaseDate

- [PR#211](https://github.com/EmbarkStudios/puffin/pull/211/) Fix broken flamegraph interaction with egui 0.27.1 ()

## [0.21.0] - 2024-04-06
- [PR#201](https://github.com/EmbarkStudios/puffin/pull/201) Update to egui `0.27`

## [0.20.0] - 2024-02-14

- [PR#188](https://github.com/EmbarkStudios/puffin/pull/188) Update to `puffin_egui` version `0.26`
- [PR#184](https://github.com/EmbarkStudios/puffin/pull/184) Propagate errors from `eframe::run_native`

## [0.19.0] - 2024-01-17

- [PR#179](https://github.com/EmbarkStudios/puffin/pull/179) Update to `puffin_egui` version `0.25`

## [0.18.0] - 2023-11-24
- [PR#161](https://github.com/EmbarkStudios/puffin/pull/166) Update to egui and eframe `0.24`

## [0.17.0] - 2023-09-28

- [PR#161](https://github.com/EmbarkStudios/puffin/pull/161) Update to egui `0.23`

## [0.16.0] - 2023-05-24

* Upgrade to `eframe` 0.22
- Upgrade to `puffin` 0.16
-
## [0.15.0] - 2023-04-24
## [0.14.0] - 2023-02-09
* Upgrade to `puffin_egui` 0.21

## [0.13.2] - 2023-01-30

- Upgrade to `puffin_egui` 0.19.2
- Upgrade to `puffin` 0.14.2
## [0.13.1] - 2023-01-20

- [PR#115](https://github.com/EmbarkStudios/puffin/pull/115) Fix broken flamegraph interaction

## [0.13.0] - 2022-12-13

- Upgrade to `puffin_egui` 0.19.1
- Upgrade to `eframe` 0.20
- **Breaking:** WASM32 `start()` function in crate root is now async.

## [0.12.1] - 2022-10-17
- [PR#93](https://github.com/EmbarkStudios/puffin/pull/93) Update to `egui` 0.19.

## [0.12.0] - 2022-05-11
- [PR#74](https://github.com/EmbarkStudios/puffin/pull/74) Update to `egui` 0.18.

## [0.11.0] - 2022-02-07
### Changed
- [PR#64](https://github.com/EmbarkStudios/puffin/pull/64) updated dependencies and cleaned up crate metadata.

## [0.10.1] - 2022-01-11
### Changed
- Update to latest `eframe` and `winit`
- Switch renderer from `egui_glium` to `egui_glow`.

## [0.10.0] - 2021-11-16
### Changed
- In-memory compression of frames to use up less RAM.

### Added
- Add slider to control how many recent frames to store.
- Add ability to profile `puffin_viewer` from itself.

## [0.9.2] - 2021-11-08
### Fixed
- Fix vertical scrolling in flamgraph

## [0.9.1] - 2021-11-05
### Fixed
- Normalize frame height based on what frames are visible.

## [0.9.0] - 2021-11-02
### Changed
- Use [`egui_glow`](https://github.com/emilk/egui/tree/master/egui_glow) backend (slightly experimental, but compiles and runs faster).

## [0.8.0] - 2021-10-29
### Changed
- Change `--file` option to instead be a positional argument.

## [0.7.1] - 2021-10-21
### Added
- Add a scope filter to focus on certain scopes.

## [0.7.0] - 2021-10-12
### Added
- You can now select multiple frames.

## [0.6.1] - 2021-09-08
### Fixed
- Fix deadlock when saving a file.

## [0.6.0] - 2021-09-06
### Changed
- Better compressed network stream and files (50% smaller).

### Added
- Added stats view to find unnecessary scopes.

## [0.5.0]
### Added
- Load and save recordings as `.puffin` files.

## [0.4.0]
### Added
- Add support for compressed TCP stream (up to 75% bandwidth reduction).

## [0.3.0]
### Added
First release: connect to a `puffin_server` over HTTP to live view a profiler stream

<!-- next-url -->
[Unreleased]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.21.0...HEAD
[0.21.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.20.0...puffin_viewer-0.21.0
[0.20.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.19.0...puffin_viewer-0.20.0
[0.19.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.18.0...puffin_viewer-0.19.0
[0.18.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.17.0...puffin_viewer-0.18.0
[0.17.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.16.0...puffin_viewer-0.17.0
[0.16.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.15.0...puffin_viewer-0.16.0
[0.15.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.14.0...puffin_viewer-0.15.0
[0.14.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.13.2...puffin_viewer-0.14.0
[0.13.2]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.13.2...puffin_viewer-0.13.2
[0.13.2]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.13.1...puffin_viewer-0.13.2
[0.13.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.13.0...puffin_viewer-0.13.1
[0.13.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.12.1...puffin_viewer-0.13.0
[0.12.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.12.0...puffin_viewer-0.12.1
[0.12.0]: https://github.com/EmbarkStudios/puffin/compare/0.11.0...puffin_viewer-0.12.0
[0.11.0]: https://github.com/EmbarkStudios/puffin/compare/0.10.1...puffin_viewer-0.11.0
[0.10.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.10.0...puffin_viewer-0.10.1
[0.10.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.9.2...puffin_viewer-0.10.0
[0.9.2]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.9.1...puffin_viewer-0.9.2
[0.9.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.9.0...puffin_viewer-0.9.1
[0.9.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.8.0...puffin_viewer-0.9.0
[0.8.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.7.1...puffin_viewer-0.8.0
[0.7.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.7.0...puffin_viewer-0.7.1
[0.7.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.6.1...puffin_viewer-0.7.0
[0.6.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.6.0...puffin_viewer-0.6.1
[0.6.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.5.0...puffin_viewer-0.6.0
[0.5.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.4.0...puffin_viewer-0.5.0
[0.4.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.3.0...puffin_viewer-0.4.0
[0.3.0]: https://github.com/EmbarkStudios/puffin/releases/tag/puffin_viewer-0.3.0
