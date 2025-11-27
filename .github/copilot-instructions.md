# Project-Specific Guidelines

## Overview
This project is an editor that aims to provide typical text editing features
as known from visual editors like Notepad++ but runs in the terminal. I don't
plan to provide APIs for external use, so there is no need for public
functions, structs, or modules.

When changing code, always check that tests still pass and the compiler shows
no warnings (not in build and not in test). If you add new features, please
also add tests for them.

## Rust-Specific Guidelines

- **Edition**: Use Rust 2024 edition or later when specified in Cargo.toml
`- **No public APIs**: Internal tools don't need public functions, structs, or modules
    - Use `pub(crate)` only when needed for cross-module access
- **Dependencies**: Prefer well-maintained, minimal dependencies
- **Error handling**: Use `Result` and `?` operator, avoid `unwrap()` in production code
- **Formatting**: Run `cargo fmt` before committing

## Documentation

- Update README.md when adding user-facing features
- Update ARCHITECTURE.md when changing module structure
- Keep inline comments focused and meaningful
- Document configuration options in both code and settings files