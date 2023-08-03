# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2023-08-03

### Changed
- `ConcurrentSpec` trait now has the sequential specification as an associated type and reuses the `Op` and `Ret` types from it.
- `new()` method in the `SequentialSpec` and `ConcurrentSpec` traits was deleted in favor of the `Default` trait.
- Bumped `loom` version to 0.6.

## [0.1.1] - 2023-07-14

### Added
- More badges to README.md.
- Automatic releasing infrastructure. 

## [0.1.0] - 2023-07-14

### Added
- Initial release.