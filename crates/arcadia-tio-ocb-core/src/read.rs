#![allow(dead_code)]

//! OCB/v2 metadata and object readers.

#[cfg(test)]
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
#[cfg(not(any(unix, windows)))]
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use super::format::{
    OCB_BOOTSTRAP_MAGIC_V1, OCB_BOOTSTRAP_MAGIC_V2, OCB_BOOTSTRAP_PAGE_V1_LEN,
    OCB_BOOTSTRAP_PAGE_V2_LEN, OCB_COLUMN_CHUNK_MAGIC_V1, OCB_COLUMN_CHUNK_V1_HEADER_LEN,
    OCB_NULL_U32, OCB_ROOT_V1_LEN, OcbBodyKindV1, OcbBodyRefV2, OcbBootstrapPageV1,
    OcbBootstrapPageV2, OcbChecksumKindV1, OcbChunkCodecV1, OcbColumnChunkDescV1,
    OcbColumnChunkObjectV1, OcbColumnStatsV1, OcbDictionaryIndexV1, OcbDictionaryValueKindV1,
    OcbDictionaryValuesV1, OcbLogicalKindV1, OcbNullabilityV1, OcbOrderingProofV1, OcbRootSlotV2,
    OcbRootV1, OcbRootV2, OcbRowGroupDescV1, OcbRowGroupIndexDeltaV1, OcbRowGroupIndexV1,
    OcbSchemaV1, OcbStringTableV1, crc32c, crc32c_finish, crc32c_init, crc32c_update,
};
use super::resource_limits::{
    MetadataMaterializationBudget, OCB_POLICY_A_MAX_ENCODED_OBJECT_BYTES, OcbResourceLimits,
};
use crate::{ArcadiaTioError, OcbFailureCause, Result};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OcbReadObjectAttribution {
    pub(crate) read_io: Duration,
    pub(crate) checksum: Duration,
    pub(crate) bytes_read: u64,
}

#[derive(Debug, Clone)]
pub struct OcbMetadataV1 {
    pub root: OcbRootV1,
    pub string_table: OcbStringTableV1,
    pub schema: OcbSchemaV1,
    pub dictionary_index: Option<OcbDictionaryIndexV1>,
    pub row_group_index: OcbRowGroupIndexV1,
    pub row_group_positions_by_id: HashMap<u32, usize>,
    pub ordering_proof: Option<OcbOrderingProofV1>,
    pub file_len: u64,
    pub appendable: bool,
    pub root_generation: u64,
    pub previous_root_generation: Option<u64>,
    pub resource_limits: OcbResourceLimits,
    pub open_metadata_materialized_bytes: u64,
    pub open_auxiliary_encoded_bytes: u64,
}

impl OcbMetadataV1 {
    pub(crate) fn row_group_by_id(&self, row_group_id: u32) -> Option<&OcbRowGroupDescV1> {
        #[cfg(test)]
        ROW_GROUP_INDEX_LOOKUP_COUNT.with(|count| count.set(count.get().saturating_add(1)));
        let position = *self.row_group_positions_by_id.get(&row_group_id)?;
        self.row_group_index.row_groups.get(position)
    }
}

#[cfg(test)]
std::thread_local! {
    static ROW_GROUP_INDEX_LOOKUP_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_row_group_index_lookup_count_for_test() {
    ROW_GROUP_INDEX_LOOKUP_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn row_group_index_lookup_count_for_test() -> usize {
    ROW_GROUP_INDEX_LOOKUP_COUNT.with(Cell::get)
}

#[derive(Debug)]
struct OcbRootCandidateV2 {
    slot: OcbRootSlotV2,
    root: OcbRootV2,
}

#[cfg(feature = "private-maintenance")]
#[derive(Debug, Clone)]
pub struct OcbMaintenanceAnalysisV2 {
    pub file_len: u64,
    pub selected_slot_id: u16,
    pub selected_root_generation: u64,
    pub previous_root_generation: Option<u64>,
    pub selected_root_end_offset: u64,
    pub selected_snapshot_end_offset: u64,
    pub metadata: OcbMetadataV1,
    pub rejected_candidates: Vec<OcbRootCandidateDiagnosticV2>,
}

#[cfg(feature = "private-maintenance")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcbRootCandidateDiagnosticV2 {
    pub slot_id: Option<u16>,
    pub generation: Option<u64>,
    pub message: String,
}

fn read_exact_ocb<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<()> {
    reader.read_exact(buf).map_err(|err| {
        if err.kind() == ErrorKind::UnexpectedEof {
            ArcadiaTioError::ocb_corrupt_file("OCB object is truncated")
        } else {
            ArcadiaTioError::Io(err)
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcbOpenValidationMode {
    MetadataGraph,
    FullPayload,
}

/// One stable opened OCB identity plus the diagnostic pathname used to open it.
#[derive(Debug)]
pub(crate) struct OcbReadSource {
    file: File,
    file_len: u64,
    diagnostic_path: PathBuf,
    #[cfg(not(any(unix, windows)))]
    fallback_cursor_lock: Mutex<()>,
}

impl OcbReadSource {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        Self::from_file(file, path.to_path_buf())
    }

    pub(crate) fn from_file(file: File, diagnostic_path: PathBuf) -> Result<Self> {
        let file_len = file.metadata()?.len();
        Ok(Self {
            file,
            file_len,
            diagnostic_path,
            #[cfg(not(any(unix, windows)))]
            fallback_cursor_lock: Mutex::new(()),
        })
    }

    pub(crate) const fn file_len(&self) -> u64 {
        self.file_len
    }

    pub(crate) fn diagnostic_path(&self) -> &Path {
        &self.diagnostic_path
    }

    pub(crate) fn cursor(&self) -> OcbReadCursor<'_> {
        OcbReadCursor {
            source: self,
            position: 0,
        }
    }

    #[cfg(unix)]
    fn read_at(&self, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
        std::os::unix::fs::FileExt::read_at(&self.file, buffer, offset)
    }

    #[cfg(windows)]
    fn read_at(&self, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
        std::os::windows::fs::FileExt::seek_read(&self.file, buffer, offset)
    }

    #[cfg(not(any(unix, windows)))]
    fn read_at(&self, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
        let _guard = self
            .fallback_cursor_lock
            .lock()
            .map_err(|_| std::io::Error::other("OCB read-source cursor lock is poisoned"))?;
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(offset))?;
        file.read(buffer)
    }
}

/// Independent logical cursor backed by positioned reads on one `OcbReadSource`.
#[derive(Debug)]
pub(crate) struct OcbReadCursor<'a> {
    source: &'a OcbReadSource,
    position: u64,
}

impl Read for OcbReadCursor<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.source.file_len.saturating_sub(self.position);
        if buffer.is_empty() || remaining == 0 {
            return Ok(0);
        }
        let available = usize::try_from(remaining).unwrap_or(usize::MAX);
        let read_len = buffer.len().min(available);
        let read = self
            .source
            .read_at(&mut buffer[..read_len], self.position)?;
        self.position = self
            .position
            .checked_add(read as u64)
            .ok_or_else(|| std::io::Error::other("OCB read-source cursor position overflows"))?;
        Ok(read)
    }
}

impl Seek for OcbReadCursor<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::End(delta) => i128::from(self.source.file_len) + i128::from(delta),
            SeekFrom::Current(delta) => i128::from(self.position) + i128::from(delta),
        };
        let next = u64::try_from(next).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "OCB read-source cursor seek is outside the file envelope",
            )
        })?;
        self.position = next;
        Ok(next)
    }
}

pub fn read_metadata(path: &Path) -> Result<OcbMetadataV1> {
    let source = OcbReadSource::open(path)?;
    read_metadata_from_source_with_validation_and_resource_limits(
        &source,
        OcbOpenValidationMode::MetadataGraph,
        OcbResourceLimits::policy_a(),
    )
}

pub(crate) fn read_metadata_from_source_with_validation_and_resource_limits(
    source: &OcbReadSource,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
) -> Result<OcbMetadataV1> {
    let mut file = source.cursor();
    let file_len = source.file_len();
    if file_len < OCB_BOOTSTRAP_PAGE_V1_LEN as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB file is shorter than bootstrap page",
        ));
    }

    let mut magic = [0u8; 8];
    file.seek(SeekFrom::Start(0))?;
    read_exact_ocb(&mut file, &mut magic)?;
    file.seek(SeekFrom::Start(0))?;

    match magic {
        OCB_BOOTSTRAP_MAGIC_V1 => {
            read_metadata_v1(&mut file, file_len, validation, resource_limits)
        }
        OCB_BOOTSTRAP_MAGIC_V2 => {
            read_metadata_v2(&mut file, file_len, validation, resource_limits)
        }
        _ => Err(ArcadiaTioError::ocb_unsupported_format(
            "invalid OCB bootstrap magic",
        )),
    }
}

fn read_metadata_v1(
    file: &mut (impl Read + Seek),
    file_len: u64,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
) -> Result<OcbMetadataV1> {
    let bootstrap = OcbBootstrapPageV1::read_from(&mut *file)?;
    bootstrap.root_ref.validate(OcbBodyKindV1::Root, file_len)?;
    if bootstrap.root_ref.length != OCB_ROOT_V1_LEN as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root object length is invalid",
        ));
    }

    let root_bytes = read_object_bytes_with_resource_limits(
        file,
        file_len,
        bootstrap.root_ref,
        OcbBodyKindV1::Root,
        resource_limits,
    )?;
    let root = OcbRootV1::read_from(Cursor::new(root_bytes))?;
    let mut metadata_budget = MetadataMaterializationBudget::from_limits(resource_limits);
    let mut metadata = read_metadata_objects_with_resource_limits(
        file,
        file_len,
        root,
        resource_limits,
        &mut metadata_budget,
    )?;
    let root_column_chunk_count = metadata
        .root
        .column_count
        .checked_mul(metadata.root.row_group_count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB v1 root column chunk count overflows",
        ))?;
    let mut auxiliary_objects = OcbOpenAuxiliaryObjectCache::new(resource_limits);
    let row_group_positions_by_id = validate_metadata_graph(
        file,
        &metadata,
        root_column_chunk_count,
        validation,
        resource_limits,
        None,
        &mut auxiliary_objects,
        &mut metadata_budget,
    )?;
    metadata.row_group_positions_by_id = row_group_positions_by_id;
    metadata.open_metadata_materialized_bytes = metadata_budget.charged_bytes();
    metadata.open_auxiliary_encoded_bytes = auxiliary_objects.total_unique_bytes;
    Ok(metadata)
}

fn read_metadata_v2(
    file: &mut (impl Read + Seek),
    file_len: u64,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
) -> Result<OcbMetadataV1> {
    let bootstrap = OcbBootstrapPageV2::read_from(&mut *file)?;
    select_v2_metadata(file, file_len, &bootstrap, validation, resource_limits)
}

#[cfg(feature = "private-maintenance")]
pub fn analyze_v2_maintenance(
    path: &Path,
    validation: OcbOpenValidationMode,
) -> Result<OcbMaintenanceAnalysisV2> {
    let source = OcbReadSource::open(path)?;
    let mut file = source.cursor();
    let file_len = source.file_len();
    if file_len < OCB_BOOTSTRAP_PAGE_V1_LEN as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB file is shorter than bootstrap page",
        ));
    }
    let mut magic = [0u8; 8];
    file.seek(SeekFrom::Start(0))?;
    read_exact_ocb(&mut file, &mut magic)?;
    file.seek(SeekFrom::Start(0))?;
    match magic {
        OCB_BOOTSTRAP_MAGIC_V2 => {
            let bootstrap = OcbBootstrapPageV2::read_from(&mut file)?;
            analyze_v2_maintenance_from_bootstrap(
                &mut file,
                file_len,
                &bootstrap,
                validation,
                OcbResourceLimits::policy_a(),
            )
        }
        OCB_BOOTSTRAP_MAGIC_V1 => Err(ArcadiaTioError::ocb_unsupported_format(
            "OCB maintenance analysis requires an appendable OCB file",
        )),
        _ => Err(ArcadiaTioError::ocb_unsupported_format(
            "invalid OCB bootstrap magic",
        )),
    }
}

#[cfg(feature = "private-maintenance")]
fn analyze_v2_maintenance_from_bootstrap(
    file: &mut (impl Read + Seek),
    file_len: u64,
    bootstrap: &OcbBootstrapPageV2,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
) -> Result<OcbMaintenanceAnalysisV2> {
    let mut candidates = Vec::new();
    let mut diagnostics = Vec::new();
    for (idx, slot_result) in bootstrap.decoded_root_slots().into_iter().enumerate() {
        let slot_id = idx as u16;
        let slot = match slot_result {
            Ok(slot) => slot,
            Err(err) => {
                if !is_discardable_candidate_error(&err) {
                    return Err(err);
                }
                diagnostics.push(root_candidate_diagnostic(
                    Some(slot_id),
                    None,
                    format!("OCB root slot could not be decoded: {err}"),
                ));
                continue;
            }
        };
        if slot.is_empty() {
            continue;
        }
        if let Err(err) = slot.validate_candidate(slot_id, file_len) {
            if !is_discardable_candidate_error(&err) {
                return Err(err);
            }
            diagnostics.push(root_candidate_diagnostic(
                Some(slot_id),
                Some(slot.generation),
                format!("OCB root slot candidate is invalid: {err}"),
            ));
            continue;
        }
        let root_bytes = match read_object_bytes_with_resource_limits(
            file,
            file_len,
            slot.root_ref,
            OcbBodyKindV1::Root,
            resource_limits,
        ) {
            Ok(bytes) => bytes,
            Err(err) => {
                if !is_discardable_candidate_error(&err) {
                    return Err(err);
                }
                diagnostics.push(root_candidate_diagnostic(
                    Some(slot_id),
                    Some(slot.generation),
                    format!("OCB root object could not be read: {err}"),
                ));
                continue;
            }
        };
        let root = match OcbRootV2::read_from(Cursor::new(root_bytes)) {
            Ok(root) => root,
            Err(err) => {
                if !is_discardable_candidate_error(&err) {
                    return Err(err);
                }
                diagnostics.push(root_candidate_diagnostic(
                    Some(slot_id),
                    Some(slot.generation),
                    format!("OCB root object could not be decoded: {err}"),
                ));
                continue;
            }
        };
        if let Err(err) = root.validate_references(file_len) {
            if !is_discardable_candidate_error(&err) {
                return Err(err);
            }
            diagnostics.push(root_candidate_diagnostic(
                Some(slot_id),
                Some(slot.generation),
                format!("OCB root references are invalid: {err}"),
            ));
            continue;
        }
        if let Err(err) = slot.validate_root(&root) {
            if !is_discardable_candidate_error(&err) {
                return Err(err);
            }
            diagnostics.push(root_candidate_diagnostic(
                Some(slot_id),
                Some(slot.generation),
                format!("OCB root slot/root metadata is inconsistent: {err}"),
            ));
            continue;
        }
        candidates.push(OcbRootCandidateV2 { slot, root });
    }

    let mut auxiliary_objects = OcbOpenAuxiliaryObjectCache::new(resource_limits);
    while let Some(max_generation) = candidates
        .iter()
        .map(|candidate| candidate.slot.generation)
        .max()
    {
        let first_ref = candidates
            .iter()
            .find(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists")
            .slot
            .root_ref;
        let conflicting_same_generation = candidates.iter().any(|candidate| {
            candidate.slot.generation == max_generation && candidate.slot.root_ref != first_ref
        });
        if conflicting_same_generation {
            for candidate in candidates
                .iter()
                .filter(|candidate| candidate.slot.generation == max_generation)
            {
                diagnostics.push(root_candidate_diagnostic(
                    Some(candidate.slot.slot_id),
                    Some(candidate.slot.generation),
                    "OCB root generation has conflicting root references",
                ));
            }
            candidates.retain(|candidate| candidate.slot.generation != max_generation);
            continue;
        }
        let selected_idx = candidates
            .iter()
            .position(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists");
        let candidate = candidates.remove(selected_idx);
        match validate_v2_root_referenced_objects_with_scope(
            file,
            file_len,
            &candidate.root,
            validation,
            resource_limits,
            &mut auxiliary_objects,
        ) {
            Ok(metadata) => {
                let selected_root_end_offset = body_ref_end(candidate.slot.root_ref)?;
                let selected_snapshot_end_offset =
                    selected_snapshot_referenced_end(&candidate.slot, &candidate.root, &metadata)?;
                return Ok(OcbMaintenanceAnalysisV2 {
                    file_len,
                    selected_slot_id: candidate.slot.slot_id,
                    selected_root_generation: candidate.slot.generation,
                    previous_root_generation: (!candidate.slot.previous_root_ref.is_null())
                        .then_some(candidate.slot.previous_generation),
                    selected_root_end_offset,
                    selected_snapshot_end_offset,
                    metadata,
                    rejected_candidates: diagnostics,
                });
            }
            Err(err) => {
                if !is_discardable_candidate_error(&err) {
                    return Err(err);
                }
                diagnostics.push(root_candidate_diagnostic(
                    Some(candidate.slot.slot_id),
                    Some(candidate.slot.generation),
                    format!("OCB selected root candidate failed validation: {err}"),
                ));
                candidates.retain(|candidate| candidate.slot.generation != max_generation);
            }
        }
    }

    Err(ArcadiaTioError::ocb_corrupt_file(
        "OCB root selection found no valid root slot",
    ))
}

fn is_discardable_candidate_error(error: &ArcadiaTioError) -> bool {
    matches!(
        error.ocb_failure_cause(),
        Some(OcbFailureCause::CorruptFile)
    )
}

#[cfg(feature = "private-maintenance")]
fn root_candidate_diagnostic(
    slot_id: Option<u16>,
    generation: Option<u64>,
    message: impl Into<String>,
) -> OcbRootCandidateDiagnosticV2 {
    OcbRootCandidateDiagnosticV2 {
        slot_id,
        generation,
        message: message.into(),
    }
}

fn select_v2_metadata(
    file: &mut (impl Read + Seek),
    file_len: u64,
    bootstrap: &OcbBootstrapPageV2,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
) -> Result<OcbMetadataV1> {
    let mut candidates = Vec::new();
    for (idx, slot_result) in bootstrap.decoded_root_slots().into_iter().enumerate() {
        let slot = match slot_result {
            Ok(slot) => slot,
            Err(err) if is_discardable_candidate_error(&err) => continue,
            Err(err) => return Err(err),
        };
        if slot.is_empty() {
            continue;
        }
        if let Err(err) = slot.validate_candidate(idx as u16, file_len) {
            if is_discardable_candidate_error(&err) {
                continue;
            }
            return Err(err);
        }
        let root_bytes = match read_object_bytes_with_resource_limits(
            file,
            file_len,
            slot.root_ref,
            OcbBodyKindV1::Root,
            resource_limits,
        ) {
            Ok(bytes) => bytes,
            Err(err) if is_discardable_candidate_error(&err) => continue,
            Err(err) => return Err(err),
        };
        let root = match OcbRootV2::read_from(Cursor::new(root_bytes)) {
            Ok(root) => root,
            Err(err) if is_discardable_candidate_error(&err) => continue,
            Err(err) => return Err(err),
        };
        if let Err(err) = root.validate_references(file_len) {
            if is_discardable_candidate_error(&err) {
                continue;
            }
            return Err(err);
        }
        if let Err(err) = slot.validate_root(&root) {
            if is_discardable_candidate_error(&err) {
                continue;
            }
            return Err(err);
        }
        candidates.push(OcbRootCandidateV2 { slot, root });
    }
    choose_v2_metadata(file, file_len, candidates, validation, resource_limits)
}

fn choose_v2_metadata(
    file: &mut (impl Read + Seek),
    file_len: u64,
    mut candidates: Vec<OcbRootCandidateV2>,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
) -> Result<OcbMetadataV1> {
    let mut auxiliary_objects = OcbOpenAuxiliaryObjectCache::new(resource_limits);
    while let Some(max_generation) = candidates
        .iter()
        .map(|candidate| candidate.slot.generation)
        .max()
    {
        let first_ref = candidates
            .iter()
            .find(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists")
            .slot
            .root_ref;
        let conflicting_same_generation = candidates.iter().any(|candidate| {
            candidate.slot.generation == max_generation && candidate.slot.root_ref != first_ref
        });
        if conflicting_same_generation {
            candidates.retain(|candidate| candidate.slot.generation != max_generation);
            continue;
        }
        let selected_idx = candidates
            .iter()
            .position(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists");
        let candidate = candidates.remove(selected_idx);
        match validate_v2_root_referenced_objects_with_scope(
            file,
            file_len,
            &candidate.root,
            validation,
            resource_limits,
            &mut auxiliary_objects,
        ) {
            Ok(metadata) => return Ok(metadata),
            Err(err) if is_discardable_candidate_error(&err) => {}
            Err(err) => return Err(err),
        }
        candidates.retain(|candidate| candidate.slot.generation != max_generation);
    }

    Err(ArcadiaTioError::ocb_corrupt_file(
        "OCB root selection found no valid root slot",
    ))
}

pub(crate) fn read_metadata_objects(
    file: &mut (impl Read + Seek),
    file_len: u64,
    root: OcbRootV1,
) -> Result<OcbMetadataV1> {
    let resource_limits = OcbResourceLimits::policy_a();
    let mut metadata_budget = MetadataMaterializationBudget::from_limits(resource_limits);
    read_metadata_objects_with_resource_limits(
        file,
        file_len,
        root,
        resource_limits,
        &mut metadata_budget,
    )
}

fn read_metadata_objects_with_resource_limits(
    file: &mut (impl Read + Seek),
    file_len: u64,
    root: OcbRootV1,
    resource_limits: OcbResourceLimits,
    metadata_budget: &mut MetadataMaterializationBudget,
) -> Result<OcbMetadataV1> {
    let schema_bytes = read_object_bytes_with_resource_limits(
        file,
        file_len,
        root.schema_ref,
        OcbBodyKindV1::Schema,
        resource_limits,
    )?;
    let schema = OcbSchemaV1::read_from_bytes_with_budget(schema_bytes, metadata_budget)?;

    let string_table_bytes = read_object_bytes_with_resource_limits(
        file,
        file_len,
        schema.string_table_ref,
        OcbBodyKindV1::StringTable,
        resource_limits,
    )?;
    let string_table =
        OcbStringTableV1::read_from_bytes_with_budget(string_table_bytes, metadata_budget)?;

    let dictionary_index = if root.dictionary_index_ref.is_null() {
        None
    } else {
        let bytes = read_object_bytes_with_resource_limits(
            file,
            file_len,
            root.dictionary_index_ref,
            OcbBodyKindV1::DictionaryIndex,
            resource_limits,
        )?;
        Some(OcbDictionaryIndexV1::read_from_bytes_with_budget(
            bytes,
            metadata_budget,
        )?)
    };

    let row_group_index_bytes = read_object_bytes_with_resource_limits(
        file,
        file_len,
        root.row_group_index_ref,
        OcbBodyKindV1::RowGroupIndex,
        resource_limits,
    )?;
    let row_group_index =
        OcbRowGroupIndexV1::read_from_bytes_with_budget(row_group_index_bytes, metadata_budget)?;

    let ordering_proof = if root.ordering_proof_ref.is_null() {
        None
    } else {
        let bytes = read_object_bytes_with_resource_limits(
            file,
            file_len,
            root.ordering_proof_ref,
            OcbBodyKindV1::OrderingProof,
            resource_limits,
        )?;
        Some(OcbOrderingProofV1::read_from_bytes_with_budget(
            bytes,
            metadata_budget,
        )?)
    };

    Ok(OcbMetadataV1 {
        root,
        string_table,
        schema,
        dictionary_index,
        row_group_index,
        row_group_positions_by_id: HashMap::new(),
        ordering_proof,
        file_len,
        appendable: false,
        root_generation: 0,
        previous_root_generation: None,
        resource_limits,
        open_metadata_materialized_bytes: metadata_budget.charged_bytes(),
        open_auxiliary_encoded_bytes: 0,
    })
}

pub fn read_metadata_objects_v2(
    file: &mut (impl Read + Seek),
    file_len: u64,
    root: &OcbRootV2,
) -> Result<OcbMetadataV1> {
    let resource_limits = OcbResourceLimits::policy_a();
    let mut metadata_budget = MetadataMaterializationBudget::from_limits(resource_limits);
    read_metadata_objects_v2_with_resource_limits(
        file,
        file_len,
        root,
        resource_limits,
        &mut metadata_budget,
    )
}

fn read_metadata_objects_v2_with_resource_limits(
    file: &mut (impl Read + Seek),
    file_len: u64,
    root: &OcbRootV2,
    resource_limits: OcbResourceLimits,
    metadata_budget: &mut MetadataMaterializationBudget,
) -> Result<OcbMetadataV1> {
    let mut metadata = read_metadata_objects_with_resource_limits(
        file,
        file_len,
        root.to_v1_root(),
        resource_limits,
        metadata_budget,
    )?;
    metadata.appendable = true;
    metadata.root_generation = root.generation;
    metadata.previous_root_generation =
        (!root.previous_root_ref.is_null()).then_some(root.previous_generation);
    if root.commit_diagnostics_ref.kind == OcbBodyKindV1::RowGroupIndexDelta {
        let delta_bytes = read_object_bytes_with_resource_limits(
            file,
            file_len,
            root.commit_diagnostics_ref,
            OcbBodyKindV1::RowGroupIndexDelta,
            resource_limits,
        )?;
        let delta =
            OcbRowGroupIndexDeltaV1::read_from_bytes_with_budget(delta_bytes, metadata_budget)?;
        apply_row_group_index_delta(&mut metadata, delta, metadata_budget)?;
    }
    metadata.open_metadata_materialized_bytes = metadata_budget.charged_bytes();
    Ok(metadata)
}

pub fn selected_snapshot_referenced_end(
    slot: &OcbRootSlotV2,
    root: &OcbRootV2,
    metadata: &OcbMetadataV1,
) -> Result<u64> {
    let mut max_end = OCB_BOOTSTRAP_PAGE_V2_LEN as u64;
    update_max_ref_end(&mut max_end, slot.root_ref)?;
    update_max_ref_end(&mut max_end, slot.previous_root_ref)?;
    update_max_ref_end(&mut max_end, slot.commit_diagnostics_ref)?;
    update_max_ref_end(&mut max_end, root.previous_root_ref)?;
    update_max_ref_end(&mut max_end, root.schema_ref)?;
    update_max_ref_end(&mut max_end, root.dictionary_index_ref)?;
    update_max_ref_end(&mut max_end, root.row_group_index_ref)?;
    update_max_ref_end(&mut max_end, root.ordering_proof_ref)?;
    update_max_ref_end(&mut max_end, root.debug_json_ref)?;
    update_max_ref_end(&mut max_end, root.first_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.last_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.append_first_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.append_last_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.commit_diagnostics_ref)?;
    update_max_ref_end(&mut max_end, metadata.schema.string_table_ref)?;
    if let Some(dictionary_index) = &metadata.dictionary_index {
        for dictionary in &dictionary_index.dictionaries {
            update_max_ref_end(&mut max_end, dictionary.values_ref)?;
        }
    }
    for row_group in &metadata.row_group_index.row_groups {
        update_max_ref_end(&mut max_end, row_group.first_key_tuple_ref)?;
        update_max_ref_end(&mut max_end, row_group.last_key_tuple_ref)?;
    }
    for chunk in &metadata.row_group_index.column_chunks {
        update_max_ref_end(&mut max_end, chunk.value_ref)?;
        update_max_ref_end(&mut max_end, chunk.validity_ref)?;
    }
    if let Some(ordering_proof) = &metadata.ordering_proof {
        for proof in &ordering_proof.row_group_proofs {
            update_max_ref_end(&mut max_end, proof.first_tuple_ref)?;
            update_max_ref_end(&mut max_end, proof.last_tuple_ref)?;
        }
    }
    Ok(max_end)
}

fn update_max_ref_end(max_end: &mut u64, reference: OcbBodyRefV2) -> Result<()> {
    if reference.is_null() {
        return Ok(());
    }
    let end = body_ref_end(reference)?;
    *max_end = (*max_end).max(end);
    Ok(())
}

fn body_ref_end(reference: OcbBodyRefV2) -> Result<u64> {
    reference
        .offset
        .checked_add(reference.length)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB body reference range overflows",
        ))
}

fn apply_row_group_index_delta(
    metadata: &mut OcbMetadataV1,
    delta: OcbRowGroupIndexDeltaV1,
    metadata_budget: &mut MetadataMaterializationBudget,
) -> Result<()> {
    let OcbMetadataV1 {
        row_group_index: index,
        ordering_proof,
        ..
    } = metadata;
    if index.row_groups.len() != delta.base_row_group_count as usize
        || index.column_chunks.len() != delta.base_column_chunk_count as usize
        || index.stats.len() != delta.base_stat_count as usize
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index delta base counts do not match base index",
        ));
    }
    if delta.flags != index.flags {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index delta flags do not match base index",
        ));
    }
    let mut target_ordering_proof = if delta.row_group_ordering_proofs.is_empty() {
        if delta.base_ordering_proof_count != 0 || !delta.ordering_keys.is_empty() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering metadata is inconsistent",
            ));
        }
        None
    } else {
        if delta.base_ordering_proof_count != delta.base_row_group_count
            || delta.row_group_ordering_proofs.len() != delta.row_groups.len()
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering partition does not match row-group delta",
            ));
        }
        let proof = ordering_proof
            .as_mut()
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta requires ordering proof",
            ))?;
        if proof.row_group_proofs.len() != delta.base_ordering_proof_count as usize {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering base count does not match base proof",
            ));
        }
        if proof.keys != delta.ordering_keys {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering keys do not match base proof",
            ));
        }
        Some(proof)
    };

    checked_delta_destination_len(index.row_groups.len(), delta.row_groups.len())?;
    checked_delta_destination_len(index.column_chunks.len(), delta.column_chunks.len())?;
    checked_delta_destination_len(index.stats.len(), delta.stats.len())?;
    if let Some(proof) = target_ordering_proof.as_ref() {
        checked_delta_destination_len(
            proof.row_group_proofs.len(),
            delta.row_group_ordering_proofs.len(),
        )?;
    }

    // The base and delta parser allocations are already charged. Charge the
    // additional retained descriptors that the destination vectors will own
    // before any reserve or move, after every delta/base structural relation
    // above has been validated.
    metadata_budget.charge(row_group_index_delta_destination_extension_bytes(&delta)?)?;

    index
        .row_groups
        .try_reserve_exact(delta.row_groups.len())
        .map_err(|_| row_group_index_delta_allocation_error())?;
    index
        .column_chunks
        .try_reserve_exact(delta.column_chunks.len())
        .map_err(|_| row_group_index_delta_allocation_error())?;
    index
        .stats
        .try_reserve_exact(delta.stats.len())
        .map_err(|_| row_group_index_delta_allocation_error())?;
    if let Some(proof) = target_ordering_proof.as_mut() {
        proof
            .row_group_proofs
            .try_reserve_exact(delta.row_group_ordering_proofs.len())
            .map_err(|_| row_group_index_delta_allocation_error())?;
    }

    if let Some(proof) = target_ordering_proof {
        proof
            .row_group_proofs
            .extend(delta.row_group_ordering_proofs);
    }
    index.row_groups.extend(delta.row_groups);
    index.column_chunks.extend(delta.column_chunks);
    index.stats.extend(delta.stats);
    Ok(())
}

fn row_group_index_delta_destination_extension_bytes(
    delta: &OcbRowGroupIndexDeltaV1,
) -> Result<u64> {
    let mut bytes = 0u64;
    for part in [
        checked_retained_vec_bytes::<OcbRowGroupDescV1>(delta.row_groups.len())?,
        checked_retained_vec_bytes::<OcbColumnChunkDescV1>(delta.column_chunks.len())?,
        checked_retained_vec_bytes::<OcbColumnStatsV1>(delta.stats.len())?,
        checked_retained_vec_bytes::<super::format::OcbRowGroupOrderingProofV1>(
            delta.row_group_ordering_proofs.len(),
        )?,
    ] {
        bytes = bytes
            .checked_add(part)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta destination size overflows",
            ))?;
    }
    Ok(bytes)
}

fn checked_retained_vec_bytes<T>(count: usize) -> Result<u64> {
    let count = u64::try_from(count).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group index delta destination count exceeds u64")
    })?;
    count
        .checked_mul(std::mem::size_of::<T>() as u64)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index delta destination size overflows",
        ))
}

fn checked_delta_destination_len(base: usize, additional: usize) -> Result<usize> {
    base.checked_add(additional)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index delta destination length overflows",
        ))
}

fn row_group_index_delta_allocation_error() -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::new(
        ErrorKind::OutOfMemory,
        "OCB row-group index delta allocation failed within resource limit",
    ))
}

pub(crate) fn read_column_chunk_from_source_with_resource_limits(
    source: &OcbReadSource,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    resource_limits: OcbResourceLimits,
) -> Result<OcbColumnChunkObjectV1> {
    let mut file = source.cursor();
    read_column_chunk_from_reader_with_resource_limits(&mut file, file_len, chunk, resource_limits)
}

pub fn read_column_chunk_from_reader_with_resource_limits(
    file: &mut (impl Read + Seek),
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    resource_limits: OcbResourceLimits,
) -> Result<OcbColumnChunkObjectV1> {
    validate_column_chunk_object_header(&mut *file, file_len, chunk, resource_limits)?;
    let bytes = read_object_bytes_with_resource_limits(
        &mut *file,
        file_len,
        chunk.value_ref,
        OcbBodyKindV1::ColumnChunk,
        resource_limits,
    )?;
    OcbColumnChunkObjectV1::read_from_bytes(bytes)
}

pub(crate) fn read_column_chunk_from_source_with_attribution_and_resource_limits(
    source: &OcbReadSource,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    attribution: &mut OcbReadObjectAttribution,
    resource_limits: OcbResourceLimits,
) -> Result<OcbColumnChunkObjectV1> {
    let mut file = source.cursor();
    validate_column_chunk_object_header(&mut file, file_len, chunk, resource_limits)?;
    let bytes = read_object_bytes_with_attribution_and_resource_limits(
        &mut file,
        file_len,
        chunk.value_ref,
        OcbBodyKindV1::ColumnChunk,
        attribution,
        resource_limits,
    )?;
    OcbColumnChunkObjectV1::read_from_bytes(bytes)
}

pub(crate) fn read_uncompressed_fixed_binary_chunk_from_source_into_with_resource_limits(
    source: &OcbReadSource,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    out: &mut [u8],
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    read_uncompressed_fixed_binary_chunk_from_source_into_inner(
        source,
        file_len,
        chunk,
        out,
        None,
        resource_limits,
    )
}

pub(crate) fn read_uncompressed_fixed_binary_chunk_from_source_into_with_attribution_and_resource_limits(
    source: &OcbReadSource,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    out: &mut [u8],
    attribution: &mut OcbReadObjectAttribution,
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    read_uncompressed_fixed_binary_chunk_from_source_into_inner(
        source,
        file_len,
        chunk,
        out,
        Some(attribution),
        resource_limits,
    )
}

fn read_uncompressed_fixed_binary_chunk_from_source_into_inner(
    source: &OcbReadSource,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    out: &mut [u8],
    mut attribution: Option<&mut OcbReadObjectAttribution>,
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    if chunk.physical_type != super::format::OcbPhysicalTypeV1::FixedBinary
        || chunk.codec != OcbChunkCodecV1::None
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB direct chunk fill requires uncompressed fixed-binary data",
        ));
    }

    let mut file = source.cursor();
    let read_started = Instant::now();
    let header = validate_column_chunk_object_header(&mut file, file_len, chunk, resource_limits)?;
    let payload_bytes = chunk
        .value_ref
        .length
        .checked_sub(u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object is too short",
        ))?;
    if payload_bytes != out.len() as u64 {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB direct fill buffer byte length does not match uncompressed fixed-binary chunk",
        ));
    }

    read_exact_ocb(&mut file, out)?;
    let mut trailing_crc = [0u8; 4];
    read_exact_ocb(&mut file, &mut trailing_crc)?;
    if let Some(attr) = attribution.as_mut() {
        attr.read_io += read_started.elapsed();
        attr.bytes_read = attr.bytes_read.saturating_add(chunk.value_ref.length);
    }

    let checksum_started = Instant::now();
    let content_crc = crc32c_update(crc32c_update(crc32c_init(), &header), out);
    let actual_body_checksum = crc32c_finish(crc32c_update(content_crc, &trailing_crc));
    let actual_object_checksum = crc32c_finish(crc32c_update(content_crc, &[0, 0, 0, 0]));
    if let Some(attr) = attribution.as_mut() {
        attr.checksum += checksum_started.elapsed();
    }
    if chunk.value_ref.checksum_kind == OcbChecksumKindV1::Crc32c
        && actual_body_checksum != chunk.value_ref.checksum
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB body reference checksum mismatch",
        ));
    }
    if actual_object_checksum != u32::from_le_bytes(trailing_crc) {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk crc mismatch",
        ));
    }

    #[cfg(test)]
    UNCOMPRESSED_FIXED_BINARY_DIRECT_FILL_COUNT.with(|count| {
        count.set(count.get().saturating_add(1));
    });
    Ok(())
}

#[cfg(test)]
std::thread_local! {
    static UNCOMPRESSED_FIXED_BINARY_DIRECT_FILL_COUNT: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn uncompressed_fixed_binary_direct_fill_count_for_test() -> u64 {
    UNCOMPRESSED_FIXED_BINARY_DIRECT_FILL_COUNT.with(std::cell::Cell::get)
}

pub(crate) fn validate_v2_root_referenced_objects(
    file: &mut (impl Read + Seek),
    file_len: u64,
    root: &OcbRootV2,
) -> Result<()> {
    let resource_limits = OcbResourceLimits::policy_a();
    let mut auxiliary_objects = OcbOpenAuxiliaryObjectCache::new(resource_limits);
    validate_v2_root_referenced_objects_with_scope(
        file,
        file_len,
        root,
        OcbOpenValidationMode::FullPayload,
        resource_limits,
        &mut auxiliary_objects,
    )?;
    Ok(())
}

pub fn validate_v2_root_referenced_metadata(
    file: &mut (impl Read + Seek),
    file_len: u64,
    root: &OcbRootV2,
) -> Result<()> {
    let resource_limits = OcbResourceLimits::policy_a();
    let mut auxiliary_objects = OcbOpenAuxiliaryObjectCache::new(resource_limits);
    validate_v2_root_referenced_objects_with_scope(
        file,
        file_len,
        root,
        OcbOpenValidationMode::MetadataGraph,
        resource_limits,
        &mut auxiliary_objects,
    )?;
    Ok(())
}

fn validate_v2_root_referenced_objects_with_scope(
    file: &mut (impl Read + Seek),
    file_len: u64,
    root: &OcbRootV2,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
    auxiliary_objects: &mut OcbOpenAuxiliaryObjectCache,
) -> Result<OcbMetadataV1> {
    // Candidate metadata is not cached across fallback: a rejected candidate
    // and its allocations are dropped before the next (of at most two) root
    // candidates is attempted. Bound each candidate's peak logical metadata
    // materialization independently. The auxiliary cache is intentionally
    // open-scoped because it retains validation summaries and bounds repeated
    // referenced-object work across candidates.
    let mut metadata_budget = MetadataMaterializationBudget::from_limits(resource_limits);
    let mut metadata = read_metadata_objects_v2_with_resource_limits(
        file,
        file_len,
        root,
        resource_limits,
        &mut metadata_budget,
    )?;
    let row_group_positions_by_id = validate_v2_metadata_graph(
        file,
        &metadata,
        root,
        validation,
        resource_limits,
        auxiliary_objects,
        &mut metadata_budget,
    )?;
    metadata.row_group_positions_by_id = row_group_positions_by_id;

    validate_optional_object(
        file,
        file_len,
        root.debug_json_ref,
        OcbBodyKindV1::DebugJsonMetadata,
        resource_limits,
    )?;
    match root.commit_diagnostics_ref.kind {
        OcbBodyKindV1::Unknown => {}
        OcbBodyKindV1::DebugJsonMetadata => validate_optional_object(
            file,
            file_len,
            root.commit_diagnostics_ref,
            OcbBodyKindV1::DebugJsonMetadata,
            resource_limits,
        )?,
        OcbBodyKindV1::RowGroupIndexDelta => validate_optional_object(
            file,
            file_len,
            root.commit_diagnostics_ref,
            OcbBodyKindV1::RowGroupIndexDelta,
            resource_limits,
        )?,
        _ => {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root commit_diagnostics_ref has invalid kind",
            ));
        }
    };
    metadata.open_metadata_materialized_bytes = metadata_budget.charged_bytes();
    metadata.open_auxiliary_encoded_bytes = auxiliary_objects.total_unique_bytes;
    Ok(metadata)
}

fn validate_v2_metadata_graph(
    file: &mut (impl Read + Seek),
    metadata: &OcbMetadataV1,
    root: &OcbRootV2,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
    auxiliary_objects: &mut OcbOpenAuxiliaryObjectCache,
    metadata_budget: &mut MetadataMaterializationBudget,
) -> Result<HashMap<u32, usize>> {
    validate_v2_root_semantics(root, metadata)?;
    validate_metadata_graph(
        file,
        metadata,
        root.column_chunk_count,
        validation,
        resource_limits,
        Some(root),
        auxiliary_objects,
        metadata_budget,
    )
}

fn validate_metadata_graph(
    file: &mut (impl Read + Seek),
    metadata: &OcbMetadataV1,
    root_column_chunk_count: u32,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
    v2_root: Option<&OcbRootV2>,
    auxiliary_objects: &mut OcbOpenAuxiliaryObjectCache,
    metadata_budget: &mut MetadataMaterializationBudget,
) -> Result<HashMap<u32, usize>> {
    validate_root_counts(metadata, root_column_chunk_count)?;
    let columns_by_id = validate_schema_graph(metadata)?;
    auxiliary_objects.preflight(metadata, v2_root)?;
    validate_dictionary_graph(
        file,
        metadata,
        &columns_by_id,
        auxiliary_objects,
        metadata_budget,
    )?;
    let row_group_positions_by_id = validate_row_group_graph(
        file,
        metadata,
        &columns_by_id,
        validation,
        resource_limits,
        auxiliary_objects,
    )?;
    validate_ordering_graph(
        file,
        metadata,
        &columns_by_id,
        &row_group_positions_by_id,
        auxiliary_objects,
    )?;
    if let Some(root) = v2_root {
        for reference in [
            root.first_key_tuple_ref,
            root.last_key_tuple_ref,
            root.append_first_key_tuple_ref,
            root.append_last_key_tuple_ref,
        ] {
            auxiliary_objects.validate_key_tuple(file, metadata.file_len, reference)?;
        }
    }
    Ok(row_group_positions_by_id)
}

/// One exact auxiliary body reference as it participates in open-time work.
///
/// Reserved fields are intentionally part of the key as well: only byte-for-byte
/// identical `OcbBodyRefV2` values share validation work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OcbAuxiliaryBodyRefKey {
    offset: u64,
    length: u64,
    kind: u16,
    flags: u16,
    checksum_kind: u16,
    reserved0: u16,
    checksum: u32,
    reserved1: u32,
}

impl From<OcbBodyRefV2> for OcbAuxiliaryBodyRefKey {
    fn from(reference: OcbBodyRefV2) -> Self {
        Self {
            offset: reference.offset,
            length: reference.length,
            kind: reference.kind as u16,
            flags: reference.flags,
            checksum_kind: reference.checksum_kind as u16,
            reserved0: reference.reserved0,
            checksum: reference.checksum,
            reserved1: reference.reserved1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OcbDictionaryValuesSummary {
    value_kind: OcbDictionaryValueKindV1,
    entry_count: u64,
    fixed_width: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OcbAuxiliaryObjectState {
    Pending,
    KeyTupleValidated,
    DictionaryValues(OcbDictionaryValuesSummary),
}

/// Open-scoped validation cache for repeated dictionary and key-tuple bodies.
///
/// `max_owned_selected_compressed_bytes` independently bounds the aggregate
/// encoded bytes of unique auxiliary objects encountered while opening a file.
/// This covers dictionary-value and key-tuple bodies. Full-payload column
/// chunks, debug JSON, and commit diagnostics are deliberately excluded: they
/// do not become an owned selected result and remain bounded by their applicable
/// per-object limits. Exact full references are charged and read once;
/// references that differ by flags, checksum metadata, or reserved fields
/// remain distinct.
#[derive(Debug)]
struct OcbOpenAuxiliaryObjectCache {
    objects: HashMap<OcbAuxiliaryBodyRefKey, OcbAuxiliaryObjectState>,
    total_unique_bytes: u64,
    resource_limits: OcbResourceLimits,
    #[cfg(test)]
    key_tuple_read_count: u64,
    #[cfg(test)]
    dictionary_parse_count: u64,
}

impl OcbOpenAuxiliaryObjectCache {
    fn new(resource_limits: OcbResourceLimits) -> Self {
        Self {
            objects: HashMap::new(),
            total_unique_bytes: 0,
            resource_limits,
            #[cfg(test)]
            key_tuple_read_count: 0,
            #[cfg(test)]
            dictionary_parse_count: 0,
        }
    }

    fn preflight(&mut self, metadata: &OcbMetadataV1, v2_root: Option<&OcbRootV2>) -> Result<()> {
        self.preflight_with(metadata.file_len, |visitor| {
            visit_auxiliary_object_references(metadata, v2_root, visitor)
        })
    }

    fn preflight_with(
        &mut self,
        file_len: u64,
        mut visit_all: impl FnMut(
            &mut dyn FnMut(OcbBodyRefV2, OcbBodyKindV1) -> Result<()>,
        ) -> Result<()>,
    ) -> Result<()> {
        // Complete structural validation is a separate first pass so a bad or
        // out-of-range reference remains CorruptFile even when the aggregate
        // work set would also exceed the caller's resource policy.
        let mut validate = |reference: OcbBodyRefV2, expected_kind: OcbBodyKindV1| {
            reference.validate(expected_kind, file_len)
        };
        visit_all(&mut validate)?;

        let mut charge = |reference: OcbBodyRefV2, expected_kind: OcbBodyKindV1| {
            self.charge_unique(reference, expected_kind)
        };
        visit_all(&mut charge)?;
        Ok(())
    }

    fn charge_unique(
        &mut self,
        reference: OcbBodyRefV2,
        expected_kind: OcbBodyKindV1,
    ) -> Result<()> {
        debug_assert_eq!(reference.kind, expected_kind);
        let key = OcbAuxiliaryBodyRefKey::from(reference);
        if self.objects.contains_key(&key) {
            return Ok(());
        }
        let total_unique_bytes = self
            .total_unique_bytes
            .checked_add(reference.length)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB open auxiliary-object byte total overflows",
            ))?;
        if total_unique_bytes > self.resource_limits.max_owned_selected_compressed_bytes() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB open auxiliary-object bytes exceed resource limit",
            ));
        }
        self.objects
            .try_reserve(1)
            .map_err(|_| metadata_graph_allocation_error())?;
        self.objects.insert(key, OcbAuxiliaryObjectState::Pending);
        self.total_unique_bytes = total_unique_bytes;
        Ok(())
    }

    fn dictionary_values_summary<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        file_len: u64,
        reference: OcbBodyRefV2,
        metadata_budget: &mut MetadataMaterializationBudget,
    ) -> Result<OcbDictionaryValuesSummary> {
        let key = OcbAuxiliaryBodyRefKey::from(reference);
        match self.objects.get(&key) {
            Some(OcbAuxiliaryObjectState::DictionaryValues(summary)) => return Ok(*summary),
            Some(OcbAuxiliaryObjectState::Pending) => {}
            Some(OcbAuxiliaryObjectState::KeyTupleValidated) | None => {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB dictionary values reference was not preflighted",
                ));
            }
        }

        let values_bytes = read_object_bytes_inner_with_limits(
            reader,
            file_len,
            reference,
            OcbBodyKindV1::DictionaryValues,
            None,
            self.resource_limits.max_encoded_object_bytes(),
            usize::MAX as u64,
        )?;
        let values =
            OcbDictionaryValuesV1::read_from_bytes_with_budget(values_bytes, metadata_budget)?;
        let summary = OcbDictionaryValuesSummary {
            value_kind: values.value_kind,
            entry_count: values.values.len() as u64,
            fixed_width: values.fixed_width,
        };
        let state = self.objects.get_mut(&key).ok_or_else(|| {
            ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values reference disappeared after preflight",
            )
        })?;
        *state = OcbAuxiliaryObjectState::DictionaryValues(summary);
        #[cfg(test)]
        {
            self.dictionary_parse_count = self.dictionary_parse_count.saturating_add(1);
        }
        Ok(summary)
    }

    fn validate_key_tuple<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        file_len: u64,
        reference: OcbBodyRefV2,
    ) -> Result<()> {
        if reference.is_null() {
            return Ok(());
        }
        let key = OcbAuxiliaryBodyRefKey::from(reference);
        match self.objects.get(&key) {
            Some(OcbAuxiliaryObjectState::KeyTupleValidated) => return Ok(()),
            Some(OcbAuxiliaryObjectState::Pending) => {}
            Some(OcbAuxiliaryObjectState::DictionaryValues(_)) | None => {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB key-tuple reference was not preflighted",
                ));
            }
        }

        let _bytes = read_object_bytes_inner_with_limits(
            reader,
            file_len,
            reference,
            OcbBodyKindV1::KeyTuple,
            None,
            self.resource_limits.max_encoded_object_bytes(),
            usize::MAX as u64,
        )?;
        let state = self.objects.get_mut(&key).ok_or_else(|| {
            ArcadiaTioError::ocb_corrupt_file("OCB key-tuple reference disappeared after preflight")
        })?;
        *state = OcbAuxiliaryObjectState::KeyTupleValidated;
        #[cfg(test)]
        {
            self.key_tuple_read_count = self.key_tuple_read_count.saturating_add(1);
        }
        Ok(())
    }
}

fn visit_auxiliary_object_references(
    metadata: &OcbMetadataV1,
    v2_root: Option<&OcbRootV2>,
    visitor: &mut dyn FnMut(OcbBodyRefV2, OcbBodyKindV1) -> Result<()>,
) -> Result<()> {
    if let Some(dictionary_index) = &metadata.dictionary_index {
        for dictionary in &dictionary_index.dictionaries {
            // Dictionary value bodies are mandatory when a descriptor exists.
            visitor(dictionary.values_ref, OcbBodyKindV1::DictionaryValues)?;
        }
    }
    for row_group in &metadata.row_group_index.row_groups {
        visit_optional_key_tuple(row_group.first_key_tuple_ref, visitor)?;
        visit_optional_key_tuple(row_group.last_key_tuple_ref, visitor)?;
    }
    if let Some(ordering_proof) = &metadata.ordering_proof {
        for proof in &ordering_proof.row_group_proofs {
            visit_optional_key_tuple(proof.first_tuple_ref, visitor)?;
            visit_optional_key_tuple(proof.last_tuple_ref, visitor)?;
        }
    }
    if let Some(root) = v2_root {
        visit_optional_key_tuple(root.first_key_tuple_ref, visitor)?;
        visit_optional_key_tuple(root.last_key_tuple_ref, visitor)?;
        visit_optional_key_tuple(root.append_first_key_tuple_ref, visitor)?;
        visit_optional_key_tuple(root.append_last_key_tuple_ref, visitor)?;
    }
    Ok(())
}

fn visit_optional_key_tuple(
    reference: OcbBodyRefV2,
    visitor: &mut dyn FnMut(OcbBodyRefV2, OcbBodyKindV1) -> Result<()>,
) -> Result<()> {
    if !reference.is_null() {
        visitor(reference, OcbBodyKindV1::KeyTuple)?;
    }
    Ok(())
}

fn validate_v2_root_semantics(root: &OcbRootV2, metadata: &OcbMetadataV1) -> Result<()> {
    let is_first_root = root.previous_root_ref.is_null();
    if is_first_root {
        if root.previous_generation != 0 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB first root must not reference a previous generation",
            ));
        }
    } else if root.previous_generation.checked_add(1) != Some(root.generation) {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root previous generation metadata is invalid",
        ));
    }
    let append_row_end = root
        .append_base_row
        .checked_add(root.append_row_count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row range overflows",
        ))?;
    if append_row_end > root.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row range exceeds total rows",
        ));
    }
    let append_group_end = root
        .append_base_row_group
        .checked_add(root.append_row_group_count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row-group range overflows",
        ))?;
    if append_group_end > root.row_group_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row-group range exceeds total row groups",
        ));
    }
    if root.append_row_count == 0 || root.append_row_group_count == 0 {
        if !(is_first_root && root.row_count == 0 && root.row_group_count == 0) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root append range must be non-empty",
            ));
        }
    }
    let appended_rows = metadata
        .row_group_index
        .row_groups
        .iter()
        .filter(|row_group| {
            row_group.row_group_id >= root.append_base_row_group
                && row_group.row_group_id < append_group_end
        })
        .try_fold(0u64, |acc, row_group| acc.checked_add(row_group.row_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row count overflows",
        ))?;
    if appended_rows != root.append_row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row count does not match appended row groups",
        ));
    }
    Ok(())
}

fn validate_root_counts(metadata: &OcbMetadataV1, root_column_chunk_count: u32) -> Result<()> {
    if metadata.root.column_count as usize != metadata.schema.columns.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root column_count does not match schema",
        ));
    }
    if metadata.root.row_group_count as usize != metadata.row_group_index.row_groups.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_group_count does not match row-group index",
        ));
    }
    if root_column_chunk_count as usize != metadata.row_group_index.column_chunks.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root column_chunk_count does not match row-group index",
        ));
    }
    if metadata.root.dictionary_count > 0 && metadata.dictionary_index.is_none() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root dictionary_count requires dictionary index",
        ));
    }
    match (&metadata.dictionary_index, metadata.root.dictionary_count) {
        (Some(index), count) if index.dictionaries.len() != count as usize => {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root dictionary_count does not match dictionary index",
            ));
        }
        (None, 0) | (Some(_), _) => {}
        (None, _) => unreachable!("handled above"),
    }
    let total_rows = metadata
        .row_group_index
        .row_groups
        .iter()
        .try_fold(0u64, |acc, row_group| acc.checked_add(row_group.row_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row_count overflows"))?;
    if total_rows != metadata.root.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_count does not match row groups",
        ));
    }
    Ok(())
}

fn validate_schema_graph<'a>(
    metadata: &'a OcbMetadataV1,
) -> Result<HashMap<u32, &'a super::format::OcbColumnDescV1>> {
    let mut columns_by_id = HashMap::new();
    columns_by_id
        .try_reserve(metadata.schema.columns.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    let mut seen_names = HashSet::new();
    seen_names
        .try_reserve(metadata.schema.columns.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    for column in &metadata.schema.columns {
        if columns_by_id.insert(column.column_id, column).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column ids",
            ));
        }
        let name = metadata
            .string_table
            .strings
            .get(column.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB column name string id is out of range",
            ))?;
        if !seen_names.insert(name) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column names",
            ));
        }
        column.value_byte_width()?;
        if column.logical_kind == OcbLogicalKindV1::DictionaryCode
            && column.dictionary_id == OCB_NULL_U32
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary-coded column must reference a dictionary",
            ));
        }
        if column.logical_kind == OcbLogicalKindV1::DictionaryCode
            && column.physical_type == super::format::OcbPhysicalTypeV1::FixedBinary
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary-coded column cannot use fixed-binary physical type",
            ));
        }
    }
    Ok(columns_by_id)
}

fn validate_dictionary_graph(
    file: &mut (impl Read + Seek),
    metadata: &OcbMetadataV1,
    columns_by_id: &HashMap<u32, &super::format::OcbColumnDescV1>,
    auxiliary_objects: &mut OcbOpenAuxiliaryObjectCache,
    metadata_budget: &mut MetadataMaterializationBudget,
) -> Result<()> {
    let Some(dictionary_index) = &metadata.dictionary_index else {
        for column in columns_by_id.values() {
            if column.logical_kind == OcbLogicalKindV1::DictionaryCode {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB dictionary-coded column references missing dictionary index",
                ));
            }
        }
        return Ok(());
    };

    let mut dictionaries_by_id = HashMap::new();
    dictionaries_by_id
        .try_reserve(dictionary_index.dictionaries.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    let mut seen_names = HashSet::new();
    seen_names
        .try_reserve(dictionary_index.dictionaries.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    for dictionary in &dictionary_index.dictionaries {
        if dictionaries_by_id
            .insert(dictionary.dictionary_id, dictionary)
            .is_some()
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary index has duplicate dictionary ids",
            ));
        }
        let name = metadata
            .string_table
            .strings
            .get(dictionary.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary name string id is out of range",
            ))?;
        if !seen_names.insert(name) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary index has duplicate dictionary names",
            ));
        }
        let values = auxiliary_objects.dictionary_values_summary(
            file,
            metadata.file_len,
            dictionary.values_ref,
            metadata_budget,
        )?;
        if values.value_kind != dictionary.value_kind
            || values.entry_count != u64::from(dictionary.entry_count)
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values do not match dictionary index",
            ));
        }
    }

    for column in columns_by_id.values() {
        if column.logical_kind != OcbLogicalKindV1::DictionaryCode {
            continue;
        }
        let dictionary = dictionaries_by_id.get(&column.dictionary_id).ok_or(
            ArcadiaTioError::ocb_corrupt_file("OCB column references unknown dictionary"),
        )?;
        if dictionary.code_physical_type != column.physical_type {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary code physical type does not match column",
            ));
        }
    }
    Ok(())
}

fn validate_row_group_graph(
    file: &mut (impl Read + Seek),
    metadata: &OcbMetadataV1,
    columns_by_id: &HashMap<u32, &super::format::OcbColumnDescV1>,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
    auxiliary_objects: &mut OcbOpenAuxiliaryObjectCache,
) -> Result<HashMap<u32, usize>> {
    let mut expected_columns = HashSet::new();
    expected_columns
        .try_reserve(columns_by_id.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    expected_columns.extend(columns_by_id.keys().copied());
    let mut row_group_positions_by_id = HashMap::new();
    row_group_positions_by_id
        .try_reserve(metadata.row_group_index.row_groups.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    let mut expected_base_row = 0u64;
    let mut expected_chunk_begin = 0u64;
    let mut expected_stat_begin = 0u64;
    for (row_group_position, row_group) in metadata.row_group_index.row_groups.iter().enumerate() {
        validate_row_group_desc(
            row_group,
            row_group_position,
            &mut row_group_positions_by_id,
            &mut expected_base_row,
            &mut expected_chunk_begin,
            &mut expected_stat_begin,
        )?;
        auxiliary_objects.validate_key_tuple(
            file,
            metadata.file_len,
            row_group.first_key_tuple_ref,
        )?;
        auxiliary_objects.validate_key_tuple(
            file,
            metadata.file_len,
            row_group.last_key_tuple_ref,
        )?;

        let chunks = checked_chunks_for_row_group(metadata, row_group)?;
        let mut chunk_columns = HashSet::new();
        chunk_columns
            .try_reserve(chunks.len())
            .map_err(|_| metadata_graph_allocation_error())?;
        for chunk in chunks {
            validate_chunk_desc(
                file,
                metadata,
                row_group,
                chunk,
                columns_by_id,
                validation,
                resource_limits,
            )?;
            if !chunk_columns.insert(chunk.column_id) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB row group has duplicate chunk descriptors for a column",
                ));
            }
        }
        if chunk_columns != expected_columns {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group chunk descriptors do not cover all schema columns",
            ));
        }
        let stats = checked_stats_for_row_group(metadata, row_group)?;
        let mut stat_columns = HashSet::new();
        stat_columns
            .try_reserve(stats.len())
            .map_err(|_| metadata_graph_allocation_error())?;
        for stat in stats {
            validate_stat_desc(row_group, stat, columns_by_id)?;
            if !stat_columns.insert(stat.column_id) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB row group has duplicate stats for a column",
                ));
            }
        }
    }
    if expected_chunk_begin != metadata.row_group_index.column_chunks.len() as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor ranges do not cover the chunk table",
        ));
    }
    if expected_stat_begin != metadata.row_group_index.stats.len() as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat ranges do not cover the stats table",
        ));
    }
    Ok(row_group_positions_by_id)
}

fn validate_row_group_desc(
    row_group: &OcbRowGroupDescV1,
    row_group_position: usize,
    row_group_positions_by_id: &mut HashMap<u32, usize>,
    expected_base_row: &mut u64,
    expected_chunk_begin: &mut u64,
    expected_stat_begin: &mut u64,
) -> Result<()> {
    if row_group_positions_by_id
        .insert(row_group.row_group_id, row_group_position)
        .is_some()
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index has duplicate row group ids",
        ));
    }
    if row_group.base_row != *expected_base_row {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group base rows are not contiguous",
        ));
    }
    if row_group.chunk_desc_begin != *expected_chunk_begin {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor ranges are not contiguous",
        ));
    }
    if row_group.stat_begin != *expected_stat_begin {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat ranges are not contiguous",
        ));
    }
    *expected_base_row = expected_base_row.checked_add(row_group.row_count).ok_or(
        ArcadiaTioError::ocb_corrupt_file("OCB row-group base row range overflows"),
    )?;
    *expected_chunk_begin = expected_chunk_begin
        .checked_add(u64::from(row_group.chunk_desc_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range overflows",
        ))?;
    *expected_stat_begin = expected_stat_begin
        .checked_add(u64::from(row_group.stat_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range overflows",
        ))?;
    Ok(())
}

fn validate_chunk_desc(
    file: &mut (impl Read + Seek),
    metadata: &OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
    chunk: &OcbColumnChunkDescV1,
    columns_by_id: &HashMap<u32, &super::format::OcbColumnDescV1>,
    validation: OcbOpenValidationMode,
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    if chunk.row_group_id != row_group.row_group_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor references a different row group",
        ));
    }
    let column = columns_by_id
        .get(&chunk.column_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk descriptor references unknown column",
        ))?;
    if chunk.physical_type != column.physical_type {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk descriptor does not match schema",
        ));
    }
    if chunk.row_count != row_group.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk row_count does not match row group",
        ));
    }
    let expected_bytes = column.expected_value_bytes(chunk.row_count)?;
    if chunk.uncompressed_bytes != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk byte length does not match row count and physical type",
        ));
    }
    if !chunk.validity_ref.is_null() {
        if column.nullability != OcbNullabilityV1::Nullable {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB non-null column chunk cannot have a validity bitmap",
            ));
        }
        chunk
            .validity_ref
            .validate(OcbBodyKindV1::ValidityBitmap, metadata.file_len)?;
        if chunk.validity_ref.length != chunk.row_count.div_ceil(8) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB validity bitmap length does not match row count",
            ));
        }
    }
    match validation {
        OcbOpenValidationMode::MetadataGraph => {
            validate_column_chunk_object_header(file, metadata.file_len, chunk, resource_limits)?;
        }
        OcbOpenValidationMode::FullPayload => {
            validate_column_chunk_object_streaming(
                file,
                metadata.file_len,
                chunk,
                resource_limits,
            )?;
        }
    }
    if !chunk.validity_ref.is_null() {
        if validation == OcbOpenValidationMode::FullPayload {
            let validity = read_object_bytes_with_resource_limits(
                file,
                metadata.file_len,
                chunk.validity_ref,
                OcbBodyKindV1::ValidityBitmap,
                resource_limits,
            )?;
            if validity.len() as u64 != chunk.row_count.div_ceil(8) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB validity bitmap length does not match row count",
                ));
            }
        }
    }
    Ok(())
}

fn validate_stat_desc(
    row_group: &OcbRowGroupDescV1,
    stat: &OcbColumnStatsV1,
    columns_by_id: &HashMap<u32, &super::format::OcbColumnDescV1>,
) -> Result<()> {
    if stat.row_group_id != row_group.row_group_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat references a different row group",
        ));
    }
    let column = columns_by_id
        .get(&stat.column_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB stats descriptor references unknown column",
        ))?;
    if stat.physical_type != column.physical_type {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB stat dtype does not match schema column dtype",
        ));
    }
    if stat.physical_type == super::format::OcbPhysicalTypeV1::FixedBinary {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB fixed-binary columns do not support scalar stats",
        ));
    }
    Ok(())
}

fn validate_ordering_graph(
    file: &mut (impl Read + Seek),
    metadata: &OcbMetadataV1,
    columns_by_id: &HashMap<u32, &super::format::OcbColumnDescV1>,
    row_group_positions_by_id: &HashMap<u32, usize>,
    auxiliary_objects: &mut OcbOpenAuxiliaryObjectCache,
) -> Result<()> {
    let Some(ordering_proof) = &metadata.ordering_proof else {
        return Ok(());
    };
    if ordering_proof.row_group_proofs.len() != metadata.row_group_index.row_groups.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB ordering proof row-group count does not match row-group index",
        ));
    }
    let mut seen_key_columns = HashSet::new();
    seen_key_columns
        .try_reserve(ordering_proof.keys.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    for key in &ordering_proof.keys {
        let column = columns_by_id
            .get(&key.column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof key references unknown column",
            ))?;
        if column.physical_type == super::format::OcbPhysicalTypeV1::FixedBinary {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB fixed-binary column cannot be an ordering key",
            ));
        }
        if !seen_key_columns.insert(key.column_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof has duplicate key columns",
            ));
        }
    }
    let mut seen_proof_row_groups = HashSet::new();
    seen_proof_row_groups
        .try_reserve(ordering_proof.row_group_proofs.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    for proof in &ordering_proof.row_group_proofs {
        if !row_group_positions_by_id.contains_key(&proof.row_group_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof references unknown row group",
            ));
        }
        if !seen_proof_row_groups.insert(proof.row_group_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof has duplicate row-group proofs",
            ));
        }
        auxiliary_objects.validate_key_tuple(file, metadata.file_len, proof.first_tuple_ref)?;
        auxiliary_objects.validate_key_tuple(file, metadata.file_len, proof.last_tuple_ref)?;
    }
    let mut expected_proof_row_groups = HashSet::new();
    expected_proof_row_groups
        .try_reserve(row_group_positions_by_id.len())
        .map_err(|_| metadata_graph_allocation_error())?;
    expected_proof_row_groups.extend(row_group_positions_by_id.keys().copied());
    if seen_proof_row_groups != expected_proof_row_groups {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB ordering proof row groups do not cover row-group index",
        ));
    }
    Ok(())
}

fn metadata_graph_allocation_error() -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::new(
        ErrorKind::OutOfMemory,
        "OCB metadata-graph allocation failed within resource limit",
    ))
}

fn checked_chunks_for_row_group<'a>(
    metadata: &'a OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
) -> Result<&'a [OcbColumnChunkDescV1]> {
    let begin = usize::try_from(row_group.chunk_desc_begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group chunk descriptor begin is too large")
    })?;
    let end = begin
        .checked_add(row_group.chunk_desc_count as usize)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range overflows",
        ))?;
    metadata
        .row_group_index
        .column_chunks
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range is out of bounds",
        ))
}

fn checked_stats_for_row_group<'a>(
    metadata: &'a OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
) -> Result<&'a [OcbColumnStatsV1]> {
    let begin = usize::try_from(row_group.stat_begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group stat descriptor begin is too large")
    })?;
    let end = begin.checked_add(row_group.stat_count as usize).ok_or(
        ArcadiaTioError::ocb_corrupt_file("OCB row-group stat descriptor range overflows"),
    )?;
    metadata
        .row_group_index
        .stats
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range is out of bounds",
        ))
}

fn validate_column_chunk_object_header(
    file: &mut (impl Read + Seek),
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    resource_limits: OcbResourceLimits,
) -> Result<[u8; OCB_COLUMN_CHUNK_V1_HEADER_LEN as usize]> {
    let reference = chunk.value_ref;
    reference.validate(OcbBodyKindV1::ColumnChunk, file_len)?;
    if reference.length < u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object is too short",
        ));
    }

    file.seek(SeekFrom::Start(reference.offset))?;
    let mut header = [0u8; OCB_COLUMN_CHUNK_V1_HEADER_LEN as usize];
    read_exact_ocb(file, &mut header)?;
    if header[0..8] != OCB_COLUMN_CHUNK_MAGIC_V1 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "invalid OCB column chunk magic",
        ));
    }
    let version = u16::from_le_bytes(header[8..10].try_into().expect("version bytes"));
    let header_len = u16::from_le_bytes(header[10..12].try_into().expect("header len bytes"));
    let physical_type_raw = u16::from_le_bytes(header[12..14].try_into().expect("dtype bytes"));
    let codec_raw = u16::from_le_bytes(header[14..16].try_into().expect("codec bytes"));
    let row_group_id = u32::from_le_bytes(header[20..24].try_into().expect("row group bytes"));
    let column_id = u32::from_le_bytes(header[24..28].try_into().expect("column bytes"));
    let row_count = u64::from_le_bytes(header[28..36].try_into().expect("row count bytes"));
    let value_bytes = u64::from_le_bytes(header[36..44].try_into().expect("value bytes"));
    if version != 1 {
        return Err(ArcadiaTioError::ocb_unsupported_format(
            "unsupported OCB column chunk object version",
        ));
    }
    if header_len != OCB_COLUMN_CHUNK_V1_HEADER_LEN
        || physical_type_raw != chunk.physical_type as u16
        || codec_raw != chunk.codec as u16
        || row_group_id != chunk.row_group_id
        || column_id != chunk.column_id
        || row_count != chunk.row_count
        || value_bytes != chunk.uncompressed_bytes
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object does not match descriptor",
        ));
    }
    let min_object_len = u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4;
    if reference.length < min_object_len {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk value byte length does not match descriptor",
        ));
    }
    let encoded_bytes =
        reference
            .length
            .checked_sub(min_object_len)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB column chunk object is too short",
            ))?;
    if chunk.codec == OcbChunkCodecV1::None
        && u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN)
            .checked_add(value_bytes)
            .and_then(|length| length.checked_add(4))
            != Some(reference.length)
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB uncompressed column chunk object length does not match descriptor",
        ));
    }
    if chunk.codec == OcbChunkCodecV1::Zstd && reference.length == min_object_len {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB zstd column chunk object has empty payload",
        ));
    }
    if reference.length > usize::MAX as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object exceeds addressable memory",
        ));
    }
    if reference.length > resource_limits.max_encoded_object_bytes() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB encoded object exceeds resource limit",
        ));
    }
    if encoded_bytes > resource_limits.max_compressed_chunk_bytes() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB encoded column chunk payload exceeds resource limit",
        ));
    }
    if chunk.uncompressed_bytes > resource_limits.max_decompressed_chunk_bytes() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB decoded column chunk payload exceeds resource limit",
        ));
    }
    Ok(header)
}

fn validate_column_chunk_object_streaming(
    file: &mut (impl Read + Seek),
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    let reference = chunk.value_ref;
    let header = validate_column_chunk_object_header(file, file_len, chunk, resource_limits)?;
    let encoded_bytes = reference
        .length
        .checked_sub(u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object is too short",
        ))?;

    let mut body_crc = crc32c_update(crc32c_init(), &header);
    let mut object_crc = crc32c_update(crc32c_init(), &header);
    let mut remaining = encoded_bytes;
    let mut buf = [0u8; 8192];
    let mut compressed_payload = if chunk.codec == OcbChunkCodecV1::Zstd {
        let encoded_len = usize::try_from(encoded_bytes).map_err(|_| {
            ArcadiaTioError::ocb_corrupt_file(
                "OCB zstd column chunk payload exceeds addressable memory",
            )
        })?;
        let mut payload = Vec::new();
        payload.try_reserve_exact(encoded_len).map_err(|_| {
            ArcadiaTioError::Io(std::io::Error::new(
                ErrorKind::OutOfMemory,
                "OCB compressed payload allocation failed within resource limit",
            ))
        })?;
        Some(payload)
    } else {
        None
    };
    while remaining > 0 {
        let take = if remaining > buf.len() as u64 {
            buf.len()
        } else {
            usize::try_from(remaining).expect("remaining fits the stack buffer")
        };
        read_exact_ocb(file, &mut buf[..take])?;
        body_crc = crc32c_update(body_crc, &buf[..take]);
        object_crc = crc32c_update(object_crc, &buf[..take]);
        if let Some(payload) = compressed_payload.as_mut() {
            payload.extend_from_slice(&buf[..take]);
        }
        remaining -= take as u64;
    }
    let mut trailing_crc = [0u8; 4];
    read_exact_ocb(file, &mut trailing_crc)?;
    body_crc = crc32c_update(body_crc, &trailing_crc);
    object_crc = crc32c_update(object_crc, &[0, 0, 0, 0]);
    if reference.checksum_kind == OcbChecksumKindV1::Crc32c
        && crc32c_finish(body_crc) != reference.checksum
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB body reference checksum mismatch",
        ));
    }
    if crc32c_finish(object_crc) != u32::from_le_bytes(trailing_crc) {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk crc mismatch",
        ));
    }
    if let Some(payload) = compressed_payload {
        let object = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: chunk.physical_type,
            codec: chunk.codec,
            flags: 0,
            row_group_id: chunk.row_group_id,
            column_id: chunk.column_id,
            row_count: chunk.row_count,
            uncompressed_bytes: chunk.uncompressed_bytes,
            payload,
            crc32c: u32::from_le_bytes(trailing_crc),
        };
        let decoded = object.decode_payload_with_limits(
            resource_limits.max_compressed_chunk_bytes(),
            resource_limits.max_decompressed_chunk_bytes(),
        )?;
        if decoded.len() as u64 != chunk.uncompressed_bytes {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB zstd column chunk decoded byte length does not match descriptor",
            ));
        }
    }
    Ok(())
}

fn validate_optional_object(
    file: &mut (impl Read + Seek),
    file_len: u64,
    reference: OcbBodyRefV2,
    kind: OcbBodyKindV1,
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    if !reference.is_null() {
        let _bytes = read_object_bytes_with_resource_limits(
            file,
            file_len,
            reference,
            kind,
            resource_limits,
        )?;
    }
    Ok(())
}

pub fn read_object_bytes(
    file: &mut (impl Read + Seek),
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
) -> Result<Vec<u8>> {
    read_object_bytes_inner(file, file_len, reference, expected_kind, None)
}

pub(crate) fn read_object_bytes_with_resource_limits(
    file: &mut (impl Read + Seek),
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    resource_limits: OcbResourceLimits,
) -> Result<Vec<u8>> {
    read_object_bytes_inner_with_limits(
        file,
        file_len,
        reference,
        expected_kind,
        None,
        resource_limits.max_encoded_object_bytes(),
        usize::MAX as u64,
    )
}

pub(crate) fn read_object_bytes_with_attribution(
    file: &mut (impl Read + Seek),
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    attribution: &mut OcbReadObjectAttribution,
) -> Result<Vec<u8>> {
    read_object_bytes_inner(file, file_len, reference, expected_kind, Some(attribution))
}

pub(crate) fn read_object_bytes_with_attribution_and_resource_limits(
    file: &mut (impl Read + Seek),
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    attribution: &mut OcbReadObjectAttribution,
    resource_limits: OcbResourceLimits,
) -> Result<Vec<u8>> {
    read_object_bytes_inner_with_limits(
        file,
        file_len,
        reference,
        expected_kind,
        Some(attribution),
        resource_limits.max_encoded_object_bytes(),
        usize::MAX as u64,
    )
}

fn read_object_bytes_inner(
    file: &mut (impl Read + Seek),
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    attribution: Option<&mut OcbReadObjectAttribution>,
) -> Result<Vec<u8>> {
    read_object_bytes_inner_with_limits(
        file,
        file_len,
        reference,
        expected_kind,
        attribution,
        OCB_POLICY_A_MAX_ENCODED_OBJECT_BYTES,
        usize::MAX as u64,
    )
}

fn read_object_bytes_inner_with_limits<R: Read + Seek>(
    reader: &mut R,
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    mut attribution: Option<&mut OcbReadObjectAttribution>,
    max_object_bytes: u64,
    addressable_bytes: u64,
) -> Result<Vec<u8>> {
    reference.validate(expected_kind, file_len)?;
    if reference.length > addressable_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB body reference length exceeds addressable memory",
        ));
    }
    let object_len = usize::try_from(reference.length).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB body reference length exceeds addressable memory")
    })?;
    if reference.length > max_object_bytes {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB encoded object exceeds resource limit",
        ));
    }
    let read_started = Instant::now();
    reader.seek(SeekFrom::Start(reference.offset))?;
    let mut bytes = allocate_object_bytes_fallibly(object_len)?;
    read_exact_ocb(reader, &mut bytes)?;
    if let Some(attr) = attribution.as_mut() {
        attr.read_io += read_started.elapsed();
        attr.bytes_read = attr.bytes_read.saturating_add(reference.length);
    }
    match reference.checksum_kind {
        OcbChecksumKindV1::None => {}
        OcbChecksumKindV1::Crc32c => {
            let checksum_started = Instant::now();
            let actual = crc32c(&bytes);
            if let Some(attr) = attribution.as_mut() {
                attr.checksum += checksum_started.elapsed();
            }
            if actual != reference.checksum {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB body reference checksum mismatch",
                ));
            }
        }
    }
    Ok(bytes)
}

fn allocate_object_bytes_fallibly(object_len: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(object_len).map_err(|_| {
        ArcadiaTioError::Io(std::io::Error::new(
            ErrorKind::OutOfMemory,
            "OCB object allocation failed within resource limit",
        ))
    })?;
    bytes.resize(object_len, 0);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;

    use super::super::format::{
        OcbDictionaryDescV1, OcbPhysicalTypeV1, OcbRowGroupOrderingProofV1,
    };
    use super::*;

    #[test]
    fn read_source_cursors_are_independent_and_keep_the_open_envelope() {
        let root = PathBuf::from(".tmp/ocb-tests");
        fs::create_dir_all(&root).expect("create OCB read-source test directory");
        let path = root.join(format!("read-source-cursors-{}.ocb", std::process::id()));
        let _ = fs::remove_file(&path);
        fs::write(&path, b"abcdefgh").expect("write read-source fixture");

        let source = OcbReadSource::open(&path).expect("open stable read source");
        assert_eq!(source.file_len(), 8);
        assert_eq!(source.diagnostic_path(), path);

        let mut first = source.cursor();
        let mut second = source.cursor();
        let mut bytes = [0u8; 3];
        first.read_exact(&mut bytes).expect("read first cursor");
        assert_eq!(&bytes, b"abc");
        second
            .seek(SeekFrom::Start(4))
            .expect("seek independent cursor");
        second.read_exact(&mut bytes).expect("read second cursor");
        assert_eq!(&bytes, b"efg");
        first.read_exact(&mut bytes).expect("continue first cursor");
        assert_eq!(&bytes, b"def");

        let first_position = first.stream_position().expect("capture cursor position");
        assert!(first.seek(SeekFrom::Current(-100)).is_err());
        assert_eq!(
            first.stream_position().expect("cursor remains positioned"),
            first_position
        );

        OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open fixture for append")
            .write_all(b"ij")
            .expect("append beyond captured envelope");
        let mut captured_end = source.cursor();
        assert_eq!(captured_end.seek(SeekFrom::End(0)).expect("seek end"), 8);
        assert_eq!(captured_end.read(&mut bytes).expect("read captured end"), 0);

        drop(source);
        fs::remove_file(&path).expect("remove read-source fixture");
    }

    struct CountingCursor {
        inner: Cursor<Vec<u8>>,
        read_calls: u64,
        bytes_read: u64,
    }

    impl CountingCursor {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                inner: Cursor::new(bytes),
                read_calls: 0,
                bytes_read: 0,
            }
        }
    }

    impl Read for CountingCursor {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(buf)?;
            self.read_calls = self.read_calls.saturating_add(1);
            self.bytes_read = self.bytes_read.saturating_add(read as u64);
            Ok(read)
        }
    }

    impl Seek for CountingCursor {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(position)
        }
    }

    fn preflight_auxiliary_references(
        references: &[(OcbBodyRefV2, OcbBodyKindV1)],
        file_len: u64,
        max_auxiliary_bytes: u64,
    ) -> Result<OcbOpenAuxiliaryObjectCache> {
        let limits = OcbResourceLimits::policy_a()
            .with_max_owned_selected_compressed_bytes(max_auxiliary_bytes)?;
        let mut cache = OcbOpenAuxiliaryObjectCache::new(limits);
        cache.preflight_with(file_len, |visitor| {
            for &(reference, expected_kind) in references {
                visitor(reference, expected_kind)?;
            }
            Ok(())
        })?;
        Ok(cache)
    }

    fn preflight_and_validate_key_tuple_references(
        reader: &mut CountingCursor,
        references: &[(OcbBodyRefV2, OcbBodyKindV1)],
        file_len: u64,
        max_auxiliary_bytes: u64,
    ) -> Result<OcbOpenAuxiliaryObjectCache> {
        let mut cache = preflight_auxiliary_references(references, file_len, max_auxiliary_bytes)?;
        for &(reference, expected_kind) in references {
            assert_eq!(expected_kind, OcbBodyKindV1::KeyTuple);
            cache.validate_key_tuple(reader, file_len, reference)?;
        }
        Ok(cache)
    }

    fn object_ref(offset: u64, length: u64, checksum: u32) -> OcbBodyRefV2 {
        OcbBodyRefV2::new(offset, length, OcbBodyKindV1::DebugJsonMetadata, checksum)
    }

    fn empty_delta_test_metadata() -> OcbMetadataV1 {
        OcbMetadataV1 {
            root: OcbRootV1 {
                version: 1,
                flags: 0,
                row_count: 0,
                column_count: 0,
                row_group_count: 0,
                dictionary_count: 0,
                schema_ref: OcbBodyRefV2::NULL,
                dictionary_index_ref: OcbBodyRefV2::NULL,
                row_group_index_ref: OcbBodyRefV2::NULL,
                ordering_proof_ref: OcbBodyRefV2::NULL,
                debug_json_ref: OcbBodyRefV2::NULL,
                created_unix_nanos: 0,
                content_flags: 0,
                crc32c: 0,
            },
            string_table: OcbStringTableV1 {
                version: 1,
                strings: Vec::new(),
                crc32c: 0,
            },
            schema: OcbSchemaV1 {
                version: 1,
                string_table_ref: OcbBodyRefV2::NULL,
                columns: Vec::new(),
                crc32c: 0,
            },
            dictionary_index: None,
            row_group_index: OcbRowGroupIndexV1 {
                version: 1,
                flags: 0,
                row_groups: Vec::new(),
                column_chunks: Vec::new(),
                stats: Vec::new(),
                crc32c: 0,
            },
            row_group_positions_by_id: HashMap::new(),
            ordering_proof: None,
            file_len: 0,
            appendable: true,
            root_generation: 1,
            previous_root_generation: None,
            resource_limits: OcbResourceLimits::policy_a(),
            open_metadata_materialized_bytes: 0,
            open_auxiliary_encoded_bytes: 0,
        }
    }

    fn single_row_group_delta() -> OcbRowGroupIndexDeltaV1 {
        OcbRowGroupIndexDeltaV1 {
            version: 1,
            flags: 0,
            base_row_group_count: 0,
            base_column_chunk_count: 0,
            base_stat_count: 0,
            base_ordering_proof_count: 0,
            row_groups: vec![OcbRowGroupDescV1 {
                row_group_id: 0,
                flags: 0,
                base_row: 0,
                row_count: 0,
                chunk_desc_begin: 0,
                chunk_desc_count: 0,
                stat_begin: 0,
                stat_count: 0,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            }],
            column_chunks: Vec::new(),
            stats: Vec::new(),
            ordering_keys: Vec::new(),
            row_group_ordering_proofs: Vec::new(),
            crc32c: 0,
        }
    }

    #[test]
    fn row_group_delta_destination_budget_is_exact_aggregate_and_structure_first() {
        let delta = single_row_group_delta();
        let extension_bytes = row_group_index_delta_destination_extension_bytes(&delta)
            .expect("compute retained destination extension");
        assert_eq!(
            extension_bytes,
            std::mem::size_of::<OcbRowGroupDescV1>() as u64
        );

        let already_charged = 17;
        let exact_limit = already_charged + extension_bytes;
        let mut exact_budget = MetadataMaterializationBudget::new(exact_limit);
        exact_budget
            .charge(already_charged)
            .expect("seed parser materialization charge");
        let mut exact_metadata = empty_delta_test_metadata();
        apply_row_group_index_delta(&mut exact_metadata, delta.clone(), &mut exact_budget)
            .expect("exact aggregate budget accepts destination extension");
        assert_eq!(exact_budget.charged_bytes(), exact_limit);
        assert_eq!(exact_metadata.row_group_index.row_groups.len(), 1);

        let mut short_budget = MetadataMaterializationBudget::new(exact_limit - 1);
        short_budget
            .charge(already_charged)
            .expect("seed parser materialization charge");
        let mut short_metadata = empty_delta_test_metadata();
        let err =
            apply_row_group_index_delta(&mut short_metadata, delta.clone(), &mut short_budget)
                .expect_err("aggregate exact-1 budget must fail before reserve or move");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::InvalidInput)
        );
        assert_eq!(short_budget.charged_bytes(), already_charged);
        assert!(short_metadata.row_group_index.row_groups.is_empty());

        let mut malformed_delta = delta;
        malformed_delta.base_row_group_count = 1;
        let mut structure_first_metadata = empty_delta_test_metadata();
        let mut zero_budget = MetadataMaterializationBudget::new(0);
        let err = apply_row_group_index_delta(
            &mut structure_first_metadata,
            malformed_delta,
            &mut zero_budget,
        )
        .expect_err("malformed base relation must precede budget failure");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );
        assert_eq!(zero_budget.charged_bytes(), 0);
        assert!(
            structure_first_metadata
                .row_group_index
                .row_groups
                .is_empty()
        );
    }

    #[test]
    fn v2_candidate_fallback_discards_only_corruption() {
        assert!(is_discardable_candidate_error(
            &ArcadiaTioError::ocb_corrupt_file("corrupt candidate")
        ));
        assert!(!is_discardable_candidate_error(
            &ArcadiaTioError::ocb_invalid_input("policy failure")
        ));
        assert!(!is_discardable_candidate_error(
            &ArcadiaTioError::ocb_unsupported_format("unsupported candidate")
        ));
        assert!(!is_discardable_candidate_error(&ArcadiaTioError::Io(
            std::io::Error::other("I/O failure")
        )));
    }

    #[test]
    fn bounded_object_read_checks_structure_addressability_and_policy_before_allocation() {
        let bytes = [1u8, 2, 3, 4];
        let reference = object_ref(0, bytes.len() as u64, crc32c(&bytes));
        let mut reader = Cursor::new(bytes);
        assert_eq!(
            read_object_bytes_inner_with_limits(
                &mut reader,
                bytes.len() as u64,
                reference,
                OcbBodyKindV1::DebugJsonMetadata,
                None,
                bytes.len() as u64,
                usize::MAX as u64,
            )
            .expect("exact-limit object read"),
            bytes
        );

        let mut reader = Cursor::new([]);
        let err = read_object_bytes_inner_with_limits(
            &mut reader,
            4,
            object_ref(0, 4, 0),
            OcbBodyKindV1::DebugJsonMetadata,
            None,
            3,
            4,
        )
        .expect_err("object limit+1 must fail before I/O");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::InvalidInput)
        );

        let mut reader = Cursor::new([]);
        let err = read_object_bytes_inner_with_limits(
            &mut reader,
            u64::MAX,
            object_ref(u64::MAX - 1, 4, 0),
            OcbBodyKindV1::DebugJsonMetadata,
            None,
            1,
            u64::MAX,
        )
        .expect_err("reference arithmetic overflow must stay corruption");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let mut reader = Cursor::new([]);
        let err = read_object_bytes_inner_with_limits(
            &mut reader,
            3,
            object_ref(0, 4, 0),
            OcbBodyKindV1::DebugJsonMetadata,
            None,
            2,
            4,
        )
        .expect_err("beyond-file reference must precede policy failure");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let mut reader = Cursor::new([]);
        let err = read_object_bytes_inner_with_limits(
            &mut reader,
            4,
            object_ref(0, 4, 0),
            OcbBodyKindV1::DebugJsonMetadata,
            None,
            4,
            3,
        )
        .expect_err("simulated usize overflow must fail before allocation");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let mut reader = Cursor::new([1u8, 2, 3]);
        let err = read_object_bytes_inner_with_limits(
            &mut reader,
            4,
            object_ref(0, 4, 0),
            OcbBodyKindV1::DebugJsonMetadata,
            None,
            4,
            4,
        )
        .expect_err("truncated object must stay corruption");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let err = allocate_object_bytes_fallibly(usize::MAX)
            .expect_err("capacity overflow must not allocate");
        assert!(matches!(err, ArcadiaTioError::Io(ref io) if io.kind() == ErrorKind::OutOfMemory));
    }

    #[test]
    fn auxiliary_duplicates_are_charged_read_and_parsed_once() {
        let dictionary = OcbDictionaryValuesV1 {
            version: 1,
            value_kind: OcbDictionaryValueKindV1::Utf8,
            fixed_width: 0,
            values: vec![b"alpha".to_vec(), b"beta".to_vec()],
            crc32c: 0,
        };
        let mut dictionary_bytes = Vec::new();
        dictionary
            .write_to(&mut dictionary_bytes)
            .expect("encode dictionary values");
        let key_tuple_bytes = b"key-tuple".to_vec();

        let dictionary_ref = OcbBodyRefV2::new(
            0,
            dictionary_bytes.len() as u64,
            OcbBodyKindV1::DictionaryValues,
            crc32c(&dictionary_bytes),
        );
        let key_tuple_ref = OcbBodyRefV2::new(
            dictionary_bytes.len() as u64,
            key_tuple_bytes.len() as u64,
            OcbBodyKindV1::KeyTuple,
            crc32c(&key_tuple_bytes),
        );
        let file_len = dictionary_ref.length + key_tuple_ref.length;
        let references = [
            (dictionary_ref, OcbBodyKindV1::DictionaryValues),
            (dictionary_ref, OcbBodyKindV1::DictionaryValues),
            (key_tuple_ref, OcbBodyKindV1::KeyTuple),
            (key_tuple_ref, OcbBodyKindV1::KeyTuple),
        ];
        let mut cache = preflight_auxiliary_references(&references, file_len, file_len)
            .expect("exact aggregate limit accepts unique objects");
        assert_eq!(cache.objects.len(), 2);
        assert_eq!(cache.total_unique_bytes, file_len);

        let mut file_bytes = dictionary_bytes;
        file_bytes.extend_from_slice(&key_tuple_bytes);
        let mut reader = CountingCursor::new(file_bytes);
        let mut metadata_budget =
            MetadataMaterializationBudget::from_limits(OcbResourceLimits::policy_a());
        let first = cache
            .dictionary_values_summary(&mut reader, file_len, dictionary_ref, &mut metadata_budget)
            .expect("first dictionary validation");
        let charged_after_first = metadata_budget.charged_bytes();
        let second = cache
            .dictionary_values_summary(&mut reader, file_len, dictionary_ref, &mut metadata_budget)
            .expect("cached dictionary validation");
        assert_eq!(first, second);
        assert_eq!(first.value_kind, OcbDictionaryValueKindV1::Utf8);
        assert_eq!(first.entry_count, 2);
        assert_eq!(first.fixed_width, 0);
        assert!(charged_after_first > 0);
        assert_eq!(metadata_budget.charged_bytes(), charged_after_first);
        cache
            .validate_key_tuple(&mut reader, file_len, key_tuple_ref)
            .expect("first key-tuple validation");
        cache
            .validate_key_tuple(&mut reader, file_len, key_tuple_ref)
            .expect("cached key-tuple validation");

        assert_eq!(cache.dictionary_parse_count, 1);
        assert_eq!(cache.key_tuple_read_count, 1);
        assert_eq!(reader.read_calls, 2);
        assert_eq!(reader.bytes_read, file_len);
    }

    #[test]
    fn auxiliary_reference_inventory_covers_root_row_group_ordering_and_dictionary_refs() {
        let auxiliary_ref = |offset, kind| OcbBodyRefV2::new(offset, 1, kind, 0);
        let dictionary_ref = auxiliary_ref(1, OcbBodyKindV1::DictionaryValues);
        let row_first = auxiliary_ref(2, OcbBodyKindV1::KeyTuple);
        let row_last = auxiliary_ref(3, OcbBodyKindV1::KeyTuple);
        let ordering_first = auxiliary_ref(4, OcbBodyKindV1::KeyTuple);
        let ordering_last = auxiliary_ref(5, OcbBodyKindV1::KeyTuple);
        let root_first = auxiliary_ref(6, OcbBodyKindV1::KeyTuple);
        let root_last = auxiliary_ref(7, OcbBodyKindV1::KeyTuple);
        let append_first = auxiliary_ref(8, OcbBodyKindV1::KeyTuple);
        let append_last = row_first;
        let file_len = 9;

        let metadata = OcbMetadataV1 {
            root: OcbRootV1 {
                version: 1,
                flags: 0,
                row_count: 0,
                column_count: 0,
                row_group_count: 1,
                dictionary_count: 1,
                schema_ref: OcbBodyRefV2::NULL,
                dictionary_index_ref: OcbBodyRefV2::NULL,
                row_group_index_ref: OcbBodyRefV2::NULL,
                ordering_proof_ref: OcbBodyRefV2::NULL,
                debug_json_ref: OcbBodyRefV2::NULL,
                created_unix_nanos: 0,
                content_flags: 0,
                crc32c: 0,
            },
            string_table: OcbStringTableV1 {
                version: 1,
                strings: Vec::new(),
                crc32c: 0,
            },
            schema: OcbSchemaV1 {
                version: 1,
                string_table_ref: OcbBodyRefV2::NULL,
                columns: Vec::new(),
                crc32c: 0,
            },
            dictionary_index: Some(OcbDictionaryIndexV1 {
                version: 1,
                dictionaries: vec![OcbDictionaryDescV1 {
                    dictionary_id: 0,
                    name_string_id: 0,
                    code_physical_type: OcbPhysicalTypeV1::I32,
                    value_kind: OcbDictionaryValueKindV1::Utf8,
                    flags: 0,
                    values_ref: dictionary_ref,
                    entry_count: 0,
                    reserved0: 0,
                }],
                crc32c: 0,
            }),
            row_group_index: OcbRowGroupIndexV1 {
                version: 1,
                flags: 0,
                row_groups: vec![OcbRowGroupDescV1 {
                    row_group_id: 0,
                    flags: 0,
                    base_row: 0,
                    row_count: 0,
                    chunk_desc_begin: 0,
                    chunk_desc_count: 0,
                    stat_begin: 0,
                    stat_count: 0,
                    first_key_tuple_ref: row_first,
                    last_key_tuple_ref: row_last,
                }],
                column_chunks: Vec::new(),
                stats: Vec::new(),
                crc32c: 0,
            },
            row_group_positions_by_id: HashMap::new(),
            ordering_proof: Some(OcbOrderingProofV1 {
                version: 1,
                flags: 0,
                keys: Vec::new(),
                row_group_proofs: vec![OcbRowGroupOrderingProofV1 {
                    row_group_id: 0,
                    flags: 0,
                    first_tuple_ref: ordering_first,
                    last_tuple_ref: ordering_last,
                }],
                crc32c: 0,
            }),
            file_len,
            appendable: true,
            root_generation: 1,
            previous_root_generation: None,
            resource_limits: OcbResourceLimits::policy_a(),
            open_metadata_materialized_bytes: 0,
            open_auxiliary_encoded_bytes: 0,
        };
        let root = OcbRootV2 {
            version: 2,
            flags: 0,
            generation: 1,
            previous_generation: 0,
            previous_root_ref: OcbBodyRefV2::NULL,
            append_base_row: 0,
            append_row_count: 0,
            append_base_row_group: 0,
            append_row_group_count: 0,
            row_count: 0,
            column_count: 0,
            row_group_count: 1,
            dictionary_count: 1,
            column_chunk_count: 0,
            schema_ref: OcbBodyRefV2::NULL,
            dictionary_index_ref: OcbBodyRefV2::NULL,
            row_group_index_ref: OcbBodyRefV2::NULL,
            ordering_proof_ref: OcbBodyRefV2::NULL,
            debug_json_ref: OcbBodyRefV2::NULL,
            first_key_tuple_ref: root_first,
            last_key_tuple_ref: root_last,
            append_first_key_tuple_ref: append_first,
            append_last_key_tuple_ref: append_last,
            commit_diagnostics_ref: OcbBodyRefV2::NULL,
            created_unix_nanos: 0,
            content_flags: 0,
            crc32c: 0,
        };

        let mut visited = Vec::new();
        visit_auxiliary_object_references(&metadata, Some(&root), &mut |reference, kind| {
            visited.push((reference, kind));
            Ok(())
        })
        .expect("visit complete auxiliary inventory");
        assert_eq!(
            visited,
            vec![
                (dictionary_ref, OcbBodyKindV1::DictionaryValues),
                (row_first, OcbBodyKindV1::KeyTuple),
                (row_last, OcbBodyKindV1::KeyTuple),
                (ordering_first, OcbBodyKindV1::KeyTuple),
                (ordering_last, OcbBodyKindV1::KeyTuple),
                (root_first, OcbBodyKindV1::KeyTuple),
                (root_last, OcbBodyKindV1::KeyTuple),
                (append_first, OcbBodyKindV1::KeyTuple),
                (append_last, OcbBodyKindV1::KeyTuple),
            ]
        );

        let exact_limits = OcbResourceLimits::policy_a()
            .with_max_owned_selected_compressed_bytes(8)
            .expect("exact inventory limit");
        let mut cache = OcbOpenAuxiliaryObjectCache::new(exact_limits);
        cache
            .preflight(&metadata, Some(&root))
            .expect("all inventory refs fit after exact deduplication");
        assert_eq!(cache.objects.len(), 8);
        assert_eq!(cache.total_unique_bytes, 8);

        let short_limits = exact_limits
            .with_max_owned_selected_compressed_bytes(7)
            .expect("exact-minus-one inventory limit");
        let mut short_cache = OcbOpenAuxiliaryObjectCache::new(short_limits);
        let err = short_cache
            .preflight(&metadata, Some(&root))
            .expect_err("full inventory must not omit a unique reference");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
    }

    #[test]
    fn auxiliary_two_unique_aggregate_limit_is_exact_and_duplicate_aware_before_io() {
        let first_bytes = [1u8, 2, 3, 4];
        let second_bytes = [5u8, 6, 7, 8];
        let first = OcbBodyRefV2::new(
            0,
            first_bytes.len() as u64,
            OcbBodyKindV1::KeyTuple,
            crc32c(&first_bytes),
        );
        let second = OcbBodyRefV2::new(
            first.length,
            second_bytes.len() as u64,
            OcbBodyKindV1::KeyTuple,
            crc32c(&second_bytes),
        );
        let references = [
            (first, OcbBodyKindV1::KeyTuple),
            (first, OcbBodyKindV1::KeyTuple),
            (second, OcbBodyKindV1::KeyTuple),
            (second, OcbBodyKindV1::KeyTuple),
        ];
        let aggregate = first.length + second.length;
        let mut bytes = first_bytes.to_vec();
        bytes.extend_from_slice(&second_bytes);

        let mut exact_reader = CountingCursor::new(bytes.clone());
        let exact = preflight_and_validate_key_tuple_references(
            &mut exact_reader,
            &references,
            aggregate,
            aggregate,
        )
        .expect("two unique references fit their exact aggregate limit");
        assert_eq!(exact.objects.len(), 2);
        assert_eq!(exact.total_unique_bytes, aggregate);
        assert_eq!(exact.key_tuple_read_count, 2);
        assert_eq!(exact_reader.bytes_read, aggregate);

        let mut excess_reader = CountingCursor::new(bytes);
        let err = preflight_and_validate_key_tuple_references(
            &mut excess_reader,
            &references,
            aggregate,
            aggregate - 1,
        )
        .expect_err("two unique references at exact-minus-one must fail before body I/O");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
        assert_eq!(excess_reader.read_calls, 0);
        assert_eq!(excess_reader.bytes_read, 0);
    }

    #[test]
    fn auxiliary_cache_budget_and_deduplication_span_root_candidates() {
        let shared_bytes = [1u8, 2];
        let first_only_bytes = [3u8, 4, 5];
        let second_only_bytes = [6u8, 7, 8];
        let shared = OcbBodyRefV2::new(
            0,
            shared_bytes.len() as u64,
            OcbBodyKindV1::KeyTuple,
            crc32c(&shared_bytes),
        );
        let first_only = OcbBodyRefV2::new(
            shared.length,
            first_only_bytes.len() as u64,
            OcbBodyKindV1::KeyTuple,
            crc32c(&first_only_bytes),
        );
        let second_only = OcbBodyRefV2::new(
            shared.length + first_only.length,
            second_only_bytes.len() as u64,
            OcbBodyKindV1::KeyTuple,
            crc32c(&second_only_bytes),
        );
        let file_len = shared.length + first_only.length + second_only.length;
        let first_candidate = [
            (shared, OcbBodyKindV1::KeyTuple),
            (first_only, OcbBodyKindV1::KeyTuple),
        ];
        let second_candidate = [
            (shared, OcbBodyKindV1::KeyTuple),
            (second_only, OcbBodyKindV1::KeyTuple),
        ];

        let exact_limits = OcbResourceLimits::policy_a()
            .with_max_owned_selected_compressed_bytes(file_len)
            .expect("exact multi-candidate limit");
        let mut exact_cache = OcbOpenAuxiliaryObjectCache::new(exact_limits);
        exact_cache
            .preflight_with(file_len, |visitor| {
                for &(reference, kind) in &first_candidate {
                    visitor(reference, kind)?;
                }
                Ok(())
            })
            .expect("first candidate preflight");

        let mut bytes = shared_bytes.to_vec();
        bytes.extend_from_slice(&first_only_bytes);
        bytes.extend_from_slice(&second_only_bytes);
        let mut reader = CountingCursor::new(bytes);
        exact_cache
            .validate_key_tuple(&mut reader, file_len, shared)
            .expect("first candidate shared tuple");
        exact_cache
            .validate_key_tuple(&mut reader, file_len, first_only)
            .expect("first candidate unique tuple");
        exact_cache
            .preflight_with(file_len, |visitor| {
                for &(reference, kind) in &second_candidate {
                    visitor(reference, kind)?;
                }
                Ok(())
            })
            .expect("second candidate preflight");
        exact_cache
            .validate_key_tuple(&mut reader, file_len, shared)
            .expect("second candidate reuses shared tuple");
        exact_cache
            .validate_key_tuple(&mut reader, file_len, second_only)
            .expect("second candidate unique tuple");
        assert_eq!(exact_cache.objects.len(), 3);
        assert_eq!(exact_cache.total_unique_bytes, file_len);
        assert_eq!(exact_cache.key_tuple_read_count, 3);
        assert_eq!(reader.bytes_read, file_len);

        let short_limits = exact_limits
            .with_max_owned_selected_compressed_bytes(file_len - 1)
            .expect("multi-candidate exact-minus-one limit");
        let mut short_cache = OcbOpenAuxiliaryObjectCache::new(short_limits);
        short_cache
            .preflight_with(file_len, |visitor| {
                for &(reference, kind) in &first_candidate {
                    visitor(reference, kind)?;
                }
                Ok(())
            })
            .expect("first candidate remains within cumulative limit");
        let err = short_cache
            .preflight_with(file_len, |visitor| {
                for &(reference, kind) in &second_candidate {
                    visitor(reference, kind)?;
                }
                Ok(())
            })
            .expect_err("second candidate must retain the first candidate's charges");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));

        let invalid = OcbBodyRefV2::new(file_len, 1, OcbBodyKindV1::KeyTuple, 0);
        let err = short_cache
            .preflight_with(file_len, |visitor| {
                visitor(second_only, OcbBodyKindV1::KeyTuple)?;
                visitor(invalid, OcbBodyKindV1::KeyTuple)
            })
            .expect_err("candidate structural invalidity must precede cumulative policy failure");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::CorruptFile));
    }

    #[test]
    fn auxiliary_invalid_reference_precedes_aggregate_policy_failure() {
        let valid = OcbBodyRefV2::new(0, 4, OcbBodyKindV1::KeyTuple, 0);
        let out_of_range = OcbBodyRefV2::new(4, 1, OcbBodyKindV1::KeyTuple, 0);
        let references = [
            (valid, OcbBodyKindV1::KeyTuple),
            (out_of_range, OcbBodyKindV1::KeyTuple),
        ];
        let err = preflight_auxiliary_references(&references, 4, 0)
            .expect_err("out-of-range reference must precede zero aggregate limit");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::CorruptFile));
    }

    #[test]
    fn auxiliary_same_range_with_different_checksum_is_not_deduplicated() {
        let bytes = vec![9u8, 8, 7, 6];
        let valid = OcbBodyRefV2::new(
            0,
            bytes.len() as u64,
            OcbBodyKindV1::KeyTuple,
            crc32c(&bytes),
        );
        let different_checksum = OcbBodyRefV2 {
            checksum: valid.checksum ^ 1,
            ..valid
        };
        let references = [
            (valid, OcbBodyKindV1::KeyTuple),
            (different_checksum, OcbBodyKindV1::KeyTuple),
        ];
        let aggregate = valid.length * 2;
        let mut cache = preflight_auxiliary_references(&references, valid.length, aggregate)
            .expect("distinct full references fit exact aggregate");
        assert_eq!(cache.objects.len(), 2);
        assert_eq!(cache.total_unique_bytes, aggregate);

        let mut reader = CountingCursor::new(bytes);
        cache
            .validate_key_tuple(&mut reader, valid.length, valid)
            .expect("valid checksum");
        let err = cache
            .validate_key_tuple(&mut reader, valid.length, different_checksum)
            .expect_err("different checksum must be read and validated independently");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::CorruptFile));
        assert_eq!(reader.bytes_read, aggregate);
    }

    #[test]
    fn auxiliary_same_range_with_different_kind_is_not_deduplicated() {
        let key_tuple = OcbBodyRefV2::new(0, 4, OcbBodyKindV1::KeyTuple, 0);
        let dictionary = OcbBodyRefV2 {
            kind: OcbBodyKindV1::DictionaryValues,
            ..key_tuple
        };
        let references = [
            (key_tuple, OcbBodyKindV1::KeyTuple),
            (dictionary, OcbBodyKindV1::DictionaryValues),
        ];
        let cache = preflight_auxiliary_references(&references, 4, 8)
            .expect("kind-distinct references fit exact aggregate");
        assert_eq!(cache.objects.len(), 2);
        assert_eq!(cache.total_unique_bytes, 8);
    }
}
