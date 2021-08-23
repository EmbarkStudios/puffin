# Changelog
All notable changes to `puffin` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

* Speed up `GlobalProfiler::new_frame`.


## 0.6.0 - 2021-07-05

* Handle Windows, which uses backslash (`\`) as path separator.


## 0.5.2

* Add opt-in `serde` support.


## 0.5.1

* Remove stderr warning about empty frames.


## 0.5.0

* `GlobalProfiler` now store recent history and the slowest frames.
