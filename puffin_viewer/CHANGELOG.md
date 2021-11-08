# Changelog

All notable changes to `puffin_viewer` will be documented in this file.


## Unreleased


## 0.9.2 - 2021-11-08
* Fix vertical scrolling in flamgraph


## 0.9.1 - 2021-11-05
* Normalize frame height based on what frames are visible.


## 0.9.0 - 2021-11-02
* Use [`egui_glow`](https://github.com/emilk/egui/tree/master/egui_glow) backend (slightly experimental, but compiles and runs faster).


## 0.8.0 - 2021-10-29
* Change `--file` option to instead be a positional argument.


## 0.7.1 - 2021-10-21
* Add a scope filter to focus on certain scopes.


## 0.7.0 - 2021-10-12
* You can now select multiple frames.


## 0.6.1 - 2021-09-08
* Fix deadlock when saving a file.


## 0.6.0 - 2021-09-06
* Better compressed network stream and files (50% smaller).
* Added stats view to find unnecessary scopes.


## 0.5.0
* Load and save recordings as `.puffin` files.


## 0.4.0
* Add support for compressed TCP stream (up to 75% bandwidth reduction).


## 0.3.0
First release: connect to a `puffin_server` over HTTP to live view a profiler stream
