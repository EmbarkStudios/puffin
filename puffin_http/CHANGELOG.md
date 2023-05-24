<!-- markdownlint-disable blanks-around-headings blanks-around-lists no-duplicate-heading -->

# Changelog

All notable changes to `puffin_http` will be documented in this file.

<!-- next-header -->
## [Unreleased] - ReleaseDate
- Upgrade to `puffin` 0.16

## [0.12.0] - 2023-04-24
## [0.11.1] - 2023-01-30
- Upgrade to `puffin` 0.14.2

## [0.11.0] - 2022-11-07
- Update to puffin 0.14.0

## [0.10.1] - 2022-10-17
- Update crates: ruzstd, criterion, rfd

## [0.10.0] - 2022-02-07
### Changed
- [PR#64](https://github.com/EmbarkStudios/puffin/pull/64) updated dependencies and cleaned up crate metadata.

## [0.9.0] - 2021-11-16
### Changed
- In-memory compression of frames to use up less RAM.

## [0.8.0] - 2021-11-12
### Changed
- Update to puffin 0.11.0.

## [0.7.3] - 2021-11-08
### Added
- Add `Server::num_clients`.

## [0.7.2] - 2021-10-28
### Fixed
- Send all outstanding frames on shutdown.

## [0.7.0] - 2021-10-12
### Fixed
- Nothing new.

## [0.6.0] - 2021-09-20
### Changed
- Better handle slow clients, especially when there are multiple clients.

## [0.5.1] - 2021-09-16
### Fixed
- Fix high-bandwidth connection interruptions.

## [0.5.0] - 2021-09-06
### Changed
- Switch from lz4 to zstd compression for 50% file size and bandwidth reduction.

## [0.4.1] - 2021-08-24
### Fixed
- Do less work when no clients are connected.

## [0.4.0] - 2021-08-23
### Changed
- Remove `Server::update` (no longer needed).
- Compress the TCP stream (approximately 75% bandwidth reduction).

## [0.3.0]
### Added
- Initial release

<!-- next-url -->
[Unreleased]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.12.0...HEAD
[0.12.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.11.1...puffin_http-0.12.0
[0.11.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.11.0...puffin_http-0.11.1
[0.10.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.10.1...puffin_http-0.11.0
[0.10.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.10.0...puffin_http-0.10.1
[0.10.0]: https://github.com/EmbarkStudios/puffin/compare/0.9.0...puffin_http-0.10.0
[0.9.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.8.0...puffin_http-0.9.0
[0.8.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.7.3...puffin_http-0.8.0
[0.7.3]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.7.2...puffin_http-0.7.3
[0.7.2]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.7.0...puffin_http-0.7.2
[0.7.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.6.0...puffin_http-0.7.0
[0.6.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.5.1...puffin_http-0.6.0
[0.5.1]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.5.0...puffin_http-0.5.1
[0.5.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.4.0...puffin_http-0.5.0
[0.4.0]: https://github.com/EmbarkStudios/puffin/compare/puffin_http-0.3.0...puffin_http-0.4.0
[0.3.0]: https://github.com/EmbarkStudios/puffin/releases/tag/puffin_http-0.3.0
