<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog
All notable changes to `puffin-imgui` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

<!-- next-header -->
## [Unreleased] - ReleaseDate
## [0.16.0] - 2022-02-07
### Changed
- [PR#64](https://github.com/EmbarkStudios/puffin/pull/64) updated dependencies and cleaned up crate metadata.

## [0.15.0] - 2021-11-16
### Changed
- In-memory compression of frames to use up less RAM.

## [0.14.0] - 2021-11-12
### Added
- Add slider for controlling number of frames recorded.
- Show total frames recorded and their total size.
- Add checkbox to toggle the profiling scopes.

### Changed
- Lower the default number of recorded frames to 600.

## [0.13.4] - 2021-11-05
### Fixed
- Normalize frame height based on what frames are visible.

## [0.13.3] - 2021-11-02
### Fixed
- Fix occasional flickering when viewing merged scopes.

## [0.13.2] - 2021-10-28
### Added
- Add `ProfilerUi::global_frame_view` to access the profiler data.

## [0.13.1] - 2021-10-21
### Added
- Add a scope filter to focus on certain scopes.

## [0.13.0] - 2021-10-12
### Fixed
- Nothing new

## [0.12.0] - 2021-09-20
### Changed
- Update to imgui 0.8.0

## [0.11.0] - 2021-09-06
### Changed
- Update puffin

## [0.10.0] - 2021-08-23
### Changed
- Show frame index.

### Fixed
- Fix "Toggle with spacebar." tooltip always showing.

## [0.9.0]
### Changed
- Paint flamegraph top-down
- Scrollable flamegraph
- More compact UI

### Added
- Option to sort threads by name
- Drag with right mouse button to zoom
- Toggle play/pause with spacebar
- Show all scopes (even tiny ones)

## [0.8.0]
### Added
- Select frames from recent history or from among the slowest ever.
- Nicer colors.
- Simpler interaction (drag to pan, scroll to zoom, click to focus, double-click to reset).

<!-- next-url -->
[Unreleased]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.16.0...HEAD
[0.16.0]: https://github.com/EmbarkStudios/puffin/compare/0.15.0...puffin-imgui-0.16.0
[0.15.0]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.14.0...puffin-imgui-0.15.0
[0.14.0]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.13.4...puffin-imgui-0.14.0
[0.13.4]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.13.3...puffin-imgui-0.13.4
[0.13.3]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.13.2...puffin-imgui-0.13.3
[0.13.2]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.13.1...puffin-imgui-0.13.2
[0.13.1]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.13.0...puffin-imgui-0.13.1
[0.13.0]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.12.0...puffin-imgui-0.13.0
[0.12.0]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.11.0...puffin-imgui-0.12.0
[0.11.0]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.10.0...puffin-imgui-0.11.0
[0.10.0]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.9.0...puffin-imgui-0.10.0
[0.9.0]: https://github.com/EmbarkStudios/puffin/compare/puffin-imgui-0.8.0...puffin-imgui-0.9.0
[0.8.0]: https://github.com/EmbarkStudios/puffin/releases/tag/puffin-imgui-0.8.0
