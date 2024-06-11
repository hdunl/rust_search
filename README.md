# Rust Search

Rust Search is a Rust-based application designed for efficient file searching within directories.

## Features

- **Parallel File Search**: Utilizes `rayon` for parallel file traversal and search, enhancing performance and reducing search times.
- **ZIP File Inspection**: Supports inspection of `.zip` files, searching for matching files within the archives.
- **Detailed Search Statistics**: Provides comprehensive search statistics, including total files scanned, matched files, total time taken, and processing speed.
- **File Explorer Integration**: Allows direct opening of file locations from the search results, facilitating easy file access.

## Dependencies

- [eframe](https://docs.rs/eframe/latest/eframe/): GUI framework for building native applications.
- [rayon](https://docs.rs/rayon/latest/rayon/): Data parallelism library for Rust.
- [walkdir](https://docs.rs/walkdir/latest/walkdir/): Recursive directory traversal library.
- [zip](https://docs.rs/zip/latest/zip/): ZIP archive reading library.
- [log](https://docs.rs/log/latest/log/): Logging framework for Rust applications.
- [native-dialog](https://docs.rs/native-dialog/latest/native_dialog/): Native file dialog for Rust.
