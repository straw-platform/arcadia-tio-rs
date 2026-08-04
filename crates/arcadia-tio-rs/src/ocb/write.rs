use super::*;

/// Create an appendable OCB file and publish its first committed root.
pub fn create(path: impl AsRef<Path>, spec: &WriteSpec) -> OcbResult<()> {
    write_path(path, spec, true, None)
}

/// Create an appendable OCB file with explicit writer options.
pub fn create_with_options(
    path: impl AsRef<Path>,
    spec: &WriteSpec,
    options: WriteOptions,
) -> OcbResult<()> {
    write_path(path, spec, true, Some(options))
}

/// Create an appendable OCB file and return diagnostic counters.
pub fn create_with_report(
    path: impl AsRef<Path>,
    spec: &WriteSpec,
    options: WriteOptions,
) -> OcbResult<WriteReport> {
    write_path_report(path, spec, true, options)
}

/// Append one sorted suffix commit to an existing appendable OCB file.
pub fn append(path: impl AsRef<Path>, spec: &WriteSpec) -> OcbResult<()> {
    write_path(path, spec, false, None)
}

/// Append one sorted suffix commit with explicit writer options.
pub fn append_with_options(
    path: impl AsRef<Path>,
    spec: &WriteSpec,
    options: WriteOptions,
) -> OcbResult<()> {
    write_path(path, spec, false, Some(options))
}

/// Append one sorted suffix commit and return diagnostic counters.
pub fn append_with_report(
    path: impl AsRef<Path>,
    spec: &WriteSpec,
    options: WriteOptions,
) -> OcbResult<WriteReport> {
    write_path_report(path, spec, false, options)
}
