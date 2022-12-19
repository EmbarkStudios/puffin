<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# `egui_puffin` changelog

All notable changes to the egui crate will be documented in this file.

<!-- next-header -->
## [Unreleased] - ReleaseDate

- [PR#115](https://github.com/EmbarkStudios/puffin/pull/115) Fix broken flamegraph interaction
- Change `chrono` to `time`

## [0.19.0] - 2022-12-13

- [PR#112](https://github.com/EmbarkStudios/puffin/pull/112) You can now compile and run `puffin_egui` on the web
- Upgrade to `egui` 0.20
- Upgrade to `eframe` 0.20

## [0.18.0] - 2022-11-08

- Require `puffin` 0.14.0

## [0.17.0] - 2022-10-17

- Add ability to hide and show thread lanes.
- Add ability to collapse thread lanes.
- Add ability double click on scope in thread lane applies the scope as filter.
- [PR#93](https://github.com/EmbarkStudios/puffin/pull/93) Update to `egui` 0.19.
- Add scope filter option for stats panel.

## [0.16.0] - 2022-06-22
### Changed
- [PR#87](https://github.com/EmbarkStudios/puffin/pull/87) Only run pack passes if packing is enabled on the view

## [0.15.0] - 2022-05-11
- [PR#74](https://github.com/EmbarkStudios/puffin/pull/74) Update to `egui` 0.18.

## [0.14.0] - 2022-04-12
- [PR#71](https://github.com/EmbarkStudios/puffin/pull/71) Update to `egui` 0.17.

## [0.13.0] - 2022-02-07ß
### Changed
- [PR#64](https://github.com/EmbarkStudios/puffin/pull/64) updated dependencies and cleaned up crate metadata.

## [0.12.0] - 2022-01-11
### Changed
- Update to `egui` 0.16.

### Fixed
- Fix compilation for `wasm32-unknown-unknown`.

## [0.11.0] - 2021-11-16
### Added
- Show total frames recorded and their total size.
- Add slider to control how many recent frames to store.

### Fixed
- In-memory compression of frames to use up less RAM.

## [0.10.3] - 2021-11-08
### Fixed
- Fix vertical scrolling in flamgraph.

### Added
- Show thread names in stats tab.

## [0.10.2] - 2021-11-05
### Fixed
- Normalize frame height based on what frames are visible.

## [0.10.1] - 2021-11-02
### Added
- Show scrollbar for history of recent frames.
- Show date-time of when a frame was recorded.
- Show compressed size of selected frame.

### Fixed
- Fix occasional flickering when viewing merged scopes.
- Handle gaps in incoming frames.

## [0.10.0] - 2021-10-29
### Changed
- Update to egui 0.15.

## [0.9.1] - 2021-10-21
### Added
- Add a scope filter to focus on certain scopes.

## [0.9.0] - 2021-10-12
### Added
- You can now select multiple frames.

## [0.8.0] - 2021-09-20
### Changed
- `ProfilerUi` now takes by argument the profiling data to view. You may want to use `GlobalProfilerUi` instead.

## [0.7.0] - 2021-09-06
### Added
- Add a stats panel for finding high-bandwidth scopes.

## [0.6.0] - 2021-08-25
### Changed
- Update to egui 0.14

## [0.5.0] - 2021-08-23
### Changed
- Show frame index.

## [0.4.0] - 2021-07-05
### Changed
- Update to egui 0.13
- Paint flamegraph top-down
- More compact UI
- Show all scopes (even tiny ones)

### Added
- Scrollable flamegraph
- Option to sort threads by name
- Drag with right mouse button to zoom
- Toggle play/pause with spacebar

## [0.3.0] - 2021-05-27
### Added
- History viewer.

### Changed
- Update to puffin 0.5.1.

## [0.2.0] - 2021-05-13
### Changed
- Update to egui 0.12.
- Remove drag-to-zoom (scroll to zoom instead).

## [0.1.0] - 2021-05-05
### Added
- Show flamegraph plot of either latest frame, a spike frame, or a paused frame.
- The view supports viewing merged sibling scopes.

<!-- next-url -->
[Unreleased]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.19.0...HEAD
[0.19.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.17.0...puffin_egui-0.19.0
[0.17.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.15.0...puffin_egui-0.17.0
[0.15.0]: https://github.com/EmbarkStudios/puffin/compare/0.14.0...puffin_egui-0.15.0
[0.14.0]: https://github.com/EmbarkStudios/puffin/compare/0.13.0...puffin_egui-0.14.0
[0.13.0]: https://github.com/EmbarkStudios/puffin/compare/0.12.0...puffin_egui-0.13.0
[0.12.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.11.0...puffin_egui-0.12.0
[0.11.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.10.3...puffin_egui-0.11.0
[0.10.3]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.10.2...puffin_egui-0.10.3
[0.10.2]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.10.1...puffin_egui-0.10.2
[0.10.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.10.0...puffin_egui-0.10.1
[0.10.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.9.0...puffin_egui-0.10.0
[0.9.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.9.0...puffin_egui-0.9.1
[0.9.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.8.0...puffin_egui-0.9.0
[0.8.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.7.0...puffin_egui-0.8.0
[0.7.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.6.0...puffin_egui-0.7.0
[0.6.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.5.0...puffin_egui-0.6.0
[0.5.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.4.0...puffin_egui-0.5.0
[0.4.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.3.0...puffin_egui-0.4.0
[0.3.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.2.1...puffin_egui-0.3.0
[0.2.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_egui-0.1.0...puffin_egui-0.2.1
[0.1.0]: https://github.com/EmbarkStudios/puffin/releases/tag/puffin_egui-0.1.0
