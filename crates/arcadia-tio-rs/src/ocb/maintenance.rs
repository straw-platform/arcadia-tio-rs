use super::*;

/// Certify one local compact-L2 physical-v2 OCB artifact.
///
/// This is a read-only control-plane check. It does not expose hot typed
/// reads, manifest expansion, remote fetch, or performance claims.
pub fn certify_compact_l2_physical_v2_artifact(
    path: impl AsRef<Path>,
    options: CompactL2PhysicalV2ArtifactCertificationOptions,
) -> OcbResult<CompactL2PhysicalV2ArtifactCertificationReport> {
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let expected_hash = options
        .expected_legacy_payload_hash_fnv1a64
        .as_deref()
        .map(CString::new)
        .transpose()
        .map_err(|_| OcbError::invalid_input("expected legacy payload hash contains NUL"))?;
    let raw_options = raw_compact_l2_physical_v2_artifact_certification_options(
        &options,
        expected_hash
            .as_ref()
            .map_or(ptr::null(), |hash| hash.as_ptr()),
    );
    let mut raw_report = empty_compact_l2_physical_v2_artifact_certification_report();
    let status = unsafe {
        sys::arcadia_tio_ocb_certify_compact_l2_physical_v2_artifact(
            path.as_ptr(),
            &raw_options,
            &mut raw_report,
        )
    };
    let guard = CompactL2PhysicalV2ArtifactCertificationReportGuard(raw_report);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last(
            "OCB compact-L2 physical-v2 artifact certification failed",
        ));
    }
    unsafe { compact_l2_physical_v2_artifact_certification_report_from_raw(&guard.0) }
}

/// Truncate orphan tail bytes after the latest valid appendable OCB root.
pub fn cleanup_orphan_tail(path: impl AsRef<Path>) -> OcbResult<CleanupResult> {
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let mut raw = sys::ArcadiaTioOcbCleanupResult {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbCleanupResult>(),
        truncated: 0,
        reserved: [0; 3],
    };
    let status = unsafe { sys::arcadia_tio_ocb_cleanup_orphan_tail(path.as_ptr(), &mut raw) };
    if status == sys::ARCADIA_TIO_ERROR_OK {
        Ok(CleanupResult {
            truncated: raw.truncated != 0,
        })
    } else {
        Err(OcbError::last("OCB cleanup_orphan_tail failed"))
    }
}

/// Analyze selected-snapshot root state and orphan-tail cleanup need without mutation.
pub fn maintenance_analyze(path: impl AsRef<Path>) -> OcbResult<MaintenanceReport> {
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let mut raw_report = empty_maintenance_report();
    let status =
        unsafe { sys::arcadia_tio_ocb_maintenance_analyze(path.as_ptr(), &mut raw_report) };
    let guard = MaintenanceReportGuard(raw_report);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last("OCB maintenance_analyze failed"));
    }
    unsafe { maintenance_report_from_raw(&guard.0) }
}

/// Truncate orphan tail bytes and return a structured cleanup report.
pub fn cleanup_orphan_tail_report(path: impl AsRef<Path>) -> OcbResult<CleanupReport> {
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let mut raw_report = empty_cleanup_report();
    let status =
        unsafe { sys::arcadia_tio_ocb_cleanup_orphan_tail_report(path.as_ptr(), &mut raw_report) };
    let guard = CleanupReportGuard(raw_report);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last("OCB cleanup_orphan_tail_report failed"));
    }
    unsafe { cleanup_report_from_raw(&guard.0) }
}

/// Copy one source file's selected committed OCB snapshot to a new destination file.
///
/// The source file is never modified. Orphan tail bytes beyond the selected snapshot
/// boundary are excluded from the destination and reported.
pub fn copy_selected_snapshot(
    source_path: impl AsRef<Path>,
    destination_path: impl AsRef<Path>,
    options: SnapshotExportOptions,
) -> OcbResult<SnapshotExportReport> {
    let source_path = path_to_cstring(source_path).map_err(OcbError::from_tio_error)?;
    let destination_path = path_to_cstring(destination_path).map_err(OcbError::from_tio_error)?;
    let raw_options = raw_snapshot_export_options(options);
    let mut raw_report = empty_snapshot_export_report();
    let status = unsafe {
        sys::arcadia_tio_ocb_copy_selected_snapshot(
            source_path.as_ptr(),
            destination_path.as_ptr(),
            &raw_options,
            &mut raw_report,
        )
    };
    let guard = SnapshotExportReportGuard(raw_report);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last("OCB copy_selected_snapshot failed"));
    }
    unsafe { snapshot_export_report_from_raw(&guard.0) }
}

/// Build a generic selected-snapshot manifest from compatible local OCB files.
pub fn build_manifest_from_files<P, I>(
    manifest_path: impl AsRef<Path>,
    input_paths: I,
) -> OcbResult<Manifest>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = P>,
{
    build_manifest_from_files_with_options(
        manifest_path,
        input_paths,
        ManifestBuildOptions::default(),
    )
}

/// Build a generic selected-snapshot manifest with explicit build options.
pub fn build_manifest_from_files_with_options<P, I>(
    manifest_path: impl AsRef<Path>,
    input_paths: I,
    options: ManifestBuildOptions,
) -> OcbResult<Manifest>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = P>,
{
    let manifest_path = path_to_cstring(manifest_path).map_err(OcbError::from_tio_error)?;
    let input_path_strings = input_paths
        .into_iter()
        .map(|path| path_to_cstring(path).map_err(OcbError::from_tio_error))
        .collect::<OcbResult<Vec<_>>>()?;
    let input_path_ptrs = input_path_strings
        .iter()
        .map(|path| path.as_ptr())
        .collect::<Vec<_>>();
    let input_paths_ptr = if input_path_ptrs.is_empty() {
        ptr::null()
    } else {
        input_path_ptrs.as_ptr()
    };
    let raw_options = RawManifestBuildOptions::new(&options)?;
    let mut raw_manifest = empty_manifest();
    let status = unsafe {
        sys::arcadia_tio_ocb_manifest_build_from_files(
            manifest_path.as_ptr(),
            input_paths_ptr,
            input_path_ptrs.len(),
            &raw_options.raw,
            &mut raw_manifest,
        )
    };
    let guard = ManifestGuard(raw_manifest);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last("OCB manifest build_from_files failed"));
    }
    unsafe { manifest_from_raw(&guard.0) }
}

/// Validate a selected-snapshot manifest against current local files.
pub fn validate_manifest_files(
    manifest_path: impl AsRef<Path>,
    manifest: &Manifest,
) -> OcbResult<ManifestValidationReport> {
    validate_manifest_files_with_options(manifest_path, manifest, OpenOptions::default())
}

/// Validate a selected-snapshot manifest with explicit open-validation options.
pub fn validate_manifest_files_with_options(
    manifest_path: impl AsRef<Path>,
    manifest: &Manifest,
    options: OpenOptions,
) -> OcbResult<ManifestValidationReport> {
    let manifest_path = path_to_cstring(manifest_path).map_err(OcbError::from_tio_error)?;
    let raw_manifest = RawManifest::new(manifest)?;
    let raw_options = raw_open_options(options);
    let mut raw_report = empty_manifest_validation_report();
    let status = unsafe {
        sys::arcadia_tio_ocb_manifest_validate_files(
            manifest_path.as_ptr(),
            &raw_manifest.raw,
            &raw_options,
            &mut raw_report,
        )
    };
    let guard = ManifestValidationReportGuard(raw_report);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last("OCB manifest validate_files failed"));
    }
    unsafe { manifest_validation_report_from_raw(&guard.0) }
}
