//! Public Rust mutation/history and universe-aware tutorial.
//!
//! Rewrites here are narrow dense examples over published entries. Clear-block
//! mutation is shown as an explicit unsupported boundary for this tiny public
//! wrapper fixture. Universe remapping is explicit and payload-driven; the
//! tutorial never infers remaps from equal lengths, display names, or labels.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AppendWithUniverseOptions, AxisIdentityInput, AxisKind, CreateOptions, CreateUniverseOptions,
    DType, DimSpec, EntrySelector, ErrorCode, ExplicitUniverseAxisTarget,
    HistoricalQuerySourceKind, HistoricalReadWithShapePolicyOptions, ReadShapePolicy,
    ReadWithShapePolicyOptions, SlotUniverseBindings, SlotUniverseRemaps, TensorData, TensorFile,
    UniverseBinding, UniverseRemap,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TutorialTempDir::new("mutation_history_universe")?;

    demo_rewrite_pop_revert_and_history(temp.path())?;
    demo_universe_authoring_and_remap(temp.path())?;

    println!(
        "mutation/history/universe ok: rewrite, pop/revert, and remap reads passed in {}",
        temp.path().display()
    );
    Ok(())
}

fn demo_rewrite_pop_revert_and_history(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("mutation_history.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );

    let mut file = TensorFile::create(&path, options)?;
    let base = [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    file.append_f32(&base, &[3, 2])?;
    let base_commit = file.head_commit()?.commit_seq;

    file.rewrite_slice_f32(
        &[EntrySelector::Take(vec![0, 2]), EntrySelector::All],
        &[10.0, 11.0, 50.0, 51.0],
        &[2, 2],
    )?;
    let rewrite_commit = file.head_commit()?.commit_seq;
    assert!(rewrite_commit > base_commit);
    assert_eq!(
        file.read_all()?.data,
        TensorData::F32(vec![10.0, 11.0, 3.0, 4.0, 50.0, 51.0])
    );

    let historical_base = file.read_at_commit(base_commit, &[])?;
    assert_eq!(historical_base.shape, vec![3, 2]);
    assert_eq!(historical_base.data, TensorData::F32(base.to_vec()));

    let clear_boundary = file
        .clear_blocks(&[])
        .expect_err("clear_blocks is an explicit unsupported boundary here");
    assert_eq!(clear_boundary.code(), ErrorCode::Unimplemented);

    file.append_f32(&[7.0, 8.0], &[1, 2])?;
    file.pop()?;
    assert_eq!(
        file.read_all()?.data,
        TensorData::F32(vec![10.0, 11.0, 3.0, 4.0, 50.0, 51.0])
    );
    let after_pop_commit = file.head_commit()?.commit_seq;
    assert!(after_pop_commit > rewrite_commit);

    file.append_f32(&[9.0, 10.0], &[1, 2])?;
    file.revert_commit(base_commit)?;
    let current_head = file.head_commit()?.commit_seq;
    assert!(current_head > after_pop_commit);
    assert_eq!(file.read_all()?.data, TensorData::F32(base.to_vec()));

    let retained_rewrite = file.read_at_commit(rewrite_commit, &[])?;
    assert_eq!(
        retained_rewrite.data,
        TensorData::F32(vec![10.0, 11.0, 3.0, 4.0, 50.0, 51.0])
    );

    let visible = file.list_commits(Some(3))?;
    assert_eq!(visible[0].commit_seq, current_head);
    assert!(visible.len() >= 2);
    assert!(visible[1].commit_seq < current_head);

    drop(file);
    let reopened = TensorFile::open(&path)?;
    assert_eq!(reopened.head_commit()?.commit_seq, current_head);
    assert_eq!(
        reopened.read_at_commit(base_commit, &[])?.data,
        TensorData::F32(base.to_vec())
    );

    Ok(())
}

fn demo_universe_authoring_and_remap(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("universe_remap.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let universe_options = CreateUniverseOptions::new(vec![AxisIdentityInput::universe_aware(1)]);
    let family = uuid(42);

    let mut file = TensorFile::create_with_universe(&path, options, universe_options)?;

    let first_append = AppendWithUniverseOptions {
        slots: vec![SlotUniverseBindings::new(vec![UniverseBinding::new(
            1,
            family,
            uuid(1),
            2,
        )])],
        remap_slots: vec![SlotUniverseRemaps::new(vec![UniverseRemap::new(
            1,
            family,
            uuid(2),
            2,
            vec![1, 0],
        )])],
    };
    let first_range = file.append_f32_with_universe(&[1.0, 2.0], &[1, 2], &first_append)?;
    assert_eq!((first_range.start, first_range.end), (0, 1));
    let first_commit = file.head_commit()?.commit_seq;

    let second_append = AppendWithUniverseOptions {
        slots: vec![
            SlotUniverseBindings::new(vec![UniverseBinding::new(1, family, uuid(2), 2)]),
            SlotUniverseBindings::new(vec![UniverseBinding::new(1, family, uuid(3), 2)]),
        ],
        remap_slots: vec![
            SlotUniverseRemaps::new(vec![UniverseRemap::new(1, family, uuid(3), 2, vec![1, 0])]),
            SlotUniverseRemaps::default(),
        ],
    };
    let second_range =
        file.append_f32_with_universe(&[3.0, 4.0, 5.0, 6.0], &[2, 2], &second_append)?;
    assert_eq!((second_range.start, second_range.end), (1, 3));

    let current_selectors = [
        EntrySelector::Range { start: 2, end: 3 },
        EntrySelector::All,
    ];
    let current = file.read_with_shape_policy_dense(
        &current_selectors,
        ReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitUniverse(vec![
            universe_target(family, 3, 2),
        ])),
        -1.0,
    )?;
    assert_eq!(current.value.tensor.shape, vec![1, 2]);
    assert_eq!(current.value.tensor.data, TensorData::F32(vec![5.0, 6.0]));

    let remapped_selectors = [
        EntrySelector::Range { start: 1, end: 3 },
        EntrySelector::All,
    ];
    let remapped = file.read_with_shape_policy_dense(
        &remapped_selectors,
        ReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitUniverse(vec![
            universe_target(family, 3, 2),
        ])),
        -1.0,
    )?;
    assert_eq!(remapped.value.tensor.shape, vec![2, 2]);
    assert_eq!(
        remapped.value.tensor.data,
        TensorData::F32(vec![4.0, 3.0, 5.0, 6.0])
    );

    let historical_selectors = [
        EntrySelector::Range { start: 0, end: 1 },
        EntrySelector::All,
    ];
    let historical_exact = file.read_at_commit_with_shape_policy_dense(
        first_commit,
        &historical_selectors,
        HistoricalReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitUniverse(vec![
            universe_target(family, 1, 2),
        ])),
        -1.0,
    )?;
    assert_eq!(historical_exact.value.tensor.shape, vec![1, 2]);
    assert_eq!(
        historical_exact.value.tensor.data,
        TensorData::F32(vec![1.0, 2.0])
    );
    assert_eq!(historical_exact.execution.query_commit_seq, first_commit);
    assert_eq!(
        historical_exact.execution.query_source_kind,
        HistoricalQuerySourceKind::RetainedVisibleCommit
    );

    let historical_remapped = file.read_at_commit_with_shape_policy_dense(
        first_commit,
        &historical_selectors,
        HistoricalReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitUniverse(vec![
            universe_target(family, 2, 2),
        ])),
        -1.0,
    )?;
    assert_eq!(historical_remapped.value.tensor.shape, vec![1, 2]);
    assert_eq!(
        historical_remapped.value.tensor.data,
        TensorData::F32(vec![2.0, 1.0])
    );

    let wrong_version = file
        .read_with_shape_policy(
            &current_selectors,
            ReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitUniverse(vec![
                universe_target(family, 9, 2),
            ])),
        )
        .expect_err("wrong explicit-universe version should fail visibly");
    assert_eq!(wrong_version.code(), ErrorCode::InvalidArgument);

    Ok(())
}

fn uuid(value: u8) -> [u8; 16] {
    [value; 16]
}

fn universe_target(family: [u8; 16], version: u8, length: u64) -> ExplicitUniverseAxisTarget {
    ExplicitUniverseAxisTarget::new(1, family, uuid(version), length)
}

struct TutorialTempDir {
    path: PathBuf,
}

impl TutorialTempDir {
    fn new(label: &str) -> std::io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "arcadia_tio_rust_tutorial_{label}_{}_{}",
            process::id(),
            nanos
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TutorialTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
