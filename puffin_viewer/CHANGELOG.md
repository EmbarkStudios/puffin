<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to `puffin_viewer` will be documented in this file.

<!-- next-header -->
## [Unreleased] - ReleaseDate

## [0.12.0] - 2022-05-11
- [PR#74](https://github.com/EmbarkStudios/puffin/pull/74) Update to `egui` 0.18.

## [0.11.0] - 2022-02-07
### Changed
- [PR#64](https://github.com/EmbarkStudios/puffin/pull/64) updated dependencies and cleaned up crate metadata.

## [0.10.1] - 2022-01-11
### Changed
- Update to latest `eframe` and `winit`
- Swich renderer from `egui_glium` to `egui_glow`.

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
[Unreleased]: https://github.com/EmbarkStudios/puffin/compare/puffin_viewer-0.12.0...HEAD
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
