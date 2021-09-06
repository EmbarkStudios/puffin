# Changelog

All notable changes to `puffin_http` will be documented in this file.


## Unreleased


## 0.5.0 - 2021-09-06
* Switch from lz4 to zstd compression for 50% file size and bandwidth reduction.


## 0.4.1 - 2021-08-24
* Do less work when no clients are connected.


## 0.4.0 - 2021-08-23
* Remove `Server::update` (no longer needed).
* Compress the TCP stream (approximately 75% bandwidth reduction).


## 0.3.0
* Initial release

