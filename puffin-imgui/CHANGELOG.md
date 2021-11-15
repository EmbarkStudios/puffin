# Changelog
All notable changes to `puffin-imgui` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## Unreleased
* In-memory compression of frames to use up less RAM.


## 0.14.0 - 2021-11-12
* Add slider for controlling number of frames recorded.
* Show total frames recorded and their total size.
* Lower the default number of recorded frames to 600.
* Add checkbox to toggle the profiling scopes.


## 0.13.4 - 2021-11-05
* Normalize frame height based on what frames are visible.


## 0.13.3 - 2021-11-02
* Fix occasional flickering when viewing merged scopes.


## 0.13.2 - 2021-10-28
* Add `ProfilerUi::global_frame_view` to access the profiler data.


## 0.13.1 - 2021-10-21
* Add a scope filter to focus on certain scopes.


## 0.13.0 - 2021-10-12
* Nothing new


## 0.12.0 - 2021-09-20
* Update to imgui 0.8.0


## 0.11.0 - 2021-09-06
* Update puffin


## 0.10.0 - 2021-08-23
* Fix "Toggle with spacebar." tooltip always showing.
* Show frame index.


## 0.9.0
* Paint flamegraph top-down
* Scrollable flamegraph
* Option to sort threads by name
* Drag with right mouse button to zoom
* Toggle play/pause with spacebar
* More compact UI
* Show all scopes (even tiny ones)


## 0.8.0
* Select frames from recent history or from among the slowest ever.
* Nicer colors.
* Simpler interaction (drag to pan, scroll to zoom, click to focus, double-click to reset).
